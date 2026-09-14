//! Quoted inflation price grids and stripped optionlet volatility.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, yoy_index};
use crate::inflation_vol_api::unit_code;
use crate::rates_api::{curve, finite, period};
use crate::settings_api::settings;
use crate::time_api::{bool_flag, calendar, convention, date, day_counter};
use libitofin::handle::RelinkableHandle;
use libitofin::math::{interpolations::linear::Linear, matrix::Matrix};
use libitofin::pricingengines::YoYInflationCapFloorEngine;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::inflation::yoycapfloortermpricesurface::{
    InterpolatedYoYCapFloorTermPriceSurface, YoYCapFloorTermPriceSurface,
};
use libitofin::termstructures::volatility::{
    InterpolatedYoYOptionletStripper, KInterpolatedYoYOptionletVolatilitySurface,
    VolatilityTermStructure, YoYOptionletStripper, YoYOptionletVolatilitySurface,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinInflationPeriod {
    pub length: i32,
    pub unit: i32,
}
#[repr(C)]
pub struct ItofinYoYPriceSurfaceConfig {
    pub fixing_days: u32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub index: u64,
    pub interpolation: i32,
    pub nominal: u64,
    pub day_counter: u64,
    pub calendar: u64,
    pub convention: i32,
    pub cap_strikes: *const f64,
    pub cap_count: usize,
    pub floor_strikes: *const f64,
    pub floor_count: usize,
    pub maturities: *const ItofinInflationPeriod,
    pub maturity_count: usize,
    pub cap_prices: *const f64,
    pub cap_price_count: usize,
    pub floor_prices: *const f64,
    pub floor_price_count: usize,
    pub settings: u64,
}
fn matrix(data: &[f64], rows: usize, cols: usize) -> BindingResult<Matrix> {
    if rows.checked_mul(cols) != Some(data.len()) || rows == 0 || cols == 0 {
        return Err(BindingError::invalid("invalid price matrix shape"));
    }
    if data.iter().any(|v| !v.is_finite()) {
        return Err(BindingError::invalid("nonfinite matrix value"));
    }
    let mut m = Matrix::with_size(rows, cols);
    for (i, row) in data.chunks(cols).enumerate() {
        m.row_mut(i).copy_from_slice(row);
    }
    Ok(m)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_price_surface_new(
    ctx: *mut Context,
    a: ItofinYoYPriceSurfaceConfig,
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
            let caps = input_slice(a.cap_strikes, a.cap_count)?;
            let floors = input_slice(a.floor_strikes, a.floor_count)?;
            if caps.iter().chain(floors).any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite strike"));
            }
            let maturities = input_slice(a.maturities, a.maturity_count)?
                .iter()
                .map(|p| period(p.length, p.unit))
                .collect::<BindingResult<Vec<_>>>()?;
            let cap_prices = matrix(
                input_slice(a.cap_prices, a.cap_price_count)?,
                a.cap_count,
                a.maturity_count,
            )?;
            let floor_prices = matrix(
                input_slice(a.floor_prices, a.floor_price_count)?,
                a.floor_count,
                a.maturity_count,
            )?;
            let surface = InterpolatedYoYCapFloorTermPriceSurface::new(
                a.fixing_days,
                period(a.lag_length, a.lag_unit)?,
                yoy_index(c, a.index)?,
                cpi_interpolation(a.interpolation)?,
                curve(c, a.nominal)?,
                day_counter(c, a.day_counter)?,
                calendar(c, a.calendar)?,
                convention(a.convention)?,
                caps.to_vec(),
                floors.to_vec(),
                maturities,
                cap_prices,
                floor_prices,
                settings(c, a.settings)?,
            )?;
            output(out, c.insert(shared(surface))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_price_surface_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    serial: i32,
    strike: f64,
    extrapolate: u8,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let s = c.get::<Shared<InterpolatedYoYCapFloorTermPriceSurface>>(id)?;
            let d = date(serial)?;
            output(
                out,
                match query {
                    0 => s.cap_price(d, finite(strike)?)?,
                    1 => s.floor_price(d, finite(strike)?)?,
                    2 => s.atm_yoy_swap_rate(d, bool_flag(extrapolate)?)?,
                    _ => return Err(BindingError::invalid("invalid price query")),
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
pub unsafe extern "C" fn itofin_yoy_price_surface_strikes(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let s = c.get::<Shared<InterpolatedYoYCapFloorTermPriceSurface>>(id)?;
            let values = s.strikes();
            output(required, values.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < values.len() {
                return Err(BindingError::invalid("strike buffer too small"));
            }
            check_ptr(out)?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
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
pub unsafe extern "C" fn itofin_yoy_price_surface_maturities(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinInflationPeriod,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let s = c.get::<Shared<InterpolatedYoYCapFloorTermPriceSurface>>(id)?;
            let values = s.maturities();
            output(required, values.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < values.len() {
                return Err(BindingError::invalid("maturity buffer too small"));
            }
            check_ptr(out)?;
            for (i, v) in values.iter().enumerate() {
                output(
                    out.add(i),
                    ItofinInflationPeriod {
                        length: v.length(),
                        unit: unit_code(v.units())?,
                    },
                )?;
            }
            Ok(())
        })
    }
}
#[repr(C)]
pub struct ItofinKYoYVolConfig {
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub prices: u64,
    pub index: u64,
    pub nominal: u64,
    pub slope: f64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_k_yoy_vol_new(
    ctx: *mut Context,
    a: ItofinKYoYVolConfig,
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
            let link = RelinkableHandle::<dyn YoYOptionletVolatilitySurface>::empty();
            let pricer = shared_mut(YoYInflationCapFloorEngine::unit_displaced(
                yoy_index(c, a.index)?,
                link.handle(),
                curve(c, a.nominal)?,
            ));
            let stripper = shared(InterpolatedYoYOptionletStripper::<Linear>::new())
                as Shared<dyn YoYOptionletStripper>;
            let prices = c.get::<Shared<InterpolatedYoYCapFloorTermPriceSurface>>(a.prices)?
                as Shared<dyn YoYCapFloorTermPriceSurface>;
            let surface = KInterpolatedYoYOptionletVolatilitySurface::<Linear>::new(
                a.settlement_days,
                calendar(c, a.calendar)?,
                convention(a.convention)?,
                day_counter(c, a.day_counter)?,
                period(a.lag_length, a.lag_unit)?,
                prices,
                &pricer,
                &link,
                stripper,
                finite(a.slope)?,
                settings(c, a.settings)?,
            )?;
            output(
                out,
                c.insert((shared(surface), period(a.lag_length, a.lag_unit)?))?,
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
pub unsafe extern "C" fn itofin_k_yoy_vol_query(
    ctx: *mut Context,
    id: u64,
    query: i32,
    serial: i32,
    strike: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let (s, lag) = c.get::<(
                Shared<KInterpolatedYoYOptionletVolatilitySurface<Linear>>,
                libitofin::time::period::Period,
            )>(id)?;
            output(
                out,
                match query {
                    0 => s.volatility(date(serial)?, finite(strike)?, lag)?,
                    1 => s.base_date()?.serial_number() as f64,
                    2 => s.min_strike(),
                    3 => s.max_strike(),
                    4 => s.max_date().serial_number() as f64,
                    _ => return Err(BindingError::invalid("invalid stripped volatility query")),
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
pub unsafe extern "C" fn itofin_k_yoy_vol_slice(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    strikes: *mut f64,
    vols: *mut f64,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let (s, _lag) = c.get::<(
                Shared<KInterpolatedYoYOptionletVolatilitySurface<Linear>>,
                libitofin::time::period::Period,
            )>(id)?;
            let (k, v) = s.d_slice(date(serial)?)?;
            output(required, k.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < k.len() {
                return Err(BindingError::invalid("slice buffer too small"));
            }
            check_ptr(strikes)?;
            check_ptr(vols)?;
            std::ptr::copy_nonoverlapping(k.as_ptr(), strikes, k.len());
            std::ptr::copy_nonoverlapping(v.as_ptr(), vols, v.len());
            Ok(())
        })
    }
}
