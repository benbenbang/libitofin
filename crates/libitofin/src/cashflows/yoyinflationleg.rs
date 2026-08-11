//! The year-on-year inflation leg builder.
//!
//! Port of the `yoyInflationLeg` half of `ql/cashflows/yoyinflationcoupon.hpp`
//! (`:97-132`) and `.cpp` (`:67-232`): a fluent builder turning a [`Schedule`]
//! plus notionals, fixing days, gearings and spreads into a sequence of
//! [`YoYInflationCoupon`]s over a [`YoYInflationIndex`], mirroring
//! [`IborLeg`](super::IborLeg). The first and last periods may be short or long,
//! in which case the coupon accrues against a reference period one tenor away
//! from the stub, so a schedule-aware day counter still sees a regular period
//! and the observation lag still runs off a full year-on-year window.
//!
//! The leg carries two calendars, which are not interchangeable: the payment
//! dates are rolled on the builder's own `payment_calendar` under
//! [`with_payment_adjustment`](YoYInflationLeg::with_payment_adjustment)
//! (`.cpp:176`), while the stub reference dates are rolled on the *schedule's*
//! calendar under the schedule's own convention (`.cpp:177-184`).
//!
//! ## Divergences from QuantLib
//!
//! C++ ends the builder with `operator Leg()`, returning one [`Leg`] that holds
//! a mix of plain and capped coupons and recovering either with a
//! `dynamic_pointer_cast`. The port has no downcast, so it splits the paths by
//! return type, as [`IborLeg`](super::IborLeg) does:
//! [`coupons`](YoYInflationLeg::coupons) keeps the plain
//! [`YoYInflationCoupon`]s,
//! [`capped_floored_coupons`](YoYInflationLeg::capped_floored_coupons) the
//! wrapped ones, and [`build`](YoYInflationLeg::build) erases whichever the cap
//! and floor lists select. A leg is therefore capped as a whole rather than
//! coupon by coupon; C++'s per-coupon mix would need a `Null<Rate>()` entry
//! inside the cap vector, which no test-suite caller writes.
//!
//! The stub reference date uses `calendar.advance_by_period(end, -tenor, bdc)`
//! where C++ writes `calendar.adjust(end - tenor, bdc)` (`.cpp:179`), as
//! [`IborLeg`](super::IborLeg) does. The two agree for a tenor in months or
//! years and differ only for one in days, which advances over business days.
//!
//! ## Deferred (visible)
//!
//! The zero-gearing collapse to a `FixedRateCoupon` (`.cpp:185-192`) is also
//! deferred, but by *accepting* the gearing rather than by rejecting it - the
//! opposite of [`IborLeg`](super::IborLeg), whose coupon refuses a zero gearing
//! outright. A zero-geared [`YoYInflationCoupon`] pays
//! `nominal * accrual * (0 * fixing + spread)`, which is what the C++
//! `FixedRateCoupon` pays at `effectiveFixedRate = spread` under simple
//! compounding, so the two agree on every amount the port can produce. They
//! differ in what they refuse: the C++ fixed coupon reads no index at all,
//! whereas the port's pricer still resolves the fixing and propagates a missing
//! one as an error.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::capflooredyoyinflationcoupon::CappedFlooredYoYInflationCoupon;
use crate::cashflows::yoyinflationcoupon::{
    SwapletYoYInflationCouponPricer, YoYInflationCoupon, YoYInflationCouponPricer,
};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real, Spread};
use crate::{fail, require};

/// Builds a sequence of [`YoYInflationCoupon`]s from a [`Schedule`].
#[must_use]
pub struct YoYInflationLeg {
    schedule: Schedule,
    payment_calendar: Calendar,
    yoy_index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    notionals: Vec<Real>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: BusinessDayConvention,
    fixing_days: Vec<Natural>,
    gearings: Vec<Real>,
    spreads: Vec<Spread>,
    caps: Vec<Rate>,
    floors: Vec<Rate>,
}

impl YoYInflationLeg {
    /// A leg over `schedule` paying `yoy_index` observed `observation_lag` back
    /// under `interpolation`, paying on `payment_calendar` with the
    /// `ModifiedFollowing` convention (`.cpp:67-73`, the default of
    /// `hpp:126`).
    pub fn new(
        schedule: Schedule,
        payment_calendar: Calendar,
        yoy_index: Shared<YoYInflationIndex>,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
    ) -> YoYInflationLeg {
        YoYInflationLeg {
            schedule,
            payment_calendar,
            yoy_index,
            observation_lag,
            interpolation,
            notionals: Vec::new(),
            payment_day_counter: None,
            payment_adjustment: BusinessDayConvention::ModifiedFollowing,
            fixing_days: Vec::new(),
            gearings: Vec::new(),
            spreads: Vec::new(),
            caps: Vec::new(),
            floors: Vec::new(),
        }
    }

    /// One notional for every coupon.
    pub fn with_notional(self, notional: Real) -> YoYInflationLeg {
        self.with_notionals(vec![notional])
    }

    /// A notional per coupon; the last one carries over to any coupon beyond the
    /// end of the list.
    pub fn with_notionals(mut self, notionals: Vec<Real>) -> YoYInflationLeg {
        self.notionals = notionals;
        self
    }

    /// The day counter the coupons accrue with. The leg has no default: a
    /// coupon cannot be built without one.
    pub fn with_payment_day_counter(mut self, day_counter: DayCounter) -> YoYInflationLeg {
        self.payment_day_counter = Some(day_counter);
        self
    }

    /// The convention the payment dates are adjusted with.
    pub fn with_payment_adjustment(mut self, convention: BusinessDayConvention) -> YoYInflationLeg {
        self.payment_adjustment = convention;
        self
    }

    /// One fixing-days count for every coupon.
    pub fn with_fixing_days(self, fixing_days: Natural) -> YoYInflationLeg {
        self.with_fixing_days_per_coupon(vec![fixing_days])
    }

    /// A fixing-days count per coupon; the last one carries over.
    pub fn with_fixing_days_per_coupon(mut self, fixing_days: Vec<Natural>) -> YoYInflationLeg {
        self.fixing_days = fixing_days;
        self
    }

    /// One gearing for every coupon.
    pub fn with_gearing(self, gearing: Real) -> YoYInflationLeg {
        self.with_gearings(vec![gearing])
    }

    /// A gearing per coupon; the last one carries over.
    pub fn with_gearings(mut self, gearings: Vec<Real>) -> YoYInflationLeg {
        self.gearings = gearings;
        self
    }

    /// One spread for every coupon.
    pub fn with_spread(self, spread: Spread) -> YoYInflationLeg {
        self.with_spreads(vec![spread])
    }

    /// A spread per coupon; the last one carries over.
    pub fn with_spreads(mut self, spreads: Vec<Spread>) -> YoYInflationLeg {
        self.spreads = spreads;
        self
    }

    /// One cap for every coupon (`withCaps`, `.cpp:125-128`).
    pub fn with_caps(self, cap: Rate) -> YoYInflationLeg {
        self.with_caps_per_coupon(vec![cap])
    }

    /// A cap per coupon; the last one carries over. An empty list, the default,
    /// leaves the coupons uncapped.
    ///
    /// Setting a cap or a floor makes [`build`](Self::build) and
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) produce
    /// [`CappedFlooredYoYInflationCoupon`]s, and withholds the default swaplet
    /// pricer (`.cpp:226-229`): a capped coupon needs one carrying an optionlet
    /// volatility, which the caller installs through
    /// [`set_yoy_coupon_pricer`].
    pub fn with_caps_per_coupon(mut self, caps: Vec<Rate>) -> YoYInflationLeg {
        self.caps = caps;
        self
    }

    /// One floor for every coupon (`withFloors`, `.cpp:135-138`).
    pub fn with_floors(self, floor: Rate) -> YoYInflationLeg {
        self.with_floors_per_coupon(vec![floor])
    }

    /// A floor per coupon; the last one carries over. An empty list, the
    /// default, leaves the coupons unfloored. See
    /// [`with_caps_per_coupon`](Self::with_caps_per_coupon).
    pub fn with_floors_per_coupon(mut self, floors: Vec<Rate>) -> YoYInflationLeg {
        self.floors = floors;
        self
    }

    /// The coupons the leg is made of, no pricer attached.
    fn raw_coupons(&self) -> QlResult<Vec<Shared<YoYInflationCoupon>>> {
        let Some(payment_day_counter) = &self.payment_day_counter else {
            fail!("no payment daycounter given");
        };
        require!(!self.notionals.is_empty(), "no notional given");
        let size = self.schedule.len();
        require!(size >= 2, "schedule with {size} date(s) spans no period");
        let periods = size - 1;
        require!(
            self.notionals.len() <= periods,
            "too many notionals ({}), only {periods} required",
            self.notionals.len()
        );
        require!(
            self.gearings.len() <= periods,
            "too many gearings ({}), only {periods} required",
            self.gearings.len()
        );
        require!(
            self.spreads.len() <= periods,
            "too many spreads ({}), only {periods} required",
            self.spreads.len()
        );
        require!(
            self.caps.len() <= periods,
            "too many caps ({}), only {periods} required",
            self.caps.len()
        );
        require!(
            self.floors.len() <= periods,
            "too many floors ({}), only {periods} required",
            self.floors.len()
        );

        let calendar = self.schedule.calendar();
        let convention = self.schedule.business_day_convention();
        let stub = |period: usize| {
            self.schedule.has_tenor()
                && self.schedule.has_is_regular()
                && !self.schedule.is_regular_at(period)
        };

        let mut coupons = Vec::with_capacity(periods);
        for i in 0..periods {
            let start = self.schedule.date(i);
            let end = self.schedule.date(i + 1);
            let mut reference_start = start;
            let mut reference_end = end;
            if i == 0 && stub(1) {
                reference_start =
                    calendar.advance_by_period(end, -self.schedule.tenor(), convention, false);
            }
            if i == periods - 1 && stub(i + 1) {
                reference_end =
                    calendar.advance_by_period(start, self.schedule.tenor(), convention, false);
            }
            let payment_date = self.payment_calendar.adjust(end, self.payment_adjustment);
            let coupon = YoYInflationCoupon::new(
                payment_date,
                broadcast(&self.notionals, i, 1.0),
                start,
                end,
                broadcast(&self.fixing_days, i, 0),
                Shared::clone(&self.yoy_index),
                self.observation_lag,
                self.interpolation,
                payment_day_counter.clone(),
                broadcast(&self.gearings, i, 1.0),
                broadcast(&self.spreads, i, 0.0),
                Some(reference_start),
                Some(reference_end),
            );
            coupons.push(shared(coupon));
        }
        Ok(coupons)
    }

    /// The plain coupons the leg is made of, each carrying the default
    /// [`SwapletYoYInflationCouponPricer`] (`.cpp:226-229`).
    ///
    /// That pricer holds no nominal curve, so the coupons answer
    /// [`rate`](crate::cashflows::Coupon::rate) and
    /// [`amount`](crate::cashflows::Coupon::amount) but refuse a
    /// [`swaplet_price`](YoYInflationCouponPricer::swaplet_price) until a
    /// discounting pricer replaces it.
    ///
    /// With a cap or floor set the coupons come back unpriced, the C++ guard
    /// withholding the default pricer;
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) is then the
    /// intended entry.
    ///
    /// # Errors
    ///
    /// Errors if no payment day counter or no notional was given, if the
    /// schedule holds fewer than two dates, or if more notionals, gearings,
    /// spreads, caps or floors were given than the schedule has periods.
    pub fn coupons(&self) -> QlResult<Vec<Shared<YoYInflationCoupon>>> {
        let coupons = self.raw_coupons()?;
        if self.caps.is_empty() && self.floors.is_empty() {
            for coupon in &coupons {
                coupon.set_pricer(default_pricer());
            }
        }
        Ok(coupons)
    }

    /// The coupons wrapped in their per-coupon cap and floor (`.cpp:207-222`),
    /// no pricer attached: the caller installs one carrying an optionlet
    /// volatility through [`set_yoy_coupon_pricer`].
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons), plus any
    /// [`CappedFlooredYoYInflationCoupon::new`] precondition (a cap below its
    /// floor).
    pub fn capped_floored_coupons(&self) -> QlResult<Vec<Shared<CappedFlooredYoYInflationCoupon>>> {
        let raw = self.raw_coupons()?;
        let mut coupons = Vec::with_capacity(raw.len());
        for (i, underlying) in raw.into_iter().enumerate() {
            let cap = pick(&self.caps, i);
            let floor = pick(&self.floors, i);
            coupons.push(shared(CappedFlooredYoYInflationCoupon::new(
                underlying, cap, floor,
            )?));
        }
        Ok(coupons)
    }

    /// The coupons as a [`Leg`], with their concrete type erased.
    ///
    /// The plain path erases [`coupons`](Self::coupons); with a cap or floor set
    /// it erases [`capped_floored_coupons`](Self::capped_floored_coupons), whose
    /// coupons carry no pricer, so the caller must install one on the concrete
    /// coupons through [`set_yoy_coupon_pricer`] before erasing rather than
    /// through this method.
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons) or
    /// [`capped_floored_coupons`](Self::capped_floored_coupons).
    pub fn build(&self) -> QlResult<Leg> {
        if self.caps.is_empty() && self.floors.is_empty() {
            Ok(self
                .coupons()?
                .into_iter()
                .map(|coupon| coupon as Shared<dyn CashFlow>)
                .collect())
        } else {
            Ok(self
                .capped_floored_coupons()?
                .into_iter()
                .map(|coupon| coupon as Shared<dyn CashFlow>)
                .collect())
        }
    }
}

/// A coupon a year-on-year inflation pricer can be attached to (a
/// [`YoYInflationCoupon`] or a [`CappedFlooredYoYInflationCoupon`]), so
/// [`set_yoy_coupon_pricer`] spans both.
pub trait AttachYoYInflationPricer {
    /// Attaches `pricer` to the coupon.
    fn attach_pricer(&self, pricer: SharedMut<dyn YoYInflationCouponPricer>);
}

impl AttachYoYInflationPricer for YoYInflationCoupon {
    fn attach_pricer(&self, pricer: SharedMut<dyn YoYInflationCouponPricer>) {
        self.set_pricer(pricer);
    }
}

impl AttachYoYInflationPricer for CappedFlooredYoYInflationCoupon {
    fn attach_pricer(&self, pricer: SharedMut<dyn YoYInflationCouponPricer>) {
        self.set_pricer(pricer);
    }
}

/// Attaches `pricer` to every coupon, overriding any default the builder set.
///
/// The free `setCouponPricer(Leg&, pricer)` of `couponpricer.cpp`, taking the
/// concrete coupons rather than an erased [`Leg`] since the port cannot downcast
/// a [`CashFlow`] back to a coupon. Generic over the coupon type so a plain or a
/// capped/floored leg can be priced the same way. The name carries the `yoy`
/// that C++ leaves to overloading: the crate re-exports its cash-flow items
/// flat, where [`set_coupon_pricer`](super::set_coupon_pricer) is already the
/// floating-rate one.
pub fn set_yoy_coupon_pricer<C: AttachYoYInflationPricer>(
    coupons: &[Shared<C>],
    pricer: SharedMut<dyn YoYInflationCouponPricer>,
) {
    for coupon in coupons {
        coupon.attach_pricer(pricer.clone());
    }
}

/// The `index`-th rate as a cap/floor, or `None` when the list is empty
/// (an uncapped or unfloored coupon).
fn pick(rates: &[Rate], index: usize) -> Option<Rate> {
    if rates.is_empty() {
        None
    } else {
        Some(broadcast(rates, index, 0.0))
    }
}

/// The default coupon pricer `operator Leg()` attaches (`.cpp:229`).
fn default_pricer() -> SharedMut<dyn YoYInflationCouponPricer> {
    shared_mut(SwapletYoYInflationCouponPricer::new()) as SharedMut<dyn YoYInflationCouponPricer>
}

/// The `index`-th value, the last one when the list is shorter, or `default`
/// when the list is empty (`detail::get`).
fn broadcast<T: Clone>(values: &[T], index: usize, default: T) -> T {
    match values.last() {
        None => default,
        Some(last) => values.get(index).unwrap_or(last).clone(),
    }
}

#[cfg(test)]
mod tests {
    //! The one literal QuantLib supplies for this builder is the fixing date of
    //! the first year-on-year swap coupon of `testYYTermStructure`
    //! (`inflation.cpp:1176-1178`), reached below without the swap or the curve
    //! that surround it there: the schedule the helper builds
    //! (`inflationhelpers.cpp:314-321`) is reconstructed directly.
    //!
    //! The amount oracle takes its shape from `makeYoYLeg`
    //! (`inflationcapflooredcoupon.cpp:198-220`) - an annual, forward-generated,
    //! unadjusted UK schedule paid `ModifiedFollowing` over `Thirty360` - but
    //! moves it onto published historical fixings. `makeYoYLeg` starts on its
    //! 13 August 2007 evaluation date and forecasts every fixing off the
    //! bootstrapped year-on-year curve, which lands with `Y3`; the amounts
    //! themselves are then checked against [`Cpi::lagged_yoy_rate`], whose
    //! numbers `testCpiYoY*Interpolation` pins.
    //!
    //! The stub schedule is the one `testPartialScheduleLegConstruction`
    //! (`cashflows.cpp`) hands the ibor leg, whose reference dates
    //! [`IborLeg`](super::super::IborLeg) already pins: the two builders run the
    //! same `FloatingLeg` stub arithmetic, so the same schedule must give the
    //! same reference periods.

    use super::*;
    use crate::cashflows::coupon::Coupon;
    use crate::currency::Currency;
    use crate::indexes::Region;
    use crate::indexes::index::Index;
    use crate::indexes::inflationindex::Cpi;
    use crate::settings::Settings;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Date;
    use crate::time::date::Month::{August, February, June, March, September};
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Rate;

    const NOTIONAL: Real = 1_000_000.0;

    fn uk() -> Calendar {
        UnitedKingdom::new(unitedkingdom::Market::Settlement)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    fn lag() -> Period {
        Period::new(2, TimeUnit::Months)
    }

    /// UK `YY_RPI` as of 10 February 2022, carrying whatever year-on-year rates
    /// `rates` publishes. Every date the coupons below read is behind that
    /// horizon, so nothing forecasts off the empty curve handle.
    fn published_index(rates: &[(Date, Rate)]) -> Shared<YoYInflationIndex> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, February, 2022));
        let index = shared(YoYInflationIndex::new(
            "YY_RPI".into(),
            Region::uk(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::gbp(),
            settings,
        ));
        for &(date, rate) in rates {
            index.add_fixing(date, rate).expect("publishing a rate");
        }
        index
    }

    fn leg(schedule: Schedule, index: Shared<YoYInflationIndex>) -> YoYInflationLeg {
        YoYInflationLeg::new(schedule, uk(), index, lag(), CpiInterpolationType::Flat)
            .with_notional(NOTIONAL)
            .with_payment_day_counter(day_counter())
    }

    /// `inflation.cpp:1176-1178`: the first coupon of the 13 August 2008
    /// year-on-year swap fixes on 13 June 2008. The swap helper builds its leg
    /// off a one-year, unadjusted, backward-generated UK schedule from the
    /// 13 August 2007 evaluation date to the quoted maturity
    /// (`inflationhelpers.cpp:314-321`), so the leg has a single regular
    /// coupon whose reference-period end is the maturity itself and whose
    /// fixing date is that end lagged two months.
    #[test]
    fn the_front_coupon_fixes_two_months_before_its_reference_period_end() {
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2007))
            .to(Date::new(13, August, 2008))
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();

        let coupons = leg(schedule.clone(), published_index(&[]))
            .coupons()
            .expect("the leg is fully specified");

        assert_eq!(coupons.len(), 1);
        assert_eq!(
            coupons[0].reference_period_end(),
            Date::new(13, August, 2008)
        );
        assert_eq!(coupons[0].fixing_date(), Date::new(13, June, 2008));

        let rolled = leg(schedule, published_index(&[]))
            .with_fixing_days(3)
            .coupons()
            .expect("the leg is fully specified");
        assert_eq!(rolled[0].fixing_date(), Date::new(10, June, 2008));
    }

    /// Five annual coupons off `makeYoYLeg`'s schedule shape, each paying
    /// `nominal * accrualPeriod * (gearing * laggedYoYRate + spread)`. The three
    /// broadcast shapes are exercised at once: notionals given in full, a
    /// two-element gearing list held over the last three coupons, and a scalar
    /// spread. A gearing that changes between the second and third coupon keeps
    /// a broken carry-over from passing.
    #[test]
    fn each_coupon_gears_and_spreads_its_lagged_rate() {
        let rates: Vec<(Date, Rate)> = (2016..=2020)
            .map(|year| {
                (
                    Date::new(1, June, year),
                    0.02 + 0.001 * f64::from(year - 2016),
                )
            })
            .collect();
        let index = published_index(&rates);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2020))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();

        let notionals = vec![1e6, 2e6, 3e6, 4e6, 5e6];
        let gearings = vec![1.5, 2.5];
        let spread = 0.0035;
        let coupons = leg(schedule, Shared::clone(&index))
            .with_notionals(notionals.clone())
            .with_gearings(gearings)
            .with_spread(spread)
            .coupons()
            .expect("the leg is fully specified");

        assert_eq!(coupons.len(), 5);
        let expected_gearings = [1.5, 2.5, 2.5, 2.5, 2.5];
        for (i, coupon) in coupons.iter().enumerate() {
            let fixing = Cpi::lagged_yoy_rate(
                &index,
                coupon.accrual_end_date(),
                lag(),
                CpiInterpolationType::Flat,
            )
            .expect("the observed month is published");
            assert!(
                (fixing - rates[i].1).abs() < 1e-12,
                "coupon {i} observed {fixing}"
            );

            let expected =
                notionals[i] * coupon.accrual_period() * (expected_gearings[i] * fixing + spread);
            let amount = Coupon::amount(&**coupon).expect("the observed month is published");
            assert!(
                (amount - expected).abs() < 1e-8,
                "coupon {i} paid {amount}, expected {expected}"
            );
        }

        assert_eq!(coupons[0].accrual_end_date(), Date::new(13, August, 2016));
        assert_eq!(
            coupons[0].coupon_base().payment_date(),
            Date::new(15, August, 2016)
        );
    }

    /// The `IborLeg` reference dates of `testPartialScheduleLegConstruction`:
    /// an irregular first period pulls the reference start back a tenor from
    /// the accrual end, an irregular last one pushes the reference end a tenor
    /// on from the accrual start, and a regular schedule leaves both alone.
    #[test]
    fn an_irregular_period_accrues_against_a_full_reference_period() {
        let index = published_index(&[]);
        let irregular = MakeSchedule::new()
            .from(Date::new(15, September, 2017))
            .to(Date::new(30, September, 2020))
            .with_next_to_last_date(Date::new(25, September, 2020))
            .with_frequency(Frequency::Semiannual)
            .backwards()
            .build();

        let coupons = leg(irregular, Shared::clone(&index))
            .coupons()
            .expect("the leg is fully specified");
        let last = coupons.last().expect("the schedule spans periods");
        assert_eq!(
            coupons[0].reference_period_start(),
            Date::new(25, March, 2017)
        );
        assert_eq!(
            coupons[0].reference_period_end(),
            Date::new(25, September, 2017)
        );
        assert_eq!(
            last.reference_period_start(),
            Date::new(25, September, 2020)
        );
        assert_eq!(last.reference_period_end(), Date::new(25, March, 2021));

        let regular = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2020))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();
        let coupons = leg(regular, index)
            .coupons()
            .expect("the leg is fully specified");
        for coupon in coupons {
            assert_eq!(coupon.reference_period_start(), coupon.accrual_start_date());
            assert_eq!(coupon.reference_period_end(), coupon.accrual_end_date());
        }
    }

    /// The builder attaches the swaplet pricer itself (`.cpp:228-229`), so a
    /// coupon rates without one being installed by hand. That pricer carries no
    /// nominal curve, so it rates but refuses to price.
    #[test]
    fn the_builder_attaches_a_rating_but_not_discounting_pricer() {
        let index = published_index(&[(Date::new(1, June, 2016), 0.02)]);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2016))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();

        let coupons = leg(schedule, index)
            .coupons()
            .expect("the leg is fully specified");
        let pricer = coupons[0].pricer().expect("the builder attached a pricer");
        Coupon::amount(&*coupons[0]).expect("a rate needs no curve");

        let err = pricer
            .borrow()
            .swaplet_price()
            .expect_err("prices need a nominal curve");
        assert!(
            err.message().contains("no nominal term structure provided"),
            "err was: {err}"
        );
    }

    /// A zero gearing is built as a plain coupon rather than collapsed to a
    /// `FixedRateCoupon` (`.cpp:185-192`), and pays what that collapse would
    /// have paid: `nominal * accrualPeriod * spread`.
    #[test]
    fn a_zero_gearing_pays_its_spread_alone() {
        let index = published_index(&[(Date::new(1, June, 2016), 0.02)]);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2016))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();
        let spread = 0.0035;

        let coupons = leg(schedule, index)
            .with_gearing(0.0)
            .with_spread(spread)
            .coupons()
            .expect("the leg is fully specified");

        let expected = NOTIONAL * coupons[0].accrual_period() * spread;
        let amount = Coupon::amount(&*coupons[0]).expect("the observed month is published");
        assert!((amount - expected).abs() < 1e-10, "amount was {amount}");
    }

    /// A cap withholds the default pricer (`.cpp:226-229`), so the plain
    /// coupons come back unpriced and the capped ones are the intended entry.
    /// The caps broadcast as the other lists do: two given, the second held over
    /// the remaining coupons.
    #[test]
    fn a_capped_leg_withholds_the_default_pricer_and_broadcasts_its_caps() {
        let index = published_index(&[]);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2018))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();
        let capped =
            || leg(schedule.clone(), Shared::clone(&index)).with_caps_per_coupon(vec![0.05, 0.06]);

        let plain = capped().coupons().expect("the leg is fully specified");
        assert_eq!(plain.len(), 3);
        for coupon in &plain {
            assert!(
                coupon.pricer().is_none(),
                "a capped leg withholds the pricer"
            );
        }

        let coupons = capped()
            .capped_floored_coupons()
            .expect("the leg is fully specified");
        assert_eq!(coupons.len(), 3);
        let expected_caps = [0.05, 0.06, 0.06];
        for (i, coupon) in coupons.iter().enumerate() {
            assert!(coupon.is_capped() && !coupon.is_floored());
            assert!((coupon.effective_cap() - expected_caps[i]).abs() < 1e-15);
        }
    }

    /// `set_yoy_coupon_pricer` installs across a capped leg, which is how the
    /// pricer the builder withheld arrives. The install lands on the underlying
    /// coupon, the one instance the wrapper's rate path reads.
    #[test]
    fn a_pricer_is_installed_across_a_capped_leg() {
        let index = published_index(&[]);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2017))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();

        let coupons = leg(schedule, index)
            .with_floors(0.01)
            .capped_floored_coupons()
            .expect("the leg is fully specified");
        assert_eq!(coupons.len(), 2);
        for coupon in &coupons {
            assert!(coupon.is_floored() && !coupon.is_capped());
            assert!(coupon.underlying().pricer().is_none());
        }

        let pricer = shared_mut(SwapletYoYInflationCouponPricer::new())
            as SharedMut<dyn YoYInflationCouponPricer>;
        set_yoy_coupon_pricer(&coupons, pricer.clone());

        for coupon in &coupons {
            let installed = coupon
                .underlying()
                .pricer()
                .expect("a pricer was installed");
            assert!(SharedMut::ptr_eq(&installed, &pricer));
        }
    }

    #[test]
    fn an_underspecified_or_oversized_leg_is_an_error() {
        let index = published_index(&[]);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, August, 2015))
            .to(Date::new(13, August, 2020))
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();
        let bare = || {
            YoYInflationLeg::new(
                schedule.clone(),
                uk(),
                Shared::clone(&index),
                lag(),
                CpiInterpolationType::Flat,
            )
        };

        let Err(no_day_counter) = bare().with_notional(NOTIONAL).coupons() else {
            panic!("a coupon needs a day counter");
        };
        assert!(
            no_day_counter.message().contains("no payment daycounter"),
            "err was: {no_day_counter}"
        );

        let Err(no_notional) = bare().with_payment_day_counter(day_counter()).coupons() else {
            panic!("a coupon needs a notional");
        };
        assert!(
            no_notional.message().contains("no notional given"),
            "err was: {no_notional}"
        );

        let Err(too_many) = bare()
            .with_payment_day_counter(day_counter())
            .with_notional(NOTIONAL)
            .with_gearings(vec![1.0; 6])
            .coupons()
        else {
            panic!("the schedule has five periods");
        };
        assert!(
            too_many.message().contains("too many gearings (6), only 5"),
            "err was: {too_many}"
        );

        let Err(too_many_caps) = bare()
            .with_payment_day_counter(day_counter())
            .with_notional(NOTIONAL)
            .with_caps_per_coupon(vec![0.05; 6])
            .capped_floored_coupons()
        else {
            panic!("the schedule has five periods");
        };
        assert!(
            too_many_caps
                .message()
                .contains("too many caps (6), only 5"),
            "err was: {too_many_caps}"
        );
    }
}
