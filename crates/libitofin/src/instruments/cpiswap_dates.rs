use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::time::{
    businessdayconvention::BusinessDayConvention as Bdc, calendar::Calendar, date::Date,
    period::Period, schedule::Schedule, timeunit::TimeUnit,
};

use crate::cashflows::cpicoupon::checked_cpi_offset;
use crate::cashflows::cpidates::{checked_cpi_adjust as adjust, checked_cpi_day as step};

pub(super) fn advance(
    cal: &Calendar,
    date: Date,
    period: Period,
    roll: Bdc,
    end_of_month: bool,
) -> QlResult<Date> {
    let length = period.length();
    let units = period.units();
    if length == 0 {
        return adjust(cal, date, roll);
    }
    let shifted = match units {
        TimeUnit::Days => {
            let direction = length.signum();
            let mut d = date;
            for _ in 0..length.unsigned_abs() {
                d = step(d, direction)?;
                while cal.is_holiday(d) {
                    d = step(d, direction)?;
                }
            }
            d
        }
        TimeUnit::Weeks | TimeUnit::Months | TimeUnit::Years => {
            let magnitude = length.checked_abs().ok_or_else(|| {
                crate::errors::QlError::new("date period overflow", file!(), line!())
            })?;
            checked_cpi_offset(date, Period::new(magnitude, units), length >= 0)?
        }
        _ => crate::fail!("unsupported date period unit"),
    };
    if end_of_month && matches!(units, TimeUnit::Months | TimeUnit::Years) {
        if roll == Bdc::Unadjusted && Date::is_end_of_month(date) {
            return Ok(Date::end_of_month(shifted));
        }
        if roll != Bdc::Unadjusted && date >= adjust(cal, Date::end_of_month(date), Bdc::Preceding)?
        {
            return adjust(cal, Date::end_of_month(shifted), Bdc::Preceding);
        }
    }
    adjust(cal, shifted, roll)
}

pub(super) fn schedule(schedule: &Schedule, roll: Bdc) -> QlResult<()> {
    let calendar = schedule.calendar();
    for date in schedule.dates() {
        adjust(calendar, *date, roll)?;
    }
    if schedule.has_tenor() && schedule.has_is_regular() {
        let tenor = schedule.tenor();
        crate::require!(tenor.length() > 0, "schedule tenor must be positive");
        if !schedule.is_regular_at(1) {
            advance(
                calendar,
                schedule.date(1),
                -tenor,
                schedule.business_day_convention(),
                false,
            )?;
        }
        if !schedule.is_regular_at(schedule.len() - 1) {
            advance(
                calendar,
                schedule.date(schedule.len() - 2),
                tenor,
                schedule.business_day_convention(),
                false,
            )?;
        }
    }
    Ok(())
}

pub(super) struct CheckedIborCoupon(pub(super) crate::shared::Shared<crate::cashflows::IborCoupon>);
impl CheckedIborCoupon {
    fn check(&self) -> QlResult<()> {
        use crate::indexes::{Index, InterestRateIndex};
        let index = self.0.ibor_index();
        let days = i32::try_from(index.fixing_days()).map_err(|_| {
            crate::errors::QlError::new("index fixing days overflow", file!(), line!())
        })?;
        let calendar = index.fixing_calendar();
        let fixing = advance(
            &calendar,
            self.0.accrual_start_date(),
            Period::new(-days, TimeUnit::Days),
            Bdc::Preceding,
            false,
        )?;
        let today = Index::settings(&**index).evaluation_date().ok_or_else(|| {
            crate::errors::QlError::new("evaluation date is unset", file!(), line!())
        })?;
        if fixing < today
            || (fixing == today
                && (Index::settings(&**index).enforces_todays_historic_fixings()
                    || index.has_historical_fixing(fixing)))
        {
            return Ok(());
        }
        let value = advance(
            &calendar,
            fixing,
            Period::new(days, TimeUnit::Days),
            Bdc::Following,
            false,
        )?;
        advance(
            &index.maturity_calendar(),
            value,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        )?;
        let natural = advance(
            &index.value_calendar(),
            fixing,
            Period::new(days, TimeUnit::Days),
            Bdc::Following,
            false,
        )?;
        let natural = advance(
            &index.maturity_calendar(),
            natural,
            Period::new(0, TimeUnit::Days),
            Bdc::Following,
            false,
        )?;
        advance(
            &index.maturity_calendar(),
            natural,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        )?;
        if Index::settings(&**index).using_at_par_coupons() {
            let next = advance(
                &calendar,
                self.0.accrual_end_date(),
                Period::new(-days, TimeUnit::Days),
                Bdc::Following,
                false,
            )?;
            advance(
                &calendar,
                next,
                Period::new(days, TimeUnit::Days),
                Bdc::Following,
                false,
            )?;
            step(value, 1)?;
        }
        Ok(())
    }
}
impl crate::patterns::observable::AsObservable for CheckedIborCoupon {
    fn observable(&self) -> &crate::patterns::observable::Observable {
        self.0.observable()
    }
}
impl crate::cashflows::Coupon for CheckedIborCoupon {
    fn coupon_base(&self) -> &crate::cashflows::CouponBase {
        self.0.coupon_base()
    }
    fn day_counter(&self) -> crate::time::daycounter::DayCounter {
        self.0.day_counter()
    }
    fn amount(&self) -> QlResult<f64> {
        self.check()?;
        self.0.amount()
    }
    fn rate(&self) -> QlResult<f64> {
        self.check()?;
        self.0.rate()
    }
    fn accrued_amount(&self, date: Date) -> QlResult<f64> {
        if date > self.0.accrual_start_date() && date <= self.0.coupon_base().payment_date() {
            self.check()?;
        }
        self.0.accrued_amount(date)
    }
}

#[cfg(test)]
#[path = "cpiswap_dates_tests.rs"]
mod tests;
