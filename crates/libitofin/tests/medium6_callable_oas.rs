//! QuantLib 1.43 native spreads and independently normalized settlement quotes.

#[path = "fixtures/medium6_callable_oas/support.rs"]
mod support;

#[path = "fixtures/medium6_callable_oas/scaling.rs"]
mod scaling;

use libitofin::instrument::Instrument;
use libitofin::instruments::CallableBondArguments;
use libitofin::interestrate::Compounding;
use libitofin::pricingengine::PricingEngine;
use libitofin::shared::{Shared, shared_mut};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use libitofin::time::frequency::Frequency;
use support::{calls, close, date, engine, fixed, maturity, reference, settings, zero};

#[test]
fn thirty_native_compounding_daycounter_and_settlement_cases() {
    for line in include_str!("fixtures/medium6_callable_oas/reference.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = line.split('\t').collect();
        let lag = c[1].parse().unwrap();
        let comp = [
            Compounding::Continuous,
            Compounding::Compounded,
            Compounding::Simple,
            Compounding::SimpleThenCompounded,
            Compounding::CompoundedThenSimple,
        ][c[2].parse::<usize>().unwrap()];
        let dc: DayCounter = if c[3] == "365" {
            Actual365Fixed::new()
        } else {
            Actual360::new()
        };
        let spread = c[4].parse().unwrap();
        let expected = c[6].parse().unwrap();
        let s = settings();
        let e = engine(s.clone());
        let (price, recovered) = if c[0] == "fixed" {
            let b = fixed(lag, s, calls());
            (
                b.clean_price_oas(spread, &e, dc.clone(), comp, Frequency::Semiannual, None)
                    .unwrap(),
                b.oas(
                    expected,
                    &e,
                    dc,
                    comp,
                    Frequency::Semiannual,
                    None,
                    1e-10,
                    100,
                    0.0,
                )
                .unwrap(),
            )
        } else {
            let b = zero(lag, s, calls());
            (
                b.clean_price_oas(spread, &e, dc.clone(), comp, Frequency::Semiannual, None)
                    .unwrap(),
                b.oas(
                    expected,
                    &e,
                    dc,
                    comp,
                    Frequency::Semiannual,
                    None,
                    1e-10,
                    100,
                    0.0,
                )
                .unwrap(),
            )
        };
        close(price, expected, 2e-10);
        close(recovered, spread, 2e-10);
        if lag == 0 {
            close(price, c[5].parse().unwrap(), 2e-10);
        }
    }
}

#[test]
fn pure_probes_preserve_bond_and_engine_state_on_success_and_failure() {
    let s = settings();
    let e = shared_mut(engine(s.clone()));
    let mut b = fixed(2, s, calls());
    b.base_mut().set_pricing_engine(e.clone());
    let old = b.npv().unwrap();
    let engine_value = e.borrow().results().as_instrument_results().unwrap().value;
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
    let observer = shared_mut(support::UpdateCounter(notifications.clone()));
    b.base().register_observer(&(observer.clone() as _));
    b.base_mut().freeze();
    let price = b
        .clean_price_oas(
            0.015,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
        )
        .unwrap();
    let recovered = b
        .oas(
            price,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
            1e-10,
            100,
            0.0,
        )
        .unwrap();
    close(recovered, 0.015, 1e-10);
    assert!(
        b.oas(
            price,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
            1e-10,
            1,
            0.0
        )
        .is_err()
    );
    assert_eq!(b.npv().unwrap().to_bits(), old.to_bits());
    assert_eq!(
        e.borrow().results().as_instrument_results().unwrap().value,
        engine_value
    );
    assert!(Shared::ptr_eq(
        b.base().pricing_engine().unwrap(),
        &(e.clone() as _)
    ));
    assert!(b.base().is_calculated());
    assert_eq!(notifications.get(), 0);
    b.base_mut().unfreeze();
    assert_eq!(notifications.get(), 1);
}

#[test]
fn explicit_settlement_and_exact_event_flags_match_rebuilt_contracts() {
    let s = settings();
    let e = engine(s.clone());
    let b = fixed(0, s.clone(), calls());
    for settle in [
        date(7, Month::January, 2027),
        date(7, Month::August, 2028),
        date(7, Month::January, 2029),
    ] {
        let shifted = fixed((settle - reference()) as u32, s.clone(), calls());
        let expected = shifted
            .clean_price_oas(
                0.0,
                &e,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::NoFrequency,
                None,
            )
            .unwrap();
        for include in [false, true] {
            s.set_include_reference_date_events(include);
            s.set_include_todays_cash_flows(Some(include));
            let price = b
                .clean_price_oas(
                    0.0,
                    &e,
                    Actual365Fixed::new(),
                    Compounding::Continuous,
                    Frequency::NoFrequency,
                    Some(settle),
                )
                .unwrap();
            close(price, expected, 1e-12);
        }
    }
}

#[test]
fn zero_spread_matches_existing_engine_and_rejects_invalid_inputs() {
    let s = settings();
    let e = shared_mut(engine(s.clone()));
    let mut b = zero(2, s.clone(), calls());
    b.base_mut().set_pricing_engine(e.clone());
    let ordinary = b.clean_price().unwrap();
    close(
        b.clean_price_oas(
            0.0,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
        )
        .unwrap(),
        ordinary,
        1e-12,
    );
    for spread in [f64::NAN, f64::INFINITY, -1e308] {
        assert!(
            b.clean_price_oas(
                spread,
                &e.borrow(),
                Actual365Fixed::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                None
            )
            .is_err()
        );
    }
    for settle in [Date::null(), reference() - 1, maturity(), maturity() + 1] {
        assert!(
            b.clean_price_oas(
                0.0,
                &e.borrow(),
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::NoFrequency,
                Some(settle)
            )
            .is_err()
        );
    }
    assert!(
        b.clean_price_oas(
            0.0,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Compounded,
            Frequency::NoFrequency,
            None
        )
        .is_err()
    );
    assert!(
        b.oas(
            f64::NAN,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
            1e-10,
            100,
            0.0
        )
        .is_err()
    );
    assert!(
        b.oas(
            ordinary,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None,
            0.0,
            100,
            0.0
        )
        .is_err()
    );
    let mut args = CallableBondArguments::default();
    b.setup_arguments(&mut args).unwrap();
    assert!(e.borrow().price_with_spread(&args, f64::INFINITY).is_err());
    s.set_evaluation_date(Date::max_date());
    assert!(
        b.clean_price_oas(
            0.0,
            &e.borrow(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::NoFrequency,
            None
        )
        .is_err()
    );
}
