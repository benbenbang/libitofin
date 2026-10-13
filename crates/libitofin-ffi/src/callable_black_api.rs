//! Live relative forward-yield-volatility Black pricing and immutable inversion.
use crate::bond_api::NativeBond;
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::market_api::quote;
use crate::rates_api::{curve, finite, settings};
use libitofin::instruments::BondPrice;
use libitofin::pricingengines::bond::BlackCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::date::Date;

#[derive(Clone)]
pub(crate) struct NativeCallableBlackEngine {
    pub(crate) engine: SharedMut<BlackCallableFixedRateBondEngine>,
    pub(crate) settings: Shared<Settings<Date>>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCallableImpliedVolatilityConfig {
    /// Presence bits: accuracy=1, max_evaluations=2, min_vol=4, max_vol=8.
    pub flags: u32,
    pub accuracy: f64,
    pub max_evaluations: u64,
    pub min_vol: f64,
    pub max_vol: f64,
}

fn controls(a: ItofinCallableImpliedVolatilityConfig) -> BindingResult<(f64, usize, f64, f64)> {
    if a.flags & !15 != 0 {
        return Err(BindingError::invalid(
            "invalid implied volatility presence flags",
        ));
    }
    let accuracy = if a.flags & 1 != 0 {
        finite(a.accuracy)?
    } else {
        1e-10
    };
    let evaluations = if a.flags & 2 != 0 {
        usize::try_from(a.max_evaluations)
            .map_err(|_| BindingError::invalid("evaluation limit overflow"))?
    } else {
        100
    };
    let min_vol = if a.flags & 4 != 0 {
        finite(a.min_vol)?
    } else {
        0.0
    };
    let max_vol = if a.flags & 8 != 0 {
        finite(a.max_vol)?
    } else {
        1.0
    };
    if accuracy <= 0.0 || evaluations == 0 || min_vol < 0.0 || max_vol <= min_vol {
        return Err(BindingError::invalid("invalid implied volatility controls"));
    }
    Ok((accuracy, evaluations, min_vol, max_vol))
}

#[unsafe(no_mangle)]
/// Construct with live quote/curve graphs, without eager market valuation.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_black_callable_bond_engine_new(
    ctx: *mut Context,
    yield_volatility: u64,
    discount: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let volatility = quote(c, yield_volatility)?;
            let discount = curve(c, discount)?;
            let settings = settings(c, settings_id)?;
            let engine = shared_mut(BlackCallableFixedRateBondEngine::new(
                volatility,
                discount,
                settings.clone(),
            ));
            output(
                out,
                c.insert(NativeCallableBlackEngine { engine, settings })?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// Install only on actual callable products sharing the original Settings.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_set_black_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let bond = c.get::<NativeBond>(id)?;
            let engine = c.get::<NativeCallableBlackEngine>(engine)?;
            if matches!(&bond.bond, BondBacking::Plain(_)) {
                return Err(BindingError::invalid(
                    "Black engine requires a callable bond",
                ));
            }
            if !Shared::ptr_eq(&bond.bond.borrow().settings_handle(), &engine.settings) {
                return Err(BindingError::invalid(
                    "bond and Black engine settings differ",
                ));
            }
            bond.bond.with_instrument_mut(|bond| {
                bond.base_mut().set_pricing_engine(engine.engine);
            });
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
/// Invert a settlement quote per 100 without changing the installed engine or cache.
/// Price types are Clean=0 and Dirty=1; absent controls use 1e-10, 100, 0, 1.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_implied_volatility(
    ctx: *mut Context,
    id: u64,
    target: f64,
    price_type: i32,
    discount: u64,
    a: ItofinCallableImpliedVolatilityConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let target = finite(target)?;
            let target = match price_type {
                0 => BondPrice::Clean(target),
                1 => BondPrice::Dirty(target),
                _ => return Err(BindingError::invalid("invalid bond price type")),
            };
            let (accuracy, evaluations, min_vol, max_vol) = controls(a)?;
            let discount = curve(c, discount)?;
            let bond = c.get::<NativeBond>(id)?;
            if matches!(&bond.bond, BondBacking::Plain(_)) {
                return Err(BindingError::invalid(
                    "implied volatility requires a callable bond",
                ));
            }
            bond.settlement(None)?;
            let value = match &bond.bond {
                BondBacking::Plain(_) => {
                    return Err(BindingError::invalid(
                        "implied volatility requires a callable bond",
                    ));
                }
                BondBacking::CallableFixed(bond) => bond.borrow().implied_volatility(
                    target,
                    discount,
                    accuracy,
                    evaluations,
                    min_vol,
                    max_vol,
                )?,
                BondBacking::CallableZero(bond) => bond.borrow().implied_volatility(
                    target,
                    discount,
                    accuracy,
                    evaluations,
                    min_vol,
                    max_vol,
                )?,
            };
            output(out, finite(value)?)
        })
    }
}

#[cfg(test)]
#[path = "callable_black_api_tests.rs"]
mod tests;
