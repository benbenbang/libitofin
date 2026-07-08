//! Shared oracle for the optimizer tests, ported from
//! `test-suite/optimizers.cpp`: minimize the parabola y = x*x + x + 1
//! starting from x = -100 and compare against the analytic minimum.

use crate::math::array::Array;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::types::Real;

pub(crate) struct OneDimensionalPolynomialDegreeN {
    coefficients: Array,
}

impl OneDimensionalPolynomialDegreeN {
    pub(crate) fn new(coefficients: Array) -> Self {
        OneDimensionalPolynomialDegreeN { coefficients }
    }
}

impl CostFunction for OneDimensionalPolynomialDegreeN {
    fn value(&self, x: &Array) -> Real {
        assert_eq!(x.size(), 1, "independent variable must be 1 dimensional");
        self.coefficients
            .iter()
            .enumerate()
            .map(|(i, c)| c * x[0].powi(i as i32))
            .sum()
    }

    fn values(&self, x: &Array) -> Array {
        Array::filled(1, self.value(x))
    }
}

fn max_difference(a: &Array, b: &Array) -> Real {
    (a - b).iter().fold(0.0, |acc: Real, d| acc.max(d.abs()))
}

/// Runs `method` on the parabola a*x^2 + b*x + c with a = b = c = 1 from
/// x = -100 and checks completion and accuracy as `optimizers.cpp` does.
pub(crate) fn check_parabola_minimization(method: &mut dyn OptimizationMethod, name: &str) {
    let (a, b, c) = (1.0, 1.0, 1.0);
    let cost = OneDimensionalPolynomialDegreeN::new(Array::from([c, b, a]));
    let constraint = NoConstraint;
    let initial_value = Array::from([-100.0]);
    let root_epsilon = 1e-8;
    let function_epsilon = 1e-8;
    let end_criteria =
        EndCriteria::new(10000, Some(100), root_epsilon, function_epsilon, Some(1e-8)).unwrap();
    let x_min_expected = Array::from([-b / (2.0 * a)]);
    let y_min_expected = Array::from([-(b * b - 4.0 * a * c) / (4.0 * a)]);

    let mut problem = Problem::new(&cost, &constraint, initial_value);
    let ec_result = method.minimize(&mut problem, &end_criteria).unwrap();

    let x_min_calculated = problem.current_value().clone();
    let y_min_calculated = problem.values(&x_min_calculated);
    let completed = !matches!(
        ec_result,
        EndCriteriaType::None | EndCriteriaType::MaxIterations | EndCriteriaType::Unknown
    );
    let x_error = max_difference(&x_min_calculated, &x_min_expected);
    let y_error = max_difference(&y_min_calculated, &y_min_expected);
    let correct = x_error <= root_epsilon || y_error <= function_epsilon;

    assert!(
        completed && correct,
        "optimizer: {name}\n    \
         function evaluations: {}\n    \
         gradient evaluations: {}\n    \
         x expected:  {x_min_expected:?}\n    \
         x calculated: {x_min_calculated:?} (error {x_error:e}, rootEpsilon {root_epsilon:e})\n    \
         y expected:  {y_min_expected:?}\n    \
         y calculated: {y_min_calculated:?} (error {y_error:e}, functionEpsilon {function_epsilon:e})\n    \
         end criteria result: {ec_result}",
        problem.function_evaluation(),
        problem.gradient_evaluation(),
    );
}
