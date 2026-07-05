//! Criteria to end an optimization process.
//!
//! Port of `ql/math/optimization/endcriteria.{hpp,cpp}`. QuantLib's nullable
//! constructor arguments (`Null<Size>`, `Null<Real>`) become `Option`s, and
//! the constructor's `QL_REQUIRE`s make construction fallible. As an
//! extension over QuantLib, the stationarity, accuracy and gradient-norm
//! checks never report convergence on non-finite inputs; in C++ a NaN falls
//! through every `>=` comparison and counts as converged.

use std::fmt;

use crate::errors::QlResult;
use crate::require;
use crate::types::{Real, Size};

/// The reason an optimization run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndCriteriaType {
    /// No criterion met yet.
    None,
    /// The maximum number of iterations was reached.
    MaxIterations,
    /// The independent variable reached a stationary point.
    StationaryPoint,
    /// The function value reached a stationary point.
    StationaryFunctionValue,
    /// The function value fell below the accuracy threshold.
    StationaryFunctionAccuracy,
    /// The gradient norm fell below its threshold.
    ZeroGradientNorm,
    /// The function tolerance is too small for further improvement.
    FunctionEpsilonTooSmall,
    /// Unknown reason.
    Unknown,
}

impl EndCriteriaType {
    /// Whether this outcome counts as a successful minimization.
    pub fn succeeded(self) -> bool {
        matches!(
            self,
            EndCriteriaType::StationaryPoint
                | EndCriteriaType::StationaryFunctionValue
                | EndCriteriaType::StationaryFunctionAccuracy
        )
    }
}

impl fmt::Display for EndCriteriaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EndCriteriaType::None => "None",
            EndCriteriaType::MaxIterations => "MaxIterations",
            EndCriteriaType::StationaryPoint => "StationaryPoint",
            EndCriteriaType::StationaryFunctionValue => "StationaryFunctionValue",
            EndCriteriaType::StationaryFunctionAccuracy => "StationaryFunctionAccuracy",
            EndCriteriaType::ZeroGradientNorm => "ZeroGradientNorm",
            EndCriteriaType::FunctionEpsilonTooSmall => "FunctionEpsilonTooSmall",
            EndCriteriaType::Unknown => "Unknown",
        })
    }
}

/// Iteration and stationarity thresholds ending an optimization.
#[derive(Clone, Copy, Debug)]
pub struct EndCriteria {
    max_iterations: Size,
    max_stationary_state_iterations: Size,
    root_epsilon: Real,
    function_epsilon: Real,
    gradient_norm_epsilon: Real,
}

impl EndCriteria {
    /// Builds the criteria; `max_stationary_state_iterations` defaults to
    /// `min(max_iterations / 2, 100)` and `gradient_norm_epsilon` to
    /// `function_epsilon`.
    ///
    /// # Errors
    ///
    /// Fails unless `1 < max_stationary_state_iterations < max_iterations`,
    /// or if any epsilon is negative or non-finite (an extension over
    /// QuantLib, which accepts them: a NaN tolerance would make every
    /// comparison-based check misfire).
    pub fn new(
        max_iterations: Size,
        max_stationary_state_iterations: Option<Size>,
        root_epsilon: Real,
        function_epsilon: Real,
        gradient_norm_epsilon: Option<Real>,
    ) -> QlResult<Self> {
        let max_stationary_state_iterations =
            max_stationary_state_iterations.unwrap_or_else(|| (max_iterations / 2).min(100));
        require!(
            max_stationary_state_iterations > 1,
            "maxStationaryStateIterations ({max_stationary_state_iterations}) must be greater than one"
        );
        require!(
            max_stationary_state_iterations < max_iterations,
            "maxStationaryStateIterations ({max_stationary_state_iterations}) must be less than maxIterations ({max_iterations})"
        );
        for (name, epsilon) in [
            ("rootEpsilon", Some(root_epsilon)),
            ("functionEpsilon", Some(function_epsilon)),
            ("gradientNormEpsilon", gradient_norm_epsilon),
        ] {
            if let Some(epsilon) = epsilon
                && (!epsilon.is_finite() || epsilon < 0.0)
            {
                crate::fail!("{name} ({epsilon}) must be finite and non-negative");
            }
        }
        Ok(EndCriteria {
            max_iterations,
            max_stationary_state_iterations,
            root_epsilon,
            function_epsilon,
            gradient_norm_epsilon: gradient_norm_epsilon.unwrap_or(function_epsilon),
        })
    }

    /// Tests if the number of iterations is not too big and a minimum point
    /// is not reached; the C++ `operator()`.
    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &self,
        iteration: Size,
        stat_state_iterations: &mut Size,
        positive_optimization: bool,
        fold: Real,
        _normgold: Real,
        fnew: Real,
        normgnew: Real,
        ec_type: &mut EndCriteriaType,
    ) -> bool {
        self.check_max_iterations(iteration, ec_type)
            || self.check_stationary_function_value(fold, fnew, stat_state_iterations, ec_type)
            || self.check_stationary_function_accuracy(fnew, positive_optimization, ec_type)
            || self.check_zero_gradient_norm(normgnew, ec_type)
    }

    /// Tests if the number of iterations reached `max_iterations`.
    pub fn check_max_iterations(&self, iteration: Size, ec_type: &mut EndCriteriaType) -> bool {
        if iteration < self.max_iterations {
            return false;
        }
        *ec_type = EndCriteriaType::MaxIterations;
        true
    }

    /// Tests if the root variation stayed below `root_epsilon` for more than
    /// `max_stationary_state_iterations` consecutive iterations.
    pub fn check_stationary_point(
        &self,
        x_old: Real,
        x_new: Real,
        stat_state_iterations: &mut Size,
        ec_type: &mut EndCriteriaType,
    ) -> bool {
        let diff = (x_new - x_old).abs();
        if !diff.is_finite() || diff >= self.root_epsilon {
            *stat_state_iterations = 0;
            return false;
        }
        *stat_state_iterations += 1;
        if *stat_state_iterations <= self.max_stationary_state_iterations {
            return false;
        }
        *ec_type = EndCriteriaType::StationaryPoint;
        true
    }

    /// Tests if the function variation stayed below `function_epsilon` for
    /// more than `max_stationary_state_iterations` consecutive iterations.
    pub fn check_stationary_function_value(
        &self,
        fx_old: Real,
        fx_new: Real,
        stat_state_iterations: &mut Size,
        ec_type: &mut EndCriteriaType,
    ) -> bool {
        let diff = (fx_new - fx_old).abs();
        if !diff.is_finite() || diff >= self.function_epsilon {
            *stat_state_iterations = 0;
            return false;
        }
        *stat_state_iterations += 1;
        if *stat_state_iterations <= self.max_stationary_state_iterations {
            return false;
        }
        *ec_type = EndCriteriaType::StationaryFunctionValue;
        true
    }

    /// Tests if the function value is below `function_epsilon`; only
    /// meaningful when the cost function is known to be positive.
    pub fn check_stationary_function_accuracy(
        &self,
        f: Real,
        positive_optimization: bool,
        ec_type: &mut EndCriteriaType,
    ) -> bool {
        if !positive_optimization || !f.is_finite() || f >= self.function_epsilon {
            return false;
        }
        *ec_type = EndCriteriaType::StationaryFunctionAccuracy;
        true
    }

    /// Tests if the gradient norm is below `gradient_norm_epsilon`.
    pub fn check_zero_gradient_norm(
        &self,
        gradient_norm: Real,
        ec_type: &mut EndCriteriaType,
    ) -> bool {
        if !gradient_norm.is_finite() || gradient_norm >= self.gradient_norm_epsilon {
            return false;
        }
        *ec_type = EndCriteriaType::ZeroGradientNorm;
        true
    }

    /// The maximum number of iterations.
    pub fn max_iterations(&self) -> Size {
        self.max_iterations
    }

    /// The maximum number of iterations in a stationary state.
    pub fn max_stationary_state_iterations(&self) -> Size {
        self.max_stationary_state_iterations
    }

    /// The tolerance on the independent variable.
    pub fn root_epsilon(&self) -> Real {
        self.root_epsilon
    }

    /// The tolerance on the function value.
    pub fn function_epsilon(&self) -> Real {
        self.function_epsilon
    }

    /// The tolerance on the gradient norm.
    pub fn gradient_norm_epsilon(&self) -> Real {
        self.gradient_norm_epsilon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_defaults_and_validation() {
        let ec = EndCriteria::new(1000, None, 1e-8, 1e-8, None).unwrap();
        assert_eq!(ec.max_stationary_state_iterations(), 100);
        assert_eq!(ec.gradient_norm_epsilon(), 1e-8);

        let ec = EndCriteria::new(50, None, 1e-8, 1e-9, Some(1e-10)).unwrap();
        assert_eq!(ec.max_stationary_state_iterations(), 25);
        assert_eq!(ec.gradient_norm_epsilon(), 1e-10);

        assert!(EndCriteria::new(1000, Some(1), 1e-8, 1e-8, None).is_err());
        assert!(EndCriteria::new(100, Some(100), 1e-8, 1e-8, None).is_err());
    }

    #[test]
    fn rejects_invalid_tolerances() {
        assert!(EndCriteria::new(100, Some(10), Real::NAN, 1e-8, None).is_err());
        assert!(EndCriteria::new(100, Some(10), 1e-8, -1e-8, None).is_err());
        assert!(EndCriteria::new(100, Some(10), 1e-8, Real::INFINITY, None).is_err());
        assert!(EndCriteria::new(100, Some(10), 1e-8, 1e-8, Some(Real::NAN)).is_err());
        // zero stays allowed, as in QuantLib's own usage
        assert!(EndCriteria::new(100, Some(10), 0.0, 0.0, Some(0.0)).is_ok());
    }

    #[test]
    fn max_iterations_check() {
        let ec = EndCriteria::new(10, Some(5), 1e-8, 1e-8, None).unwrap();
        let mut ec_type = EndCriteriaType::None;
        assert!(!ec.check_max_iterations(9, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::None);
        assert!(ec.check_max_iterations(10, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::MaxIterations);
    }

    #[test]
    fn stationary_point_requires_consecutive_hits() {
        let ec = EndCriteria::new(100, Some(2), 1e-8, 1e-8, None).unwrap();
        let mut ec_type = EndCriteriaType::None;
        let mut stat = 0;
        assert!(!ec.check_stationary_point(0.0, 1e-9, &mut stat, &mut ec_type));
        assert!(!ec.check_stationary_point(0.0, 1e-9, &mut stat, &mut ec_type));
        // a large move resets the counter
        assert!(!ec.check_stationary_point(0.0, 1.0, &mut stat, &mut ec_type));
        assert_eq!(stat, 0);
        assert!(!ec.check_stationary_point(0.0, 1e-9, &mut stat, &mut ec_type));
        assert!(!ec.check_stationary_point(0.0, 1e-9, &mut stat, &mut ec_type));
        assert!(ec.check_stationary_point(0.0, 1e-9, &mut stat, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::StationaryPoint);
    }

    #[test]
    fn stationary_function_value_counts_like_stationary_point() {
        let ec = EndCriteria::new(100, Some(2), 1e-8, 1e-8, None).unwrap();
        let mut ec_type = EndCriteriaType::None;
        let mut stat = 0;
        for _ in 0..2 {
            assert!(!ec.check_stationary_function_value(1.0, 1.0, &mut stat, &mut ec_type));
        }
        assert!(ec.check_stationary_function_value(1.0, 1.0, &mut stat, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::StationaryFunctionValue);
    }

    #[test]
    fn function_accuracy_and_gradient_norm_checks() {
        let ec = EndCriteria::new(100, Some(2), 1e-8, 1e-8, None).unwrap();
        let mut ec_type = EndCriteriaType::None;
        assert!(!ec.check_stationary_function_accuracy(1e-9, false, &mut ec_type));
        assert!(!ec.check_stationary_function_accuracy(1.0, true, &mut ec_type));
        assert!(ec.check_stationary_function_accuracy(1e-9, true, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::StationaryFunctionAccuracy);
        assert!(!ec.check_zero_gradient_norm(1.0, &mut ec_type));
        assert!(ec.check_zero_gradient_norm(1e-9, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::ZeroGradientNorm);
    }

    #[test]
    fn non_finite_inputs_never_report_convergence() {
        let ec = EndCriteria::new(100, Some(2), 1e-8, 1e-8, None).unwrap();
        let mut ec_type = EndCriteriaType::None;
        let mut stat = 0;
        for _ in 0..5 {
            assert!(!ec.check_stationary_point(0.0, Real::NAN, &mut stat, &mut ec_type));
            assert_eq!(stat, 0);
        }
        for _ in 0..5 {
            assert!(!ec.check_stationary_function_value(
                Real::NAN,
                Real::NAN,
                &mut stat,
                &mut ec_type
            ));
            assert_eq!(stat, 0);
        }
        assert!(!ec.check_stationary_function_accuracy(Real::NAN, true, &mut ec_type));
        assert!(!ec.check_zero_gradient_norm(Real::NAN, &mut ec_type));
        assert_eq!(ec_type, EndCriteriaType::None);
    }

    #[test]
    fn succeeded_and_display() {
        assert!(EndCriteriaType::StationaryPoint.succeeded());
        assert!(EndCriteriaType::StationaryFunctionValue.succeeded());
        assert!(EndCriteriaType::StationaryFunctionAccuracy.succeeded());
        assert!(!EndCriteriaType::None.succeeded());
        assert!(!EndCriteriaType::MaxIterations.succeeded());
        assert!(!EndCriteriaType::ZeroGradientNorm.succeeded());
        assert_eq!(EndCriteriaType::MaxIterations.to_string(), "MaxIterations");
    }
}
