//! Explicit-base CPI coupon and plain pricer adapters.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, zero_index};
use crate::rates_api::{curve, finite, period};
use crate::time_api::{date, day_counter};
use libitofin::cashflows::{Coupon, CpiCoupon, CpiCouponPricer};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};

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
            let coupon = CpiCoupon::new(
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
            )?;
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
/// 4 accrued amount at date, 5 accrual period.
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
