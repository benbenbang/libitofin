//! Immutable conventional-OAS clean-price sensitivities.
use crate::bond_analytics_api::compounding;
use crate::bond_api::NativeBond;
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::callable_tree_api::NativeCallableTreeEngine;
use crate::rates_api::finite;
use crate::time_api::{day_counter, frequency};
use libitofin::shared::Shared;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCallableBondRiskConfig {
    pub tree_engine: u64,
    pub day_counter: u64,
    /// Simple=0, Compounded=1, Continuous=2, SimpleThenCompounded=3, CompoundedThenSimple=4.
    pub compounding: i32,
    /// Annual=0, Semiannual=1, Quarterly=2, Monthly=3.
    pub frequency: i32,
    /// Bit 1 selects an explicit positive bump; zero preserves native None (2e-4).
    pub flags: u32,
    pub bump: f64,
}

fn risk(
    c: &Context,
    id: u64,
    spread: f64,
    a: ItofinCallableBondRiskConfig,
    convexity: bool,
) -> BindingResult<f64> {
    let spread = finite(spread)?;
    if a.flags & !1 != 0 {
        return Err(BindingError::invalid("invalid risk presence flags"));
    }
    let bump = if a.flags & 1 != 0 {
        let value = finite(a.bump)?;
        if value <= 0.0 {
            return Err(BindingError::invalid("bump must be positive"));
        }
        Some(value)
    } else {
        None
    };
    let dc = day_counter(c, a.day_counter)?;
    let compounding = compounding(a.compounding)?;
    let frequency = frequency(a.frequency)?;
    let bond = c.get::<NativeBond>(id)?;
    let engine = c.get::<NativeCallableTreeEngine>(a.tree_engine)?;
    if matches!(&bond.bond, BondBacking::Plain(_)) {
        return Err(BindingError::invalid("risk requires a callable bond"));
    }
    if !Shared::ptr_eq(&bond.bond.borrow().settings_handle(), &engine.settings) {
        return Err(BindingError::invalid(
            "bond and tree engine settings differ",
        ));
    }
    bond.settlement(None)?;
    let engine = engine.engine.borrow();
    let value = match &bond.bond {
        BondBacking::CallableFixed(bond) => {
            let bond = bond.borrow();
            if convexity {
                bond.effective_convexity(spread, &engine, dc, compounding, frequency, bump)?
            } else {
                bond.effective_duration(spread, &engine, dc, compounding, frequency, bump)?
            }
        }
        BondBacking::CallableZero(bond) => {
            let bond = bond.borrow();
            if convexity {
                bond.effective_convexity(spread, &engine, dc, compounding, frequency, bump)?
            } else {
                bond.effective_duration(spread, &engine, dc, compounding, frequency, bump)?
            }
        }
        BondBacking::Plain(_) => {
            return Err(BindingError::invalid("risk requires a callable bond"));
        }
    };
    finite(value)
}

#[unsafe(no_mangle)]
/// Effective clean-price duration from the explicit immutable tree, without installed-engine changes.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_effective_duration(
    ctx: *mut Context,
    id: u64,
    spread: f64,
    config: ItofinCallableBondRiskConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, risk(c, id, spread, config, false)?)
        })
    }
}

#[unsafe(no_mangle)]
/// Effective clean-price convexity from the explicit immutable tree, without installed-engine changes.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_effective_convexity(
    ctx: *mut Context,
    id: u64,
    spread: f64,
    config: ItofinCallableBondRiskConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, risk(c, id, spread, config, true)?)
        })
    }
}

#[cfg(test)]
#[path = "callable_risk_api_tests.rs"]
mod tests;
