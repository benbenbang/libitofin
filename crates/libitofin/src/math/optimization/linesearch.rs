//! Line searches for gradient-based optimization methods.
//!
//! Port of `ql/math/optimization/linesearch.{hpp,cpp}` and
//! `armijo.{hpp,cpp}`. The C++ abstract base class with protected state
//! becomes the [`LineSearch`] trait; the feasibility-preserving step update
//! shared with `Constraint::update` is reused from there. The unused `eps`
//! constructor argument of the C++ `ArmijoLineSearch` is dropped. One
//! deviation: the trait adds [`LineSearch::reset`], called by the driver at
//! the start of every minimization; in C++ the gradient stored by a previous
//! run leaks into the first directional derivative of the next one, so
//! reusing a method instance on another problem misbehaves.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::problem::Problem;
use crate::types::{Real, Size};

/// A one-dimensional search for the step size along a search direction.
pub trait LineSearch {
    /// Performs the line search from the problem's current value with
    /// initial step `t_ini`, returning the step taken.
    ///
    /// # Errors
    ///
    /// Fails when no feasible step can be found against the constraint.
    fn search(
        &mut self,
        problem: &mut Problem<'_>,
        ec_type: &mut EndCriteriaType,
        end_criteria: &EndCriteria,
        t_ini: Real,
    ) -> QlResult<Real>;

    /// The last point evaluated.
    fn last_x(&self) -> &Array;

    /// The cost function value at the last point.
    fn last_function_value(&self) -> Real;

    /// The cost function gradient at the last point.
    fn last_gradient(&self) -> &Array;

    /// The squared norm of the gradient at the last point.
    fn last_gradient_norm2(&self) -> Real;

    /// Whether the last search succeeded.
    fn succeeded(&self) -> bool;

    /// The current search direction.
    fn search_direction(&self) -> &Array;

    /// Sets the search direction.
    fn set_search_direction(&mut self, direction: Array);

    /// Clears state carried over from a previous run so the next search
    /// starts fresh.
    fn reset(&mut self);
}

/// Armijo line search.
///
/// The search stops at a step `t` such that `f(x + t*d) - f(x) <= -alpha t
/// f'(x + t*d)` while `t/beta` violates that bound (see Polak, "Algorithms
/// and consistent approximations", Springer, 1997).
pub struct ArmijoLineSearch {
    alpha: Real,
    beta: Real,
    search_direction: Array,
    xtd: Array,
    gradient: Array,
    qt: Real,
    qpt: Real,
    succeed: bool,
}

impl ArmijoLineSearch {
    /// An Armijo search with acceptance slope `alpha` in `[0, 1]` and
    /// contraction factor `beta` in `(0, 1)`.
    ///
    /// # Panics
    ///
    /// Panics if `alpha` is not a finite value in `[0, 1]`, or if `beta` is
    /// not a finite value in `(0, 1)`: `beta = 1` never contracts the step
    /// and `beta = 0` collapses it, so neither can drive the search, and a
    /// non-finite parameter would poison every comparison.
    pub fn new(alpha: Real, beta: Real) -> Self {
        assert!(
            alpha.is_finite() && (0.0..=1.0).contains(&alpha),
            "alpha ({alpha}) must be finite and in [0, 1]"
        );
        assert!(
            beta.is_finite() && beta > 0.0 && beta < 1.0,
            "beta ({beta}) must be finite and in (0, 1)"
        );
        ArmijoLineSearch {
            alpha,
            beta,
            search_direction: Array::new(),
            xtd: Array::new(),
            gradient: Array::new(),
            qt: 0.0,
            qpt: 0.0,
            succeed: true,
        }
    }
}

impl Default for ArmijoLineSearch {
    /// The QuantLib defaults: `alpha = 0.05`, `beta = 0.65`.
    fn default() -> Self {
        ArmijoLineSearch::new(0.05, 0.65)
    }
}

impl LineSearch for ArmijoLineSearch {
    fn search(
        &mut self,
        problem: &mut Problem<'_>,
        ec_type: &mut EndCriteriaType,
        end_criteria: &EndCriteria,
        t_ini: Real,
    ) -> QlResult<Real> {
        self.succeed = true;
        let mut max_iter = false;
        let mut t = t_ini;
        let mut loop_number: Size = 0;

        let q0 = problem.function_value();
        let qp0 = problem.gradient_norm_value();

        self.qt = q0;
        self.qpt = if self.gradient.is_empty() {
            qp0
        } else {
            -self.gradient.dot(&self.search_direction)
        };

        self.gradient = Array::with_size(problem.current_value().size());
        self.xtd = problem.current_value().clone();
        t = problem
            .constraint()
            .update(&mut self.xtd, &self.search_direction, t)?;
        self.qt = problem.value(&self.xtd);

        if self.qt - q0 > -self.alpha * t * self.qpt {
            loop {
                loop_number += 1;
                t *= self.beta;
                let qtold = self.qt;
                self.xtd = problem.current_value().clone();
                t = problem
                    .constraint()
                    .update(&mut self.xtd, &self.search_direction, t)?;
                self.qt = problem.value(&self.xtd);
                problem.gradient(&mut self.gradient, &self.xtd);
                max_iter = end_criteria.check_max_iterations(loop_number, ec_type);
                let armijo_violated = self.qt - q0 > -self.alpha * t * self.qpt;
                let step_too_small = qtold - q0 <= -self.alpha * t * self.qpt / self.beta;
                if !((armijo_violated || step_too_small) && !max_iter) {
                    break;
                }
            }
        }

        if max_iter {
            self.succeed = false;
        }

        problem.gradient(&mut self.gradient, &self.xtd);
        self.qpt = self.gradient.dot(&self.gradient);
        Ok(t)
    }

    fn last_x(&self) -> &Array {
        &self.xtd
    }

    fn last_function_value(&self) -> Real {
        self.qt
    }

    fn last_gradient(&self) -> &Array {
        &self.gradient
    }

    fn last_gradient_norm2(&self) -> Real {
        self.qpt
    }

    fn succeeded(&self) -> bool {
        self.succeed
    }

    fn search_direction(&self) -> &Array {
        &self.search_direction
    }

    fn set_search_direction(&mut self, direction: Array) {
        self.search_direction = direction;
    }

    fn reset(&mut self) {
        self.gradient = Array::new();
        self.succeed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_parameters() {
        let _ = ArmijoLineSearch::new(0.0, 0.5);
        let _ = ArmijoLineSearch::new(1.0, 0.999);
        let _ = ArmijoLineSearch::default();
    }

    #[test]
    #[should_panic(expected = "beta (1) must be finite and in (0, 1)")]
    fn rejects_beta_of_one() {
        let _ = ArmijoLineSearch::new(0.05, 1.0);
    }

    #[test]
    #[should_panic(expected = "beta (0) must be finite and in (0, 1)")]
    fn rejects_beta_of_zero() {
        let _ = ArmijoLineSearch::new(0.05, 0.0);
    }

    #[test]
    #[should_panic(expected = "alpha")]
    fn rejects_non_finite_alpha() {
        let _ = ArmijoLineSearch::new(Real::NAN, 0.65);
    }

    #[test]
    #[should_panic(expected = "alpha (1.5) must be finite and in [0, 1]")]
    fn rejects_alpha_above_one() {
        let _ = ArmijoLineSearch::new(1.5, 0.65);
    }
}
