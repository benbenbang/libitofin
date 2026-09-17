use crate::boundary::*;
use crate::rng_low_discrepancy::SobolState;
use crate::rng_sequence_api::Sequence;
use libitofin::math::distributions::normal::InverseCumulativeNormal;
use libitofin::math::randomnumbers::rngtraits::InverseCumulative;
use libitofin::shared::{SharedMut, shared_mut};

pub(crate) struct GaussianSobol {
    pub(crate) source: SobolState,
    pub(crate) last: Vec<f64>,
}
impl GaussianSobol {
    pub(crate) fn next(&mut self) -> &[f64] {
        let inverse = InverseCumulativeNormal::standard();
        for (target, uniform) in self.last.iter_mut().zip(self.source.next()) {
            *target = inverse.evaluate(*uniform);
        }
        &self.last
    }
}

/// Copies Sobol state, including its current position; subsequent draws are independent.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gaussian_sobol_new(
    ctx: *mut Context,
    source: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let source = c.get::<SharedMut<Sequence>>(source)?;
            let source = match &*source.borrow() {
                Sequence::Sobol(rsg) => (**rsg).clone(),
                _ => return Err(BindingError::invalid("source must be Sobol")),
            };
            let last = vec![0.0; source.inner.dimension()];
            output(
                out,
                c.insert(shared_mut(Sequence::GaussianSobol(GaussianSobol {
                    source,
                    last,
                })))?,
            )
        })
    }
}
