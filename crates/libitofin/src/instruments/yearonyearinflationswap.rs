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
#[cfg(test)]
mod tests {
    //! `test-suite/inflation.cpp`'s year-on-year swap oracle (`testYYTermStructure`,
    //! `:1109`) prices off a *bootstrapped* curve through the swap helper, and
    //! is ported with that helper. These tests price the same instrument on a
    //! **flat, directly built** [`YoYInflationCurve`] instead, so they are
    //! self-contained and every expected number is exact.
    //!
    //! The fixture is degenerate on purpose: the two legs share a schedule, a
    //! day counter and a payment convention, and the curve is flat, so a swap
    //! struck at the curve's rate pays identical amounts on identical dates on
    //! both legs and is worth nothing whatever the discount curve. That makes
    //! the at-market case blind to the payer mapping and to the discounting;
    //! the off-market case below is what pins the signs.

    use super::*;
    use crate::currency::Currency;
    use crate::handle::Handle;
    use crate::indexes::Region;
    use crate::indexes::index::Index;
    use crate::math::interpolations::linear::Linear;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use crate::termstructures::inflation::interpolatedyoyinflationcurve::YoYInflationCurve;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Month::{August, July};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;

    const NOMINAL: Real = 1_000_000.0;
    /// The flat year-on-year rate the curve publishes everywhere.
    const CURVE_RATE: Rate = 0.03;
    /// A strike a full per cent above the curve, so the two legs cannot cancel.
    const OFF_MARKET_RATE: Rate = 0.04;
    const NOMINAL_RATE: Rate = 0.05;

    fn today() -> Date {
        Date::new(13, August, 2007)
    }

    fn maturity() -> Date {
        Date::new(13, August, 2012)
    }

    fn uk() -> Calendar {
        UnitedKingdom::new(unitedkingdom::Market::Settlement)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    fn lag() -> Period {
        Period::new(2, TimeUnit::Months)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// A quoted UK year-on-year index with no history, linked to a curve flat at
    /// [`CURVE_RATE`] over the whole span the coupons observe: every fixing date
    /// is beyond the publication horizon, so every coupon forecasts and every
    /// forecast answers the same rate.
    fn an_index(settings: &Shared<Settings<Date>>) -> Shared<YoYInflationIndex> {
        let curve = shared(
            YoYInflationCurve::new(
                today(),
                vec![Date::new(1, July, 2007), Date::new(1, July, 2015)],
                vec![CURVE_RATE, CURVE_RATE],
                Frequency::Monthly,
                Actual360::new(),
                Linear,
                None,
            )
            .expect("a well-formed year-on-year curve"),
        );
        shared(
            YoYInflationIndex::new(
                "YY_RPI".into(),
                Region::uk(),
                false,
                Frequency::Monthly,
                Period::new(1, TimeUnit::Months),
                Currency::gbp(),
                Shared::clone(settings),
            )
            .with_term_structure(Handle::new(curve as Shared<dyn YoYInflationTermStructure>)),
        )
    }

    /// A flat 5 % continuously-compounded nominal curve anchored at the
    /// evaluation date.
    fn a_discount_engine(settings: Shared<Settings<Date>>) -> SharedMut<dyn PricingEngine> {
        let curve = shared(FlatForward::with_rate(
            today(),
            NOMINAL_RATE,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;
        shared_mut(DiscountingSwapEngine::new(
            Handle::new(curve),
            None,
            None,
            None,
            settings,
        )) as SharedMut<dyn PricingEngine>
    }

    /// Five annual coupons a side, on the schedule shape the bootstrap helper
    /// builds (`inflationhelpers.cpp:314-321`): unadjusted, backward-generated,
    /// UK, one year.
    fn a_swap(swap_type: SwapType, fixed_rate: Rate) -> YearOnYearInflationSwap {
        let settings = settings_today();
        let schedule = MakeSchedule::new()
            .from(today())
            .to(maturity())
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_calendar(uk())
            .backwards()
            .build();
        let mut swap = YearOnYearInflationSwap::new(
            swap_type,
            NOMINAL,
            schedule.clone(),
            fixed_rate,
            day_counter(),
            schedule,
            an_index(&settings),
            lag(),
            CpiInterpolationType::Flat,
            0.0,
            day_counter(),
            uk(),
            BusinessDayConvention::ModifiedFollowing,
            Shared::clone(&settings),
        )
        .expect("both legs are fully specified");
        swap.base_mut()
            .set_pricing_engine(a_discount_engine(settings));
        swap
    }

    /// Two coupon legs of five coupons each, both over the same schedule, and
    /// every year-on-year coupon forecasting the curve's flat rate.
    #[test]
    fn the_two_legs_run_over_the_same_five_periods() {
        let swap = a_swap(SwapType::Payer, CURVE_RATE);

        assert_eq!(swap.swap().number_of_legs(), 2);
        assert_eq!(swap.fixed_leg().len(), 5);
        assert_eq!(swap.yoy_leg().len(), 5);
        assert_eq!(swap.yoy_coupons().len(), 5);
        for (fixed, yoy) in swap.fixed_leg().iter().zip(swap.yoy_leg()) {
            assert_eq!(fixed.date(), yoy.date());
        }
        for coupon in swap.yoy_coupons() {
            assert!((coupon.index_fixing().unwrap() - CURVE_RATE).abs() < 1e-14);
        }
    }

    /// Struck at the curve's own rate the legs cancel coupon by coupon: the
    /// fixed one pays `N * K * tau` under `Simple` compounding and the
    /// year-on-year one `N * tau * (rate + spread)` on the same `tau` and the
    /// same date, with `K = rate` and no spread.
    ///
    /// This says nothing about the payer mapping or the discounting - both
    /// cancel out of a zero - and it is
    /// [`an_off_market_payer_pays_the_fixed_leg`] that pins those.
    #[test]
    fn an_at_market_swap_is_worth_nothing_and_is_fair_at_the_curve_rate() {
        let mut swap = a_swap(SwapType::Payer, CURVE_RATE);

        assert!(swap.npv().unwrap().abs() < 1e-8);
        assert!((swap.fair_rate().unwrap() - CURVE_RATE).abs() < 1e-8);
        assert!((swap.fair_spread().unwrap() - 0.0).abs() < 1e-8);
    }

    /// The payer-sign pin (`cpp:71-74`: `payer_[0] = -1` for a `Payer`, the
    /// opposite of the zero-coupon swap's mapping).
    ///
    /// A `Payer` pays the fixed leg, so struck a per cent above the curve it is
    /// out of the money: the fixed leg's NPV is negative, the year-on-year
    /// leg's positive, and the swap's negative. Under the zero-coupon swap's
    /// `vec![false, true]` every one of those three signs flips while the fair
    /// rate below stays put, which is why the fair rate alone cannot see the
    /// mapping.
    ///
    /// The leg NPVs are read *before* the fair rate on purpose: both go through
    /// the same cached calculation, and reading a leg first is what would leave
    /// the fair rate unrecovered if the calculation were driven off the base
    /// rather than off this instrument.
    #[test]
    fn an_off_market_payer_pays_the_fixed_leg() {
        let mut swap = a_swap(SwapType::Payer, OFF_MARKET_RATE);

        let fixed = swap.fixed_leg_npv().unwrap();
        let yoy = swap.yoy_leg_npv().unwrap();
        assert!(fixed < 0.0, "a payer pays the fixed leg: {fixed}");
        assert!(yoy > 0.0, "a payer receives the year-on-year leg: {yoy}");
        assert!(swap.npv().unwrap() < 0.0);
        assert!(swap.swap().payer(0).unwrap());
        assert!(!swap.swap().payer(1).unwrap());

        assert!((swap.fair_rate().unwrap() - CURVE_RATE).abs() < 1e-8);
    }

    /// The back-solve reaches the same fair rate from either side of the trade,
    /// the engine having signed both the NPV and the BPS, and the receiver's
    /// NPV is the payer's negated.
    #[test]
    fn a_receiver_swap_negates_the_payer_npv_at_the_same_fair_rate() {
        let mut payer = a_swap(SwapType::Payer, OFF_MARKET_RATE);
        let mut receiver = a_swap(SwapType::Receiver, OFF_MARKET_RATE);

        assert!((receiver.npv().unwrap() + payer.npv().unwrap()).abs() < 1e-8);
        assert!(receiver.npv().unwrap() > 0.0);
        assert!((receiver.fair_rate().unwrap() - payer.fair_rate().unwrap()).abs() < 1e-12);
    }

    /// The inspectors report what the swap was built with.
    #[test]
    fn the_inspectors_report_the_contract_terms() {
        let swap = a_swap(SwapType::Payer, OFF_MARKET_RATE);

        assert_eq!(swap.swap_type(), SwapType::Payer);
        assert_eq!(swap.nominal(), NOMINAL);
        assert_eq!(swap.fixed_rate(), OFF_MARKET_RATE);
        assert_eq!(swap.observation_lag(), lag());
        assert_eq!(swap.interpolation(), CpiInterpolationType::Flat);
        assert_eq!(swap.spread(), 0.0);
        assert_eq!(swap.payment_calendar().name(), uk().name());
        assert_eq!(
            swap.payment_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert_eq!(swap.fixed_schedule().dates(), swap.yoy_schedule().dates());
        assert_eq!(swap.fixed_day_count().name(), day_counter().name());
        assert_eq!(swap.yoy_day_count().name(), day_counter().name());
        assert_eq!(swap.yoy_inflation_index().name(), "UK YY_RPI");
    }

    /// A swap with no engine cannot answer either fair value.
    #[test]
    fn the_fair_values_need_a_priced_swap() {
        let settings = settings_today();
        let schedule = MakeSchedule::new()
            .from(today())
            .to(maturity())
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_calendar(uk())
            .backwards()
            .build();
        let mut swap = YearOnYearInflationSwap::new(
            SwapType::Payer,
            NOMINAL,
            schedule.clone(),
            CURVE_RATE,
            day_counter(),
            schedule,
            an_index(&settings),
            lag(),
            CpiInterpolationType::Flat,
            0.0,
            day_counter(),
            uk(),
            BusinessDayConvention::ModifiedFollowing,
            settings,
        )
        .expect("both legs are fully specified");

        assert!(swap.fair_rate().is_err());
        assert!(swap.fair_spread().is_err());
    }
}
