//! Immutable explicit-tree option-adjusted spread analytics for callable bonds.
use crate::bond_analytics_api::{ItofinAnalyticsSolverConfig, compounding};
use crate::bond_api::NativeBond;
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::callable_tree_api::NativeCallableTreeEngine;
use crate::rates_api::finite;
use crate::time_api::{date, day_counter, frequency};
use libitofin::shared::Shared;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCallableOASConfig {
    pub day_counter: u64,
    /// Existing compounding tags 0 through 4.
    pub compounding: i32,
    /// Annual=0, Semiannual=1, Quarterly=2, Monthly=3.
    pub frequency: i32,
    /// Bit 1 supplies an actual unadjusted settlement; other bits reject.
    pub settlement_flags: u32,
    pub settlement: i32,
}
fn solver(a: ItofinAnalyticsSolverConfig) -> BindingResult<(f64, usize, f64)> {
    if a.flags & !7 != 0 {
        return Err(BindingError::invalid("invalid OAS solver presence flags"));
    }
    let accuracy = if a.flags & 1 != 0 {
        finite(a.accuracy)?
    } else {
        1e-10
    };
    let count = if a.flags & 2 != 0 {
        usize::try_from(a.max_evaluations)
            .map_err(|_| BindingError::invalid("evaluation limit overflow"))?
    } else {
        100
    };
    let guess = if a.flags & 4 != 0 {
        finite(a.guess)?
    } else {
        0.0
    };
    if accuracy <= 0.0 || count == 0 {
        return Err(BindingError::invalid("invalid OAS solver controls"));
    }
    Ok((accuracy, count, guess))
}
fn probe(
    c: &Context,
    id: u64,
    engine: u64,
    value: f64,
    a: ItofinCallableOASConfig,
    solve: Option<ItofinAnalyticsSolverConfig>,
) -> BindingResult<f64> {
    let value = finite(value)?;
    let bond = c.get::<NativeBond>(id)?;
    let engine = c.get::<NativeCallableTreeEngine>(engine)?;
    if matches!(&bond.bond, BondBacking::Plain(_)) {
        return Err(BindingError::invalid("OAS requires a callable bond"));
    }
    if !Shared::ptr_eq(&bond.bond.borrow().settings_handle(), &engine.settings) {
        return Err(BindingError::invalid(
            "bond and tree engine settings differ",
        ));
    }
    if a.settlement_flags & !1 != 0 {
        return Err(BindingError::invalid(
            "invalid OAS settlement presence flags",
        ));
    }
    let settlement = if a.settlement_flags & 1 != 0 {
        Some(date(a.settlement)?)
    } else {
        bond.settlement(None)?;
        None
    };
    let dc = day_counter(c, a.day_counter)?;
    let comp = compounding(a.compounding)?;
    let freq = frequency(a.frequency)?;
    let controls = solve.map(solver).transpose()?;
    let engine = engine.engine.borrow();
    macro_rules! price {
        ($b:expr) => {{
            let b = $b.borrow();
            if let Some((accuracy, count, guess)) = controls {
                b.oas(
                    value, &engine, dc, comp, freq, settlement, accuracy, count, guess,
                )?
            } else {
                b.clean_price_oas(value, &engine, dc, comp, freq, settlement)?
            }
        }};
    }
    finite(match &bond.bond {
        BondBacking::CallableFixed(b) => price!(b),
        BondBacking::CallableZero(b) => price!(b),
        BondBacking::Plain(_) => return Err(BindingError::invalid("OAS requires a callable bond")),
    })
}
#[unsafe(no_mangle)]
/// Pure clean settlement quote per 100 from a conventional decimal spread.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_clean_price_oas(
    ctx: *mut Context,
    bond: u64,
    spread: f64,
    engine: u64,
    config: ItofinCallableOASConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, probe(c, bond, engine, spread, config, None)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Recover conventional decimal OAS without altering bond or engine state.
/// Omitted solver controls use 1e-10, 100 and zero in continuous shift units.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_oas(
    ctx: *mut Context,
    bond: u64,
    clean_price: f64,
    engine: u64,
    config: ItofinCallableOASConfig,
    solver: ItofinAnalyticsSolverConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                probe(c, bond, engine, clean_price, config, Some(solver))?,
            )
        })
    }
}
#[cfg(test)]
#[path = "callable_oas_api_tests.rs"]
mod tests;
