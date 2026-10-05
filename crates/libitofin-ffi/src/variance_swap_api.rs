//! Concrete variance swaps and finite option-strip replication.
use crate::boundary::*;
use crate::time_api::date;
use libitofin::instrument::Instrument;
use libitofin::instruments::VarianceSwap;
use libitofin::option::OptionType;
use libitofin::position::Position;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{MAX_VARIANCE_SWAP_STRIKES, ReplicatingVarianceSwapEngine};
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::date::Date;

#[derive(Clone)]
struct EngineOwner {
    engine: SharedMut<dyn PricingEngine>,
    count: usize,
}
struct SwapOwner {
    swap: VarianceSwap,
    count: Option<usize>,
}

fn valid_error(error: *mut ItofinError) -> bool {
    error.is_null() || check_ptr(error).is_ok()
}
fn strike_count(values: &[f64]) -> usize {
    values
        .iter()
        .enumerate()
        .filter(|(i, v)| !values[..*i].contains(v))
        .count()
}

/// Variance units, positive notional and start-before-maturity are required.
/// Position is zero long or one short. Settings are retained.
/// # Safety
/// Pointers follow the crate-level C caller contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_variance_swap_new(
    ctx: *mut Context,
    position: i32,
    strike: f64,
    notional: f64,
    start: i32,
    maturity: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let position = match position {
                0 => Position::Long,
                1 => Position::Short,
                _ => return Err(BindingError::invalid("invalid position")),
            };
            let swap = VarianceSwap::new(
                position,
                strike,
                notional,
                date(start)?,
                date(maturity)?,
                c.get::<Shared<Settings<Date>>>(settings)?,
            )?;
            output(out, c.insert(shared_mut(SwapOwner { swap, count: None }))?)
        })
    }
}

/// Each strip has 2..4096 raw entries with at least two distinct strikes.
/// Finite positive dk and an exact shared call/put boundary are required.
/// # Safety
/// Arrays must be live for the stated lengths and outputs non-overlapping.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_replicating_variance_swap_engine_new(
    ctx: *mut Context,
    process: u64,
    dk: f64,
    calls: *const f64,
    calls_len: usize,
    puts: *const f64,
    puts_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !(2..=MAX_VARIANCE_SWAP_STRIKES).contains(&calls_len)
                || !(2..=MAX_VARIANCE_SWAP_STRIKES).contains(&puts_len)
            {
                return Err(BindingError::invalid(
                    "variance-swap strip length out of bounds",
                ));
            }
            let calls = input_slice(calls, calls_len)?;
            let puts = input_slice(puts, puts_len)?;
            let engine: SharedMut<dyn PricingEngine> =
                shared_mut(ReplicatingVarianceSwapEngine::new(
                    c.get::<Shared<GeneralizedBlackScholesProcess>>(process)?,
                    dk,
                    calls,
                    puts,
                )?);
            let count = strike_count(calls) + strike_count(puts);
            output(out, c.insert(EngineOwner { engine, count })?)
        })
    }
}

/// Attach only a replicating variance-swap engine; both owners are retained.
/// # Safety
/// Context, handles and error follow the crate-level caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_set_engine(
    ctx: *mut Context,
    swap: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            let engine = c.get::<EngineOwner>(engine)?;
            let mut owner = owner.borrow_mut();
            owner.swap.base_mut().set_pricing_engine(engine.engine);
            owner.count = Some(engine.count);
            Ok(())
        })
    }
}

/// Scalar field: zero NPV, one fair annualized variance, two strike, three notional.
/// # Safety
/// Outputs follow the crate-level caller contract and stay unchanged on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_value(
    ctx: *mut Context,
    swap: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            let mut owner = owner.borrow_mut();
            let value = match field {
                0 => owner.swap.npv()?,
                1 => owner.swap.variance()?,
                2 => owner.swap.strike(),
                3 => owner.swap.notional(),
                _ => return Err(BindingError::invalid("invalid variance-swap scalar field")),
            };
            output(out, value)
        })
    }
}

/// Integer field: zero position, one start serial, two maturity serial,
/// three calculated flag, four expired flag.
/// # Safety
/// Outputs follow the crate-level caller contract and stay unchanged on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_integer(
    ctx: *mut Context,
    swap: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            let owner = owner.borrow();
            let value = match field {
                0 => match owner.swap.position() {
                    Position::Long => 0,
                    Position::Short => 1,
                },
                1 => owner.swap.start_date().serial_number(),
                2 => owner.swap.maturity_date().serial_number(),
                3 => i32::from(owner.swap.base().is_calculated()),
                4 => i32::from(owner.swap.is_expired()?),
                _ => return Err(BindingError::invalid("invalid variance-swap integer field")),
            };
            output(out, value)
        })
    }
}

/// Force recalculation. Invalid market inputs return an error, not stale results.
/// # Safety
/// Context, handles and error follow the crate-level caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_recalculate(
    ctx: *mut Context,
    swap: u64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<SwapOwner>>(swap)?
                .borrow_mut()
                .swap
                .recalculate()?;
            Ok(())
        })
    }
}

/// Return the exact current weight count; unavailable after expiry.
/// # Safety
/// Outputs follow the crate-level caller contract and stay unchanged on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_weights_count(
    ctx: *mut Context,
    swap: u64,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            let mut owner = owner.borrow_mut();
            output(out, owner.swap.option_weights()?.len())
        })
    }
}

/// Copy fresh typed weights into exact-size arrays. Types are zero call, one put.
/// Length and pointer errors are rejected before lazy calculation.
/// Weights are unavailable after expiry.
/// # Safety
/// Arrays must be writable, aligned, exact-size and non-overlapping.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_variance_swap_weights(
    ctx: *mut Context,
    swap: u64,
    kinds: *mut i32,
    strikes: *mut f64,
    weights: *mut f64,
    len: usize,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            let mut owner = owner.borrow_mut();
            let expected = owner
                .count
                .ok_or_else(|| BindingError::invalid("no variance-swap engine"))?;
            if len != expected {
                return Err(BindingError::invalid(
                    "variance-swap weight length mismatch",
                ));
            }
            if len != 0 {
                check_ptr(kinds)?;
                check_ptr(strikes)?;
                check_ptr(weights)?;
            }
            let values = owner.swap.option_weights()?;
            if values.len() != len {
                return Err(BindingError::invalid("variance-swap weight count changed"));
            }
            for (i, value) in values.iter().enumerate() {
                kinds.add(i).write(match value.option_type {
                    OptionType::Call => 0,
                    OptionType::Put => 1,
                });
                strikes.add(i).write(value.strike);
                weights.add(i).write(value.weight);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "variance_swap_api_tests.rs"]
mod tests;
