//! Swaption and optionlet constant volatilities, including live quotes and moving dates.
use crate::boundary::*;
use crate::market_api::quote;
use crate::settings_api::settings;
use crate::smile_api::volatility_type;
use crate::time_api::{calendar, convention, date, day_counter, time_unit};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    ConstantOptionletVolatility, ConstantSwaptionVolatility, OptionletVolatilityStructure,
    SwaptionVolatilityStructure,
};
use libitofin::time::period::Period;
/// quote/settings zero select scalar volatility/fixed date respectively.
#[repr(C)]
pub struct ItofinConstantRateVolConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub volatility: f64,
    pub day_counter: u64,
    pub volatility_type: i32,
    pub shift: f64,
    pub quote: u64,
    pub settings: u64,
}
fn boolean(v: i32) -> BindingResult<bool> {
    match v {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("expected boolean 0 or 1")),
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_constant_swaption_vol_new(
    ctx: *mut Context,
    cfg: *const ItofinConstantRateVolConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let x = &*cfg;
            if !x.shift.is_finite()
                || (x.quote == 0 && (!x.volatility.is_finite() || x.volatility < 0.0))
            {
                return Err(BindingError::invalid(
                    "shift must be finite and volatility finite and nonnegative",
                ));
            }
            let cal = calendar(c, x.calendar)?;
            let dc = day_counter(c, x.day_counter)?;
            let bdc = convention(x.convention)?;
            let kind = volatility_type(x.volatility_type)?;
            let v = match (x.settings != 0, x.quote != 0) {
                (false, false) => ConstantSwaptionVolatility::new(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    x.volatility,
                    dc,
                    kind,
                    x.shift,
                ),
                (false, true) => ConstantSwaptionVolatility::with_quote(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    quote(c, x.quote)?,
                    dc,
                    kind,
                    x.shift,
                ),
                (true, false) => ConstantSwaptionVolatility::moving(
                    x.settlement_days,
                    cal,
                    bdc,
                    x.volatility,
                    dc,
                    kind,
                    x.shift,
                    settings(c, x.settings)?,
                ),
                (true, true) => ConstantSwaptionVolatility::moving_with_quote(
                    x.settlement_days,
                    cal,
                    bdc,
                    quote(c, x.quote)?,
                    dc,
                    kind,
                    x.shift,
                    settings(c, x.settings)?,
                ),
            };
            output(
                out,
                c.insert(Handle::new(
                    shared(v) as Shared<dyn SwaptionVolatilityStructure>
                ))?,
            )
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_constant_optionlet_vol_new(
    ctx: *mut Context,
    cfg: *const ItofinConstantRateVolConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let x = &*cfg;
            if !x.shift.is_finite()
                || (x.quote == 0 && (!x.volatility.is_finite() || x.volatility < 0.0))
            {
                return Err(BindingError::invalid(
                    "shift must be finite and volatility finite and nonnegative",
                ));
            }
            let cal = calendar(c, x.calendar)?;
            let dc = day_counter(c, x.day_counter)?;
            let bdc = convention(x.convention)?;
            let kind = volatility_type(x.volatility_type)?;
            let v = match (x.settings != 0, x.quote != 0) {
                (false, false) => ConstantOptionletVolatility::new(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    x.volatility,
                    dc,
                    kind,
                    x.shift,
                ),
                (false, true) => ConstantOptionletVolatility::with_quote(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    quote(c, x.quote)?,
                    dc,
                    kind,
                    x.shift,
                ),
                (true, false) => ConstantOptionletVolatility::moving(
                    x.settlement_days,
                    cal,
                    bdc,
                    x.volatility,
                    dc,
                    kind,
                    x.shift,
                    settings(c, x.settings)?,
                ),
                (true, true) => ConstantOptionletVolatility::moving_with_quote(
                    x.settlement_days,
                    cal,
                    bdc,
                    quote(c, x.quote)?,
                    dc,
                    kind,
                    x.shift,
                    settings(c, x.settings)?,
                ),
            };
            output(
                out,
                c.insert(Handle::new(
                    shared(v) as Shared<dyn OptionletVolatilityStructure>
                ))?,
            )
        })
    }
}
/// Swaption query: 0 tenor volatility, 1 tenor variance, 2 date shift, 3 date volatility.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    option_length: i32,
    option_unit: i32,
    swap_length: i32,
    swap_unit: i32,
    serial: i32,
    length: f64,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !strike.is_finite() || !length.is_finite() {
                return Err(BindingError::invalid("query inputs must be finite"));
            }
            let v = c
                .get::<Handle<dyn SwaptionVolatilityStructure>>(id)?
                .current_link()?;
            let ext = boolean(extrapolate)?;
            let result = match kind {
                0 | 1 => {
                    let o = Period::new(option_length, time_unit(option_unit)?);
                    let s = Period::new(swap_length, time_unit(swap_unit)?);
                    if kind == 0 {
                        v.volatility_tenors(o, s, strike, ext)?
                    } else {
                        v.black_variance_tenors(o, s, strike, ext)?
                    }
                }
                2 => v.shift(date(serial)?, length, ext)?,
                3 => v.volatility(date(serial)?, length, strike, ext)?,
                _ => return Err(BindingError::invalid("unknown swaption volatility query")),
            };
            output(out, result)
        })
    }
}
/// Optionlet query: 0 tenor volatility, 1 tenor variance, 2 date volatility, 3 displacement.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_vol_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    length: i32,
    unit: i32,
    serial: i32,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !strike.is_finite() {
                return Err(BindingError::invalid("strike must be finite"));
            }
            let v = c
                .get::<Handle<dyn OptionletVolatilityStructure>>(id)?
                .current_link()?;
            let ext = boolean(extrapolate)?;
            output(
                out,
                match kind {
                    0 => v.volatility_tenor(Period::new(length, time_unit(unit)?), strike, ext)?,
                    1 => {
                        v.black_variance_tenor(Period::new(length, time_unit(unit)?), strike, ext)?
                    }
                    2 => v.volatility_date(date(serial)?, strike, ext)?,
                    3 => v.displacement(),
                    _ => return Err(BindingError::invalid("unknown optionlet volatility query")),
                },
            )
        })
    }
}
/// Family 0 swaption, 1 optionlet. Action: 0 reference date, 1 extrapolation state, 2 enable, 3 disable.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rate_vol_control(
    ctx: *mut Context,
    id: u64,
    family: i32,
    action: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            macro_rules! control {
                ($v:expr) => {{
                    let v = $v;
                    match action {
                        0 => v.reference_date()?.serial_number(),
                        1 => i32::from(v.allows_extrapolation()),
                        2 => {
                            v.enable_extrapolation();
                            1
                        }
                        3 => {
                            v.disable_extrapolation();
                            0
                        }
                        _ => return Err(BindingError::invalid("unknown rate volatility action")),
                    }
                }};
            }
            output(
                out,
                match family {
                    0 => control!(
                        c.get::<Handle<dyn SwaptionVolatilityStructure>>(id)?
                            .current_link()?
                    ),
                    1 => control!(
                        c.get::<Handle<dyn OptionletVolatilityStructure>>(id)?
                            .current_link()?
                    ),
                    _ => return Err(BindingError::invalid("unknown rate volatility family")),
                },
            )
        })
    }
}
