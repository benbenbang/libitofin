/// What a per-iteration callback asks the solver to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep iterating.
    Continue,
    /// Stop now and report [`Termination::Cancelled`](crate::Termination::Cancelled).
    Stop,
}

/// A read-only view of the solver state at the end of an iteration.
#[derive(Debug, Clone, Copy)]
pub struct IterationState<'a> {
    /// The current best point.
    pub x: &'a [f64],
    /// The objective value at `x`.
    pub fun: f64,
    /// Iterations completed.
    pub nit: usize,
    /// Objective evaluations charged so far.
    pub nfev: usize,
    /// Gradient evaluations charged so far.
    pub njev: usize,
}

/// The form of one general constraint `c_i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// `c_i(x) = 0`.
    Eq,
    /// `c_i(x) >= 0`, the SciPy convention.
    Ineq,
}

/// The function a solver minimizes, and the general constraints it is subject
/// to.
///
/// Constraints are numbered `0..constraint_count()` and share the objective's
/// error type, so one [`MinimizeError`](crate::MinimizeError) covers every
/// failure. Only a method that supports constraints reads them; any other
/// method rejects an objective that declares some.
pub trait Objective {
    /// How an evaluation fails. Carried by value through
    /// [`MinimizeError::Objective`](crate::MinimizeError::Objective), never stringified.
    type Error: std::error::Error + 'static;

    /// Evaluates the objective at `x`.
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error>;

    /// Writes the gradient at `x` into `out` and returns `true`.
    ///
    /// The default returns `false`: there is no analytic gradient, so a method
    /// that needs one approximates it.
    fn gradient(&mut self, _x: &[f64], _out: &mut [f64]) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Runs once at the end of every iteration.
    ///
    /// `Ok(Flow::Stop)` cancels the run; `Err` fails it. A cancellation is not
    /// an error and an error is not a cancellation.
    fn callback(&mut self, _state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }

    /// The number of general constraints. The default declares none.
    fn constraint_count(&self) -> usize {
        0
    }

    /// The form of constraint `i`.
    ///
    /// # Panics
    ///
    /// The default panics: an objective that declares constraints must say
    /// what they are.
    fn constraint_kind(&self, i: usize) -> ConstraintKind {
        unimplemented!("constraint {i} is declared but its kind is not")
    }

    /// Evaluates constraint `i` at `x`.
    ///
    /// # Panics
    ///
    /// The default panics: an objective that declares constraints must
    /// evaluate them.
    fn constraint(&mut self, i: usize, _x: &[f64]) -> Result<f64, Self::Error> {
        unimplemented!("constraint {i} is declared but not evaluated")
    }

    /// Writes the gradient of constraint `i` at `x` into `out` and returns
    /// `true`.
    ///
    /// The default returns `false`: there is no analytic Jacobian row, so the
    /// method approximates it by finite differences.
    fn constraint_jacobian(
        &mut self,
        _i: usize,
        _x: &[f64],
        _out: &mut [f64],
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

impl<F, E> Objective for F
where
    F: FnMut(&[f64]) -> Result<f64, E>,
    E: std::error::Error + 'static,
{
    type Error = E;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self(x)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstraintKind, Objective};
    use crate::{
        Common, InvalidInput, Method, MinimizeError, NelderMeadOptions, Problem, minimize,
    };
    use std::convert::Infallible;

    struct Disc;

    impl Objective for Disc {
        type Error = Infallible;

        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            Ok(x[0] + x[1])
        }

        fn constraint_count(&self) -> usize {
            1
        }

        fn constraint_kind(&self, _i: usize) -> ConstraintKind {
            ConstraintKind::Ineq
        }

        fn constraint(&mut self, _i: usize, x: &[f64]) -> Result<f64, Self::Error> {
            Ok(1.0 - x[0] * x[0] - x[1] * x[1])
        }
    }

    #[test]
    fn a_closure_declares_no_constraints() {
        let objective = |x: &[f64]| -> Result<f64, Infallible> { Ok(x[0]) };
        assert_eq!(objective.constraint_count(), 0);
    }

    #[test]
    fn a_method_without_constraint_support_rejects_declared_constraints() {
        let problem = Problem {
            x0: vec![0.0, 0.0],
            bounds: None,
        };
        let method = Method::NelderMead(NelderMeadOptions::default());
        let result = minimize(&mut Disc, &problem, &method, &Common::default());
        assert!(matches!(
            result,
            Err(MinimizeError::InvalidInput(InvalidInput::Unsupported {
                method: "Nelder-Mead",
                option: "constraints",
            }))
        ));
    }
}
