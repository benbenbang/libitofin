//! Default-density adapter from `ql/termstructures/credit/defaultdensitystructure.cpp:71`.

use std::sync::LazyLock;

use crate::errors::QlResult;
use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::types::{Probability, Time};

/// Derives survival probabilities by integrating a curve's quoted default density.
/// Concrete curves wire the provided method into their survival implementation;
/// keeping the density hook required avoids recursive inherited defaults.
pub trait DefaultDensityStructure: DefaultProbabilityTermStructure {
    /// QuantLib's 48-point Gauss-Chebyshev fallback, remapped from `[-1, 1]`
    /// to `[0, t]`, with negative survival floored at zero. As in QuantLib,
    /// a NaN density result propagates instead of becoming zero survival.
    ///
    /// # Errors
    /// Propagates quadrature construction and density evaluation failures.
    fn survival_probability_from_default_density(&self, t: Time) -> QlResult<Probability> {
        static QUADRATURE: LazyLock<QlResult<GaussianQuadrature>> =
            LazyLock::new(|| GaussianQuadrature::chebyshev(48));
        let quadrature = QUADRATURE.as_ref().map_err(Clone::clone)?;
        let mut integral = 0.0;
        for i in (0..quadrature.order()).rev() {
            let time = (quadrature.abscissas()[i] + 1.0) * t / 2.0;
            integral += quadrature.weights()[i] * self.default_density_impl(time)?;
        }
        let survival = 1.0 - integral * t / 2.0;
        Ok(if survival < 0.0 { 0.0 } else { survival })
    }
}
