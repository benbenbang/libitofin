use super::*;
use crate::patterns::observable::AsObservable;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::AnalyticEuropeanEngine;
use crate::pricingengines::forward::test_market::{CALLS, Market, PUTS, close, today};
use crate::shared::shared_mut;

#[test]
fn variance_swap_terms_reject_nonfinite_nonpositive_and_invalid_dates() {
    let market = Market::new();
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            VarianceSwap::new(
                Position::Long,
                value,
                50000.0,
                today(),
                today() + 90,
                market.settings.clone()
            )
            .is_err()
        );
        assert!(
            VarianceSwap::new(
                Position::Long,
                0.04,
                value,
                today(),
                today() + 90,
                market.settings.clone()
            )
            .is_err()
        );
    }
    for (start, maturity) in [
        (today(), today()),
        (today() + 1, today()),
        (Date::null(), today()),
        (today(), Date::null()),
    ] {
        assert!(
            VarianceSwap::new(
                Position::Long,
                0.04,
                1.0,
                start,
                maturity,
                market.settings.clone()
            )
            .is_err()
        );
    }
}

#[test]
fn variance_swap_getters_sign_and_at_fair_variance_npv() {
    let market = Market::new();
    let mut long = market.swap(Position::Long, 0.04, CALLS, PUTS, 5.0);
    let mut short = market.swap(Position::Short, 0.04, CALLS, PUTS, 5.0);
    assert_eq!(long.position(), Position::Long);
    assert_eq!(long.strike(), 0.04);
    assert_eq!(long.notional(), 50000.0);
    assert_eq!(long.start_date(), today());
    assert_eq!(long.maturity_date(), today() + 90);
    close(short.npv().unwrap(), -long.npv().unwrap(), 0.0);
    let fair = long.variance().unwrap();
    let mut atm = market.swap(Position::Long, fair, CALLS, PUTS, 5.0);
    assert_eq!(atm.npv().unwrap(), 0.0);
    let mut weights = long.option_weights().unwrap();
    assert_eq!(weights.len(), 19);
    weights[0].weight = 42.0;
    assert_ne!(long.option_weights().unwrap()[0].weight, 42.0);
}

#[test]
fn variance_swap_expiry_clears_variance_and_weights_without_live_market() {
    let market = Market::new();
    let mut swap = market.swap(Position::Long, 0.04, CALLS, PUTS, 5.0);
    swap.npv().unwrap();
    market.settings.set_evaluation_date(today() + 90);
    market.spot.set_value(f64::NAN);
    assert!(swap.is_expired().unwrap());
    assert_eq!(swap.npv().unwrap(), 0.0);
    assert!(swap.variance().is_err());
    assert!(swap.option_weights().is_err());
    assert!(swap.base().is_calculated());
    market.settings.set_include_reference_date_events(true);
    assert!(!swap.is_expired().unwrap());
    assert!(swap.recalculate().is_err());
    assert!(!swap.base().is_calculated());
}

#[test]
fn variance_swap_missing_settings_engine_and_wrong_engine_fail_fallibly() {
    let market = Market::new();
    let mut swap = VarianceSwap::new(
        Position::Long,
        0.04,
        50000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    assert!(
        swap.npv()
            .unwrap_err()
            .message()
            .contains("null pricing engine")
    );
    swap.base_mut()
        .set_pricing_engine(shared_mut(AnalyticEuropeanEngine::new(
            market.process.clone(),
        )));
    assert!(
        swap.npv()
            .unwrap_err()
            .message()
            .contains("wrong variance swap arguments")
    );
    let unset = crate::shared::shared(Settings::new());
    let mut swap =
        VarianceSwap::new(Position::Long, 0.04, 50000.0, today(), today() + 90, unset).unwrap();
    assert!(
        swap.npv()
            .unwrap_err()
            .message()
            .contains("no evaluation date")
    );
}

#[test]
fn variance_swap_observes_all_live_quotes_and_recovers_from_invalid_values() {
    let market = Market::new();
    let mut swap = market.swap(Position::Long, 0.04, CALLS, PUTS, 5.0);
    let initial = swap.variance().unwrap();
    assert!(swap.base().is_calculated());
    for (quote, changed, original) in [
        (&market.spot, 102.0, 100.0),
        (&market.rate, 0.07, 0.05),
        (&market.dividend, 0.03, 0.0),
        (&market.vol, 0.3, 0.2),
    ] {
        quote.set_value(changed);
        assert!(!swap.base().is_calculated());
        assert_ne!(swap.variance().unwrap(), initial);
        quote.set_value(original);
        close(swap.variance().unwrap(), initial, 0.0);
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            quote.set_value(invalid);
            assert!(swap.npv().is_err());
            assert!(swap.variance().is_err());
            assert!(swap.option_weights().is_err());
            assert!(!swap.base().is_calculated());
            quote.set_value(original);
            close(swap.variance().unwrap(), initial, 0.0);
        }
    }
    for invalid in [0.0, -100.0] {
        market.spot.set_value(invalid);
        assert!(swap.npv().is_err());
        market.spot.set_value(100.0);
        close(swap.variance().unwrap(), initial, 0.0);
    }
    market.spot.reset();
    assert!(swap.npv().is_err());
    market.spot.set_value(100.0);
    close(swap.variance().unwrap(), initial, 0.0);
}

#[test]
fn variance_swap_engine_and_process_remain_owned_after_external_owners_drop() {
    let market = Market::new();
    let process = market.process.clone();
    let engine = shared_mut(
        crate::pricingengines::ReplicatingVarianceSwapEngine::new(
            process.clone(),
            5.0,
            CALLS,
            PUTS,
        )
        .unwrap(),
    );
    let mut swap = VarianceSwap::new(
        Position::Long,
        0.04,
        50000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    swap.base_mut().set_pricing_engine(engine.clone());
    drop(engine);
    drop(process);
    drop(market);
    assert!(swap.npv().unwrap().is_finite());
    assert!(swap.variance().unwrap().is_finite());
    assert_eq!(swap.option_weights().unwrap().len(), 19);
}

#[test]
fn variance_swap_settings_change_rejects_seasoned_and_restores_cache() {
    let market = Market::new();
    let mut swap = market.swap(Position::Long, 0.04, CALLS, PUTS, 5.0);
    let original = swap.npv().unwrap();
    market.settings.set_evaluation_date(today() + 1);
    assert!(!swap.base().is_calculated());
    assert!(swap.npv().unwrap_err().message().contains("spot start"));
    market.settings.set_evaluation_date(today());
    close(swap.npv().unwrap(), original, 0.0);
    market.process.observable().notify_observers();
    assert!(!swap.base().is_calculated());
    let wrong_results = VarianceSwapResults {
        instrument: InstrumentResults {
            value: Some(f64::NAN),
            ..InstrumentResults::default()
        },
        variance: Some(0.04),
        ..VarianceSwapResults::default()
    };
    assert!(swap.fetch_results(&wrong_results).is_err());
    let mut wrong = AnalyticEuropeanEngine::new(market.process.clone());
    assert!(swap.fetch_results(wrong.results()).is_err());
    wrong.reset();
}
