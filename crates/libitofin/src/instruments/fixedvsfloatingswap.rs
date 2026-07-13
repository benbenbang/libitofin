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

#[cfg(test)]
mod tests {
    //! `FixedVsFloatingSwap` is abstract: its numeric oracle (`swap.cpp`
    //! `testFairRate`, `:107`) runs through the concrete `VanillaSwap` (#322).
    //! These unit tests pin the base's own behaviour with stub legs and stub
    //! engines: the staging inversion (base built fixed-then-floating with the
    //! payer flags `swap_type` implies), the `fetchResults` fair-rate/spread
    //! fallback arithmetic and its guards, the specialised-engine path, the
    //! `setupArguments` split with the floating hook, and the argument
    //! validation.

    use super::*;
    use crate::cashflow::CashFlow;
    use crate::cashflows::SimpleCashFlow;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::instruments::swap::SwapEngine;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::schedule::MakeSchedule;

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(7, Month::July, 2026));
        settings
    }

    fn euribor(settings: &Shared<Settings<Date>>) -> Shared<IborIndex> {
        shared(Euribor::three_months(
            Handle::<dyn YieldTermStructure>::empty(),
            Shared::clone(settings),
        ))
    }

    /// An annual schedule spanning two coupon periods.
    fn fixed_schedule() -> Schedule {
        MakeSchedule::new()
            .from(Date::new(7, Month::July, 2027))
            .to(Date::new(7, Month::July, 2029))
            .with_frequency(Frequency::Annual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .build()
    }

    /// A one-flow stub standing in for the derived swap's floating leg.
    fn floating_stub_leg() -> Leg {
        vec![
            shared(SimpleCashFlow::new(1.0, Date::new(7, Month::July, 2028)).unwrap())
                as Shared<dyn CashFlow>,
        ]
    }

    fn make_swap(swap_type: SwapType, floating_nominals: Vec<Real>) -> FixedVsFloatingSwap {
        make_swap_with_hook(swap_type, floating_nominals, Box::new(|_, _| Ok(())))
    }

    fn make_swap_with_hook(
        swap_type: SwapType,
        floating_nominals: Vec<Real>,
        floating_arguments: FloatingArgumentsFn,
    ) -> FixedVsFloatingSwap {
        let settings = settings_today();
        let index = euribor(&settings);
        FixedVsFloatingSwap::new(
            swap_type,
            vec![100.0],
            fixed_schedule(),
            0.05,
            Some(Actual360::new()),
            floating_nominals,
            fixed_schedule(),
            index,
            0.001,
            Actual360::new(),
            None,
            0,
            None,
            floating_stub_leg(),
            floating_arguments,
            settings,
        )
        .unwrap()
    }

    /// An engine standing in for a generic `Swap` engine: it returns a
    /// [`SwapResults`] bundle with no fair values, so the swap must fall back.
    struct StubSwapEngine {
        base: SwapEngine,
        npv: Real,
        leg_npv: Vec<Real>,
        leg_bps: Vec<Real>,
    }

    impl AsObservable for StubSwapEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for StubSwapEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            let results = self.base.results_mut();
            results.instrument.value = Some(self.npv);
            results.leg_npv = self.leg_npv.clone();
            results.leg_bps = self.leg_bps.clone();
            Ok(())
        }
    }

    fn swap_engine(npv: Real, leg_npv: Vec<Real>, leg_bps: Vec<Real>) -> SharedMut<StubSwapEngine> {
        shared_mut(StubSwapEngine {
            base: SwapEngine::new(SwapArguments::default(), SwapResults::default()),
            npv,
            leg_npv,
            leg_bps,
        })
    }

    /// An engine standing in for a specialised `FixedVsFloatingSwap` engine: it
    /// returns fair values directly, so no fallback is taken.
    struct StubFvfEngine {
        base: FixedVsFloatingSwapEngine,
        npv: Real,
        leg_bps: Vec<Real>,
        fair_rate: Rate,
        fair_spread: Spread,
    }

    impl AsObservable for StubFvfEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for StubFvfEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            let results = self.base.results_mut();
            results.swap.instrument.value = Some(self.npv);
            results.swap.leg_bps = self.leg_bps.clone();
            results.fair_rate = Some(self.fair_rate);
            results.fair_spread = Some(self.fair_spread);
            Ok(())
        }
    }

    /// The base is built fixed-leg first, floating-leg second, and the fixed
    /// leg carries one coupon per annual schedule period.
    #[test]
    fn the_base_is_built_fixed_then_floating() {
        let swap = make_swap(SwapType::Receiver, vec![100.0]);

        assert_eq!(swap.fixed_leg().len(), 2, "two annual fixed coupons");
        assert_eq!(swap.floating_leg().len(), 1, "the supplied floating stub");
        assert_eq!(
            swap.floating_leg()[0].date(),
            Date::new(7, Month::July, 2028)
        );
    }

    /// A `Receiver` receives the fixed leg (+1) and pays the floating (-1); a
    /// `Payer` is the mirror. The multipliers reach the swap-level arguments.
    #[test]
    fn payer_type_maps_the_two_leg_multipliers() {
        let mut receiver = SwapArguments::default();
        make_swap(SwapType::Receiver, vec![100.0])
            .setup_arguments(&mut receiver)
            .unwrap();
        assert_eq!(
            receiver.payer,
            vec![1.0, -1.0],
            "receiver: +fixed, -floating"
        );

        let mut payer = SwapArguments::default();
        make_swap(SwapType::Payer, vec![100.0])
            .setup_arguments(&mut payer)
            .unwrap();
        assert_eq!(payer.payer, vec![-1.0, 1.0], "payer: -fixed, +floating");
    }

    /// A generic `Swap` engine sets no fair values, so `fetchResults` computes
    /// `fairRate = fixedRate - NPV / (legBPS[0] / bp)` and
    /// `fairSpread = spread - NPV / (legBPS[1] / bp)`.
    #[test]
    fn fair_values_fall_back_from_npv_and_leg_bps() {
        let mut swap = make_swap(SwapType::Receiver, vec![100.0]);
        swap.base_mut()
            .set_pricing_engine(swap_engine(2.0, vec![-98.0, 100.0], vec![-1.0, 2.0]));

        assert_eq!(swap.fair_rate().unwrap(), 0.05 - 2.0 / (-1.0 / BASIS_POINT));
        assert_eq!(
            swap.fair_spread().unwrap(),
            0.001 - 2.0 / (2.0 / BASIS_POINT)
        );
    }

    /// When the engine leaves the leg BPS empty, the fallback guard skips and
    /// the fair values stay unavailable (the C++ `legBPS_[i] != Null` check).
    #[test]
    fn absent_leg_bps_leaves_fair_values_unavailable() {
        let mut swap = make_swap(SwapType::Receiver, vec![100.0]);
        swap.base_mut()
            .set_pricing_engine(swap_engine(2.0, vec![], vec![]));

        assert_eq!(
            swap.fair_rate().unwrap_err().message(),
            "result not available"
        );
        assert_eq!(
            swap.fair_spread().unwrap_err().message(),
            "result not available"
        );
    }

    /// A specialised engine's fair values are used as-is; the fallback is not
    /// taken even though the leg BPS would yield a different number.
    #[test]
    fn specialised_engine_fair_values_are_used_directly() {
        let mut swap = make_swap(SwapType::Receiver, vec![100.0]);
        swap.base_mut()
            .set_pricing_engine(shared_mut(StubFvfEngine {
                base: FixedVsFloatingSwapEngine::new(
                    FixedVsFloatingSwapArguments::default(),
                    FixedVsFloatingSwapResults::default(),
                ),
                npv: 2.0,
                leg_bps: vec![-1.0, 2.0],
                fair_rate: 0.09,
                fair_spread: 0.007,
            }));

        assert_eq!(swap.fair_rate().unwrap(), 0.09);
        assert_eq!(swap.fair_spread().unwrap(), 0.007);
    }

    /// The fixed-vs-floating argument branch fills the type, nominal and fixed
    /// coupon vectors, then defers to the floating hook the derived swap
    /// supplies (#321 iterates the floating `IborCoupon`s here).
    #[test]
    fn setup_arguments_fills_fixed_vectors_and_calls_the_floating_hook() {
        let marker = Date::new(1, Month::January, 2030);
        let swap = make_swap_with_hook(
            SwapType::Receiver,
            vec![100.0],
            Box::new(move |_swap, args| {
                args.floating_pay_dates.push(marker);
                Ok(())
            }),
        );

        let mut args = FixedVsFloatingSwapArguments::default();
        swap.setup_arguments(&mut args).unwrap();

        assert_eq!(args.swap_type, Some(SwapType::Receiver));
        assert_eq!(args.nominal, Some(100.0));
        assert_eq!(args.swap.legs.len(), 2);
        assert_eq!(args.fixed_pay_dates.len(), 2);
        assert_eq!(args.fixed_reset_dates.len(), 2);
        assert_eq!(args.fixed_coupons.len(), 2);
        assert_eq!(args.fixed_nominals, vec![100.0, 100.0]);
        assert_eq!(args.floating_pay_dates, vec![marker], "the hook ran");
    }

    /// `nominal()` needs a constant nominal and `nominals()` the same nominals
    /// on both legs; differing legs make both fail.
    #[test]
    fn nominal_accessors_gate_on_constant_and_matching_nominals() {
        let same = make_swap(SwapType::Receiver, vec![100.0]);
        assert_eq!(same.nominal().unwrap(), 100.0);
        assert_eq!(same.nominals().unwrap(), &[100.0]);

        let different = make_swap(SwapType::Receiver, vec![200.0]);
        assert_eq!(
            different.nominal().unwrap_err().message(),
            "nominal is not constant"
        );
        assert_eq!(
            different.nominals().unwrap_err().message(),
            "different nominals on fixed and floating leg"
        );
    }

    /// The per-leg BPS/NPV accessors read the swap's leg results by index.
    #[test]
    fn leg_bps_and_npv_accessors_read_the_swap_results() {
        let mut swap = make_swap(SwapType::Receiver, vec![100.0]);
        swap.base_mut()
            .set_pricing_engine(swap_engine(2.0, vec![-98.0, 100.0], vec![-1.0, 2.0]));

        assert_eq!(swap.fixed_leg_npv().unwrap(), -98.0);
        assert_eq!(swap.floating_leg_npv().unwrap(), 100.0);
        assert_eq!(swap.fixed_leg_bps().unwrap(), -1.0);
        assert_eq!(swap.floating_leg_bps().unwrap(), 2.0);
    }

    /// An expired swap reports no fair values (the C++ `setupExpired` nulls
    /// them), and never consults the engine.
    #[test]
    fn an_expired_swap_has_no_fair_values() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(8, Month::July, 2030));
        let index = euribor(&settings);
        let mut swap = FixedVsFloatingSwap::new(
            SwapType::Receiver,
            vec![100.0],
            fixed_schedule(),
            0.05,
            Some(Actual360::new()),
            vec![100.0],
            fixed_schedule(),
            index,
            0.001,
            Actual360::new(),
            None,
            0,
            None,
            floating_stub_leg(),
            Box::new(|_, _| Ok(())),
            settings,
        )
        .unwrap();

        assert_eq!(swap.npv().unwrap(), 0.0);
        assert_eq!(
            swap.fair_rate().unwrap_err().message(),
            "result not available"
        );
    }

    /// The argument validation inherits the swap-level check and adds the
    /// fixed/floating vector-length checks.
    #[test]
    fn arguments_validate_rejects_vector_length_mismatches() {
        let mut args = FixedVsFloatingSwapArguments {
            fixed_nominals: vec![100.0],
            fixed_pay_dates: vec![Date::new(7, Month::July, 2028)],
            fixed_reset_dates: vec![Date::new(7, Month::July, 2027)],
            fixed_coupons: vec![],
            ..FixedVsFloatingSwapArguments::default()
        };
        assert_eq!(
            args.validate().unwrap_err().message(),
            "number of fixed payment dates different from number of fixed coupon amounts"
        );

        args.fixed_coupons = vec![5.0];
        assert!(args.validate().is_ok(), "matched lengths validate");
    }
}
