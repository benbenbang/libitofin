//! Hull-White callable tree engines retaining native model and settings graphs.
use crate::bond_api::NativeBond;
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::rates_api::settings;
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengines::bond::TreeCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::date::Date;

#[derive(Clone)]
pub(crate) struct NativeCallableTreeEngine {
    pub(crate) engine: SharedMut<TreeCallableFixedRateBondEngine>,
    pub(crate) settings: Shared<Settings<Date>>,
}
#[unsafe(no_mangle)]
/// Construct without eager valuation. Time steps must be positive and fit usize.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_tree_callable_bond_engine_new(
    ctx: *mut Context,
    model: u64,
    time_steps: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let steps = usize::try_from(time_steps)
                .map_err(|_| BindingError::invalid("tree step count overflow"))?;
            if steps == 0 {
                return Err(BindingError::invalid("tree steps must be positive"));
            }
            let model = c.get::<SharedMut<HullWhite>>(model)?;
            let settings = settings(c, settings_id)?;
            let engine = shared_mut(TreeCallableFixedRateBondEngine::new(
                model,
                steps,
                settings.clone(),
            )?);
            output(
                out,
                c.insert(NativeCallableTreeEngine { engine, settings })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// Install only on an actual callable product sharing the exact engine Settings.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_set_tree_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let bond = c.get::<NativeBond>(id)?;
            let engine = c.get::<NativeCallableTreeEngine>(engine)?;
            if matches!(&bond.bond, BondBacking::Plain(_)) {
                return Err(BindingError::invalid(
                    "tree engine requires a callable bond",
                ));
            }
            if !Shared::ptr_eq(&bond.bond.borrow().settings_handle(), &engine.settings) {
                return Err(BindingError::invalid(
                    "bond and tree engine settings differ",
                ));
            }
            bond.bond.with_instrument_mut(|bond| {
                bond.base_mut().set_pricing_engine(engine.engine);
            });
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "callable_tree_api_tests.rs"]
mod tests;
