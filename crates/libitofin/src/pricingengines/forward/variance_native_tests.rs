use super::ReplicatingVarianceSwapEngine;
use super::test_market::{Market, curve, quote_handle, today};
use super::variance_native_cases::CASES;
use super::variance_native_weights::LITERATURE_WEIGHTS;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::math::matrix::Matrix;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared, shared_mut};
use crate::termstructures::volatility::{BlackVarianceSurface, BlackVolTermStructure};
use crate::time::daycounters::actual365fixed::Actual365Fixed;

fn native_close(actual: f64, expected: f64, absolute: f64) {
    let tolerance = absolute.max(3e-12 * expected.abs());
    assert!(
        actual.is_finite() && expected.is_finite() && (actual - expected).abs() <= tolerance,
        "{actual} != {expected}, tolerance {tolerance}"
    );
}

#[test]
fn variance_replication_matches_fifteen_independent_pinned_native_cases() {
    for case in CASES {
        let market = Market::new();
        market.spot.set_value(case.spot);
        market.rate.set_value(case.rate);
        market.dividend.set_value(case.dividend);
        market.vol.set_value(case.vol);
        let process = if case.name.contains("literature") {
            let strikes: Vec<_> = (50..=135).step_by(5).map(f64::from).collect();
            let smile = [
                0.30, 0.29, 0.28, 0.27, 0.26, 0.25, 0.24, 0.23, 0.22, 0.21, 0.20, 0.19, 0.18, 0.17,
                0.16, 0.15, 0.14, 0.13,
            ];
            let mut values = Matrix::with_size(smile.len(), 1);
            for (i, value) in smile.iter().enumerate() {
                values[(i, 0)] = *value;
            }
            let volatility = shared(
                BlackVarianceSurface::new(
                    today(),
                    None,
                    &[today() + case.days],
                    strikes,
                    &values,
                    Actual365Fixed::new(),
                )
                .unwrap(),
            ) as Shared<dyn BlackVolTermStructure>;
            shared(GeneralizedBlackScholesProcess::new(
                quote_handle(&market.spot),
                Handle::new(curve(today(), &market.dividend, Actual365Fixed::new())),
                Handle::new(curve(today(), &market.rate, Actual365Fixed::new())),
                Handle::new(volatility),
            ))
        } else {
            market.process.clone()
        };
        let mut swap = VarianceSwap::new(
            case.position,
            case.strike,
            case.notional,
            today(),
            today() + case.days,
            market.settings.clone(),
        )
        .unwrap();
        swap.base_mut().set_pricing_engine(shared_mut(
            ReplicatingVarianceSwapEngine::new(process, case.dk, case.calls, case.puts).unwrap(),
        ));
        native_close(swap.variance().unwrap(), case.variance, 2e-14);
        native_close(swap.npv().unwrap(), case.npv, 2e-9);
        if case.name.contains("literature") {
            let actual = swap.option_weights().unwrap();
            assert_eq!(actual.len(), LITERATURE_WEIGHTS.len());
            for (actual, &(option_type, strike, weight)) in actual.iter().zip(LITERATURE_WEIGHTS) {
                assert_eq!(actual.option_type, option_type);
                assert_eq!(actual.strike, strike);
                native_close(actual.weight, weight, 2e-14);
            }
        }
    }
}
