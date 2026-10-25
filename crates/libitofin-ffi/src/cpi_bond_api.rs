//! Explicit-base CPI bonds through the shared bond valuation interface.
use crate::bond_api::{NativeBond, optional_date};
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::inflation_api::{cpi_interpolation, zero_index};
use crate::rates_api::{finite, settings};
use crate::time_api::{adjust, advance, convention, day_counter, schedule_calendar, time_unit};
use libitofin::instruments::CpiBond;
use libitofin::time::{period::Period, schedule::Schedule};

/// Modern CPI bond configuration. Principal pays the full CPI ratio.
#[repr(C)]
pub struct ItofinCpiBondConfig {
    pub settlement_days: u32,
    pub face_amount: f64,
    pub base_cpi: f64,
    pub observation_lag_length: i32,
    pub observation_lag_unit: i32,
    pub index: u64,
    pub observation_interpolation: i32,
    pub schedule: u64,
    pub day_counter: u64,
    pub settings: u64,
    pub payment_convention: i32,
    /// Zero selects the schedule calendar.
    pub payment_calendar: u64,
    /// Zero means no issue date.
    pub issue_date: i32,
    pub ex_coupon_calendar: u64,
    pub ex_coupon_length: i32,
    pub ex_coupon_unit: i32,
    pub ex_coupon_convention: i32,
    pub has_ex_coupon: u8,
    pub ex_coupon_end_of_month: u8,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_cpi_bond_new(
    ctx: *mut Context,
    a: ItofinCpiBondConfig,
    coupons: *const f64,
    count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            finite(a.face_amount)?;
            finite(a.base_cpi)?;
            if a.has_ex_coupon > 1 || a.ex_coupon_end_of_month > 1 {
                return Err(BindingError::invalid("invalid ex-coupon flag"));
            }
            let rates = input_slice(coupons, count)?.to_vec();
            let schedule = c.get::<Schedule>(a.schedule)?;
            let calendar = if a.payment_calendar == 0 {
                schedule_calendar(c, a.schedule)?
            } else {
                c.get::<NativeCalendar>(a.payment_calendar)?
            };
            let rule = convention(a.payment_convention)?;
            crate::cpi_cashflows_api::check_leg_dates(
                &schedule,
                &schedule_calendar(c, a.schedule)?,
                &calendar,
                rule,
            )?;
            let ex_rule = convention(a.ex_coupon_convention)?;
            let ex_calendar = if a.has_ex_coupon == 1 {
                c.get::<NativeCalendar>(a.ex_coupon_calendar)?
            } else {
                calendar.clone()
            };
            let ex_period = if a.has_ex_coupon == 1 {
                if a.ex_coupon_length < 0 {
                    return Err(BindingError::invalid(
                        "ex-coupon period must be nonnegative",
                    ));
                }
                Some(Period::new(
                    a.ex_coupon_length,
                    time_unit(a.ex_coupon_unit)?,
                ))
            } else {
                None
            };
            for &date in schedule.dates().iter().skip(1) {
                let payment = adjust(&calendar, date, rule)?;
                if let Some(period) = ex_period {
                    advance(
                        &ex_calendar,
                        payment,
                        -period.length(),
                        period.units(),
                        ex_rule,
                        a.ex_coupon_end_of_month == 1,
                    )?;
                }
            }
            let bond = CpiBond::new(
                a.settlement_days,
                a.face_amount,
                a.base_cpi,
                Period::new(a.observation_lag_length, time_unit(a.observation_lag_unit)?),
                zero_index(c, a.index)?,
                cpi_interpolation(a.observation_interpolation)?,
                schedule,
                rates,
                day_counter(c, a.day_counter)?,
                rule,
                optional_date(a.issue_date)?,
                Some(calendar.inner.clone()),
                ex_period,
                ex_calendar.inner,
                ex_rule,
                a.ex_coupon_end_of_month == 1,
                settings(c, a.settings)?,
            )?;
            let mut native = NativeBond::new_unpriced(bond.into_bond(), calendar, None);
            native.indexed = true;
            output(out, c.insert(native)?)
        })
    }
}
