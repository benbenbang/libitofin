//! Year-on-year inflation-indexed swap.
//!
//! Port of `ql/instruments/yearonyearinflationswap.{hpp,cpp}`: `class
//! YearOnYearInflationSwap : public Swap` (`yearonyearinflationswap.hpp:47`),
//! quoted as a fixed rate `K` that at inception matches the year-on-year
//! inflation `I(t_i)/I(t_{i-1}) - 1` the second leg pays, period by period.
//!
//! Unlike the zero-coupon twin, both legs are genuine coupon legs over a
//! schedule: a [`FixedRateLeg`] against a [`YoYInflationLeg`]. The plain
//! [`DiscountingSwapEngine`](crate::pricingengines::DiscountingSwapEngine)
//! prices it, all the inflation work being done by the coupons.
//!
//! ## The fair rate is back-solved, not read off a flow
//!
//! `fetchResults` (`cpp:167-193`) takes the fair rate from the engine's results
//! when the engine is a year-on-year one, and otherwise recovers it from the
//! swap NPV and the fixed leg's BPS, `K - NPV/(BPS/1e-4)` (`cpp:186-190`,
//! `basisPoint = 1.0e-4` at `:162`). A [`DiscountingSwapEngine`] fills no fair
//! rate, so that fallback is the only path there is here, exactly as in
//! [`FixedVsFloatingSwap`](super::FixedVsFloatingSwap). The engine has already
//! applied each leg's payer multiplier to its BPS, so the recovered rate carries
//! the right sign for either side of the trade.
//!
//! ## Divergences from QuantLib
//!
//! - The nested `arguments`/`results`/`engine` types (`hpp:49-51`, defined at
//!   `hpp:113-147`) are not ported, and neither is the `setupArguments`
//!   override that fills them (`cpp:79-131`): it returns immediately when the
//!   argument bundle is not a year-on-year one (`cpp:82-86`), which is the only
//!   case this crate has, since no engine consuming those arguments exists. The
//!   swap therefore rides on the generic [`SwapArguments`] / [`SwapResults`].
//! - `registerWith` over the year-on-year coupons (`cpp:66-68`) has no explicit
//!   counterpart: [`Swap::new`] registers with every flow of every leg.
//! - The evaluation-date [`Settings`] handle is passed in (D5).
//! - The interpolation is kept as a field though C++ stores none (it forwards it
//!   to the leg and forgets it), so that a caller can read back what the coupons
//!   were built with.
//!
//! [`SwapArguments`]: super::SwapArguments
//! [`SwapResults`]: super::SwapResults

use std::any::Any;

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{FixedRateLeg, YoYInflationCoupon, YoYInflationLeg};
use crate::errors::QlResult;
use crate::fail;
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::swap::{Swap, SwapResults, SwapType};
use crate::interestrate::Compounding;
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Rate, Real, Spread};
use crate::utilities::null::Null;

/// One basis point, the shift the fair rate is recovered against (`cpp:162`).
const BASIS_POINT: Real = 1.0e-4;

/// Year-on-year inflation-indexed swap: a fixed coupon leg against a leg of
/// year-on-year inflation coupons.
///
/// Composes a two-leg [`Swap`] (`legs[0]` fixed, `legs[1]` year-on-year) and
/// prices through its [`Instrument`] face; the base's own accessors are
/// reachable through [`swap`](Self::swap) / [`swap_mut`](Self::swap_mut).
pub struct YearOnYearInflationSwap {
    swap: Swap,
    swap_type: SwapType,
    nominal: Real,
    fixed_schedule: Schedule,
    fixed_rate: Rate,
    fixed_day_count: DayCounter,
    yoy_schedule: Schedule,
    yoy_index: Shared<YoYInflationIndex>,
    yoy_coupons: Vec<Shared<YoYInflationCoupon>>,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    spread: Spread,
    yoy_day_count: DayCounter,
    payment_calendar: Calendar,
    payment_convention: BusinessDayConvention,
    fair_rate: Option<Rate>,
    fair_spread: Option<Spread>,
}

impl YearOnYearInflationSwap {
    /// Builds the swap from its two schedules, the quoted rate and the index the
    /// second leg pays (`cpp:34-77`).
    ///
    /// The two schedules are independent inputs; the fixed leg takes its payment
    /// calendar from its own schedule (`cpp:52`) while the year-on-year leg pays
    /// on `payment_calendar`, the inflation index carrying no calendar of its
    /// own. C++ defaults `payment_convention` to
    /// [`ModifiedFollowing`](BusinessDayConvention::ModifiedFollowing)
    /// (`hpp:65`); the port takes it explicitly.
    ///
    /// # Errors
    ///
    /// Propagates both leg builds: the fixed leg's [`InterestRate`] frequency
    /// precondition and the year-on-year leg's coupon construction.
    ///
    /// [`InterestRate`]: crate::interestrate::InterestRate
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        nominal: Real,
        fixed_schedule: Schedule,
        fixed_rate: Rate,
        fixed_day_count: DayCounter,
        yoy_schedule: Schedule,
        yoy_index: Shared<YoYInflationIndex>,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        spread: Spread,
        yoy_day_count: DayCounter,
        payment_calendar: Calendar,
        payment_convention: BusinessDayConvention,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YearOnYearInflationSwap> {
        let fixed_leg = FixedRateLeg::new(fixed_schedule.clone())
            .with_notional(nominal)
            .with_coupon_rate(
                fixed_rate,
                fixed_day_count.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )?
            .with_payment_adjustment(payment_convention)
            .build()?;

        let yoy_coupons = YoYInflationLeg::new(
            yoy_schedule.clone(),
            payment_calendar.clone(),
            Shared::clone(&yoy_index),
            observation_lag,
            interpolation,
        )
        .with_notional(nominal)
        .with_payment_day_counter(yoy_day_count.clone())
        .with_payment_adjustment(payment_convention)
        .with_spread(spread)
        .coupons()?;
        let yoy_leg: Leg = yoy_coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect();

        let payer = match swap_type {
            SwapType::Payer => vec![true, false],
            SwapType::Receiver => vec![false, true],
        };
        let swap = Swap::new(vec![fixed_leg, yoy_leg], payer, settings)?;

        Ok(YearOnYearInflationSwap {
            swap,
            swap_type,
            nominal,
            fixed_schedule,
            fixed_rate,
            fixed_day_count,
            yoy_schedule,
            yoy_index,
            yoy_coupons,
            observation_lag,
            interpolation,
            spread,
            yoy_day_count,
            payment_calendar,
            payment_convention,
            fair_rate: None,
            fair_spread: None,
        })
    }

    /// The embedded two-leg base.
    pub fn swap(&self) -> &Swap {
        &self.swap
    }

    /// The embedded base, mutably (its on-demand-pricing accessors).
    pub fn swap_mut(&mut self) -> &mut Swap {
        &mut self.swap
    }

    /// Whether the *fixed* leg is paid or received (`type()`).
    pub fn swap_type(&self) -> SwapType {
        self.swap_type
    }

    /// The notional both legs are scaled by (`nominal()`).
    pub fn nominal(&self) -> Real {
        self.nominal
    }

    /// The fixed leg's schedule (`fixedSchedule()`).
    pub fn fixed_schedule(&self) -> &Schedule {
        &self.fixed_schedule
    }

    /// The quoted rate `K` (`fixedRate()`).
    pub fn fixed_rate(&self) -> Rate {
        self.fixed_rate
    }

    /// The day counter the fixed coupons accrue with (`fixedDayCount()`).
    pub fn fixed_day_count(&self) -> &DayCounter {
        &self.fixed_day_count
    }

    /// The year-on-year leg's schedule (`yoySchedule()`).
    pub fn yoy_schedule(&self) -> &Schedule {
        &self.yoy_schedule
    }

    /// The index the second leg pays (`yoyInflationIndex()`).
    pub fn yoy_inflation_index(&self) -> &Shared<YoYInflationIndex> {
        &self.yoy_index
    }

    /// The lag the year-on-year observations are taken at (`observationLag()`).
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// How the observations interpolate within their period.
    pub fn interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }

    /// The spread the year-on-year coupons carry over the index (`spread()`).
    pub fn spread(&self) -> Spread {
        self.spread
    }

    /// The day counter the year-on-year coupons accrue with (`yoyDayCount()`).
    pub fn yoy_day_count(&self) -> &DayCounter {
        &self.yoy_day_count
    }

    /// The calendar the year-on-year payment dates are adjusted on
    /// (`paymentCalendar()`).
    pub fn payment_calendar(&self) -> &Calendar {
        &self.payment_calendar
    }

    /// The convention both legs' payment dates are adjusted with
    /// (`paymentConvention()`).
    pub fn payment_convention(&self) -> BusinessDayConvention {
        self.payment_convention
    }

    /// The fixed leg (`fixedLeg()`).
    pub fn fixed_leg(&self) -> &Leg {
        &self.swap.legs()[0]
    }

    /// The year-on-year leg (`yoyLeg()`).
    pub fn yoy_leg(&self) -> &Leg {
        &self.swap.legs()[1]
    }

    /// The year-on-year coupons themselves, typed: the same objects
    /// [`yoy_leg`](Self::yoy_leg) holds type-erased, kept because `Rc` has no
    /// projection and this crate does not downcast (D3), so the C++
    /// `dynamic_pointer_cast<YoYInflationCoupon>` its test suite and its
    /// bootstrap helper both perform has no counterpart.
    pub fn yoy_coupons(&self) -> &[Shared<YoYInflationCoupon>] {
        &self.yoy_coupons
    }

    /// The fixed leg's NPV (`fixedLegNPV()`), priced on demand.
    ///
    /// The calculation is forced through *this* instrument rather than through
    /// the base: the two share one [`InstrumentBase`], so letting
    /// [`Swap::leg_npv`] drive it would mark the results calculated having run
    /// the base's own `fetch_results`, leaving the fair rate unrecovered.
    ///
    /// # Errors
    ///
    /// As [`Swap::leg_npv`].
    pub fn fixed_leg_npv(&mut self) -> QlResult<Real> {
        self.calculate()?;
        self.swap.leg_npv(0)
    }

    /// The year-on-year leg's NPV (`yoyLegNPV()`), priced on demand.
    ///
    /// # Errors
    ///
    /// As [`fixed_leg_npv`](Self::fixed_leg_npv).
    pub fn yoy_leg_npv(&mut self) -> QlResult<Real> {
        self.calculate()?;
        self.swap.leg_npv(1)
    }

    /// The fixed rate that would make this swap worth nothing (`fairRate()`,
    /// `cpp:134-138`), recovered from the NPV and the fixed leg's BPS.
    ///
    /// # Errors
    ///
    /// The rate must be available: a priced, non-expired swap whose engine
    /// returned the fixed leg's BPS.
    pub fn fair_rate(&mut self) -> QlResult<Rate> {
        self.calculate()?;
        let Some(value) = self.fair_rate else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The spread over the index that would make this swap worth nothing
    /// (`fairSpread()`, `cpp:140-144`), recovered from the NPV and the
    /// year-on-year leg's BPS.
    ///
    /// # Errors
    ///
    /// As [`fair_rate`](Self::fair_rate), off the second leg.
    pub fn fair_spread(&mut self) -> QlResult<Spread> {
        self.calculate()?;
        let Some(value) = self.fair_spread else {
            fail!("result not available");
        };
        Ok(value)
    }
}

impl Instrument for YearOnYearInflationSwap {
    fn base(&self) -> &InstrumentBase {
        self.swap.base()
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.swap.base_mut()
    }

    fn is_expired(&self) -> QlResult<bool> {
        self.swap.is_expired()
    }

    /// Zeroes the base's results and drops both fair values (`cpp:158-165`).
    fn setup_expired(&mut self) {
        self.swap.setup_expired();
        self.fair_rate = None;
        self.fair_spread = None;
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(arguments)
    }

    /// Reads the base's results back, then recovers the two fair values from
    /// them (`cpp:167-193`).
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)?;
        let Some(results) = (results as &dyn Any).downcast_ref::<SwapResults>() else {
            fail!("wrong result type");
        };
        let npv = results.instrument.value;
        self.fair_rate = recover(npv, results.leg_bps.first(), self.fixed_rate);
        self.fair_spread = recover(npv, results.leg_bps.get(1), self.spread);
        Ok(())
    }
}

/// `quoted - NPV/(BPS/1e-4)`, the C++ recovery (`cpp:186-190`), left unset when
/// the engine returned no NPV or no BPS for the leg.
fn recover(npv: Option<Real>, leg_bps: Option<&Real>, quoted: Real) -> Option<Real> {
    let (Some(npv), Some(&bps)) = (npv, leg_bps) else {
        return None;
    };
    (!bps.is_null()).then(|| quoted - npv / (bps / BASIS_POINT))
}
