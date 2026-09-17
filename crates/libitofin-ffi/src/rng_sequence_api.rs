use crate::boundary::*;
use crate::rng_api::{check_buffer, uniform};
use crate::rng_gaussian_sobol::GaussianSobol;
use crate::rng_low_discrepancy::SobolState;
use libitofin::math::distributions::normal::InverseCumulativeNormal;
use libitofin::math::randomnumbers::HaltonRsg;
use libitofin::math::randomnumbers::rngtraits::SequenceGenerator;
use libitofin::math::randomnumbers::{
    InverseCumulativeRsg, MersenneTwisterUniformRng, RandomSequenceGenerator,
};
use libitofin::shared::{SharedMut, shared_mut};

type Uniform = RandomSequenceGenerator<MersenneTwisterUniformRng>;
type Gaussian = InverseCumulativeRsg<Uniform, InverseCumulativeNormal>;

pub(crate) enum Sequence {
    Uniform(Box<Uniform>),
    Gaussian(Box<Gaussian>),
    Sobol(Box<SobolState>),
    Halton(HaltonRsg),
    GaussianSobol(GaussianSobol),
}
impl Sequence {
    fn check_draws(&self, count: usize) -> BindingResult<()> {
        match self {
            Self::Sobol(r) => r.check_draws(count),
            Self::GaussianSobol(r) => r.source.check_draws(count),
            _ => Ok(()),
        }
    }
    fn dimension(&self) -> usize {
        match self {
            Self::Uniform(r) => r.dimension(),
            Self::Gaussian(r) => r.dimension(),
            Self::Sobol(r) => r.inner.dimension(),
            Self::Halton(r) => r.dimension(),
            Self::GaussianSobol(r) => r.last.len(),
        }
    }
    fn last(&self) -> &[f64] {
        match self {
            Self::Uniform(r) => &r.last_sequence().value,
            Self::Gaussian(r) => &r.last_sequence().value,
            Self::Sobol(r) => r.inner.last_sequence(),
            Self::Halton(r) => r.last_sequence(),
            Self::GaussianSobol(r) => &r.last,
        }
    }
    fn next(&mut self) -> &[f64] {
        match self {
            Self::Uniform(r) => &r.next_sequence().value,
            Self::Gaussian(r) => &r.next_sequence().value,
            Self::Sobol(r) => r.next(),
            Self::Halton(r) => r.next_sequence(),
            Self::GaussianSobol(r) => r.next(),
        }
    }
}

pub(crate) fn check_dimension(dimension: usize) -> BindingResult<()> {
    if dimension == 0 || dimension > 16 * 1024 * 1024 {
        return Err(BindingError::invalid("dimension outside [1, 16777216]"));
    }
    Ok(())
}

/// Copies source state: uniform takes a scalar, Gaussian takes a uniform sequence.
/// A zero source constructs a seeded generator using dimension and seed.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_random_sequence_new(
    ctx: *mut Context,
    source: u64,
    dimension: usize,
    seed: u32,
    gaussian: bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let usg = if source != 0 && gaussian {
                let source = c.get::<SharedMut<Sequence>>(source)?;
                match &*source.borrow() {
                    Sequence::Uniform(r) => (**r).clone(),
                    _ => return Err(BindingError::invalid("source must be a uniform sequence")),
                }
            } else {
                check_dimension(dimension)?;
                if source == 0 {
                    Uniform::with_seed(dimension, seed)?
                } else {
                    Uniform::new(dimension, uniform(c, source)?)?
                }
            };
            let rsg = if gaussian {
                Sequence::Gaussian(Box::new(Gaussian::new(
                    usg,
                    InverseCumulativeNormal::standard(),
                )))
            } else {
                Sequence::Uniform(Box::new(usg))
            };
            output(out, c.insert(shared_mut(rsg))?)
        })
    }
}

/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rng_sequence_dimension(
    ctx: *mut Context,
    id: u64,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(out, c.get::<SharedMut<Sequence>>(id)?.borrow().dimension())
        })
    }
}

/// Draws count rows; last=true copies the last row without drawing and requires count=1.
/// # Safety
/// Follow the crate-level C caller contract; out contains capacity doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rng_sequence_draw(
    ctx: *mut Context,
    id: u64,
    count: usize,
    last: bool,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let rsg = c.get::<SharedMut<Sequence>>(id)?;
            let mut rsg = rsg.borrow_mut();
            let dimension = rsg.dimension();
            let len = count
                .checked_mul(dimension)
                .ok_or_else(|| BindingError::invalid("sequence size overflow"))?;
            check_buffer(out, len, capacity)?;
            if last && count != 1 {
                return Err(BindingError::invalid("last sequence requires one row"));
            }
            if !last {
                rsg.check_draws(count)?;
            }
            for row in 0..count {
                let values = if last { rsg.last() } else { rsg.next() };
                std::ptr::copy_nonoverlapping(values.as_ptr(), out.add(row * dimension), dimension);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn sequence_buffers_reject_invalid_sizes_without_advancing() {
        let mut c = Context::new();
        let mut id = 0;
        let mut values = [0.0; 3];
        unsafe {
            assert_eq!(
                itofin_random_sequence_new(&mut c, 0, 3, 42, false, &mut id, null_mut()),
                0
            );
            for (count, capacity) in [(1, 2), (usize::MAX, 3)] {
                assert_eq!(
                    itofin_rng_sequence_draw(
                        &mut c,
                        id,
                        count,
                        false,
                        values.as_mut_ptr(),
                        capacity,
                        null_mut()
                    ),
                    INVALID_ARGUMENT
                );
            }
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 0, false, null_mut(), 0, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, true, values.as_mut_ptr(), 3, null_mut()),
                0
            );
            assert_eq!(values, [0.0; 3]);
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, false, values.as_mut_ptr(), 3, null_mut()),
                0
            );
        }
        assert_eq!(
            values.as_slice(),
            Uniform::with_seed(3, 42).unwrap().next_sequence().value
        );
    }
}
