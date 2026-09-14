//! Inflation optionlet volatility and pricing distributions.
use crate::boundary::*;
use crate::inflation_api::yoy_index;
use crate::rates_api::{curve, finite, period};
use crate::settings_api::settings;
use crate::time_api::{bool_flag, calendar, convention, date, day_counter, frequency};
use libitofin::cashflows::{YoYInflationOptionletCouponPricer, YoYOptionletDistribution};
use libitofin::handle::Handle;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::YoYInflationCapFloorEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{
    ConstantYoYOptionletVolatility, YoYOptionletVolatilitySurface,
};
use libitofin::time::{frequency::Frequency, timeunit::TimeUnit};

pub(crate) fn yoy_vol(
    c: &Context,
    id: u64,
) -> BindingResult<Handle<dyn YoYOptionletVolatilitySurface>> {
    Ok(Handle::new(
        c.get::<Shared<ConstantYoYOptionletVolatility>>(id)?
            as Shared<dyn YoYOptionletVolatilitySurface>,
    ))
}
pub(crate) fn yoy_engine(c: &Context, id: u64) -> BindingResult<SharedMut<dyn PricingEngine>> {
    Ok(c.get::<SharedMut<YoYInflationCapFloorEngine>>(id)? as SharedMut<dyn PricingEngine>)
}
pub(crate) fn frequency_code(f: Frequency) -> BindingResult<i32> {
    match f {
        Frequency::Annual => Ok(0),
        Frequency::Semiannual => Ok(1),
        Frequency::Quarterly => Ok(2),
        Frequency::Monthly => Ok(3),
        _ => Err(BindingError::invalid("frequency not exposed")),
    }
}
pub(crate) fn unit_code(unit: TimeUnit) -> BindingResult<i32> {
    match unit {
        TimeUnit::Days => Ok(0),
        TimeUnit::Weeks => Ok(1),
        TimeUnit::Months => Ok(2),
        TimeUnit::Years => Ok(3),
        _ => Err(BindingError::invalid("time unit not exposed")),
    }
}
#[repr(C)]
pub struct ItofinConstantYoYVolConfig {
    pub volatility: f64,
    pub quote: u64,
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub frequency: i32,
    pub interpolated: u8,
    pub min_strike: f64,
    pub max_strike: f64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_constant_yoy_vol_new(
    ctx: *mut Context,
    a: ItofinConstantYoYVolConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.settlement_days > i32::MAX as u32 {
                return Err(BindingError::invalid(
                    "settlement days exceed core integer range",
                ));
            }
            finite(a.min_strike)?;
            finite(a.max_strike)?;
            if a.min_strike > a.max_strike {
                return Err(BindingError::invalid("strike bounds reversed"));
            }
            let cal = calendar(c, a.calendar)?;
            let conv = convention(a.convention)?;
            let dc = day_counter(c, a.day_counter)?;
            let lag = period(a.lag_length, a.lag_unit)?;
            let freq = frequency(a.frequency)?;
            let interp = bool_flag(a.interpolated)?;
            let settings = settings(c, a.settings)?;
            let vol = if a.quote == 0 {
                ConstantYoYOptionletVolatility::new(
                    finite(a.volatility)?,
                    a.settlement_days,
                    cal,
                    conv,
                    dc,
                    lag,
                    freq,
                    interp,
                    a.min_strike,
                    a.max_strike,
                    settings,
                )
            } else {
                ConstantYoYOptionletVolatility::with_quote(
                    crate::market_api::quote(c, a.quote)?,
                    a.settlement_days,
                    cal,
                    conv,
                    dc,
                    lag,
                    freq,
                    interp,
                    a.min_strike,
                    a.max_strike,
                    settings,
                )
            };
            output(out, c.insert(shared(vol))?)
        })
    }
}
#[repr(C)]
pub struct ItofinYoYVolMetadata {
    pub lag_length: i32,
    pub lag_unit: i32,
    pub frequency: i32,
    pub interpolated: u8,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_constant_yoy_vol_metadata(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinYoYVolMetadata,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<ConstantYoYOptionletVolatility>>(id)?;
            let lag = v.observation_lag();
            output(
                out,
                ItofinYoYVolMetadata {
                    lag_length: lag.length(),
                    lag_unit: unit_code(lag.units())?,
                    frequency: frequency_code(v.frequency())?,
                    interpolated: u8::from(v.index_is_interpolated()),
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
pub unsafe extern "C" fn itofin_constant_yoy_vol_base_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                c.get::<Shared<ConstantYoYOptionletVolatility>>(id)?
                    .base_date()?
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
pub unsafe extern "C" fn itofin_constant_yoy_vol_value(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    strike: f64,
    lag_length: i32,
    lag_unit: i32,
    variance: u8,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Shared<ConstantYoYOptionletVolatility>>(id)?;
            let d = date(serial)?;
            let lag = period(lag_length, lag_unit)?;
            output(
                out,
                if bool_flag(variance)? {
                    v.total_variance(d, finite(strike)?, lag)?
                } else {
                    v.volatility(d, finite(strike)?, lag)?
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
pub unsafe extern "C" fn itofin_yoy_capfloor_engine_new(
    ctx: *mut Context,
    index: u64,
    volatility: u64,
    nominal: u64,
    distribution: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = yoy_index(c, index)?;
            let vol = yoy_vol(c, volatility)?;
            let discount = curve(c, nominal)?;
            let engine = match distribution {
                0 => YoYInflationCapFloorEngine::black(index, vol, discount),
                1 => YoYInflationCapFloorEngine::unit_displaced(index, vol, discount),
                2 => YoYInflationCapFloorEngine::bachelier(index, vol, discount),
                _ => return Err(BindingError::invalid("invalid optionlet distribution")),
            };
            output(out, c.insert(shared_mut(engine))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_engine_distribution(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let engine = c.get::<SharedMut<YoYInflationCapFloorEngine>>(id)?;
            output(
                out,
                match engine.borrow().distribution() {
                    YoYOptionletDistribution::Black => 0,
                    YoYOptionletDistribution::UnitDisplaced => 1,
                    YoYOptionletDistribution::Bachelier => 2,
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
pub unsafe extern "C" fn itofin_yoy_coupon_pricer_new(
    ctx: *mut Context,
    volatility: u64,
    nominal: u64,
    distribution: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let vol = yoy_vol(c, volatility)?;
            let discount = if nominal == 0 {
                Handle::empty()
            } else {
                curve(c, nominal)?
            };
            let pricer = match distribution {
                0 => YoYInflationOptionletCouponPricer::black(vol, discount),
                1 => YoYInflationOptionletCouponPricer::unit_displaced(vol, discount),
                2 => YoYInflationOptionletCouponPricer::bachelier(vol, discount),
                _ => return Err(BindingError::invalid("invalid optionlet distribution")),
            };
            output(out, c.insert(shared_mut(pricer))?)
        })
    }
}
