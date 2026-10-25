//! Plain explicit-base CPI cash-flow and leg adapters.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::inflation_api::{bool_value, cpi_interpolation, zero_index};
use crate::rates_api::{curve, finite, period};
use crate::time_api::{convention, date, day_counter, schedule_calendar};
use libitofin::cashflow::Leg;
use libitofin::cashflows::{Coupon, CpiCoupon, CpiCouponPricer, CpiLeg};
use libitofin::handle::Handle;
use libitofin::indexes::inflationindex::InflationIndex;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc, date::Date, schedule::Schedule,
    timeunit::TimeUnit,
};

#[derive(Clone)]
pub(crate) struct CpiLegSettings(pub(crate) Shared<Settings<Date>>);
#[derive(Clone)]
struct NativeCpiLeg {
    leg: Leg,
    settings: CpiLegSettings,
}

pub(crate) fn check_leg_dates(
    schedule: &Schedule,
    schedule_calendar: &NativeCalendar,
    payment_calendar: &NativeCalendar,
    adjustment: Bdc,
) -> BindingResult<()> {
    for &date in schedule.dates() {
        crate::floating_bond_api::advance(
            payment_calendar,
            date,
            0,
            TimeUnit::Days,
            adjustment,
            false,
        )?;
    }
    if schedule.len() >= 2 && schedule.has_tenor() && schedule.has_is_regular() {
        let tenor = schedule.tenor();
        let n = schedule.len() - 1;
        let arithmetic = NativeCalendar {
            inner: libitofin::time::calendars::NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        };
        for (position, anchor, direction) in
            [(1, schedule.date(1), -1), (n, schedule.date(n - 1), 1)]
        {
            if !schedule.is_regular_at(position) {
                let steps = tenor
                    .length()
                    .checked_mul(direction)
                    .ok_or_else(|| BindingError::invalid("CPI stub tenor overflow"))?;
                let reference = crate::floating_bond_api::advance(
                    &arithmetic,
                    anchor,
                    steps,
                    tenor.units(),
                    Bdc::Unadjusted,
                    false,
                )?;
                crate::floating_bond_api::advance(
                    schedule_calendar,
                    reference,
                    0,
                    TimeUnit::Days,
                    schedule.business_day_convention(),
                    false,
                )?;
            }
        }
    }
    Ok(())
}

#[repr(C)]
pub struct ItofinCpiLegConfig {
    pub schedule: u64,
    pub index: u64,
    pub base_cpi: f64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub day_counter: u64,
    pub payment_calendar: u64,
    pub payment_convention: i32,
    pub notionals: *const f64,
    pub notionals_len: usize,
    pub fixed_rates: *const f64,
    pub fixed_rates_len: usize,
    pub growth_only: i32,
    pub ex_coupon_length: i32,
    pub ex_coupon_unit: i32,
    pub ex_coupon_calendar: u64,
    pub ex_coupon_convention: i32,
    pub ex_coupon_end_of_month: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_leg_new(
    ctx: *mut Context,
    a: ItofinCpiLegConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { cpi_leg_new(ctx, a, None, out, error) }
}

#[unsafe(no_mangle)]
/// Build a live-base-date CPI leg. Config `base_cpi` is ignored, not snapshotted.
/// A zero base date selects schedule start minus observation lag.
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_leg_from_base_date(
    ctx: *mut Context,
    a: ItofinCpiLegConfig,
    base_date: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { cpi_leg_new(ctx, a, Some(base_date), out, error) }
}

/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
unsafe fn cpi_leg_new(
    ctx: *mut Context,
    a: ItofinCpiLegConfig,
    base_date: Option<i32>,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let schedule = c.get::<Schedule>(a.schedule)?;
            let index = zero_index(c, a.index)?;
            let payment = c.get::<NativeCalendar>(a.payment_calendar)?;
            let adjustment = convention(a.payment_convention)?;
            check_leg_dates(
                &schedule,
                &schedule_calendar(c, a.schedule)?,
                &payment,
                adjustment,
            )?;
            let notionals = input_slice(a.notionals, a.notionals_len)?.to_vec();
            let rates = input_slice(a.fixed_rates, a.fixed_rates_len)?.to_vec();
            let lag = period(a.lag_length, a.lag_unit)?;
            let builder = match base_date {
                Some(base) => CpiLeg::from_base_date(
                    schedule.clone(),
                    index.clone(),
                    if base == 0 { None } else { Some(date(base)?) },
                    lag,
                ),
                None => CpiLeg::new(schedule.clone(), index.clone(), finite(a.base_cpi)?, lag),
            };
            let mut leg = builder
                .with_notionals(notionals)
                .with_fixed_rates(rates)
                .with_payment_day_counter(day_counter(c, a.day_counter)?)
                .with_payment_calendar(payment.inner)
                .with_payment_adjustment(adjustment)
                .with_observation_interpolation(cpi_interpolation(a.interpolation)?)
                .with_subtract_inflation_nominal(bool_value(a.growth_only)?);
            let eom = bool_value(a.ex_coupon_end_of_month)?;
            if a.ex_coupon_length != 0 {
                let period = period(a.ex_coupon_length, a.ex_coupon_unit)?;
                let calendar = c.get::<NativeCalendar>(a.ex_coupon_calendar)?;
                let convention = convention(a.ex_coupon_convention)?;
                for &date in schedule.dates() {
                    let payment = crate::floating_bond_api::advance(
                        &c.get::<NativeCalendar>(a.payment_calendar)?,
                        date,
                        0,
                        TimeUnit::Days,
                        adjustment,
                        false,
                    )?;
                    let steps = a
                        .ex_coupon_length
                        .checked_neg()
                        .ok_or_else(|| BindingError::invalid("ex-coupon period overflow"))?;
                    crate::floating_bond_api::advance(
                        &calendar,
                        payment,
                        steps,
                        period.units(),
                        convention,
                        eom,
                    )?;
                }
                leg = leg.with_ex_coupon_period(period, calendar.inner, convention, eom);
            }
            let leg = leg.build()?;
            let settings = index.inflation_base().settings().clone();
            output(
                out,
                c.insert(NativeCpiLeg {
                    leg,
                    settings: CpiLegSettings(settings),
                })?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_leg_build(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let native = c.get::<NativeCpiLeg>(id)?;
            output(out, c.insert_with_metadata(native.leg, native.settings)?)
        })
    }
}

#[repr(C)]
pub struct ItofinCpiCouponConfig {
    pub index: u64,
    pub base_cpi: f64,
    pub nominal: f64,
    pub fixed_rate: f64,
    pub payment: i32,
    pub start: i32,
    pub end: i32,
    pub reference_start: i32,
    pub reference_end: i32,
    pub ex_coupon: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub day_counter: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_coupon_new(
    ctx: *mut Context,
    a: ItofinCpiCouponConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { cpi_coupon_new(ctx, a, None, out, error) }
}

#[unsafe(no_mangle)]
/// Build a coupon with a live base-date denominator. Config `base_cpi` is
/// ignored, not snapshotted. Query field 6 for the fallible effective base CPI.
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_coupon_from_base_date(
    ctx: *mut Context,
    a: ItofinCpiCouponConfig,
    base_date: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { cpi_coupon_new(ctx, a, Some(base_date), out, error) }
}

/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
unsafe fn cpi_coupon_new(
    ctx: *mut Context,
    a: ItofinCpiCouponConfig,
    base_date: Option<i32>,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let optional = |value| {
                if value == 0 {
                    Ok(None)
                } else {
                    date(value).map(Some)
                }
            };
            let coupon = match base_date {
                Some(base) => CpiCoupon::from_base_date(
                    date(base)?,
                    date(a.payment)?,
                    a.nominal,
                    date(a.start)?,
                    date(a.end)?,
                    zero_index(c, a.index)?,
                    period(a.lag_length, a.lag_unit)?,
                    cpi_interpolation(a.interpolation)?,
                    day_counter(c, a.day_counter)?,
                    a.fixed_rate,
                    optional(a.reference_start)?,
                    optional(a.reference_end)?,
                    optional(a.ex_coupon)?,
                ),
                None => CpiCoupon::new(
                    a.base_cpi,
                    date(a.payment)?,
                    a.nominal,
                    date(a.start)?,
                    date(a.end)?,
                    zero_index(c, a.index)?,
                    period(a.lag_length, a.lag_unit)?,
                    cpi_interpolation(a.interpolation)?,
                    day_counter(c, a.day_counter)?,
                    a.fixed_rate,
                    optional(a.reference_start)?,
                    optional(a.reference_end)?,
                    optional(a.ex_coupon)?,
                ),
            }?;
            output(out, c.insert(shared(coupon))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_coupon_pricer_new(
    ctx: *mut Context,
    discount: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let handle = if discount == 0 {
                Handle::empty()
            } else {
                curve(c, discount)?
            };
            output(out, c.insert(shared(CpiCouponPricer::new(handle)))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_coupon_set_pricer(
    ctx: *mut Context,
    id: u64,
    pricer: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            c.get::<Shared<CpiCoupon>>(id)?
                .set_pricer(c.get::<Shared<CpiCouponPricer>>(pricer)?);
            Ok(())
        })
    }
}
/// Fields: 0 rate, 1 amount, 2 index fixing, 3 index ratio at date,
/// 4 accrued amount at date, 5 accrual period, 6 effective base fixing.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_coupon_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let coupon = c.get::<Shared<CpiCoupon>>(id)?;
            let value = match field {
                0 => coupon.rate()?,
                1 => coupon.amount()?,
                2 => coupon.index_fixing()?,
                3 => coupon.index_ratio(date(serial)?)?,
                4 => coupon.accrued_amount(date(serial)?)?,
                5 => coupon.accrual_period(),
                6 => coupon.base_fixing()?,
                _ => return Err(BindingError::invalid("unknown CPI coupon query")),
            };
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid; outputs must not overlap. The context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_pricer_value(
    ctx: *mut Context,
    pricer: u64,
    coupon: u64,
    field: i32,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let pricer = c.get::<Shared<CpiCouponPricer>>(pricer)?;
            let coupon = c.get::<Shared<CpiCoupon>>(coupon)?;
            let value = match field {
                0 => pricer.swaplet_rate(&coupon)?,
                1 => pricer.swaplet_price(&coupon)?,
                2 => pricer.accrued_rate(&coupon, date(serial)?)?,
                _ => return Err(BindingError::invalid("unknown CPI pricer query")),
            };
            output(out, finite(value)?)
        })
    }
}

#[cfg(test)]
#[path = "cpi_cashflows_tests.rs"]
mod tests;
