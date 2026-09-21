use crate::{
    Bounds, Common, Converged, Flow, InvalidInput, IterationState, Method, Minimize, MinimizeError,
    NelderMeadOptions, Objective, Problem, Termination,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum ProbeError {
    #[error("boom")]
    Boom,
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    value_fails: bool,
    callback: Result<Flow, ProbeError>,
}

fn probe(value_fails: bool, callback: Result<Flow, ProbeError>) -> Probe {
    Probe {
        value_fails,
        callback,
    }
}

impl Objective for Probe {
    type Error = ProbeError;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        if self.value_fails {
            return Err(ProbeError::Boom);
        }
        Ok(x[0])
    }

    fn callback(&mut self, _state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        self.callback
    }
}

#[test]
fn a_closure_is_an_objective() {
    let mut objective = |x: &[f64]| -> Result<f64, ProbeError> { Ok(x[1]) };
    assert_eq!(objective.value(&[1.0, 2.0]).unwrap(), 2.0);
}

#[test]
fn an_objective_supplies_no_gradient_by_default() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let mut out = [0.0; 2];
    assert!(!objective.gradient(&[1.0, 2.0], &mut out).unwrap());
}

#[test]
fn an_objective_error_is_carried_by_value() {
    let error: MinimizeError<ProbeError> = MinimizeError::Objective(ProbeError::Boom);
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.downcast_ref::<ProbeError>(), Some(&ProbeError::Boom));
}

#[test]
fn an_invalid_input_converts_into_a_minimize_error() {
    let error: MinimizeError<ProbeError> = InvalidInput::EmptyX0.into();
    assert_eq!(error.to_string(), "invalid input: x0 must not be empty");
}

#[test]
fn only_a_convergence_is_a_success() {
    assert!(Termination::Converged(Converged::GTol).is_success());
    assert!(!Termination::Cancelled.is_success());
    assert!(!Termination::Nonfinite.is_success());
}

#[test]
fn a_result_derives_its_success_and_message_from_its_status() {
    let result = Minimize::new(
        vec![1.0],
        0.5,
        3,
        7,
        2,
        Termination::Converged(Converged::XTol),
    );
    assert!(result.success);
    assert_eq!(result.message, "converged: step below the x tolerance");
    assert_eq!((result.nit, result.nfev, result.njev), (3, 7, 2));
}

fn problem() -> Problem {
    Problem {
        x0: vec![1.0, 2.0],
        bounds: None,
    }
}

fn bounded(lower: Vec<f64>, upper: Vec<f64>) -> Problem {
    Problem {
        x0: vec![1.0, 2.0],
        bounds: Some(Bounds { lower, upper }),
    }
}

fn common(maxiter: Option<usize>, maxfev: Option<usize>, tol: Option<f64>) -> Common {
    Common {
        maxiter,
        maxfev,
        tol,
    }
}

#[test]
fn x0_must_not_be_empty() {
    let problem = Problem {
        x0: Vec::new(),
        bounds: None,
    };
    assert_eq!(problem.validate(), Err(InvalidInput::EmptyX0));
}

#[test]
fn x0_must_be_finite() {
    let problem = Problem {
        x0: vec![1.0, f64::INFINITY],
        bounds: None,
    };
    let expected = InvalidInput::NonfiniteX0 { index: 1 };
    assert_eq!(problem.validate(), Err(expected));
}

#[test]
fn an_unbounded_problem_is_valid() {
    assert_eq!(problem().validate(), Ok(()));
}

#[test]
fn bounds_must_carry_one_entry_per_coordinate() {
    let expected = InvalidInput::BoundsLength {
        expected: 2,
        found: 1,
    };
    assert_eq!(bounded(vec![0.0], vec![3.0, 3.0]).validate(), Err(expected));
}

#[test]
fn a_nan_bound_is_rejected() {
    let problem = bounded(vec![0.0, f64::NAN], vec![3.0, 3.0]);
    assert_eq!(problem.validate(), Err(InvalidInput::NanBound { index: 1 }));
}

#[test]
fn an_infinite_bound_leaves_that_side_open() {
    let problem = bounded(vec![f64::NEG_INFINITY, 0.0], vec![3.0, f64::INFINITY]);
    assert_eq!(problem.validate(), Ok(()));
}

#[test]
fn a_lower_bound_must_not_exceed_its_upper_bound() {
    let problem = bounded(vec![0.0, 4.0], vec![3.0, 3.0]);
    let expected = InvalidInput::BoundsOrder { index: 1 };
    assert_eq!(problem.validate(), Err(expected));
}

#[test]
fn a_budget_must_be_positive_when_given() {
    let maxiter = InvalidInput::NotPositive { option: "maxiter" };
    assert_eq!(common(Some(0), None, None).validate(), Err(maxiter));
    let maxfev = InvalidInput::NotPositive { option: "maxfev" };
    assert_eq!(common(None, Some(0), None).validate(), Err(maxfev));
    assert_eq!(common(Some(1), Some(1), Some(1e-8)).validate(), Ok(()));
    assert_eq!(Common::default().validate(), Ok(()));
}

#[test]
fn a_tolerance_must_be_positive_when_given() {
    for tol in [0.0, -1.0, f64::NAN] {
        let expected = InvalidInput::NotPositive { option: "tol" };
        assert_eq!(common(None, None, Some(tol)).validate(), Err(expected));
    }
}

#[test]
fn a_method_rejects_an_option_it_does_not_support() {
    let method = Method::NelderMead(NelderMeadOptions::default());
    assert_eq!(method.name(), "Nelder-Mead");
    assert!(!method.supports_bounds());
    assert_eq!(method.validate(&problem()), Ok(()));
    let expected = InvalidInput::Unsupported {
        method: "Nelder-Mead",
        option: "bounds",
    };
    let bounded = bounded(vec![0.0, 0.0], vec![3.0, 3.0]);
    assert_eq!(method.validate(&bounded), Err(expected));
}

#[test]
fn nelder_mead_options_default_to_the_scipy_values() {
    let options = NelderMeadOptions::default();
    assert_eq!(options.xatol, 1e-4);
    assert_eq!(options.fatol, 1e-4);
    assert!(!options.adaptive);
    assert_eq!(options.initial_simplex, None);
}
