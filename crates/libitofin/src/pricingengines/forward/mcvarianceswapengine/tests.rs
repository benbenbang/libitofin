use super::*;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::pricingengines::forward::test_market::{Market, close, curve, quote_handle, today};
use crate::shared::shared_mut;
use crate::termstructures::volatility::{BlackVarianceCurve, BlackVolTermStructure};
use crate::time::daycounters::actual365fixed::Actual365Fixed;

#[allow(clippy::too_many_arguments)]
fn swap(
    market: &Market,
    process: Shared<GeneralizedBlackScholesProcess>,
    position: Position,
    steps: Option<usize>,
    steps_per_year: Option<usize>,
    samples: Option<usize>,
    tolerance: Option<f64>,
    max_samples: Option<usize>,
    seed: u64,
) -> VarianceSwap {
    let mut result = VarianceSwap::new(
        position,
        0.04,
        50_000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    result.base_mut().set_pricing_engine(shared_mut(
        MCVarianceSwapEngine::new(
            process,
            steps,
            steps_per_year,
            samples,
            tolerance,
            max_samples,
            seed,
        )
        .unwrap(),
    ));
    result
}

#[test]
fn native_time_dependent_variance_curve_case() {
    let market = Market::new();
    let volatility = BlackVarianceCurve::new(
        today(),
        &[today() + 36, today() + 90],
        &[0.1, 0.2],
        Actual365Fixed::new(),
        true,
    )
    .unwrap();
    let process = shared(GeneralizedBlackScholesProcess::new(
        quote_handle(&market.spot),
        Handle::new(curve(today(), &market.dividend, Actual365Fixed::new())),
        Handle::new(curve(today(), &market.rate, Actual365Fixed::new())),
        Handle::new(shared(volatility) as Shared<dyn BlackVolTermStructure>),
    ));
    let mut result = swap(
        &market,
        process,
        Position::Long,
        None,
        Some(250),
        Some(1023),
        None,
        None,
        42,
    );
    close(result.variance().unwrap(), 0.04, 3e-4);
    assert_eq!(result.samples().unwrap(), 1023);
    assert!(result.variance_error().unwrap() >= 0.0);
    assert!(result.option_weights().unwrap().is_empty());
}

#[test]
fn flat_zero_short_and_recalculation() {
    let market = Market::new();
    let mut long = swap(
        &market,
        market.process.clone(),
        Position::Long,
        Some(10),
        None,
        Some(1023),
        None,
        None,
        42,
    );
    let mut short = swap(
        &market,
        market.process.clone(),
        Position::Short,
        Some(10),
        None,
        Some(1023),
        None,
        None,
        42,
    );
    close(long.variance().unwrap(), 0.04, 2e-14);
    assert_eq!(
        long.variance().unwrap().to_bits(),
        short.variance().unwrap().to_bits()
    );
    assert_eq!(long.npv().unwrap(), -short.npv().unwrap());
    assert_eq!(
        long.error_estimate().unwrap(),
        -short.error_estimate().unwrap()
    );
    let first = long.variance().unwrap().to_bits();
    long.recalculate().unwrap();
    assert_eq!(long.variance().unwrap().to_bits(), first);
    market.vol.set_value(0.3);
    close(long.variance().unwrap(), 0.09, 2e-14);
    market.vol.set_value(0.0);
    assert_eq!(long.variance().unwrap(), 0.0);
    assert_eq!(long.variance_error().unwrap(), 0.0);
    assert!(long.npv().unwrap() < 0.0);
}

#[test]
fn tolerance_mode_reports_actual_initial_batch() {
    let market = Market::new();
    let mut result = swap(
        &market,
        market.process.clone(),
        Position::Long,
        Some(10),
        None,
        None,
        Some(1e-12),
        Some(1023),
        42,
    );
    close(result.variance().unwrap(), 0.04, 2e-14);
    assert!(result.variance_error().unwrap() <= 1e-12);
    assert_eq!(result.samples().unwrap(), 1023);
}

#[test]
fn invalid_modes_limits_and_seeds_are_rejected() {
    let market = Market::new();
    for (steps, per_year, samples, tolerance, maximum, seed) in [
        (None, None, Some(2), None, None, 42),
        (Some(1), Some(1), Some(2), None, None, 42),
        (Some(0), None, Some(2), None, None, 42),
        (Some(100_001), None, Some(2), None, None, 42),
        (Some(1), None, None, None, None, 42),
        (Some(1), None, Some(2), Some(0.1), None, 42),
        (Some(1), None, Some(1), None, None, 42),
        (Some(1), None, Some(1_000_001), None, None, 42),
        (Some(1), None, Some(3), None, Some(2), 42),
        (Some(1), None, None, Some(0.0), None, 42),
        (Some(1), None, None, Some(f64::NAN), None, 42),
        (Some(1), None, None, Some(0.1), Some(1022), 42),
        (Some(100_000), None, Some(1023), None, None, 42),
        (Some(1), None, Some(2), None, None, u32::MAX as u64 + 1),
    ] {
        assert!(
            MCVarianceSwapEngine::new(
                market.process.clone(),
                steps,
                per_year,
                samples,
                tolerance,
                maximum,
                seed
            )
            .is_err()
        );
    }
}

#[test]
fn failures_do_not_return_old_results_and_expiry_clears_sampling() {
    let market = Market::new();
    let mut result = swap(
        &market,
        market.process.clone(),
        Position::Long,
        Some(10),
        None,
        Some(2),
        None,
        None,
        42,
    );
    assert!(result.variance().is_ok());
    market.spot.set_value(-1.0);
    assert!(result.npv().is_err());
    assert!(result.variance_error().is_err());
    assert!(result.samples().is_err());
    market.spot.set_value(100.0);
    close(result.variance().unwrap(), 0.04, 2e-14);
    market.settings.set_evaluation_date(today() + 1);
    assert!(result.npv().is_err());
    market.settings.set_evaluation_date(today() + 90);
    assert_eq!(result.npv().unwrap(), 0.0);
    assert_eq!(result.error_estimate().unwrap(), 0.0);
    assert!(result.samples().is_err());
    assert!(result.variance_error().is_err());
    assert!(result.option_weights().is_err());
}

#[test]
fn replacement_with_replication_clears_sampling_fields() {
    use crate::pricingengines::ReplicatingVarianceSwapEngine;
    use crate::pricingengines::forward::test_market::{CALLS, PUTS};
    let market = Market::new();
    let mut result = swap(
        &market,
        market.process.clone(),
        Position::Long,
        Some(10),
        None,
        Some(2),
        None,
        None,
        42,
    );
    assert_eq!(result.samples().unwrap(), 2);
    assert!(result.error_estimate().unwrap().is_finite());
    result.base_mut().set_pricing_engine(shared_mut(
        ReplicatingVarianceSwapEngine::new(market.process.clone(), 5.0, CALLS, PUTS).unwrap(),
    ));
    assert!(result.variance().unwrap().is_finite());
    assert!(result.samples().is_err());
    assert!(result.variance_error().is_err());
    assert!(result.error_estimate().is_err());
    assert!(!result.option_weights().unwrap().is_empty());
}

#[test]
fn per_year_grid_truncates_with_minimum_one_and_checks_actual_work() {
    let market = Market::new();
    let mut result = swap(
        &market,
        market.process.clone(),
        Position::Long,
        None,
        Some(1),
        Some(2),
        None,
        None,
        42,
    );
    close(result.variance().unwrap(), 0.04, 2e-14);
    let mut over_work = swap(
        &market,
        market.process.clone(),
        Position::Long,
        None,
        Some(100_000),
        Some(2048),
        None,
        None,
        42,
    );
    assert!(over_work.variance().is_err());
    let mut longer = VarianceSwap::new(
        Position::Long,
        0.04,
        50_000.0,
        today(),
        today() + 730,
        market.settings.clone(),
    )
    .unwrap();
    longer.base_mut().set_pricing_engine(shared_mut(
        MCVarianceSwapEngine::new(
            market.process.clone(),
            None,
            Some(100_000),
            Some(1023),
            None,
            None,
            42,
        )
        .unwrap(),
    ));
    assert!(longer.variance().is_err());
}
