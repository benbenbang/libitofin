//! Municipal index, coupon, swap and bootstrap handles.
use crate::boundary::*;
use crate::curves_api::optional_curve;
use crate::rates_api::{finite, settings};
use crate::time_api::{bool_flag, date, day_counter};
use libitofin::cashflows::{AverageBMACoupon, Coupon};
use libitofin::indexes::{BMAIndex, Index, InterestRateIndex};
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Date;

/// Construct a retained BMA index; zero forwarding creates an empty forecast handle.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_index_new(
    ctx: *mut Context,
    forwarding: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = shared(BMAIndex::new(
                optional_curve(c, forwarding)?,
                settings(c, settings_id)?,
            ));
            output(out, c.insert(index)?)
        })
    }
}
/// Store a finite fixing on a valid weekly fixing date.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_add_fixing(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    value: f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<Shared<BMAIndex>>(id)?
                .add_fixing(date(serial)?, finite(value)?)?;
            Ok(())
        })
    }
}
/// Resolve a historical or forecast fixing.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_fixing(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    forecast_today: u8,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.get::<Shared<BMAIndex>>(id)?
                    .fixing(date(serial)?, bool_flag(forecast_today)?)?,
            )
        })
    }
}
/// Query zero: value date; one: maturity date; two: valid-fixing flag; three: historical-fixing flag (0 or 1).
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_date(
    ctx: *mut Context,
    id: u64,
    query: i32,
    serial: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = c.get::<Shared<BMAIndex>>(id)?;
            let d = date(serial)?;
            output(
                out,
                match query {
                    0 => index.value_date(d)?.serial_number(),
                    1 => index.maturity_date(d)?.serial_number(),
                    2 => i32::from(index.is_valid_fixing_date(d)),
                    3 => i32::from(index.has_historical_fixing(d)),
                    _ => return Err(BindingError::invalid("unknown BMA date query")),
                },
            )
        })
    }
}
/// Coupon constructor; zero reference dates use the accrual dates.
#[repr(C)]
pub struct ItofinBmaCouponConfig {
    pub payment_date: i32,
    pub nominal: f64,
    pub start_date: i32,
    pub end_date: i32,
    pub index: u64,
    pub day_counter: u64,
    pub gearing: f64,
    pub spread: f64,
    pub reference_start: i32,
    pub reference_end: i32,
}
/// Construct an average coupon retaining its index and historical fixings.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_coupon_new(
    ctx: *mut Context,
    cfg: *const ItofinBmaCouponConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let optional_date = |s| if s == 0 { Ok(None) } else { date(s).map(Some) };
            let coupon = AverageBMACoupon::new(
                date(a.payment_date)?,
                finite(a.nominal)?,
                date(a.start_date)?,
                date(a.end_date)?,
                c.get::<Shared<BMAIndex>>(a.index)?,
                finite(a.gearing)?,
                finite(a.spread)?,
                optional_date(a.reference_start)?,
                optional_date(a.reference_end)?,
                day_counter(c, a.day_counter)?,
            )?;
            output(out, c.insert(shared(coupon))?)
        })
    }
}
/// Query zero: coupon rate; one: payment; two: accrual year fraction.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_coupon_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let coupon = c.get::<Shared<AverageBMACoupon>>(id)?;
            output(
                out,
                match query {
                    0 => coupon.rate()?,
                    1 => coupon.amount()?,
                    2 => coupon.accrual_period(),
                    _ => return Err(BindingError::invalid("unknown BMA coupon query")),
                },
            )
        })
    }
}
/// Kind zero: index fixing schedule between start/end; one: coupon fixing dates.
/// Capacity zero queries length; insufficient capacity preserves all outputs.
/// # Safety
/// Follow the crate-level pointer and thread contract. Output holds capacity serials.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_dates(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    start: i32,
    end: i32,
    out: *mut i32,
    capacity: usize,
    length: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(length)?;
            let dates: Vec<Date> = match kind {
                0 => c
                    .get::<Shared<BMAIndex>>(id)?
                    .fixing_schedule(date(start)?, date(end)?)?
                    .dates()
                    .to_vec(),
                1 => c
                    .get::<Shared<AverageBMACoupon>>(id)?
                    .fixing_dates()
                    .to_vec(),
                _ => return Err(BindingError::invalid("unknown BMA dates kind")),
            };
            if capacity != 0 {
                if capacity < dates.len() {
                    return Err(BindingError::invalid("BMA date buffer too small"));
                }
                check_ptr(out)?;
                for (i, d) in dates.iter().enumerate() {
                    output(out.add(i), d.serial_number())?;
                }
            }
            output(length, dates.len())
        })
    }
}
/// Return the independently owned BMA fixing calendar.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_calendar(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = c.get::<Shared<BMAIndex>>(id)?;
            let calendar = crate::calendar_api::NativeCalendar {
                inner: index.fixing_calendar(),
                horizon: None,
                first_year: 1901,
            };
            output(out, c.insert(calendar)?)
        })
    }
}
/// Clear this BMA fixing history, notifying retained consumers.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_clear_fixings(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<Shared<BMAIndex>>(id)?.clear_fixings();
            Ok(())
        })
    }
}
/// Read stored history without forecasting. An absent fixing returns found=0, value=0.
/// # Safety
/// Follow the crate-level pointer and thread contract. Both outputs must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_past_fixing(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    value: *mut f64,
    found: *mut u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(value)?;
            check_ptr(found)?;
            let fixing = c.get::<Shared<BMAIndex>>(id)?.past_fixing(date(serial)?)?;
            output(value, fixing.unwrap_or(0.0))?;
            output(found, u8::from(fixing.is_some()))
        })
    }
}

#[cfg(test)]
#[path = "bma_tests.rs"]
mod tests;
