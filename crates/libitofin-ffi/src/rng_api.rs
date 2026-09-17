use crate::boundary::*;
use libitofin::math::randomnumbers::{
    BoxMullerGaussianRng, GaussianRng, MersenneTwisterUniformRng, UniformRng,
};
use libitofin::shared::{SharedMut, shared_mut};

type Gaussian = BoxMullerGaussianRng<MersenneTwisterUniformRng>;

pub(crate) fn uniform(c: &Context, id: u64) -> BindingResult<MersenneTwisterUniformRng> {
    Ok(c.get::<SharedMut<MersenneTwisterUniformRng>>(id)?
        .borrow()
        .clone())
}

pub(crate) fn check_buffer<T>(out: *mut T, len: usize, capacity: usize) -> BindingResult<()> {
    if capacity < len || len > (isize::MAX as usize) / std::mem::size_of::<T>().max(1) {
        return Err(BindingError::invalid(
            "output capacity too small or length overflow",
        ));
    }
    if len != 0 {
        check_ptr(out)?;
    }
    Ok(())
}

/// `use_seeds` selects a nonempty array; otherwise `seed` is used, with zero randomized.
/// # Safety
/// Follow the crate-level C caller contract; seeds contains len words.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_uniform_rng_new(
    ctx: *mut Context,
    seed: u32,
    seeds: *const u32,
    len: usize,
    use_seeds: bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let rng = if use_seeds {
                let seeds = input_slice(seeds, len)?;
                if seeds.is_empty() {
                    return Err(BindingError::invalid("empty seed array"));
                }
                MersenneTwisterUniformRng::from_seeds(seeds)
            } else {
                MersenneTwisterUniformRng::new(seed)
            };
            output(out, c.insert(shared_mut(rng))?)
        })
    }
}

/// Copies the uniform source state; source zero constructs a fresh seeded generator.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gaussian_rng_new(
    ctx: *mut Context,
    source: u64,
    seed: u32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let rng = if source == 0 {
                MersenneTwisterUniformRng::new(seed)
            } else {
                uniform(c, source)?
            };
            output(out, c.insert(shared_mut(Gaussian::new(rng)))?)
        })
    }
}

/// Draws count scalar values, preserving generator state between calls.
/// # Safety
/// Follow the crate-level C caller contract; out contains capacity doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rng_draw(
    ctx: *mut Context,
    id: u64,
    gaussian: bool,
    count: usize,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_buffer(out, count, capacity)?;
            if gaussian {
                let rng = c.get::<SharedMut<Gaussian>>(id)?;
                let mut rng = rng.borrow_mut();
                for i in 0..count {
                    out.add(i).write(rng.next_gaussian());
                }
            } else {
                let rng = c.get::<SharedMut<MersenneTwisterUniformRng>>(id)?;
                let mut rng = rng.borrow_mut();
                for i in 0..count {
                    out.add(i).write(rng.next_real());
                }
            }
            Ok(())
        })
    }
}

/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_uniform_rng_u32(
    ctx: *mut Context,
    id: u64,
    out: *mut u32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.get::<SharedMut<MersenneTwisterUniformRng>>(id)?
                    .borrow_mut()
                    .next_u32(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn scalar_rng_buffers_validate_before_advancing() {
        let mut c = Context::new();
        let mut id = 0;
        let mut actual = [0.0; 2];
        unsafe {
            assert_eq!(
                itofin_uniform_rng_new(&mut c, 42, std::ptr::null(), 0, false, &mut id, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_draw(&mut c, id, false, 2, actual.as_mut_ptr(), 1, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_rng_draw(&mut c, id, false, 1, null_mut(), 1, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_rng_draw(&mut c, id, true, 1, actual.as_mut_ptr(), 1, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_rng_draw(&mut c, id, false, 0, null_mut(), 0, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_draw(&mut c, id, false, 2, actual.as_mut_ptr(), 2, null_mut()),
                0
            );
        }
        let mut oracle = MersenneTwisterUniformRng::new(42);
        assert_eq!(actual, [oracle.next_real(), oracle.next_real()]);
        let mut other = Context::new();
        unsafe {
            assert_eq!(
                itofin_rng_draw(&mut other, id, false, 0, null_mut(), 0, null_mut()),
                INVALID_HANDLE
            );
        }
    }
}
