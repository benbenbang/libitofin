//! Native zero-lag and independently corrected positive-lag Black inversions.

#[path = "fixtures/medium6_callable_iv/support.rs"]
mod support;

use libitofin::handle::Handle;
use libitofin::instruments::BondPrice;
use libitofin::patterns::observable::Observer;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::BlackCallableFixedRateBondEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::time::date::Date;
use std::cell::Cell;
use support::{close, curve, date, product, settings};

#[test]
fn sixteen_independent_fixed_zero_call_put_settlement_inversions() {
    for row in include_str!("fixtures/medium6_callable_iv/reference.tsv")
        .lines()
        .skip(1)
    {
        let columns: Vec<&str> = row.split('\t').collect();
        let bond = product(
            columns[0],
            columns[1],
            columns[2].parse().unwrap(),
            columns[3].parse().unwrap(),
            settings(),
        );
        let values: Vec<f64> = columns[5..].iter().map(|x| x.parse().unwrap()).collect();
        let term = curve(columns[4]);
        let clean = bond
            .iv(
                BondPrice::Clean(values[7]),
                term.clone(),
                1e-10,
                300,
                0.0,
                1.0,
            )
            .unwrap();
        let dirty = bond
            .iv(BondPrice::Dirty(values[6]), term, 1e-10, 300, 0.0, 1.0)
            .unwrap();
        close(clean, values[8], 2e-8);
        close(dirty, values[9], 2e-8);
        close(clean, values[0], 2e-8);
        close(dirty, values[0], 2e-8);
        assert!(!bond.instrument().base().is_calculated());
        assert!(bond.instrument().base().pricing_engine().is_none());
        if columns[3] == "0" {
            close(values[6], values[2], 2e-9);
            close(clean, values[4], 2e-8);
        } else {
            assert!((values[6] - values[2]).abs() > 1e-6);
            let native_target_vol = bond
                .iv(
                    BondPrice::Dirty(values[2]),
                    curve(columns[4]),
                    1e-10,
                    300,
                    0.0,
                    1.0,
                )
                .unwrap();
            assert!((native_target_vol - values[0]).abs() > 1e-6);
        }
    }
}

#[test]
fn zero_and_upper_endpoint_roots_are_supported() {
    for kind in ["fixed", "zero"] {
        for endpoint in [0.0, 1.0] {
            let settings = settings();
            let mut bond = product(kind, "call", 1000.0, 17, settings.clone());
            let engine = shared_mut(BlackCallableFixedRateBondEngine::new(
                Handle::new(shared(SimpleQuote::new(endpoint)) as Shared<dyn Quote>),
                curve("nonflat"),
                settings,
            ));
            bond.instrument_mut().base_mut().set_pricing_engine(engine);
            let target = bond.price();
            let root = bond
                .iv(
                    BondPrice::Clean(target),
                    curve("nonflat"),
                    1e-10,
                    300,
                    0.0,
                    1.0,
                )
                .unwrap();
            assert_eq!(root, endpoint);
        }
    }
}

struct Counter(Shared<Cell<usize>>);
impl Observer for Counter {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn success_and_failure_preserve_cached_engine_results_observers_and_freeze() {
    for kind in ["fixed", "zero"] {
        let settings = settings();
        let mut bond = product(kind, "call", 1000.0, 17, settings.clone());
        let quote = shared(SimpleQuote::new(0.2));
        let engine = shared_mut(BlackCallableFixedRateBondEngine::new(
            Handle::new(quote.clone() as Shared<dyn Quote>),
            curve("nonflat"),
            settings,
        ));
        bond.instrument_mut()
            .base_mut()
            .set_pricing_engine(engine.clone());
        let cached = bond.instrument_mut().npv().unwrap();
        let target = bond.price();
        let notifications = shared(Cell::new(0));
        let observer: SharedMut<dyn Observer> = shared_mut(Counter(notifications.clone()));
        bond.instrument().base().register_observer(&observer);
        let saved_engine = bond.instrument().base().pricing_engine().unwrap().clone();
        let saved_engine_value = engine
            .borrow()
            .results()
            .as_instrument_results()
            .unwrap()
            .value;
        close(
            bond.iv(
                BondPrice::Clean(target),
                curve("nonflat"),
                1e-10,
                300,
                0.0,
                1.0,
            )
            .unwrap(),
            0.2,
            2e-8,
        );
        assert_eq!(notifications.get(), 0);
        assert!(bond.instrument().base().is_calculated());
        assert_eq!(
            bond.instrument_mut().npv().unwrap().to_bits(),
            cached.to_bits()
        );
        bond.instrument_mut().base_mut().freeze();
        close(
            bond.iv(
                BondPrice::Clean(target),
                curve("nonflat"),
                1e-10,
                300,
                0.0,
                1.0,
            )
            .unwrap(),
            0.2,
            2e-8,
        );
        for target in [f64::NAN, 1e10, -1e10] {
            assert!(
                bond.iv(
                    BondPrice::Dirty(target),
                    curve("nonflat"),
                    1e-10,
                    300,
                    0.0,
                    1.0
                )
                .is_err()
            );
        }
        assert!(SharedMut::ptr_eq(
            &saved_engine,
            bond.instrument().base().pricing_engine().unwrap()
        ));
        assert_eq!(
            engine
                .borrow()
                .results()
                .as_instrument_results()
                .unwrap()
                .value,
            saved_engine_value
        );
        assert_eq!(quote.value().unwrap(), 0.2);
        assert_eq!(notifications.get(), 0);
        assert!(bond.instrument().base().is_calculated());
        assert_eq!(
            bond.instrument_mut().npv().unwrap().to_bits(),
            cached.to_bits()
        );
        quote.set_value(0.8);
        assert_eq!(notifications.get(), 0);
        assert_eq!(
            bond.instrument_mut().npv().unwrap().to_bits(),
            cached.to_bits()
        );
        bond.instrument_mut().base_mut().unfreeze();
        assert_eq!(notifications.get(), 1);
        assert!((bond.instrument_mut().npv().unwrap() - cached).abs() > 1.0);
    }
}

#[test]
fn targets_controls_empty_curves_expiry_and_date_extremes_fail_closed() {
    for kind in ["fixed", "zero"] {
        let settings = settings();
        let bond = product(kind, "put", 100.0, 2, settings.clone());
        for target in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -100.0,
            0.0,
            1e10,
        ] {
            assert!(
                bond.iv(
                    BondPrice::Dirty(target),
                    curve("flat"),
                    1e-10,
                    300,
                    0.0,
                    1.0
                )
                .is_err()
            );
        }
        for (accuracy, budget, min, max) in [
            (0.0, 300, 0.0, 1.0),
            (f64::NAN, 300, 0.0, 1.0),
            (f64::INFINITY, 300, 0.0, 1.0),
            (1e-10, 0, 0.0, 1.0),
            (1e-10, 1, 0.0, 1.0),
            (1e-10, 300, -0.1, 1.0),
            (1e-10, 300, f64::NAN, 1.0),
            (1e-10, 300, 0.1, 0.1),
            (1e-10, 300, 0.2, 0.1),
            (1e-10, 300, 0.0, f64::INFINITY),
        ] {
            assert!(
                bond.iv(
                    BondPrice::Dirty(110.0),
                    curve("flat"),
                    accuracy,
                    budget,
                    min,
                    max
                )
                .is_err()
            );
        }
        assert!(
            bond.iv(
                BondPrice::Clean(110.0),
                Handle::empty(),
                1e-10,
                300,
                0.0,
                1.0
            )
            .is_err()
        );
        settings.set_evaluation_date(date("2034-01-15"));
        assert!(
            bond.iv(BondPrice::Clean(110.0), curve("flat"), 1e-10, 300, 0.0, 1.0)
                .is_err()
        );
        settings.set_evaluation_date(Date::max_date());
        assert!(
            bond.iv(BondPrice::Clean(110.0), curve("flat"), 1e-10, 300, 0.0, 1.0)
                .is_err()
        );
        settings.set_evaluation_date(Date::null());
        assert!(
            bond.iv(BondPrice::Clean(110.0), curve("flat"), 1e-10, 300, 0.0, 1.0)
                .is_err()
        );
    }
}

#[test]
fn quoted_unit_residuals_preserve_extreme_face_scaling() {
    for row in include_str!("fixtures/medium6_callable_iv/reference.tsv")
        .lines()
        .skip(1)
    {
        let columns: Vec<&str> = row.split('\t').collect();
        if columns[3] != "0" {
            continue;
        }
        let target: f64 = columns[12].parse().unwrap();
        for face in [1e-100, 1e-12, 100.0, 1e20, 1e100, 1e306] {
            let bond = product(columns[0], columns[1], face, 0, settings());
            let implied = bond
                .iv(
                    BondPrice::Clean(target),
                    curve("flat"),
                    1e-10,
                    300,
                    0.0,
                    1.0,
                )
                .unwrap();
            close(implied, 0.3, 2e-8);
        }
    }
}

#[test]
fn no_multi_past_or_maturity_exercises_are_accepted() {
    use libitofin::instruments::{Callability, CallabilityType};
    use support::product_with_rights;
    let right =
        |day| Callability::new(BondPrice::Clean(100.0), CallabilityType::Call, date(day)).unwrap();
    for kind in ["fixed", "zero"] {
        for rights in [
            vec![],
            vec![right("2029-04-15"), right("2030-04-15")],
            vec![right("2026-10-13")],
            vec![right("2033-01-15")],
        ] {
            let bond = product_with_rights(kind, 100.0, 2, rights, settings());
            assert!(
                bond.iv(BondPrice::Clean(90.0), curve("flat"), 1e-10, 300, 0.0, 1.0)
                    .is_err()
            );
            assert!(!bond.instrument().base().is_calculated());
        }
    }
}
