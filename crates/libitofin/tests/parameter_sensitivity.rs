use libitofin::errors::QlResult;
use libitofin::handle::{AsObservable, Handle};
use libitofin::patterns::observable::Observable;
use libitofin::quotes::{DerivedQuote, Quote, SimpleQuote};
use libitofin::risk::{
    ParameterDerivatives, SensitivityAnalysis, parameter_sensitivity, parameter_sensitivity_vector,
};
use libitofin::shared::{Shared, shared};
use libitofin::{ensure, types::Real};

const X: Real = 0.75;
const H: Real = 0.0625;

type Sensitivities = (ParameterDerivatives, ParameterDerivatives);

fn handle(quote: impl Quote + 'static) -> Handle<dyn Quote> {
    Handle::new(shared(quote) as Shared<dyn Quote>)
}

fn source(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
    Handle::new(Shared::clone(quote) as Shared<dyn Quote>)
}

fn fixture() -> (Shared<SimpleQuote>, Vec<Handle<dyn Quote>>) {
    let quote = shared(SimpleQuote::new(X));
    let parameters = vec![
        handle(DerivedQuote::new(source(&quote), |x| x * x)),
        handle(SimpleQuote::new(None)),
        handle(DerivedQuote::new(source(&quote), |x| 3.0 * x)),
    ];
    (quote, parameters)
}

fn near(actual: Option<Real>, expected: Real) {
    let actual = actual.unwrap();
    assert!((actual - expected).abs() <= 1e-10, "{actual} vs {expected}");
}

#[test]
fn a_bump_reaches_the_derived_parameter() {
    let (quote, parameters) = fixture();
    assert_eq!(
        parameters[0].current_link().unwrap().value().unwrap(),
        X * X
    );
    quote.set_value(2.0 * X);
    assert_eq!(
        parameters[0].current_link().unwrap().value().unwrap(),
        4.0 * X * X
    );
}

#[test]
fn centered_differences_are_exact_for_a_quadratic_and_skip_invalid_parameters() {
    let (quote, parameters) = fixture();
    let (first, second) =
        parameter_sensitivity(&quote, &parameters, None, H, SensitivityAnalysis::Centered).unwrap();
    near(first[0], 2.0 * X);
    near(second[0], 2.0);
    assert_eq!((first[1], second[1]), (None, None));
    near(first[2], 3.0);
    near(second[2], 0.0);
    assert_eq!(quote.value().unwrap(), X);
}

#[test]
fn one_side_differences_have_no_second_order() {
    let (quote, parameters) = fixture();
    let (first, second) =
        parameter_sensitivity(&quote, &parameters, None, H, SensitivityAnalysis::OneSide).unwrap();
    near(first[0], 2.0 * X + H);
    assert_eq!(first[1], None);
    near(first[2], 3.0);
    assert!(second.iter().all(Option::is_none));
    assert_eq!(quote.value().unwrap(), X);
}

#[test]
fn supplied_reference_values_gate_every_slot() {
    let (quote, parameters) = fixture();
    let references = [Some(X * X), None, None];
    let (first, second) = parameter_sensitivity(
        &quote,
        &parameters,
        Some(&references),
        H,
        SensitivityAnalysis::Centered,
    )
    .unwrap();
    near(first[0], 2.0 * X);
    assert_eq!((first[2], second[2]), (None, None));
    let invalid_with_reference = [None, Some(1.0), None];
    assert!(
        parameter_sensitivity(
            &quote,
            &parameters,
            Some(&invalid_with_reference),
            H,
            Default::default()
        )
        .is_err()
    );
    assert_eq!(quote.value().unwrap(), X);
    let message = parameter_sensitivity(&quote, &parameters, Some(&[None]), H, Default::default())
        .unwrap_err()
        .message()
        .to_owned();
    assert_eq!(message, "referenceValues has size 1, instead of 3");
}

#[test]
fn edge_cases_are_errors() {
    let (quote, parameters) = fixture();
    let message = |result: QlResult<Sensitivities>| result.unwrap_err().message().to_owned();
    let kind = SensitivityAnalysis::Centered;
    assert_eq!(
        message(parameter_sensitivity(&quote, &parameters, None, 0.0, kind)),
        "zero shift not allowed"
    );
    assert_eq!(
        message(parameter_sensitivity(&quote, &[], None, H, kind)),
        "empty parameters vector"
    );
    let invalid = shared(SimpleQuote::new(None));
    assert_eq!(
        message(parameter_sensitivity(&invalid, &parameters, None, H, kind)),
        "invalid SimpleQuote"
    );
    assert!(parameter_sensitivity_vector(&[], &parameters, H, kind).is_err());
    assert!(parameter_sensitivity_vector(&[invalid], &parameters, H, kind).is_err());
    assert_eq!(quote.value().unwrap(), X);
}

struct BumpSensitiveQuote {
    source: Shared<SimpleQuote>,
    baseline: Real,
    observable: Observable,
}

impl AsObservable for BumpSensitiveQuote {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Quote for BumpSensitiveQuote {
    fn value(&self) -> QlResult<Real> {
        let value = self.source.value()?;
        ensure!(value == self.baseline, "bumped away from {}", self.baseline);
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        true
    }
}

#[test]
fn a_parameter_failing_after_the_bump_is_an_error_and_restores_the_quote() {
    let (quote, mut parameters) = fixture();
    parameters.push(handle(BumpSensitiveQuote {
        source: Shared::clone(&quote),
        baseline: X,
        observable: Observable::new(),
    }));
    for kind in [SensitivityAnalysis::OneSide, SensitivityAnalysis::Centered] {
        let result = parameter_sensitivity(&quote, &parameters, None, H, kind);
        assert_eq!(result.unwrap_err().message(), "bumped away from 0.75");
        assert_eq!(quote.value().unwrap(), X);
    }
}

#[test]
fn the_vector_form_is_one_single_form_row_per_quote() {
    let (q1, q2) = (shared(SimpleQuote::new(X)), shared(SimpleQuote::new(0.5)));
    let parameters = vec![
        handle(DerivedQuote::new(source(&q1), |x| x * x)),
        handle(DerivedQuote::new(source(&q2), |x| 5.0 * x)),
    ];
    let kind = SensitivityAnalysis::Centered;
    let (first, second) = parameter_sensitivity_vector(
        &[Shared::clone(&q1), Shared::clone(&q2)],
        &parameters,
        H,
        kind,
    )
    .unwrap();
    assert_eq!(first.len(), 2);
    for (row, quote) in [&q1, &q2].into_iter().enumerate() {
        let single = parameter_sensitivity(quote, &parameters, None, H, kind).unwrap();
        assert_eq!((first[row].clone(), second[row].clone()), single);
    }
    near(first[0][0], 2.0 * X);
    near(first[0][1], 0.0);
    near(first[1][0], 0.0);
    near(first[1][1], 5.0);
    assert_eq!((q1.value().unwrap(), q2.value().unwrap()), (X, 0.5));
}
