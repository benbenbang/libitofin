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
