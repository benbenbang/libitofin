//! Fixed-vs-floating swap base.
//!
//! Port of `ql/instruments/fixedvsfloatingswap.{hpp,cpp}`: the intermediate
//! [`FixedVsFloatingSwap`] between [`Swap`] and the concrete `VanillaSwap`. The
//! C++ chain is `Instrument -> Swap -> FixedVsFloatingSwap -> VanillaSwap`
//! (`vanillaswap.hpp:65`); this is the middle hop, a two-leg swap carrying a
//! fixed rate/schedule/day-counter and a floating index/spread/schedule, with
//! `fairRate`/`fairSpread`.
//!
//! `FixedVsFloatingSwap` is abstract in C++ (the pure-virtual
//! `setupFloatingArguments`); here it is a concrete struct a derived swap
//! composes, supplying the already-built floating leg and a
//! [`FloatingArgumentsFn`] filler.
//!
//! Deviations, all by existing design decisions or the inheritance-to-composition
//! shift:
//! - The `arguments`, `results` and `engine` inner classes become the free
//!   [`FixedVsFloatingSwapArguments`], [`FixedVsFloatingSwapResults`] and
//!   [`FixedVsFloatingSwapEngine`]. Where C++ derives them from
//!   `Swap::arguments`/`Swap::results`, they embed a [`SwapArguments`] /
//!   [`SwapResults`] slice.
//! - Staging inversion: C++ builds the base in stages - `FixedVsFloatingSwap`
//!   uses the protected `Swap(2)` ctor, fills `legs_[0]` (fixed) and `payer_`,
//!   and leaves `legs_[1]` for `VanillaSwap`. The port's [`Swap`] is build-whole
//!   only, so [`FixedVsFloatingSwap::new`] instead *receives* the already-built
//!   floating leg, builds the fixed leg itself, and constructs the base whole
//!   through [`Swap::new`]. Same final state, construction order inverted.
//! - `fairRate`/`fairSpread` never come from the generic `Swap` engine
//!   (`DiscountingSwapEngine` is a `Swap::engine` and never sets them). C++
//!   reaches a `fetchResults` fallback (`fixedvsfloatingswap.cpp:197-206`)
//!   because the `dynamic_cast` to `FixedVsFloatingSwap::results` fails. Rust
//!   has no `dynamic_cast`, so [`fetch_results`](Instrument::fetch_results)
//!   reads any engine-provided fair values from a
//!   [`FixedVsFloatingSwapResults`] bundle and otherwise computes the fallback
//!   `fairRate = fixedRate - NPV / (legBPS[0] / bp)` and
//!   `fairSpread = spread - NPV / (legBPS[1] / bp)`, guarded exactly as C++ is
//!   (only when the leg BPS is available).
//! - The C++ empty-`DayCounter`/empty-`Calendar` "use the default" sentinels
//!   become `Option` params (D4/D10): an unset fixed day counter defaults to the
//!   index day counter, an unset payment convention to the floating schedule's,
//!   an unset payment calendar to the fixed schedule's. The null-index
//!   `QL_REQUIRE` is dropped: a [`Shared<IborIndex>`] cannot be null.
//! - Nominals are taken as vectors (the single C++ ctor's shape); `VanillaSwap`
//!   (#321) passes a one-element vector. `OvernightIndexedSwap` and
//!   `MultipleResetsSwap`, which also derive from this base with their own
//!   coupon types, are not ported here.

use std::any::Any;

use crate::cashflow::Leg;
use crate::cashflows::FixedRateLeg;
use crate::errors::QlResult;
use crate::indexes::{IborIndex, InterestRateIndex};
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::instruments::swap::{Swap, SwapArguments, SwapResults, SwapType};
use crate::interestrate::Compounding;
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::schedule::Schedule;
use crate::types::{Integer, Rate, Real, Spread, Time};
use crate::{fail, require};

/// One basis point, `1e-4` (the C++ `fixedvsfloatingswap.cpp:184`).
const BASIS_POINT: Real = 1.0e-4;

/// The floating-leg argument filler a derived swap supplies (the C++
/// pure-virtual `setupFloatingArguments`).
///
/// It receives the swap (for its floating leg and index) and the argument
/// bundle to fill. `VanillaSwap` (#321) supplies a closure iterating the
/// floating leg's `IborCoupon`s.
pub type FloatingArgumentsFn =
    Box<dyn Fn(&FixedVsFloatingSwap, &mut FixedVsFloatingSwapArguments) -> QlResult<()>>;

/// Arguments passed to a fixed-vs-floating swap engine (the C++
/// `FixedVsFloatingSwap::arguments`, which derives from `Swap::arguments`).
#[derive(Default)]
pub struct FixedVsFloatingSwapArguments {
    /// The swap-level arguments (legs and payer multipliers).
    pub swap: SwapArguments,
    /// Whether the swap pays or receives the fixed leg.
    pub swap_type: Option<SwapType>,
    /// The common nominal when constant across coupons, else `None`.
    pub nominal: Option<Real>,
    /// The fixed leg's nominal per coupon.
    pub fixed_nominals: Vec<Real>,
    /// The fixed leg's accrual-start (reset) date per coupon.
    pub fixed_reset_dates: Vec<Date>,
    /// The fixed leg's payment date per coupon.
    pub fixed_pay_dates: Vec<Date>,
    /// The floating leg's nominal per coupon.
    pub floating_nominals: Vec<Real>,
    /// The floating leg's accrual time per coupon.
    pub floating_accrual_times: Vec<Time>,
    /// The floating leg's accrual-start (reset) date per coupon.
    pub floating_reset_dates: Vec<Date>,
    /// The floating leg's fixing date per coupon.
    pub floating_fixing_dates: Vec<Date>,
    /// The floating leg's payment date per coupon.
    pub floating_pay_dates: Vec<Date>,
    /// The fixed leg's coupon amount per coupon.
    pub fixed_coupons: Vec<Real>,
    /// The floating leg's spread per coupon.
    pub floating_spreads: Vec<Spread>,
    /// The floating leg's coupon amount per coupon, when known.
    pub floating_coupons: Vec<Real>,
}

impl Arguments for FixedVsFloatingSwapArguments {
    fn validate(&self) -> QlResult<()> {
        self.swap.validate()?;
        require!(
            self.fixed_nominals.len() == self.fixed_pay_dates.len(),
            "number of fixed nominals different from number of fixed payment dates"
        );
        require!(
            self.fixed_reset_dates.len() == self.fixed_pay_dates.len(),
            "number of fixed start dates different from number of fixed payment dates"
        );
        require!(
            self.fixed_pay_dates.len() == self.fixed_coupons.len(),
            "number of fixed payment dates different from number of fixed coupon amounts"
        );
        require!(
            self.floating_nominals.len() == self.floating_pay_dates.len(),
            "number of floating nominals different from number of floating payment dates"
        );
        require!(
            self.floating_reset_dates.len() == self.floating_pay_dates.len(),
            "number of floating start dates different from number of floating payment dates"
        );
        require!(
            self.floating_fixing_dates.len() == self.floating_pay_dates.len(),
            "number of floating fixing dates different from number of floating payment dates"
        );
        require!(
            self.floating_accrual_times.len() == self.floating_pay_dates.len(),
            "number of floating accrual Times different from number of floating payment dates"
        );
        require!(
            self.floating_spreads.len() == self.floating_pay_dates.len(),
            "number of floating spreads different from number of floating payment dates"
        );
        require!(
            self.floating_pay_dates.len() == self.floating_coupons.len(),
            "number of floating payment dates different from number of floating coupon amounts"
        );
        Ok(())
    }
}

/// Results returned by a fixed-vs-floating swap engine (the C++
/// `FixedVsFloatingSwap::results`, which derives from `Swap::results`).
#[derive(Default)]
pub struct FixedVsFloatingSwapResults {
    /// The swap-level results (leg NPVs, BPS and discounts).
    pub swap: SwapResults,
    /// The fair fixed rate that zeroes the swap NPV.
    pub fair_rate: Option<Rate>,
    /// The fair spread over the floating index that zeroes the swap NPV.
    pub fair_spread: Option<Spread>,
}

impl Results for FixedVsFloatingSwapResults {
    fn reset(&mut self) {
        self.swap.reset();
        self.fair_rate = None;
        self.fair_spread = None;
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        self.swap.as_instrument_results()
    }
}

/// Engine base for fixed-vs-floating swaps (the C++
/// `FixedVsFloatingSwap::engine`).
pub type FixedVsFloatingSwapEngine =
    GenericEngine<FixedVsFloatingSwapArguments, FixedVsFloatingSwapResults>;

/// Fixed-vs-floating swap base.
///
/// A two-leg [`Swap`] whose first leg is the fixed leg and second the floating
/// leg. A derived swap builds the floating leg, passes it with a
/// [`FloatingArgumentsFn`] to [`FixedVsFloatingSwap::new`], and composes the
/// resulting value.
pub struct FixedVsFloatingSwap {
    swap: Swap,
    swap_type: SwapType,
    fixed_nominals: Vec<Real>,
    fixed_schedule: Schedule,
    fixed_rate: Rate,
    fixed_day_count: DayCounter,
    floating_nominals: Vec<Real>,
    floating_schedule: Schedule,
    ibor_index: Shared<IborIndex>,
    spread: Spread,
    floating_day_count: DayCounter,
    payment_convention: BusinessDayConvention,
    floating_arguments: FloatingArgumentsFn,
    fair_rate: Option<Rate>,
    fair_spread: Option<Spread>,
    constant_nominals: bool,
    same_nominals: bool,
}

impl FixedVsFloatingSwap {
    /// Builds the base from the fixed-leg parameters and an already-built
    /// floating leg (the C++ ctor, `fixedvsfloatingswap.cpp:33`, with the
    /// staging inversion from the module doc).
    ///
    /// The fixed leg (`legs_[0]`) is built here from `fixed_schedule` /
    /// `fixed_rate` / `fixed_day_count`; the `floating_leg` (`legs_[1]`) is
    /// supplied by the derived swap. Payer flags follow `swap_type`: a `Payer`
    /// swap pays the fixed leg.
    ///
    /// # Errors
    ///
    /// Propagates the fixed-leg build and the [`Swap::new`] leg/payer check.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        fixed_nominals: Vec<Real>,
        fixed_schedule: Schedule,
        fixed_rate: Rate,
        fixed_day_count: Option<DayCounter>,
        floating_nominals: Vec<Real>,
        floating_schedule: Schedule,
        ibor_index: Shared<IborIndex>,
        spread: Spread,
        floating_day_count: DayCounter,
        payment_convention: Option<BusinessDayConvention>,
        payment_lag: Integer,
        payment_calendar: Option<Calendar>,
        floating_leg: Leg,
        floating_arguments: FloatingArgumentsFn,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<FixedVsFloatingSwap> {
        let fixed_day_count = fixed_day_count.unwrap_or_else(|| ibor_index.day_counter().clone());
        let payment_convention =
            payment_convention.unwrap_or_else(|| floating_schedule.business_day_convention());
        let payment_calendar =
            payment_calendar.unwrap_or_else(|| fixed_schedule.calendar().clone());

        let fixed_leg = FixedRateLeg::new(fixed_schedule.clone())
            .with_notionals(fixed_nominals.clone())
            .with_coupon_rates(
                vec![fixed_rate],
                fixed_day_count.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )?
            .with_payment_adjustment(payment_convention)
            .with_payment_lag(payment_lag)
            .with_payment_calendar(payment_calendar)
            .build()?;

        let payer = match swap_type {
            SwapType::Payer => vec![true, false],
            SwapType::Receiver => vec![false, true],
        };

        let same_nominals = fixed_nominals == floating_nominals;
        let constant_nominals = same_nominals
            && match fixed_nominals.first() {
                Some(&front) => fixed_nominals.iter().all(|&x| x == front),
                None => true,
            };

        let swap = Swap::new(vec![fixed_leg, floating_leg], payer, settings)?;

        Ok(FixedVsFloatingSwap {
            swap,
            swap_type,
            fixed_nominals,
            fixed_schedule,
            fixed_rate,
            fixed_day_count,
            floating_nominals,
            floating_schedule,
            ibor_index,
            spread,
            floating_day_count,
            payment_convention,
            floating_arguments,
            fair_rate: None,
            fair_spread: None,
            constant_nominals,
            same_nominals,
        })
    }

    /// Whether the swap pays or receives the fixed leg (`type()`).
    pub fn swap_type(&self) -> SwapType {
        self.swap_type
    }

    /// The common nominal (`nominal()`).
    ///
    /// # Errors
    ///
    /// The nominal must be constant across coupons.
    pub fn nominal(&self) -> QlResult<Real> {
        require!(self.constant_nominals, "nominal is not constant");
        Ok(self.fixed_nominals[0])
    }

    /// The per-coupon nominals shared by both legs (`nominals()`).
    ///
    /// # Errors
    ///
    /// The two legs must carry the same nominals.
    pub fn nominals(&self) -> QlResult<&[Real]> {
        require!(
            self.same_nominals,
            "different nominals on fixed and floating leg"
        );
        Ok(&self.fixed_nominals)
    }

    /// The fixed leg's per-coupon nominals (`fixedNominals()`).
    pub fn fixed_nominals(&self) -> &[Real] {
        &self.fixed_nominals
    }

    /// The fixed leg's schedule (`fixedSchedule()`).
    pub fn fixed_schedule(&self) -> &Schedule {
        &self.fixed_schedule
    }

    /// The fixed rate (`fixedRate()`).
    pub fn fixed_rate(&self) -> Rate {
        self.fixed_rate
    }

    /// The fixed leg's day counter (`fixedDayCount()`).
    pub fn fixed_day_count(&self) -> &DayCounter {
        &self.fixed_day_count
    }

    /// The floating leg's per-coupon nominals (`floatingNominals()`).
    pub fn floating_nominals(&self) -> &[Real] {
        &self.floating_nominals
    }

    /// The floating leg's schedule (`floatingSchedule()`).
    pub fn floating_schedule(&self) -> &Schedule {
        &self.floating_schedule
    }

    /// The floating leg's forecasting index (`iborIndex()`).
    pub fn ibor_index(&self) -> &Shared<IborIndex> {
        &self.ibor_index
    }

    /// The spread over the floating index (`spread()`).
    pub fn spread(&self) -> Spread {
        self.spread
    }

    /// The floating leg's day counter (`floatingDayCount()`).
    pub fn floating_day_count(&self) -> &DayCounter {
        &self.floating_day_count
    }

    /// The payment convention (`paymentConvention()`).
    pub fn payment_convention(&self) -> BusinessDayConvention {
        self.payment_convention
    }

    /// The fixed leg (`fixedLeg()`, `legs_[0]`).
    pub fn fixed_leg(&self) -> &Leg {
        &self.swap.legs()[0]
    }

    /// The floating leg (`floatingLeg()`, `legs_[1]`).
    pub fn floating_leg(&self) -> &Leg {
        &self.swap.legs()[1]
    }

    /// The fixed leg's BPS (`fixedLegBPS()`).
    ///
    /// # Errors
    ///
    /// The engine must have provided the value.
    pub fn fixed_leg_bps(&mut self) -> QlResult<Real> {
        self.swap.leg_bps(0)
    }

    /// The fixed leg's NPV (`fixedLegNPV()`).
    ///
    /// # Errors
    ///
    /// The engine must have provided the value.
    pub fn fixed_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(0)
    }

    /// The floating leg's BPS (`floatingLegBPS()`).
    ///
    /// # Errors
    ///
    /// The engine must have provided the value.
    pub fn floating_leg_bps(&mut self) -> QlResult<Real> {
        self.swap.leg_bps(1)
    }

    /// The floating leg's NPV (`floatingLegNPV()`).
    ///
    /// # Errors
    ///
    /// The engine must have provided the value.
    pub fn floating_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(1)
    }

    /// The fair fixed rate that zeroes the swap NPV (`fairRate()`).
    ///
    /// # Errors
    ///
    /// The rate must be available (a priced, non-expired swap with a fixed-leg
    /// BPS).
    pub fn fair_rate(&mut self) -> QlResult<Rate> {
        self.calculate()?;
        let Some(value) = self.fair_rate else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The fair spread over the floating index that zeroes the swap NPV
    /// (`fairSpread()`).
    ///
    /// # Errors
    ///
    /// The spread must be available (a priced, non-expired swap with a
    /// floating-leg BPS).
    pub fn fair_spread(&mut self) -> QlResult<Spread> {
        self.calculate()?;
        let Some(value) = self.fair_spread else {
            fail!("result not available");
        };
        Ok(value)
    }
}

impl Instrument for FixedVsFloatingSwap {
    fn base(&self) -> &InstrumentBase {
        self.swap.base()
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.swap.base_mut()
    }

    fn is_expired(&self) -> QlResult<bool> {
        self.swap.is_expired()
    }

    fn setup_expired(&mut self) {
        self.swap.setup_expired();
        self.fair_rate = None;
        self.fair_spread = None;
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        if let Some(args) = (arguments as &mut dyn Any).downcast_mut::<SwapArguments>() {
            return self.swap.setup_arguments(args);
        }
        let Some(args) = (arguments as &mut dyn Any).downcast_mut::<FixedVsFloatingSwapArguments>()
        else {
            fail!("wrong argument type");
        };

        self.swap.setup_arguments(&mut args.swap)?;
        args.swap_type = Some(self.swap_type);
        args.nominal = if self.constant_nominals {
            Some(self.nominal()?)
        } else {
            None
        };

        let fixed = self.fixed_leg();
        let n = fixed.len();
        args.fixed_reset_dates = Vec::with_capacity(n);
        args.fixed_pay_dates = Vec::with_capacity(n);
        args.fixed_nominals = Vec::with_capacity(n);
        args.fixed_coupons = Vec::with_capacity(n);
        for flow in fixed {
            let Some(coupon) = flow.as_coupon() else {
                fail!("non-coupon flow on the fixed leg");
            };
            args.fixed_pay_dates.push(flow.date());
            args.fixed_reset_dates.push(coupon.accrual_start_date());
            args.fixed_coupons.push(flow.amount()?);
            args.fixed_nominals.push(coupon.nominal());
        }

        (self.floating_arguments)(self, args)
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let any = results as &dyn Any;
        let (swap_results, mut fair_rate, mut fair_spread) =
            if let Some(bundle) = any.downcast_ref::<FixedVsFloatingSwapResults>() {
                (&bundle.swap, bundle.fair_rate, bundle.fair_spread)
            } else if let Some(bundle) = any.downcast_ref::<SwapResults>() {
                (bundle, None, None)
            } else {
                fail!("wrong result type");
            };

        self.swap.fetch_results(swap_results)?;

        let npv = swap_results.instrument.value;
        if fair_rate.is_none()
            && let (Some(&bps), Some(npv)) = (swap_results.leg_bps.first(), npv)
        {
            fair_rate = Some(self.fixed_rate - npv / (bps / BASIS_POINT));
        }
        if fair_spread.is_none()
            && let (Some(&bps), Some(npv)) = (swap_results.leg_bps.get(1), npv)
        {
            fair_spread = Some(self.spread - npv / (bps / BASIS_POINT));
        }
        self.fair_rate = fair_rate;
        self.fair_spread = fair_spread;
        Ok(())
    }
}
