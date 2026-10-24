use libitofin::instruments::GapPayoff;
use libitofin::methods::finitedifferences::meshers::{FdmMesher, UniformGridMesher};
use libitofin::methods::finitedifferences::operators::FdmLinearOpLayout;
use libitofin::methods::finitedifferences::utilities::{
    FdmInnerValueCalculator, fdm_log_inner_value,
};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;
use libitofin::shared::{Shared, shared};

fn log_mesher() -> Shared<dyn FdmMesher> {
    let center = 100.0_f64.ln();
    shared(
        UniformGridMesher::new(
            shared(FdmLinearOpLayout::new(vec![3])),
            &[(center - 0.2, center + 0.2)],
        )
        .unwrap(),
    )
}

#[test]
fn gap_trigger_is_inclusive_and_negative_payment_is_not_floored() {
    let call = GapPayoff::new(OptionType::Call, 100.0, 120.0);
    let put = GapPayoff::new(OptionType::Put, 100.0, 80.0);
    assert_eq!(call.value(100.0).to_bits(), (-20.0_f64).to_bits());
    assert_eq!(put.value(100.0).to_bits(), (-20.0_f64).to_bits());
    assert_eq!(call.value(99.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(put.value(101.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(call.value(110.0).to_bits(), (-10.0_f64).to_bits());
    assert_eq!(put.value(90.0).to_bits(), (-10.0_f64).to_bits());
}

#[test]
fn negative_gap_cell_retains_point_value_fallback_in_log_space() {
    let mesher = log_mesher();
    let point = mesher.layout().iter().nth(1).unwrap();
    let loc = mesher.location(&point, 0);
    let payoff = GapPayoff::new(OptionType::Call, 50.0, 150.0);
    let calculator = fdm_log_inner_value(shared(payoff), Shared::clone(&mesher), 0);
    let half = mesher.dplus(&point, 0) / 2.0;
    let endpoint_sum = payoff.value((loc - half).exp()) + payoff.value((loc + half).exp());
    assert!(endpoint_sum < 0.0);
    let point_value = payoff.value(loc.exp());
    let actual = calculator.avg_inner_value(&point, 0.0);
    let exact_cell_average = loc.exp() * half.sinh() / half - 150.0;
    assert!(actual.is_finite());
    assert_eq!(actual.to_bits(), point_value.to_bits());
    assert!((exact_cell_average - actual).abs() > 0.1);
}

#[test]
fn cancelling_gap_endpoints_retain_native_accuracy_failure_fallback() {
    let mesher = log_mesher();
    let point = mesher.layout().iter().nth(1).unwrap();
    let loc = mesher.location(&point, 0);
    let half = mesher.dplus(&point, 0) / 2.0;
    let second_strike = ((loc - half).exp() + (loc + half).exp()) / 2.0;
    let payoff = GapPayoff::new(OptionType::Call, 50.0, second_strike);
    let accuracy = (payoff.value((loc - half).exp()) + payoff.value((loc + half).exp())) * 5e-5;
    assert!(accuracy <= f64::EPSILON);
    let calculator = fdm_log_inner_value(shared(payoff), Shared::clone(&mesher), 0);
    let actual = calculator.avg_inner_value(&point, 0.0);
    let exact_cell_average = loc.exp() * half.sinh() / half - second_strike;
    assert!(actual.is_finite());
    assert_eq!(actual.to_bits(), payoff.value(loc.exp()).to_bits());
    assert!((exact_cell_average - actual).abs() > 0.1);
}

#[test]
fn positive_gap_cell_uses_average_instead_of_negative_accuracy_fallback() {
    let mesher = log_mesher();
    let point = mesher.layout().iter().nth(1).unwrap();
    let loc = mesher.location(&point, 0);
    let half = mesher.dplus(&point, 0) / 2.0;
    let payoff = GapPayoff::new(OptionType::Call, 50.0, 20.0);
    let calculator = fdm_log_inner_value(shared(payoff), Shared::clone(&mesher), 0);
    let actual = calculator.avg_inner_value(&point, 0.0);
    let exact_cell_average = loc.exp() * half.sinh() / half - 20.0;
    assert!(actual.is_finite());
    assert!((actual - exact_cell_average).abs() < 1e-5);
    assert!((actual - payoff.value(loc.exp())).abs() > 0.1);
}
