use crate::boundary::*;
use crate::rng_api::check_buffer;
use crate::rng_sequence_api::{Sequence, check_dimension};
use libitofin::math::randomnumbers::HaltonRsg;
use libitofin::math::randomnumbers::sobol::{DirectionIntegers, PPMT_MAX_DIM, SobolRsg};
use libitofin::shared::{SharedMut, shared_mut};

#[derive(Clone)]
pub(crate) struct SobolState {
    pub(crate) inner: SobolRsg,
    counter: u64,
    first: bool,
    gray: bool,
}
impl SobolState {
    pub(crate) fn check_draws(&self, count: usize) -> BindingResult<()> {
        let limit = u64::from(u32::MAX) - u64::from(self.gray);
        let remaining = limit - self.counter + u64::from(self.first);
        if count as u128 > u128::from(remaining) {
            return Err(BindingError::invalid("Sobol sequence period exceeded"));
        }
        Ok(())
    }
    fn advance(&mut self) {
        if self.first {
            self.first = false;
        } else {
            self.counter += 1;
        }
    }
    pub(crate) fn next(&mut self) -> &[f64] {
        self.advance();
        self.inner.next_sequence()
    }
    fn integers(&mut self) -> &[u32] {
        self.advance();
        self.inner.next_int32_sequence()
    }
}

/// kind=0: Sobol; kind=1: deterministic Halton. Tables 0..9 match Python DirectionIntegers.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_low_discrepancy_new(
    ctx: *mut Context,
    kind: i32,
    dimension: usize,
    seed: u64,
    table: i32,
    gray: bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_dimension(dimension)?;
            let rsg = match kind {
                0 => {
                    if dimension > PPMT_MAX_DIM {
                        return Err(BindingError::invalid("Sobol dimension exceeds 21200"));
                    }
                    let table = match table {
                        0 => DirectionIntegers::Unit,
                        1 => DirectionIntegers::Jaeckel,
                        2 => DirectionIntegers::SobolLevitan,
                        3 => DirectionIntegers::SobolLevitanLemieux,
                        4 => DirectionIntegers::JoeKuoD5,
                        5 => DirectionIntegers::JoeKuoD6,
                        6 => DirectionIntegers::JoeKuoD7,
                        7 => DirectionIntegers::Kuo,
                        8 => DirectionIntegers::Kuo2,
                        9 => DirectionIntegers::Kuo3,
                        _ => return Err(BindingError::invalid("invalid direction integer table")),
                    };
                    Sequence::Sobol(Box::new(SobolState {
                        inner: SobolRsg::with_gray_code(dimension, seed, table, gray),
                        counter: 0,
                        first: true,
                        gray,
                    }))
                }
                1 => Sequence::Halton(HaltonRsg::new(dimension)?),
                _ => return Err(BindingError::invalid("invalid low discrepancy generator")),
            };
            output(out, c.insert(shared_mut(rsg))?)
        })
    }
}

/// skip=true positions the counter at index; otherwise draws raw Sobol integers.
/// # Safety
/// Follow the crate-level C caller contract; out contains capacity words.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_sobol_integers(
    ctx: *mut Context,
    id: u64,
    skip: bool,
    index: u32,
    out: *mut u32,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let rsg = c.get::<SharedMut<Sequence>>(id)?;
            let mut rsg = rsg.borrow_mut();
            let Sequence::Sobol(rsg) = &mut *rsg else {
                return Err(BindingError::invalid("source must be Sobol"));
            };
            let dimension = rsg.inner.dimension();
            check_buffer(out, dimension, capacity)?;
            let values = if skip {
                if index == u32::MAX {
                    return Err(BindingError::invalid("skip exceeds Sobol period"));
                }
                rsg.counter = u64::from(index);
                rsg.inner.skip_to(index)
            } else {
                rsg.check_draws(1)?;
                rsg.integers()
            };
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, dimension);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng_sequence_api::itofin_rng_sequence_draw;
    use std::ptr::null_mut;

    #[test]
    fn sobol_period_and_invalid_buffers_preserve_state() {
        let mut c = Context::new();
        let mut id = 0;
        let mut word = 0;
        let mut values = [99.0; 2];
        unsafe {
            assert_eq!(
                itofin_low_discrepancy_new(&mut c, 0, 1, 0, 1, true, &mut id, null_mut()),
                0
            );
            assert_eq!(
                itofin_sobol_integers(&mut c, id, true, 3, &mut word, 0, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_sobol_integers(&mut c, id, false, 0, &mut word, 1, null_mut()),
                0
            );
            assert_eq!(word, 1 << 31);
            assert_eq!(
                itofin_sobol_integers(&mut c, id, true, u32::MAX - 2, &mut word, 1, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 2, false, values.as_mut_ptr(), 2, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(values, [99.0; 2]);
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, true, values.as_mut_ptr(), 1, null_mut()),
                0
            );
            assert_eq!(values[0], 0.0);
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, false, values.as_mut_ptr(), 1, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, false, values.as_mut_ptr(), 1, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_sobol_integers(&mut c, id, true, 0, &mut word, 1, null_mut()),
                0
            );
            assert_eq!(
                itofin_rng_sequence_draw(&mut c, id, 1, false, values.as_mut_ptr(), 1, null_mut()),
                0
            );
        }
    }
}
