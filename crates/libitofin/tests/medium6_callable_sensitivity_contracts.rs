//! Callable sensitivity input, engine-result and frozen-cache regressions.

use std::cell::Cell;

use libitofin::patterns::observable::Observer;
use libitofin::pricingengine::PricingEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};

#[path = "fixtures/medium6_callable_sensitivities/support.rs"]
mod support;

use libitofin::instrument::Instrument;
use libitofin::interestrate::Compounding;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use support::{close, engine, fixed, reference, rights, settings, zero};

#[test]
fn zero_center_and_invalid_inputs_have_explicit_contracts() {
    let settings = settings();
    let engine = engine(settings.clone());
    let bond = zero(settings.clone(), vec![], 0.0);
    assert_eq!(
        bond.effective_duration(
            0.0,
            &engine,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
            None
        )
        .unwrap(),
        0.0
    );
    assert_eq!(
        bond.effective_convexity(
            0.0,
            &engine,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
            None
        )
        .unwrap(),
        0.0
    );
    let bond = fixed(settings, vec![]);
    for bump in [
        0.0,
        -0.001,
        f64::NAN,
        f64::INFINITY,
        f64::MIN_POSITIVE,
        f64::MAX,
    ] {
        assert!(
            bond.effective_duration(
                0.0075,
                &engine,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
                Some(bump)
            )
            .is_err()
        );
        assert!(
            bond.effective_convexity(
                0.0075,
                &engine,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
                Some(bump)
            )
            .is_err()
        );
    }
    for spread in [f64::NAN, f64::INFINITY, f64::MAX, -3.0] {
        assert!(
            bond.effective_duration(
                spread,
                &engine,
                Actual365Fixed::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                None
            )
            .is_err()
        );
    }
    assert!(!bond.base().is_calculated());
}

#[test]
fn immutable_probes_preserve_installed_engine_results_and_frozen_cache() {
    let settings = settings();
    let probe = engine(settings.clone());
    let installed = shared_mut(engine(settings.clone()));
    let attached: SharedMut<dyn PricingEngine> = installed.clone();
    let mut bond = fixed(settings.clone(), rights("call"));
    bond.base_mut().set_pricing_engine(attached.clone());
    let npv = bond.npv().unwrap();
    let settlement = bond.settlement_value().unwrap();
    let installed_value = installed
        .borrow()
        .results()
        .as_instrument_results()
        .unwrap()
        .value;
    struct Counter(Shared<Cell<usize>>);
    impl Observer for Counter {
        fn update(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = shared(Cell::new(0));
    let observer: SharedMut<dyn Observer> = shared_mut(Counter(count.clone()));
    bond.base().register_observer(&observer);
    bond.base_mut().freeze();
    let duration = bond
        .effective_duration(
            0.0075,
            &probe,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
            None,
        )
        .unwrap();
    assert!(duration > 0.0);
    assert!(
        bond.effective_convexity(
            0.0075,
            &probe,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
            Some(0.0)
        )
        .is_err()
    );
    assert_eq!(count.get(), 0);
    assert!(SharedMut::ptr_eq(
        bond.base().pricing_engine().unwrap(),
        &attached
    ));
    assert!(bond.base().is_calculated());
    assert_eq!(
        installed
            .borrow()
            .results()
            .as_instrument_results()
            .unwrap()
            .value,
        installed_value
    );
    close(bond.npv().unwrap(), npv, 0.0);
    assert_eq!(bond.settlement_value().unwrap(), settlement);
    settings.set_evaluation_date(reference() + 1);
    assert_eq!(bond.npv().unwrap(), npv);
    bond.base_mut().unfreeze();
    assert_ne!(bond.settlement_value().unwrap(), settlement);
}
