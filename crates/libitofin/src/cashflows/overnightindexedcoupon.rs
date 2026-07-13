//! The daily-compounding overnight coupon.
//!
//! Port of `ql/cashflows/overnightindexedcoupon.{hpp,cpp}`. An
//! [`OvernightIndexedCoupon`] is a [`FloatingRateCoupon`] on a concrete
//! [`OvernightIndex`] that pays the daily overnight fixings compounded over the
//! accrual period. Unlike [`IborCoupon`], whose caller attaches a pricer, this
//! coupon's constructor builds and installs its own
//! [`CompoundingOvernightIndexedCouponPricer`] (mirroring
//! `overnightindexedcoupon.cpp:181-193`, where the constructor switches on the
//! averaging method and calls `setPricer` itself). No caller ever attaches one.
//!
//! [`IborCoupon`]: super::iborcoupon::IborCoupon
//!
//! ## The rate-computation schedule
//!
//! The constructor builds the coupon's value/interest/fixing date schedule and
//! daily accrual fractions once, into a [`Shared`] [`OvernightSchedule`] held by
//! both the coupon (for its inspectors) and the pricer (for the compounding
//! loop). See [`OvernightSchedule::new`].
//!
//! ## The dispatch trap
//!
//! C++ overrides `amount()` and `accruedAmount()` and gets the compounded rate
//! back through the virtual `rate()`. The Rust port embeds a
//! [`FloatingRateCoupon`] rather than inheriting one, so a base method cannot
//! dispatch back down: [`amount`](Coupon::amount) and
//! [`accrued_amount`](Coupon::accrued_amount) are re-routed explicitly.
//! [`accrued_amount`](Coupon::accrued_amount) in particular reads
//! [`average_rate`](Self::average_rate) - the rate compounded only up to the
//! given date - not the full-period [`rate`](Coupon::rate).
//!
//! ## Divergences from QuantLib
//!
//! Only [`RateAveraging::Compound`](super::rateaveraging::RateAveraging::Compound),
//! the default, is ported; a simple-averaged coupon is refused at construction.
//! The constructor knobs `lookbackDays`, `lockoutDays`, `applyObservationShift`,
//! `telescopicValueDates`, `rateComputationStartDate`/`rateComputationEndDate`
//! and `roundingPrecision` are not ported at all - they are omitted from the
//! signature rather than accepted and ignored, so the schedule always follows
//! the default path (fixing days from the index, no lockout, no observation
//! shift). The `amount()` rounding those knobs would drive is likewise omitted.
//! `CappedFlooredOvernightIndexedCoupon`, `OvernightLeg` and the
//! arithmetic-averaging pricer are separate later tickets.

use super::coupon::{Coupon, CouponBase};
use super::couponpricer::FloatingRateCouponPricer;
use super::floatingratecoupon::FloatingRateCoupon;
use super::overnightindexedcouponpricer::{
    CompoundingOvernightIndexedCouponPricer, OvernightSchedule,
};
use super::rateaveraging::RateAveraging;
use crate::errors::QlResult;
use crate::indexes::iborindex::OvernightIndex;
use crate::indexes::index::Index;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Spread, Time};

/// A coupon paying the compounded daily overnight rate
/// (`ql/cashflows/overnightindexedcoupon.hpp`).
///
/// Built with [`new`](Self::new), which installs its own compounding pricer. Its
/// [`Coupon`] (and hence [`CashFlow`]) face delegates its dates to the embedded
/// [`FloatingRateCoupon`] and re-routes the rate-bearing methods through the
/// pricer.
///
/// [`CashFlow`]: crate::cashflow::CashFlow
pub struct OvernightIndexedCoupon {
    base: FloatingRateCoupon,
    overnight_index: Shared<OvernightIndex>,
    schedule: Shared<OvernightSchedule>,
    averaging_method: RateAveraging,
    compound_spread_daily: bool,
    pricer: SharedMut<CompoundingOvernightIndexedCouponPricer>,
}

impl OvernightIndexedCoupon {
    /// Builds an overnight coupon over `index`, installing its compounding
    /// pricer.
    ///
    /// Mirrors the C++ constructor: it composes a [`FloatingRateCoupon`] over the
    /// index (fixing days taken from the index, never in arrears), builds the
    /// rate-computation schedule, and - switching on `averaging_method` - builds
    /// and installs a [`CompoundingOvernightIndexedCouponPricer`]. A `None`
    /// `day_counter` defaults to the index's, as in the base; a null gearing is
    /// rejected there. Only [`RateAveraging::Compound`] is supported.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payment_date: Date,
        nominal: Real,
        start_date: Date,
        end_date: Date,
        index: Shared<OvernightIndex>,
        gearing: Real,
        spread: Spread,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        day_counter: Option<DayCounter>,
        averaging_method: RateAveraging,
        compound_spread_daily: bool,
        ex_coupon_date: Option<Date>,
    ) -> QlResult<OvernightIndexedCoupon> {
        require!(start_date < end_date, "startDate must be less than endDate");
        require!(
            payment_date >= end_date,
            "Payment date cannot be earlier than accrual end date"
        );
        require!(
            averaging_method == RateAveraging::Compound,
            "simple-averaged overnight coupon not ported: only compound averaging"
        );

        let schedule = shared(OvernightSchedule::new(&index, start_date, end_date)?);

        let base = FloatingRateCoupon::new(
            payment_date,
            nominal,
            start_date,
            end_date,
            None,
            index.clone(),
            gearing,
            spread,
            ref_period_start,
            ref_period_end,
            day_counter,
            false,
            ex_coupon_date,
            BusinessDayConvention::Preceding,
        )?;

        let pricer = shared_mut(CompoundingOvernightIndexedCouponPricer::new(
            index.clone(),
            schedule.clone(),
            gearing,
            spread,
            compound_spread_daily,
        ));
        base.set_pricer(pricer.clone() as SharedMut<dyn FloatingRateCouponPricer>);

        Ok(OvernightIndexedCoupon {
            base,
            overnight_index: index,
            schedule,
            averaging_method,
            compound_spread_daily,
            pricer,
        })
    }

    /// The concrete overnight index (`index()`).
    pub fn overnight_index(&self) -> &Shared<OvernightIndex> {
        &self.overnight_index
    }

    /// The fixing dates for the rates to be compounded (`fixingDates`).
    pub fn fixing_dates(&self) -> &[Date] {
        &self.schedule.fixing_dates
    }

    /// The value dates for the rates to be compounded (`valueDates`).
    pub fn value_dates(&self) -> &[Date] {
        &self.schedule.value_dates
    }

    /// The interest dates for the rates to be compounded (`interestDates`).
    pub fn interest_dates(&self) -> &[Date] {
        &self.schedule.interest_dates
    }

    /// The daily accrual (compounding) periods (`dt`).
    pub fn dt(&self) -> &[Time] {
        &self.schedule.dt
    }

    /// The averaging method (always [`RateAveraging::Compound`] here).
    pub fn averaging_method(&self) -> RateAveraging {
        self.averaging_method
    }

    /// Whether the spread is compounded daily or added after compounding
    /// (`compoundSpreadDaily`).
    pub fn compound_spread_daily(&self) -> bool {
        self.compound_spread_daily
    }

    /// The spread over the compounded index (`spread()`), delegating to the
    /// embedded [`FloatingRateCoupon`] whose private field would otherwise hide
    /// it. Read per coupon by the OIS `setupFloatingArguments` port.
    pub fn spread(&self) -> Spread {
        self.base.spread()
    }

    /// The date the coupon is fully determined: the last fixing date
    /// (`fixingDate`, overriding the base).
    pub fn fixing_date(&self) -> Date {
        *self
            .schedule
            .fixing_dates
            .last()
            .expect("an overnight schedule always has at least one fixing date")
    }

    /// The fixings to be compounded, one per fixing date (`indexFixings`): each
    /// read through the index's own decision tree (past, today's border case, or
    /// forecast).
    pub fn index_fixings(&self) -> QlResult<Vec<Rate>> {
        self.schedule
            .fixing_dates
            .iter()
            .map(|&date| self.overnight_index.fixing(date, false))
            .collect()
    }

    /// The spread reproducing the coupon amount as
    /// `gearing * effectiveIndexFixing + effectiveSpread` (`effectiveSpread`).
    pub fn effective_spread(&self) -> QlResult<Spread> {
        self.pricer.borrow().effective_spread()
    }

    /// The index fixing reproducing the coupon amount alongside
    /// [`effective_spread`](Self::effective_spread) (`effectiveIndexFixing`).
    pub fn effective_index_fixing(&self) -> QlResult<Rate> {
        self.pricer.borrow().effective_index_fixing()
    }

    /// The rate compounded only up to `date` (`averageRate`, private in C++):
    /// the rate that [`accrued_amount`](Coupon::accrued_amount) accrues at.
    fn average_rate(&self, date: Date) -> QlResult<Rate> {
        self.pricer.borrow().average_rate(date)
    }
}

impl AsObservable for OvernightIndexedCoupon {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl Coupon for OvernightIndexedCoupon {
    fn coupon_base(&self) -> &CouponBase {
        self.base.coupon_base()
    }

    fn amount(&self) -> QlResult<Real> {
        Ok(self.rate()? * self.accrual_period() * self.nominal())
    }

    fn rate(&self) -> QlResult<Rate> {
        self.base.rate()
    }

    fn day_counter(&self) -> DayCounter {
        self.base.day_counter()
    }

    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        if date <= self.accrual_start_date() || date > self.coupon_base().payment_date() {
            Ok(0.0)
        } else if self.trades_ex_coupon_on(date) {
            Ok(self.nominal() * self.average_rate(date)? * self.accrued_period(date))
        } else {
            let capped = date.min(self.accrual_end_date());
            Ok(self.nominal() * self.average_rate(capped)? * self.accrued_period(date))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Oracles from `test-suite/overnightindexedcoupon.cpp`, the six cases that
    //! build a plain compounding SOFR coupon: `testPastCouponRate`,
    //! `testPastSpreadedCouponRate`, `testCurrentCouponRate`,
    //! `testFutureCouponRate`, `testRateWhenTodayIsHoliday` and
    //! `testAccruedAmountInThePast`. The lookback/lockout/observation-shift,
    //! telescopic-value-date, Black cap/floor and `OvernightLeg` cases exercise
    //! surfaces this ticket does not port.
    //!
    //! The fixture pre-loads the C++ `CommonVars` history (the SOFR fixings at
    //! `overnightindexedcoupon.cpp:95-140`) on the default evaluation date of
    //! 23 November 2021.

    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::indexes::ibor::Sofr;
    use crate::indexes::index::Index;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::Month::{August, December, January, July, June, November, October};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    const NOTIONAL: Real = 10000.0;

    /// The C++ `CommonVars`: an evaluation date, a SOFR index over a relinkable
    /// forecast curve, and the pre-loaded fixing history. The handle is returned
    /// so a test can link a forecast curve after construction.
    fn common_vars(
        today: Date,
    ) -> (
        Shared<Settings<Date>>,
        RelinkableHandle<dyn YieldTermStructure>,
        Shared<OvernightIndex>,
    ) {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let curve: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let sofr = shared(Sofr::new(curve.handle(), settings.clone()));

        let past_dates = [
            Date::new(21, June, 2019),
            Date::new(24, June, 2019),
            Date::new(25, June, 2019),
            Date::new(26, June, 2019),
            Date::new(27, June, 2019),
            Date::new(28, June, 2019),
            Date::new(1, July, 2019),
            Date::new(2, July, 2019),
            Date::new(3, July, 2019),
            Date::new(5, July, 2019),
            Date::new(8, July, 2019),
            Date::new(9, July, 2019),
            Date::new(10, July, 2019),
            Date::new(11, July, 2019),
            Date::new(12, July, 2019),
            Date::new(15, July, 2019),
            Date::new(16, July, 2019),
            Date::new(17, July, 2019),
            Date::new(18, July, 2019),
            Date::new(19, July, 2019),
            Date::new(22, July, 2019),
            Date::new(23, July, 2019),
            Date::new(24, July, 2019),
            Date::new(25, July, 2019),
            Date::new(26, July, 2019),
            Date::new(29, July, 2019),
            Date::new(30, July, 2019),
            Date::new(31, July, 2019),
            Date::new(1, August, 2019),
            Date::new(2, August, 2019),
            Date::new(5, August, 2019),
            Date::new(18, October, 2021),
            Date::new(19, October, 2021),
            Date::new(20, October, 2021),
            Date::new(21, October, 2021),
            Date::new(22, October, 2021),
            Date::new(25, October, 2021),
            Date::new(26, October, 2021),
            Date::new(27, October, 2021),
            Date::new(28, October, 2021),
            Date::new(29, October, 2021),
            Date::new(1, November, 2021),
            Date::new(2, November, 2021),
            Date::new(3, November, 2021),
            Date::new(4, November, 2021),
            Date::new(5, November, 2021),
            Date::new(8, November, 2021),
            Date::new(9, November, 2021),
            Date::new(10, November, 2021),
            Date::new(12, November, 2021),
            Date::new(15, November, 2021),
            Date::new(16, November, 2021),
            Date::new(17, November, 2021),
            Date::new(18, November, 2021),
            Date::new(19, November, 2021),
            Date::new(22, November, 2021),
        ];
        let past_rates = [
            0.0237, 0.0239, 0.0241, 0.0243, 0.0242, 0.025, 0.0242, 0.0251, 0.0256, 0.0259, 0.0248,
            0.0245, 0.0246, 0.0241, 0.0236, 0.0246, 0.0247, 0.0247, 0.0246, 0.0241, 0.024, 0.024,
            0.0241, 0.0242, 0.0241, 0.024, 0.0239, 0.0255, 0.0219, 0.0219, 0.0213, 0.0008, 0.0009,
            0.0008, 0.0010, 0.0012, 0.0011, 0.0013, 0.0012, 0.0012, 0.0008, 0.0009, 0.0010, 0.0011,
            0.0014, 0.0013, 0.0011, 0.0009, 0.0008, 0.0007, 0.0008, 0.0008, 0.0007, 0.0009, 0.0010,
            0.0009,
        ];
        sofr.add_fixings(past_dates.into_iter().zip(past_rates))
            .unwrap();

        (settings, curve, sofr)
    }

    /// The C++ `flatRate(rate, Actual360())`: a flat continuously-compounded
    /// forward curve anchored at the evaluation date.
    fn flat_rate(reference: Date, rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    /// The C++ `CommonVars::makeCoupon`: a plain compounding SOFR coupon paid on
    /// its accrual end.
    fn make_coupon(sofr: Shared<OvernightIndex>, start: Date, end: Date) -> OvernightIndexedCoupon {
        OvernightIndexedCoupon::new(
            end,
            NOTIONAL,
            start,
            end,
            sofr,
            1.0,
            0.0,
            None,
            None,
            None,
            RateAveraging::Compound,
            false,
            None,
        )
        .unwrap()
    }

    /// The C++ `CommonVars::makeSpreadedCoupon`.
    fn make_spreaded_coupon(
        sofr: Shared<OvernightIndex>,
        start: Date,
        end: Date,
        spread: Spread,
        compound_spread_daily: bool,
    ) -> OvernightIndexedCoupon {
        OvernightIndexedCoupon::new(
            end,
            NOTIONAL,
            start,
            end,
            sofr,
            1.0,
            spread,
            None,
            None,
            None,
            RateAveraging::Compound,
            compound_spread_daily,
            None,
        )
        .unwrap()
    }

    /// `testPastCouponRate` (overnightindexedcoupon.cpp:339): a coupon entirely
    /// in the past compounds its recorded fixings to 0.000987136104, and its
    /// amount is `notional * rate * 31/360`.
    #[test]
    fn a_past_coupon_compounds_its_recorded_fixings() {
        let (_settings, _curve, sofr) = common_vars(Date::new(23, November, 2021));
        let coupon = make_coupon(
            sofr,
            Date::new(18, October, 2021),
            Date::new(18, November, 2021),
        );

        let expected_rate = 0.000987136104;
        assert!((coupon.rate().unwrap() - expected_rate).abs() < 1e-12);

        let expected_amount = NOTIONAL * expected_rate * 31.0 / 360.0;
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-8);
    }

    /// `testPastSpreadedCouponRate` (:356): the spread compounded daily gives
    /// 0.0010871445057780704; added after compounding, 0.0010871361040194164.
    #[test]
    fn a_past_spreaded_coupon_prices_both_spread_modes() {
        let (_settings, _curve, sofr) = common_vars(Date::new(23, November, 2021));

        let compounded_daily = make_spreaded_coupon(
            sofr.clone(),
            Date::new(18, October, 2021),
            Date::new(18, November, 2021),
            0.0001,
            true,
        );
        let expected_rate = 0.0010871445057780704;
        assert!((compounded_daily.rate().unwrap() - expected_rate).abs() < 1e-12);
        let expected_amount = NOTIONAL * expected_rate * 31.0 / 360.0;
        assert!((compounded_daily.amount().unwrap() - expected_amount).abs() < 1e-8);

        let added_after = make_spreaded_coupon(
            sofr,
            Date::new(18, October, 2021),
            Date::new(18, November, 2021),
            0.0001,
            false,
        );
        assert!((added_after.rate().unwrap() - 0.0010871361040194164).abs() < 1e-12);
    }

    /// `testCurrentCouponRate` (:379): a coupon spanning today forecasts today's
    /// missing fixing (0.000926701551), then reads it once recorded
    /// (0.000916700760). This is the D11 today-border fallthrough.
    #[test]
    fn a_current_coupon_forecasts_then_reads_todays_fixing() {
        let (settings, curve, sofr) = common_vars(Date::new(23, November, 2021));
        curve.link_to(flat_rate(settings.evaluation_date().unwrap(), 0.0010));

        let coupon = make_coupon(
            sofr.clone(),
            Date::new(10, November, 2021),
            Date::new(10, December, 2021),
        );

        let expected_rate = 0.000926701551;
        assert!((coupon.rate().unwrap() - expected_rate).abs() < 1e-12);
        let expected_amount = NOTIONAL * expected_rate * 30.0 / 360.0;
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-8);

        sofr.add_fixing(Date::new(23, November, 2021), 0.0007)
            .unwrap();

        let expected_rate = 0.000916700760;
        assert!((coupon.rate().unwrap() - expected_rate).abs() < 1e-12);
        let expected_amount = NOTIONAL * expected_rate * 30.0 / 360.0;
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-8);
    }

    /// `testFutureCouponRate` (:406): a coupon entirely in the future forecasts
    /// every fixing off the flat curve to 0.001000043057.
    #[test]
    fn a_future_coupon_forecasts_every_fixing() {
        let (settings, curve, sofr) = common_vars(Date::new(23, November, 2021));
        curve.link_to(flat_rate(settings.evaluation_date().unwrap(), 0.0010));

        let coupon = make_coupon(
            sofr,
            Date::new(10, December, 2021),
            Date::new(10, January, 2022),
        );

        let expected_rate = 0.001000043057;
        assert!((coupon.rate().unwrap() - expected_rate).abs() < 1e-12);
        let expected_amount = NOTIONAL * expected_rate * 31.0 / 360.0;
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-8);
    }

    /// `testRateWhenTodayIsHoliday` (:424): with the evaluation date on a
    /// weekend, the coupon spanning it prices to 0.000930035180.
    #[test]
    fn a_coupon_prices_when_today_is_a_holiday() {
        let (settings, curve, sofr) = common_vars(Date::new(23, November, 2021));
        settings.set_evaluation_date(Date::new(20, November, 2021));
        curve.link_to(flat_rate(Date::new(20, November, 2021), 0.0010));

        let coupon = make_coupon(
            sofr,
            Date::new(10, November, 2021),
            Date::new(10, December, 2021),
        );

        let expected_rate = 0.000930035180;
        assert!((coupon.rate().unwrap() - expected_rate).abs() < 1e-12);
        let expected_amount = NOTIONAL * expected_rate * 30.0 / 360.0;
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-8);
    }

    /// `testAccruedAmountInThePast` (:442): the accrued amount at an interior
    /// past date compounds only up to that date - here reproducing the
    /// 18-Oct-to-18-Nov past-coupon rate over `notional * rate * 31/360`.
    #[test]
    fn accrued_amount_in_the_past_compounds_up_to_the_date() {
        let (_settings, _curve, sofr) = common_vars(Date::new(23, November, 2021));
        let coupon = make_coupon(
            sofr,
            Date::new(18, October, 2021),
            Date::new(18, January, 2022),
        );

        let expected_amount = NOTIONAL * 0.000987136104 * 31.0 / 360.0;
        let accrued = coupon
            .accrued_amount(Date::new(18, November, 2021))
            .unwrap();
        assert!((accrued - expected_amount).abs() < 1e-8);
    }
}
