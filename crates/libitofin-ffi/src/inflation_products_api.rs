//! Inflation swaps and optionlet instruments; all dependencies stay in Rust.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, yoy_index, zero_index};
use crate::rates_api::{curve, finite, period, swap_type};
use crate::settings_api::settings;
use crate::time_api::{calendar, convention, date, day_counter};
use libitofin::instrument::Instrument;
use libitofin::instruments::{YearOnYearInflationSwap, ZeroCouponInflationSwap};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::time::schedule::Schedule;

pub(crate) fn discount_engine(c: &Context, id: u64) -> BindingResult<SharedMut<dyn PricingEngine>> {
    Ok(c.get::<SharedMut<DiscountingSwapEngine>>(id)? as SharedMut<dyn PricingEngine>)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_discounting_swap_engine_new(
    ctx: *mut Context,
    discount: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let engine = DiscountingSwapEngine::new(
                curve(c, discount)?,
                None,
                None,
                None,
                settings(c, settings_id)?,
            );
            output(out, c.insert(shared_mut(engine))?)
        })
    }
}
#[repr(C)]
pub struct ItofinZeroInflationSwapConfig {
    pub swap_type: i32,
    pub nominal: f64,
    pub start: i32,
    pub maturity: i32,
    pub fixed_calendar: u64,
    pub fixed_convention: i32,
    pub day_counter: u64,
    pub fixed_rate: f64,
    pub index: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub inflation_calendar: u64,
    pub inflation_convention: i32,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_new(
    ctx: *mut Context,
    a: ItofinZeroInflationSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let cal = if a.inflation_calendar == 0 {
                None
            } else {
                Some(calendar(c, a.inflation_calendar)?)
            };
            let conv = if a.inflation_convention == -1 {
                None
            } else {
                Some(convention(a.inflation_convention)?)
            };
            let swap = ZeroCouponInflationSwap::new(
                swap_type(a.swap_type)?,
                finite(a.nominal)?,
                date(a.start)?,
                date(a.maturity)?,
                calendar(c, a.fixed_calendar)?,
                convention(a.fixed_convention)?,
                day_counter(c, a.day_counter)?,
                finite(a.fixed_rate)?,
                zero_index(c, a.index)?,
                period(a.lag_length, a.lag_unit)?,
                cpi_interpolation(a.interpolation)?,
                cal,
                conv,
                settings(c, a.settings)?,
            )?;
            output(out, c.insert(shared_mut(swap))?)
        })
    }
}
#[repr(C)]
pub struct ItofinYoYInflationSwapConfig {
    pub swap_type: i32,
    pub nominal: f64,
    pub fixed_schedule: u64,
    pub fixed_rate: f64,
    pub fixed_day_counter: u64,
    pub yoy_schedule: u64,
    pub index: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub spread: f64,
    pub yoy_day_counter: u64,
    pub payment_calendar: u64,
    pub payment_convention: i32,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_new(
    ctx: *mut Context,
    a: ItofinYoYInflationSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let swap = YearOnYearInflationSwap::new(
                swap_type(a.swap_type)?,
                finite(a.nominal)?,
                c.get::<Schedule>(a.fixed_schedule)?,
                finite(a.fixed_rate)?,
                day_counter(c, a.fixed_day_counter)?,
                c.get::<Schedule>(a.yoy_schedule)?,
                yoy_index(c, a.index)?,
                period(a.lag_length, a.lag_unit)?,
                cpi_interpolation(a.interpolation)?,
                finite(a.spread)?,
                day_counter(c, a.yoy_day_counter)?,
                calendar(c, a.payment_calendar)?,
                convention(a.payment_convention)?,
                settings(c, a.settings)?,
            )?;
            output(out, c.insert(shared_mut(swap))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(discount_engine(c, engine)?);
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
pub unsafe extern "C" fn itofin_zero_inflation_swap_calculate(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?
                .borrow_mut()
                .calculate()?;
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
pub unsafe extern "C" fn itofin_zero_inflation_swap_is_calculated(
    ctx: *mut Context,
    id: u64,
    out: *mut u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                u8::from(
                    c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?
                        .borrow()
                        .base()
                        .is_calculated(),
                ),
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
pub unsafe extern "C" fn itofin_zero_inflation_swap_results(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            value.borrow_mut().calculate()?;
            let snapshot = crate::results_api::snapshot(value.borrow().base());
            output(out, c.insert(snapshot)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow_mut().npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_fair_rate(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow_mut().fair_rate()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_fixed_leg_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow_mut().fixed_leg_npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_inflation_leg_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow_mut().inflation_leg_npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_fixed_leg_bps(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow_mut().fixed_leg_bps()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_maturity_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow().maturity_date().serial_number())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_obs_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(out, value.borrow().obs_date().serial_number())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_swap_inflation_fixing_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<ZeroCouponInflationSwap>>(id)?;
            output(
                out,
                value
                    .borrow()
                    .inflation_cash_flow()
                    .fixing_date()
                    .serial_number(),
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
pub unsafe extern "C" fn itofin_yoy_inflation_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<YearOnYearInflationSwap>>(id)?
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(discount_engine(c, engine)?);
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
pub unsafe extern "C" fn itofin_yoy_inflation_swap_calculate(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<YearOnYearInflationSwap>>(id)?
                .borrow_mut()
                .calculate()?;
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
pub unsafe extern "C" fn itofin_yoy_inflation_swap_is_calculated(
    ctx: *mut Context,
    id: u64,
    out: *mut u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                u8::from(
                    c.get::<SharedMut<YearOnYearInflationSwap>>(id)?
                        .borrow()
                        .base()
                        .is_calculated(),
                ),
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
pub unsafe extern "C" fn itofin_yoy_inflation_swap_results(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            value.borrow_mut().calculate()?;
            let snapshot = crate::results_api::snapshot(value.borrow().base());
            output(out, c.insert(snapshot)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_fair_rate(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().fair_rate()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_fair_spread(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().fair_spread()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_fixed_leg_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().fixed_leg_npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_yoy_leg_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().yoy_leg_npv()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_fixed_rate(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().fixed_rate())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_swap_spread(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = c.get::<SharedMut<YearOnYearInflationSwap>>(id)?;
            output(out, value.borrow_mut().spread())
        })
    }
}
