//! Validated controls for the iterative bootstrap.

use crate::errors::QlResult;
use crate::require;
use crate::types::{Real, Size};

/// Optional QuantLib bounds, retries, and explicitly approximate fallbacks.
#[derive(Clone, Copy, Debug)]
pub struct IterativeBootstrapOptions {
    /// Positive accuracy override; `None` uses the curve accuracy.
    pub accuracy: Option<Real>,
    /// Initial lower bound override; `None` uses the curve traits.
    pub min_value: Option<Real>,
    /// Initial upper bound override; `None` uses the curve traits.
    pub max_value: Option<Real>,
    /// Attempts per pillar per convergence pass, including the first solve.
    pub max_attempts: Size,
    /// Positive maxima multiply by this factor; negative maxima divide by it.
    pub max_factor: Real,
    /// Negative minima multiply by this factor; positive minima divide by it.
    pub min_factor: Real,
    /// Accept an approximate scan result or unconverged final outer pass.
    pub dont_throw: bool,
    /// Number of equal subintervals in the inclusive fallback scan.
    pub dont_throw_steps: Size,
    /// Maximum function evaluations for each root solver.
    pub max_evaluations: Size,
}

impl Default for IterativeBootstrapOptions {
    fn default() -> Self {
        Self {
            accuracy: None,
            min_value: None,
            max_value: None,
            max_attempts: 1,
            max_factor: 2.0,
            min_factor: 2.0,
            dont_throw: false,
            dont_throw_steps: 10,
            max_evaluations: 100,
        }
    }
}

impl IterativeBootstrapOptions {
    /// Validate configuration without running a bootstrap.
    ///
    /// # Errors
    /// Rejects nonfinite values, unordered explicit bounds, nonpositive limits
    /// or accuracy, and widening factors smaller than one.
    pub fn validate(&self) -> QlResult<()> {
        require!(
            self.accuracy.is_none_or(|x| x.is_finite() && x > 0.0),
            "bootstrap accuracy must be finite and positive"
        );
        require!(
            self.min_value.is_none_or(Real::is_finite)
                && self.max_value.is_none_or(Real::is_finite),
            "bootstrap bounds must be finite"
        );
        if let (Some(min), Some(max)) = (self.min_value, self.max_value) {
            require!(
                min < max && (max - min).is_finite(),
                "bootstrap minimum must be below maximum with finite width"
            );
        }
        require!(
            self.max_attempts > 0 && self.dont_throw_steps > 0 && self.max_evaluations > 0,
            "bootstrap attempt, fallback step, and evaluation limits must be positive"
        );
        require!(
            self.min_factor.is_finite()
                && self.min_factor >= 1.0
                && self.max_factor.is_finite()
                && self.max_factor >= 1.0,
            "bootstrap widening factors must be finite and at least one"
        );
        Ok(())
    }
}
