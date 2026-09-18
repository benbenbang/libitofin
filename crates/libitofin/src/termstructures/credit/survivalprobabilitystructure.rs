//! Survival-probability adapter from QuantLib's `survivalprobabilitystructure.cpp`.

use crate::errors::QlResult;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::types::{Real, Time};

/// Derives default density from a curve's survival probabilities.
pub trait SurvivalProbabilityStructure: DefaultProbabilityTermStructure {
    /// QuantLib's finite-difference fallback, using a forward difference at zero.
    fn default_density_from_survival_probability(&self, t: Time) -> QlResult<Real> {
        let t1 = (t - 0.0001).max(0.0);
        let t2 = t + 0.0001;
        Ok((self.survival_probability_impl(t1)? - self.survival_probability_impl(t2)?) / (t2 - t1))
    }
}
