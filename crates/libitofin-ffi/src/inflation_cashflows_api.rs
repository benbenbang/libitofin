//! Inflation coupon construction and value inspection.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, yoy_index};
use crate::inflation_vol_api::unit_code;
use crate::rates_api::period;
use crate::time_api::{calendar, convention, day_counter};
use libitofin::cashflows::{
    CappedFlooredYoYInflationCoupon, Coupon, YoYInflationCoupon, YoYInflationCouponPricer,
    YoYInflationLeg, YoYInflationOptionletCouponPricer, set_yoy_coupon_pricer,
};
use libitofin::event::Event;
use libitofin::indexes::inflationindex::CpiInterpolationType;
use libitofin::shared::{Shared, SharedMut, shared};
use libitofin::time::schedule::Schedule;

#[repr(C)]
pub struct ItofinYoYLegConfig {
    pub schedule: u64,
    pub calendar: u64,
    pub index: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub day_counter: u64,
    pub payment_convention: i32,
    pub fixing_days: u32,
    pub notionals: *const f64,
    pub notionals_len: usize,
    pub gearings: *const f64,
    pub gearings_len: usize,
    pub spreads: *const f64,
    pub spreads_len: usize,
    pub caps: *const f64,
    pub caps_len: usize,
    pub floors: *const f64,
    pub floors_len: usize,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_leg_new(
    ctx: *mut Context,
    a: ItofinYoYLegConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.fixing_days > i32::MAX as u32 {
                return Err(BindingError::invalid(
                    "fixing days exceed core integer range",
                ));
            }
            let mut leg = YoYInflationLeg::new(
                c.get::<Schedule>(a.schedule)?,
                calendar(c, a.calendar)?,
                yoy_index(c, a.index)?,
                period(a.lag_length, a.lag_unit)?,
                cpi_interpolation(a.interpolation)?,
            )
            .with_payment_day_counter(day_counter(c, a.day_counter)?)
            .with_payment_adjustment(convention(a.payment_convention)?)
            .with_fixing_days(a.fixing_days);
            let arrays = [
                input_slice(a.notionals, a.notionals_len)?,
                input_slice(a.gearings, a.gearings_len)?,
                input_slice(a.spreads, a.spreads_len)?,
                input_slice(a.caps, a.caps_len)?,
                input_slice(a.floors, a.floors_len)?,
            ];
            if arrays.iter().any(|a| a.iter().any(|v| !v.is_finite())) {
                return Err(BindingError::invalid("nonfinite leg parameter"));
            }
            if !arrays[0].is_empty() {
                leg = leg.with_notionals(arrays[0].to_vec());
            }
            if !arrays[1].is_empty() {
                leg = leg.with_gearings(arrays[1].to_vec());
            }
            if !arrays[2].is_empty() {
                leg = leg.with_spreads(arrays[2].to_vec());
            }
            if !arrays[3].is_empty() {
                leg = leg.with_caps_per_coupon(arrays[3].to_vec());
            }
            if !arrays[4].is_empty() {
                leg = leg.with_floors_per_coupon(arrays[4].to_vec());
            }
            output(out, c.insert(shared(leg))?)
        })
    }
}
/// Query count with capacity zero. Every materialization creates fresh coupons.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_leg_coupons(
    ctx: *mut Context,
    id: u64,
    pricer: u64,
    out: *mut u64,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            let leg = c.get::<Shared<YoYInflationLeg>>(id)?;
            if pricer == 0 {
                let coupons = leg.coupons()?;
                output(required, coupons.len())?;
                if capacity == 0 {
                    return Ok(());
                }
                if capacity < coupons.len() {
                    return Err(BindingError::invalid("coupon buffer too small"));
                }
                check_ptr(out)?;
                for (i, v) in coupons.into_iter().enumerate() {
                    output(out.add(i), c.insert(v)?)?;
                }
            } else {
                let pricer = c.get::<SharedMut<YoYInflationOptionletCouponPricer>>(pricer)?;
                let coupons = leg.capped_floored_coupons()?;
                output(required, coupons.len())?;
                if capacity == 0 {
                    return Ok(());
                }
                if capacity < coupons.len() {
                    return Err(BindingError::invalid("coupon buffer too small"));
                }
                check_ptr(out)?;
                set_yoy_coupon_pricer(&coupons, pricer as SharedMut<dyn YoYInflationCouponPricer>);
                for (i, v) in coupons.into_iter().enumerate() {
                    output(out.add(i), c.insert(v)?)?;
                }
            }
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_leg_build(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let leg = c.get::<Shared<YoYInflationLeg>>(id)?.build()?;
            output(out, c.insert(leg)?)
        })
    }
}
#[repr(C)]
pub struct ItofinYoYCouponFields {
    pub nominal: f64,
    pub accrual_period: f64,
    pub gearing: f64,
    pub spread: f64,
    pub fixing_date: i32,
    pub accrual_start: i32,
    pub accrual_end: i32,
    pub payment_date: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub fixing_days: u32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_coupon_fields(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinYoYCouponFields,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<YoYInflationCoupon>>(id)?;
            let lag = v.observation_lag();
            output(
                out,
                ItofinYoYCouponFields {
                    nominal: Coupon::nominal(&*v),
                    accrual_period: Coupon::accrual_period(&*v),
                    gearing: v.gearing(),
                    spread: v.spread(),
                    fixing_date: v.fixing_date().serial_number(),
                    accrual_start: Coupon::accrual_start_date(&*v).serial_number(),
                    accrual_end: Coupon::accrual_end_date(&*v).serial_number(),
                    payment_date: Event::date(&*v).serial_number(),
                    lag_length: lag.length(),
                    lag_unit: unit_code(lag.units())?,
                    interpolation: match v.interpolation() {
                        CpiInterpolationType::Flat => 0,
                        CpiInterpolationType::Linear => 1,
                    },
                    fixing_days: v.fixing_days(),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_coupon_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<YoYInflationCoupon>>(id)?;
            output(
                out,
                match query {
                    0 => Coupon::rate(&*v)?,
                    1 => Coupon::amount(&*v)?,
                    2 => v.index_fixing()?,
                    _ => return Err(BindingError::invalid("invalid coupon query")),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_coupon_day_counter(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = c.get::<Shared<YoYInflationCoupon>>(id)?;
            output(out, c.insert(Coupon::day_counter(&*v))?)
        })
    }
}
#[repr(C)]
pub struct ItofinCappedYoYCouponFields {
    pub is_capped: u8,
    pub is_floored: u8,
    pub effective_cap: f64,
    pub effective_floor: f64,
    pub accrual_start: i32,
    pub accrual_end: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_capped_yoy_coupon_fields(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinCappedYoYCouponFields,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<CappedFlooredYoYInflationCoupon>>(id)?;
            output(
                out,
                ItofinCappedYoYCouponFields {
                    is_capped: u8::from(v.is_capped()),
                    is_floored: u8::from(v.is_floored()),
                    effective_cap: v.effective_cap(),
                    effective_floor: v.effective_floor(),
                    accrual_start: Coupon::accrual_start_date(&*v).serial_number(),
                    accrual_end: Coupon::accrual_end_date(&*v).serial_number(),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_capped_yoy_coupon_value(
    ctx: *mut Context,
    id: u64,
    amount: u8,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<CappedFlooredYoYInflationCoupon>>(id)?;
            output(
                out,
                if crate::time_api::bool_flag(amount)? {
                    Coupon::amount(&*v)?
                } else {
                    Coupon::rate(&*v)?
                },
            )
        })
    }
}
