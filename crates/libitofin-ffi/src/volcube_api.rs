//! Interpolated and SABR swaption cubes retain both base surface and ATM access.
use crate::boundary::*;
use crate::market_api::quote;
use crate::settings_api::settings;
use crate::time_api::time_unit;
use crate::volgrid_api::periods;
use libitofin::handle::Handle;
use libitofin::indexes::swapindex::SwapIndex;
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    InterpolatedSwaptionVolatilityCube, SabrSwaptionVolatilityCube, SwaptionVolatilityStructure,
};
use libitofin::time::period::Period;
#[derive(Clone)]
enum Cube {
    Interpolated(Shared<InterpolatedSwaptionVolatilityCube>),
    Sabr(Shared<SabrSwaptionVolatilityCube>),
}
#[repr(C)]
pub struct ItofinVolCubeHandles {
    pub surface: u64,
    pub cube: u64,
}
#[repr(C)]
pub struct ItofinVolCubeConfig {
    pub atm: u64,
    pub index: u64,
    pub short_index: u64,
    pub settings: u64,
    pub option_lengths: *const i32,
    pub option_units: *const i32,
    pub options: usize,
    pub swap_lengths: *const i32,
    pub swap_units: *const i32,
    pub swaps: usize,
    pub strike_spreads: *const f64,
    pub strikes: usize,
    pub vol_spreads: *const u64,
    pub vol_count: usize,
    pub guesses: *const u64,
    pub guess_count: usize,
    pub fixed: *const i32,
    pub atm_calibrated: i32,
    pub vega_weighted: i32,
    pub use_max_error: i32,
    pub max_guesses: usize,
    pub cutoff_strike: f64,
}
fn flag(x: i32) -> BindingResult<bool> {
    match x {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("expected boolean 0 or 1")),
    }
}
unsafe fn quote_grid(
    c: &Context,
    p: *const u64,
    count: usize,
    cols: usize,
) -> BindingResult<Vec<Vec<Handle<dyn Quote>>>> {
    unsafe { input_slice(p, count)? }
        .chunks(cols)
        .map(|r| r.iter().map(|&id| quote(c, id)).collect())
        .collect()
}
/// Kind: 0 interpolated, 1 SABR. Both returned handles must be released.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_cube_new(
    ctx: *mut Context,
    kind: i32,
    cfg: *const ItofinVolCubeConfig,
    out: *mut ItofinVolCubeHandles,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let x = &*cfg;
            let nodes = x
                .options
                .checked_mul(x.swaps)
                .ok_or_else(|| BindingError::invalid("cube size overflow"))?;
            if nodes == 0 || x.strikes == 0 || nodes.checked_mul(x.strikes) != Some(x.vol_count) {
                return Err(BindingError::invalid(
                    "vol spreads must have one row per tenor pair and column per strike",
                ));
            }
            let options = periods(x.option_lengths, x.option_units, x.options)?;
            let swaps = periods(x.swap_lengths, x.swap_units, x.swaps)?;
            let strikes = input_slice(x.strike_spreads, x.strikes)?.to_vec();
            if strikes.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("strike spreads must be finite"));
            }
            let vols = quote_grid(c, x.vol_spreads, x.vol_count, x.strikes)?;
            let atm = c.get::<Handle<dyn SwaptionVolatilityStructure>>(x.atm)?;
            let index = c.get::<Shared<SwapIndex>>(x.index)?;
            let short = c.get::<Shared<SwapIndex>>(x.short_index)?;
            let settings = settings(c, x.settings)?;
            let vega = flag(x.vega_weighted)?;
            let (surface, cube) = match kind {
                0 => {
                    let v = shared(InterpolatedSwaptionVolatilityCube::new(
                        atm, options, swaps, strikes, vols, index, short, vega, settings,
                    )?);
                    (
                        Handle::new(v.clone() as Shared<dyn SwaptionVolatilityStructure>),
                        Cube::Interpolated(v),
                    )
                }
                1 => {
                    if nodes.checked_mul(4) != Some(x.guess_count)
                        || !x.cutoff_strike.is_finite()
                        || x.max_guesses == 0
                    {
                        return Err(BindingError::invalid(
                            "invalid SABR calibration configuration",
                        ));
                    }
                    let f = input_slice(x.fixed, 4)?;
                    let fixed = [flag(f[0])?, flag(f[1])?, flag(f[2])?, flag(f[3])?];
                    let v = shared(SabrSwaptionVolatilityCube::new(
                        atm,
                        options,
                        swaps,
                        strikes,
                        vols,
                        index,
                        short,
                        vega,
                        quote_grid(c, x.guesses, x.guess_count, 4)?,
                        fixed,
                        flag(x.atm_calibrated)?,
                        None,
                        None,
                        None,
                        None,
                        flag(x.use_max_error)?,
                        x.max_guesses,
                        false,
                        x.cutoff_strike,
                        settings,
                    )?);
                    (
                        Handle::new(v.clone() as Shared<dyn SwaptionVolatilityStructure>),
                        Cube::Sabr(v),
                    )
                }
                _ => return Err(BindingError::invalid("unknown cube type")),
            };
            let surface = c.insert(surface)?;
            let cube = c.insert(cube)?;
            output(out, ItofinVolCubeHandles { surface, cube })
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_cube_atm(
    ctx: *mut Context,
    id: u64,
    option_length: i32,
    option_unit: i32,
    swap_length: i32,
    swap_unit: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let o = Period::new(option_length, time_unit(option_unit)?);
            let s = Period::new(swap_length, time_unit(swap_unit)?);
            let result = match c.get::<Cube>(id)? {
                Cube::Interpolated(v) => v.cube().atm_strike_from_tenor(o, s)?,
                Cube::Sabr(v) => v.cube().atm_strike_from_tenor(o, s)?,
            };
            output(out, result)
        })
    }
}
