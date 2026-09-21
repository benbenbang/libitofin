//! Fallible Poisson generator construction, copying and drawing.

use crate::boundary::*;
use crate::rng_api::check_buffer;
use crate::rng_sequence_api::check_dimension;
use libitofin::math::randomnumbers::{PoissonPseudoRandom, PoissonRsg};
use libitofin::shared::{SharedMut, shared_mut};

/// Construct a Poisson sequence. Dimension one also supplies scalar draws.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_poisson_rng_new(
    ctx: *mut Context,
    dimension: usize,
    seed: u32,
    lambda: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_dimension(dimension)?;
            output(
                out,
                c.insert(shared_mut(PoissonPseudoRandom::with_lambda(
                    dimension, seed, lambda,
                )?))?,
            )
        })
    }
}

/// Copy a generator's current state into an independent native owner.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_poisson_rng_copy(
    ctx: *mut Context,
    source: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let copy = c.get::<SharedMut<PoissonRsg>>(source)?.borrow().clone();
            output(out, c.insert(shared_mut(copy))?)
        })
    }
}

/// Draw one sequence when last=0, or copy the last success when last=1.
/// Errors preserve output and the last successful sequence; attempted draws advance state.
/// # Safety
/// Follow the crate-level C caller contract; out has capacity doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_poisson_rng_draw(
    ctx: *mut Context,
    id: u64,
    last: i32,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !(0..=1).contains(&last) {
                return Err(BindingError::invalid("invalid last flag"));
            }
            let generator = c.get::<SharedMut<PoissonRsg>>(id)?;
            let mut generator = generator.borrow_mut();
            check_buffer(out, generator.dimension(), capacity)?;
            let sample = if last == 1 {
                generator.last_sequence()
            } else {
                generator.next_sequence()?
            };
            std::ptr::copy_nonoverlapping(sample.value.as_ptr(), out, sample.value.len());
            Ok(())
        })
    }
}
