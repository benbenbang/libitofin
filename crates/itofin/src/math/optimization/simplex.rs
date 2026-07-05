//! Downhill simplex optimization method.
//!
//! Port of `ql/math/optimization/simplex.{hpp,cpp}` (Nelder-Mead as in
//! "Numerical Recipes in C", 2nd edition, chapter 10, with the GSL exit
//! strategy on the simplex size instead of the function value).

use crate::errors::QlResult;
use crate::fail;
use crate::math::array::Array;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use crate::types::{Real, Size};

fn compute_simplex_size(vertices: &[Array]) -> Real {
    let mut center = Array::with_size(vertices[0].size());
    for vertex in vertices {
        center = &center + vertex;
    }
    center = &center * (1.0 / vertices.len() as Real);
    let mut result = 0.0;
    for vertex in vertices {
        result += (vertex - &center).norm2();
    }
    result / vertices.len() as Real
}

/// Multi-dimensional downhill simplex method.
///
/// At each iteration the worst vertex is moved through the opposite face of
/// the simplex to a better point, contracting downhill around the best vertex
/// when stuck in a valley. It needs no gradient evaluations.
pub struct Simplex {
    lambda: Real,
    vertices: Vec<Array>,
    values: Array,
    sum: Array,
}

impl Simplex {
    /// A simplex method with characteristic length scale `lambda`.
    ///
    /// # Panics
    ///
    /// Panics if `lambda` is not finite and positive: with no length scale
    /// the initial vertices all coincide with the starting point and the
    /// method reports convergence there immediately.
    pub fn new(lambda: Real) -> Self {
        assert!(
            lambda.is_finite() && lambda > 0.0,
            "lambda ({lambda}) must be finite and positive"
        );
        Simplex {
            lambda,
            vertices: Vec::new(),
            values: Array::new(),
            sum: Array::new(),
        }
    }

    /// The characteristic length scale.
    pub fn lambda(&self) -> Real {
        self.lambda
    }

    fn extrapolate(
        &mut self,
        problem: &mut Problem<'_>,
        i_highest: Size,
        factor: &mut Real,
    ) -> Real {
        let mut p_try;
        loop {
            let dimensions = self.values.size() - 1;
            let factor1 = (1.0 - *factor) / dimensions as Real;
            let factor2 = factor1 - *factor;
            p_try = &(&self.sum * factor1) - &(&self.vertices[i_highest] * factor2);
            *factor *= 0.5;
            if problem.constraint().test(&p_try) || factor.abs() <= Real::EPSILON {
                break;
            }
        }
        if factor.abs() <= Real::EPSILON {
            return self.values[i_highest];
        }
        *factor *= 2.0;
        let v_try = problem.value(&p_try);
        if v_try < self.values[i_highest] {
            self.values[i_highest] = v_try;
            self.sum = &(&self.sum + &p_try) - &self.vertices[i_highest];
            self.vertices[i_highest] = p_try;
        }
        v_try
    }
}

impl OptimizationMethod for Simplex {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        // end criteria on x, as in GSL, rather than on f(x) as in Numerical
        // Recipes: the latter reports x=0 as the minimum of x*x+x+1 when
        // started from -100 with lambda 1.0
        let xtol = end_criteria.root_epsilon();
        let mut max_stationary_state_iterations = end_criteria.max_stationary_state_iterations();
        let mut ec_type = EndCriteriaType::None;
        problem.reset();

        let mut x = problem.current_value().clone();
        if !problem.constraint().test(&x) {
            fail!("initial guess {x:?} is not in the feasible region");
        }
        let mut iteration_number: Size = 0;

        let n = x.size();
        require!(n > 0, "no variables given");
        self.vertices = vec![x.clone(); n + 1];
        for i in 0..n {
            let mut direction = Array::with_size(n);
            direction[i] = 1.0;
            problem
                .constraint()
                .update(&mut self.vertices[i + 1], &direction, self.lambda)?;
        }
        self.values = Array::with_size(n + 1);
        for i in 0..=n {
            self.values[i] = problem.value(&self.vertices[i]);
        }

        loop {
            self.sum = Array::with_size(n);
            for i in 0..=n {
                self.sum = &self.sum + &self.vertices[i];
            }
            // determine the best (lowest), worst (highest) and second-worst
            // (next-highest) vertices
            let mut i_lowest = 0;
            let (mut i_highest, mut i_next_highest) = if self.values[0] < self.values[1] {
                (1, 0)
            } else {
                (0, 1)
            };
            for i in 1..=n {
                if self.values[i] > self.values[i_highest] {
                    i_next_highest = i_highest;
                    i_highest = i;
                } else if self.values[i] > self.values[i_next_highest] && i != i_highest {
                    i_next_highest = i;
                }
                if self.values[i] < self.values[i_lowest] {
                    i_lowest = i;
                }
            }

            let simplex_size = compute_simplex_size(&self.vertices);
            iteration_number += 1;
            if simplex_size < xtol
                || end_criteria.check_max_iterations(iteration_number, &mut ec_type)
            {
                end_criteria.check_stationary_point(
                    0.0,
                    0.0,
                    &mut max_stationary_state_iterations,
                    &mut ec_type,
                );
                end_criteria.check_max_iterations(iteration_number, &mut ec_type);
                x = self.vertices[i_lowest].clone();
                let low = self.values[i_lowest];
                problem.set_function_value(low);
                problem.set_current_value(x);
                return Ok(ec_type);
            }

            let mut factor = -1.0;
            let mut v_try = self.extrapolate(problem, i_highest, &mut factor);
            if v_try <= self.values[i_lowest] && factor == -1.0 {
                factor = 2.0;
                self.extrapolate(problem, i_highest, &mut factor);
            } else if factor.abs() > Real::EPSILON && v_try >= self.values[i_next_highest] {
                let v_save = self.values[i_highest];
                factor = 0.5;
                v_try = self.extrapolate(problem, i_highest, &mut factor);
                if v_try >= v_save && factor.abs() > Real::EPSILON {
                    for i in 0..=n {
                        if i != i_lowest {
                            self.vertices[i] =
                                &(&self.vertices[i] + &self.vertices[i_lowest]) * 0.5;
                            self.values[i] = problem.value(&self.vertices[i]);
                        }
                    }
                }
            }
            // if extrapolation failed against the constraints, exit
            if factor.abs() <= Real::EPSILON {
                x = self.vertices[i_lowest].clone();
                let low = self.values[i_lowest];
                problem.set_function_value(low);
                problem.set_current_value(x);
                return Ok(EndCriteriaType::StationaryFunctionValue);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::testsupport::check_parabola_minimization;

    #[test]
    fn minimizes_one_dimensional_parabola() {
        check_parabola_minimization(&mut Simplex::new(0.1), "Simplex");
    }

    #[test]
    #[should_panic(expected = "lambda (0) must be finite and positive")]
    fn rejects_a_zero_lambda() {
        let _ = Simplex::new(0.0);
    }

    #[test]
    #[should_panic(expected = "lambda (-0.1) must be finite and positive")]
    fn rejects_a_negative_lambda() {
        let _ = Simplex::new(-0.1);
    }

    #[test]
    #[should_panic(expected = "lambda (NaN) must be finite and positive")]
    fn rejects_a_non_finite_lambda() {
        let _ = Simplex::new(Real::NAN);
    }

    #[test]
    fn rejects_an_empty_parameter_vector() {
        use crate::math::optimization::constraint::NoConstraint;
        use crate::math::optimization::costfunction::CostFunction;

        struct AnyCost;
        impl CostFunction for AnyCost {
            fn values(&self, x: &Array) -> Array {
                x.clone()
            }
        }
        let cost = AnyCost;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::new());
        let end_criteria = EndCriteria::new(100, Some(10), 1e-8, 1e-8, None).unwrap();
        let err = Simplex::new(0.1)
            .minimize(&mut problem, &end_criteria)
            .unwrap_err();
        assert_eq!(err.message(), "no variables given");
    }
}
