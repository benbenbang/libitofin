//! Calendar-day weighted BMA coupons and legs, matching QuantLib's average BMA coupon.
use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{Coupon, CouponBase};
use crate::errors::QlResult;
use crate::indexes::{BMAIndex, Index, InterestRateIndex};
use crate::patterns::observable::{AsObservable, Observable, ResetThenNotify};
use crate::shared::{Shared, SharedMut, shared};
use crate::time::{
    businessdayconvention::BusinessDayConvention as Bdc, date::Date, daycounter::DayCounter,
    schedule::Schedule, timeunit::TimeUnit,
};

/// Coupon paying the calendar-day weighted mean of weekly BMA fixings.
pub struct AverageBMACoupon {
    base: CouponBase,
    index: Shared<BMAIndex>,
    gearing: f64,
    spread: f64,
    day_counter: DayCounter,
    fixing_dates: Vec<Date>,
    observable: Shared<Observable>,
    _forwarder: SharedMut<ResetThenNotify>,
}
impl AverageBMACoupon {
    /// Builds a coupon and its bracketing fixing dates.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payment: Date,
        nominal: f64,
        start: Date,
        end: Date,
        index: Shared<BMAIndex>,
        gearing: f64,
        spread: f64,
        ref_start: Option<Date>,
        ref_end: Option<Date>,
        day_counter: DayCounter,
    ) -> QlResult<Self> {
        crate::require!(
            start < end && payment >= Date::min_date() && payment <= Date::max_date(),
            "invalid BMA coupon dates"
        );
        crate::require!(
            nominal.is_finite() && gearing.is_finite() && spread.is_finite(),
            "BMA coupon inputs must be finite"
        );
        crate::require!(
            start >= Date::min_date() + 14 && end <= Date::max_date() - 14,
            "BMA coupon dates outside supported range"
        );
        let mut fixing_start =
            index
                .fixing_calendar()
                .advance(start, -1, TimeUnit::Days, Bdc::Preceding, false);
        while !index.is_valid_fixing_date(fixing_start) || index.value_date(fixing_start)? > start {
            fixing_start -= 1;
        }
        let fixing_dates = index.fixing_schedule(fixing_start, end)?.dates().to_vec();
        let (observable, forwarder) = ResetThenNotify::forwarder();
        index.observable().register_observer(
            &(forwarder.clone() as SharedMut<dyn crate::patterns::observable::Observer>),
        );
        Ok(Self {
            base: CouponBase::new(payment, nominal, start, end, ref_start, ref_end, None),
            index,
            gearing,
            spread,
            day_counter,
            fixing_dates,
            observable,
            _forwarder: forwarder,
        })
    }
    /// Dates delimiting the fixing intervals, including the terminal boundary.
    pub fn fixing_dates(&self) -> &[Date] {
        &self.fixing_dates
    }
    /// Fixings at all schedule boundaries.
    pub fn index_fixings(&self) -> QlResult<Vec<f64>> {
        self.fixing_dates
            .iter()
            .map(|&d| self.index.fixing(d, false))
            .collect()
    }
    /// Underlying index retained by this coupon.
    pub fn index(&self) -> &Shared<BMAIndex> {
        &self.index
    }
    /// Multiplicative rate factor.
    pub fn gearing(&self) -> f64 {
        self.gearing
    }
    /// Additive spread.
    pub fn spread(&self) -> f64 {
        self.spread
    }
}
/// Stateless BMA arithmetic-average pricer.
pub struct AverageBMACouponPricer;
impl AverageBMACouponPricer {
    /// Computes the rate, propagating missing fixing and curve errors.
    pub fn swaplet_rate(coupon: &AverageBMACoupon) -> QlResult<f64> {
        let start = coupon.accrual_start_date();
        let end = coupon.accrual_end_date();
        let mut weighted = 0.0;
        let mut covered = 0;
        for dates in coupon.fixing_dates.windows(2) {
            let from = coupon.index.value_date(dates[0])?.max(start);
            let to = coupon.index.value_date(dates[1])?.min(end);
            if to > from {
                let days = to - from;
                weighted += coupon.index.fixing(dates[0], false)? * f64::from(days);
                covered += days;
            }
        }
        crate::require!(
            covered == end - start,
            "BMA fixing dates do not cover accrual period"
        );
        finite_coupon(coupon.gearing * weighted / f64::from(end - start) + coupon.spread)
    }
}
impl AsObservable for AverageBMACoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}
impl Coupon for AverageBMACoupon {
    fn coupon_base(&self) -> &CouponBase {
        &self.base
    }
    fn amount(&self) -> QlResult<f64> {
        finite_coupon(self.nominal() * self.accrual_period() * self.rate()?)
    }
    fn rate(&self) -> QlResult<f64> {
        AverageBMACouponPricer::swaplet_rate(self)
    }
    fn day_counter(&self) -> DayCounter {
        self.day_counter.clone()
    }
    fn accrued_amount(&self, date: Date) -> QlResult<f64> {
        if date <= self.accrual_start_date() || date > self.base.payment_date() {
            Ok(0.0)
        } else {
            finite_coupon(self.nominal() * self.rate()? * self.accrued_period(date))
        }
    }
}
/// Builds average-BMA coupons on a payment schedule.
pub struct AverageBMALeg {
    schedule: Schedule,
    index: Shared<BMAIndex>,
    notional: f64,
    day_counter: DayCounter,
    convention: Bdc,
}
impl AverageBMALeg {
    /// Creates a unit-notional leg using the index day counter.
    pub fn new(schedule: Schedule, index: Shared<BMAIndex>) -> Self {
        Self {
            day_counter: index.day_counter().clone(),
            schedule,
            index,
            notional: 1.0,
            convention: Bdc::Following,
        }
    }
    /// Sets the leg notional.
    pub fn with_notional(mut self, value: f64) -> Self {
        self.notional = value;
        self
    }
    /// Sets the coupon day counter.
    pub fn with_payment_day_counter(mut self, value: DayCounter) -> Self {
        self.day_counter = value;
        self
    }
    /// Sets the payment-date adjustment.
    pub fn with_payment_adjustment(mut self, value: Bdc) -> Self {
        self.convention = value;
        self
    }
    /// Builds the retained coupon cash flows, including irregular reference periods.
    pub fn build(&self) -> QlResult<Leg> {
        crate::require!(
            self.schedule.len() >= 2,
            "BMA schedule requires at least two dates"
        );
        let n = self.schedule.len() - 1;
        self.schedule
            .dates()
            .windows(2)
            .enumerate()
            .map(|(i, dates)| {
                let (start, end) = (dates[0], dates[1]);
                let cal = self.schedule.calendar();
                let mut ref_start = start;
                let mut ref_end = end;
                if self.schedule.has_is_regular()
                    && self.schedule.has_tenor()
                    && !self.schedule.is_regular_at(i + 1)
                {
                    if i == 0 {
                        ref_start = cal.adjust(end - self.schedule.tenor(), self.convention);
                    }
                    if i == n - 1 {
                        ref_end = cal.adjust(start + self.schedule.tenor(), self.convention);
                    }
                }
                Ok(shared(AverageBMACoupon::new(
                    cal.adjust(end, self.convention),
                    self.notional,
                    start,
                    end,
                    self.index.clone(),
                    1.0,
                    0.0,
                    Some(ref_start),
                    Some(ref_end),
                    self.day_counter.clone(),
                )?) as Shared<dyn CashFlow>)
            })
            .collect()
    }
}

fn finite_coupon(value: f64) -> QlResult<f64> {
    crate::require!(value.is_finite(), "non-finite BMA coupon result");
    Ok(value)
}
