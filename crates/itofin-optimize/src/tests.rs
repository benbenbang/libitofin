use crate::{
    Converged, Flow, InvalidInput, IterationState, Minimize, MinimizeError, Objective, Termination,
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
