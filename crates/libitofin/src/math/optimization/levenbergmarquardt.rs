//! Levenberg-Marquardt optimization method.
//!
//! Port of `ql/math/optimization/levenbergmarquardt.{hpp,cpp}`, wrapping the
//! MINPACK [`lmdif`] routine. It has a built-in forward-difference scheme to
//! compute the Jacobian, used by default; with
//! `use_cost_functions_jacobian` the cost function's own `jacobian` method
//! (a central difference by default, order 2 but costlier) is used instead.
//!
//! Several deviations guard against silent false convergence, all stemming
//! from the penalty fallbacks that `ProblemAdapter` returns for a bad
//! evaluation (which yield a zero/flat Jacobian and let lmdif report
//! convergence). The starting point is validated against the constraint -
//! QuantLib accepts an infeasible start and converges there - and the
//! initial residuals, the initial analytic Jacobian (when
//! `use_cost_functions_jacobian` is set), and the final scalar cost are all
//! checked for finiteness, so a feasible point whose evaluation is NaN/inf
//! fails loudly instead of returning a non-finite optimum. The final point
//! is also re-validated against the constraint: the flat penalty is not
//! always worse than a huge valid residual, so lmdif can accept an
//! infeasible step and stop there.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::lmdif::{LmdifCostFunction, lmdif};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use crate::types::Real;

/// Levenberg-Marquardt method for least-squares problems.
pub struct LevenbergMarquardt {
    epsfcn: Real,
    xtol: Real,
    gtol: Real,
    use_cost_functions_jacobian: bool,
}

impl LevenbergMarquardt {
    /// A Levenberg-Marquardt method with the given finite-difference step
    /// seed and x/gradient tolerances, optionally using the cost function's
    /// own Jacobian.
    pub fn new(epsfcn: Real, xtol: Real, gtol: Real, use_cost_functions_jacobian: bool) -> Self {
        LevenbergMarquardt {
            epsfcn,
            xtol,
            gtol,
            use_cost_functions_jacobian,
        }
    }
}

impl Default for LevenbergMarquardt {
    /// The QuantLib defaults: `epsfcn = xtol = gtol = 1e-8`, built-in
    /// forward-difference Jacobian.
    fn default() -> Self {
        LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false)
    }
}

/// Adapts a [`Problem`] to the [`lmdif`] callbacks, steering the optimizer
/// away from infeasible or non-finite regions.
struct ProblemAdapter<'a, 'b> {
    problem: &'a mut Problem<'b>,
    init_jacobian: Option<Matrix>,
    m: usize,
    n: usize,
}

impl LmdifCostFunction for ProblemAdapter<'_, '_> {
    fn fcn(&mut self, x: &[Real], fvec: &mut [Real]) {
        let xt: Array = x.iter().copied().collect();
        if self.problem.constraint().test(&xt) {
            let tmp = self.problem.values(&xt);
            if tmp.size() == fvec.len() && tmp.iter().all(|value| value.is_finite()) {
                fvec.copy_from_slice(&tmp);
                return;
            }
        }
        // Constraint violated, wrong residual count, or non-finite values:
        // return a large, uniform penalty so the optimizer steers away. A
        // fixed constant is used instead of the initial cost values because
        // the latter can be very small (even zero) when the starting point
        // is near-optimal, which would fail to deter the optimizer from
        // exploring infeasible regions.
        fvec.fill(1.0e10);
    }

    fn has_jacobian(&self) -> bool {
        self.init_jacobian.is_some()
    }

    fn jacobian(&mut self, x: &[Real], fjac: &mut [Real]) {
        let xt: Array = x.iter().copied().collect();
        if self.problem.constraint().test(&xt) {
            let mut tmp = Matrix::with_size(self.m, self.n);
            self.problem.cost_function().jacobian(&mut tmp, &xt);
            if (0..self.m).all(|i| tmp.row(i).iter().all(|value| value.is_finite())) {
                for j in 0..self.n {
                    for i in 0..self.m {
                        fjac[i + self.m * j] = tmp[(i, j)];
                    }
                }
                return;
            }
        }
        // Constraint violated or Jacobian produced non-finite values:
        // return the initial Jacobian so the optimizer doesn't diverge
        let init = self
            .init_jacobian
            .as_ref()
            .expect("jacobian callback requires an initial jacobian");
        for j in 0..self.n {
            for i in 0..self.m {
                fjac[i + self.m * j] = init[(i, j)];
            }
        }
    }
}

impl OptimizationMethod for LevenbergMarquardt {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        problem.reset();
        let init_x = problem.current_value().clone();
        if !problem.constraint().test(&init_x) {
            crate::fail!("initial guess {init_x:?} is not in the feasible region");
        }
        let init_cost_values = problem.cost_function().values(&init_x);
        if !init_cost_values.iter().all(|value| value.is_finite()) {
            crate::fail!("initial cost values {init_cost_values:?} are not all finite");
        }
        let m = init_cost_values.size();
        let n = init_x.size();
        let init_jacobian = if self.use_cost_functions_jacobian {
            let mut jacobian = Matrix::with_size(m, n);
            problem.cost_function().jacobian(&mut jacobian, &init_x);
            // ProblemAdapter::jacobian falls back to this matrix on any bad
            // later evaluation, so a non-finite initial jacobian would poison
            // MINPACK instead of failing loudly.
            if !(0..m).all(|i| jacobian.row(i).iter().all(|value| value.is_finite())) {
                crate::fail!("initial jacobian is not all finite");
            }
            Some(jacobian)
        } else {
            None
        };
        // magic number recommended by the MINPACK documentation
        let factor = 100.0;
        // lmdif evaluates the cost function n+1 times per iteration
        // (technically 2n+1 with use_cost_functions_jacobian, which lmdif
        // doesn't account for)
        let Some(maxfev) = end_criteria.max_iterations().checked_mul(n + 1) else {
            crate::fail!(
                "maxIterations ({}) times {} evaluations per iteration overflows the evaluation budget",
                end_criteria.max_iterations(),
                n + 1
            );
        };

        // requirements; checked here to get more detailed error messages
        require!(n > 0, "no variables given");
        require!(
            m >= n,
            "less functions ({m}) than available variables ({n})"
        );
        if end_criteria.function_epsilon() < 0.0 {
            crate::fail!("negative f tolerance");
        }
        if self.xtol < 0.0 {
            crate::fail!("negative x tolerance");
        }
        if self.gtol < 0.0 {
            crate::fail!("negative g tolerance");
        }
        require!(maxfev > 0, "null number of evaluations");

        let mut xx = init_x.to_vec();
        let mut fvec = vec![0.0; m];
        let mut adapter = ProblemAdapter {
            problem,
            init_jacobian,
            m,
            n,
        };
        let info = lmdif(
            m,
            n,
            &mut xx,
            &mut fvec,
            end_criteria.function_epsilon(),
            self.xtol,
            self.gtol,
            maxfev,
            self.epsfcn,
            factor,
            &mut adapter,
        );

        require!(info != 0, "MINPACK: improper input parameters");
        require!(
            info != 7,
            "MINPACK: xtol is too small. no further improvement in the approximate solution x is possible."
        );
        require!(
            info != 8,
            "MINPACK: gtol is too small. fvec is orthogonal to the columns of the jacobian to machine precision."
        );
        let ec_type = match info {
            // 2 and 3 should be StationaryPoint, 4 a new gradient-related
            // value, but QuantLib keeps StationaryFunctionValue for
            // backwards compatibility
            1..=4 => EndCriteriaType::StationaryFunctionValue,
            5 => EndCriteriaType::MaxIterations,
            6 => EndCriteriaType::FunctionEpsilonTooSmall,
            _ => crate::fail!("unknown MINPACK result: {info}"),
        };

        let x: Array = xx.into_iter().collect();
        // the flat penalty is not always worse than a huge valid residual,
        // so lmdif can accept and stop at an infeasible point
        if !problem.constraint().test(&x) {
            crate::fail!("final point {x:?} is not in the feasible region");
        }
        problem.set_current_value(x);
        let function_value = problem.cost_function().value(problem.current_value());
        if !function_value.is_finite() {
            crate::fail!("final cost value ({function_value}) is not finite");
        }
        problem.set_function_value(function_value);
        Ok(ec_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::constraint::{NoConstraint, PositiveConstraint};
    use crate::math::optimization::costfunction::CostFunction;
    use crate::math::optimization::testsupport::{
        OneDimensionalPolynomialDegreeN, check_parabola_minimization,
    };

    #[test]
    fn minimizes_one_dimensional_parabola() {
        check_parabola_minimization(
            &mut LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false),
            "Levenberg Marquardt",
        );
    }

    #[test]
    fn minimizes_one_dimensional_parabola_with_cost_functions_jacobian() {
        check_parabola_minimization(
            &mut LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, true),
            "Levenberg Marquardt (cost function's jacobian)",
        );
    }

    #[test]
    fn rejects_an_infeasible_starting_point() {
        let cost = OneDimensionalPolynomialDegreeN::new(Array::from([1.0, 1.0, 1.0]));
        let constraint = PositiveConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([-1.0]));
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-8, 1e-8, None).unwrap();
        let err = LevenbergMarquardt::default()
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert!(
            err.message().contains("feasible region"),
            "unexpected message: {}",
            err.message()
        );
    }

    #[test]
    fn rejects_a_feasible_start_with_non_finite_residuals() {
        // Feasible everywhere, but the residuals are NaN: without the finite
        // check lmdif sees a flat penalty, a zero jacobian, and reports
        // convergence at a non-finite cost.
        struct NanCost;
        impl CostFunction for NanCost {
            fn values(&self, x: &Array) -> Array {
                Array::filled(1, x[0] - Real::NAN)
            }
        }
        let cost = NanCost;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([1.0]));
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-8, 1e-8, None).unwrap();
        let err = LevenbergMarquardt::default()
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert!(
            err.message().contains("finite"),
            "unexpected message: {}",
            err.message()
        );
    }

    #[test]
    fn tolerates_a_cost_function_with_a_varying_residual_count() {
        // The residual count depends on x, so trial points can return a
        // different length than the initial evaluation; those evaluations
        // must take the penalty path instead of panicking.
        struct VaryingLength;
        impl CostFunction for VaryingLength {
            fn values(&self, x: &Array) -> Array {
                if x[0] > 0.0 {
                    Array::from([x[0], x[0]])
                } else {
                    Array::from([x[0]])
                }
            }
        }
        let cost = VaryingLength;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([1.0]));
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-8, 1e-8, None).unwrap();
        let _ = LevenbergMarquardt::default().minimize(&mut problem, &end_criteria);
    }

    #[test]
    fn rejects_a_non_finite_cost_function_jacobian() {
        // Residuals are finite, but the analytic jacobian is NaN: with
        // use_cost_functions_jacobian set, that matrix becomes the adapter's
        // fallback, so it must be rejected up front instead of poisoning
        // MINPACK.
        struct NanJacobian;
        impl CostFunction for NanJacobian {
            fn values(&self, x: &Array) -> Array {
                Array::filled(1, x[0])
            }
            fn jacobian(&self, jac: &mut Matrix, _x: &Array) {
                jac[(0, 0)] = Real::NAN;
            }
        }
        let cost = NanJacobian;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([1.0]));
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-8, 1e-8, None).unwrap();
        let err = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, true)
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert!(
            err.message().contains("jacobian is not all finite"),
            "unexpected message: {}",
            err.message()
        );
    }

    #[test]
    fn rejects_an_infeasible_final_point() {
        // The unconstrained minimum is far below zero and the residuals
        // there dwarf the adapter's 1.0e10 penalty, so lmdif accepts an
        // infeasible step and "converges" outside the feasible region.
        struct HugeResidual;
        impl CostFunction for HugeResidual {
            fn values(&self, x: &Array) -> Array {
                Array::filled(1, x[0] + 1.0e11)
            }
        }
        let cost = HugeResidual;
        let constraint = PositiveConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([1.0]));
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-8, 1e-8, None).unwrap();
        let err = LevenbergMarquardt::default()
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert!(
            err.message().contains("feasible region"),
            "unexpected message: {}",
            err.message()
        );
    }

    #[test]
    fn rejects_a_max_evaluation_count_that_overflows() {
        let cost = OneDimensionalPolynomialDegreeN::new(Array::from([1.0, 1.0, 1.0]));
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([-100.0]));
        let end_criteria = EndCriteria::new(usize::MAX, Some(100), 1e-8, 1e-8, None).unwrap();
        let err = LevenbergMarquardt::default()
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert!(
            err.message().contains("overflow"),
            "unexpected message: {}",
            err.message()
        );
    }

    // The goal of this cost function is simply to call another optimization
    // inside, in order to test nested optimizations
    struct OptimizationBasedCostFunction;

    impl CostFunction for OptimizationBasedCostFunction {
        fn value(&self, _x: &Array) -> Real {
            1.0
        }

        fn values(&self, _x: &Array) -> Array {
            // dummy nested optimization
            let inner_cost = OneDimensionalPolynomialDegreeN::new(Array::filled(3, 1.0));
            let constraint = NoConstraint;
            let mut problem = Problem::new(&inner_cost, &constraint, Array::filled(1, 100.0));
            let mut method = LevenbergMarquardt::default();
            let end_criteria = EndCriteria::new(1000, Some(100), 1e-5, 1e-5, Some(1e-5)).unwrap();
            let _ = method.minimize(&mut problem, &end_criteria);
            // return dummy result
            Array::filled(1, 0.0)
        }
    }

    #[test]
    fn supports_nested_optimizations() {
        let cost = OptimizationBasedCostFunction;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::filled(1, 0.0));
        let mut method = LevenbergMarquardt::default();
        let end_criteria = EndCriteria::new(1000, Some(100), 1e-5, 1e-5, Some(1e-5)).unwrap();
        method.minimize(&mut problem, &end_criteria).unwrap();
    }
}
