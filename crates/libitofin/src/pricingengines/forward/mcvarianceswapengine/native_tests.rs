use super::native_cases::{CASES, NativeCase};
use super::*;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::pricingengines::forward::test_market::{Market, curve, quote_handle, today};
use crate::shared::shared_mut;
use crate::termstructures::volatility::{BlackVarianceCurve, BlackVolTermStructure};
use crate::time::daycounters::actual365fixed::Actual365Fixed;

fn compare(actual: f64, expected: f64, absolute: f64) {
    let tolerance = absolute.max(3e-12 * expected.abs());
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

fn fixture(case: &NativeCase) -> VarianceSwap {
    let market = Market::new();
    let [spot, rate, dividend, volatility, strike, notional] = case.market;
    market.spot.set_value(spot);
    market.rate.set_value(rate);
    market.dividend.set_value(dividend);
    market.vol.set_value(volatility);
    let process = match case.kind {
        "external_local_vol" => super::stochastic_tests::process(&market, false),
        "variance_curve" => {
            let volatility = BlackVarianceCurve::new(
                today(),
                &[today() + 36, today() + 90],
                &[0.1, 0.2],
                Actual365Fixed::new(),
                true,
            )
            .unwrap();
            shared(GeneralizedBlackScholesProcess::new(
                quote_handle(&market.spot),
                Handle::new(curve(today(), &market.dividend, Actual365Fixed::new())),
                Handle::new(curve(today(), &market.rate, Actual365Fixed::new())),
                Handle::new(shared(volatility) as Shared<dyn BlackVolTermStructure>),
            ))
        }
        "constant" => market.process.clone(),
        _ => panic!("unknown native case {}", case.name),
    };
    let [days, steps, per_year, samples, maximum, seed] = case.counts;
    let mut result = VarianceSwap::new(
        if case.short {
            Position::Short
        } else {
            Position::Long
        },
        strike,
        notional,
        today(),
        today() + days as i32,
        market.settings.clone(),
    )
    .unwrap();
    let engine = MCVarianceSwapEngine::new(
        process,
        (steps > 0).then_some(steps),
        (per_year > 0).then_some(per_year),
        (samples > 0).then_some(samples),
        (case.tolerance > 0.0).then_some(case.tolerance),
        (maximum > 0).then_some(maximum),
        seed as u64,
    )
    .unwrap();
    let steps = engine.config.time_steps(days as f64 / 365.0).unwrap();
    let grid = TimeGrid::new(days as f64 / 365.0, steps).unwrap();
    assert_eq!(
        [steps, (grid.back().unwrap() / grid.dt(0)) as usize],
        case.grid
    );
    result.base_mut().set_pricing_engine(shared_mut(engine));
    result
}

#[test]
fn pinned_native_fixed_seed_cases_and_adaptive_sample_counts() {
    for case in CASES {
        let mut swap = fixture(case);
        compare(swap.variance().unwrap(), case.expected[0], 2e-14);
        compare(swap.npv().unwrap(), case.expected[1], 2e-9);
        compare(swap.variance_error().unwrap(), case.expected[2], 2e-14);
        compare(swap.error_estimate().unwrap(), case.expected[3], 2e-9);
        assert_eq!(swap.samples().unwrap(), case.samples, "{}", case.name);
        assert!(swap.option_weights().unwrap().is_empty());
        if case.name == "literature_curve" {
            assert!((swap.variance().unwrap() - 0.04).abs() <= 3e-4);
        }
    }
}
