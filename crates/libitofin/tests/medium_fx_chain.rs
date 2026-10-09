use libitofin::currency::Currency;
use libitofin::exchangerate::{ExchangeRate, ExchangeRateType};
use libitofin::money::Money;

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 8.0 * f64::EPSILON * expected.abs().max(1.0),
        "{actual} differs from {expected}"
    );
}

fn rate(source: Currency, target: Currency, value: f64) -> ExchangeRate {
    ExchangeRate::checked_new(source, target, value).unwrap()
}

#[test]
fn compiled_quantlib_four_orientations_and_reversed_arguments() {
    let eur = Currency::eur();
    let usd = Currency::usd();
    let gbp = Currency::gbp();
    for (first, second, source, target, stored, reversed, forward, backward, shared) in [
        (
            rate(eur.clone(), usd.clone(), 1.2),
            rate(eur.clone(), gbp.clone(), 0.8),
            usd.clone(),
            gbp.clone(),
            0.6666666666666667,
            1.4999999999999998,
            66.66666666666667,
            150.0,
            eur.clone(),
        ),
        (
            rate(eur.clone(), usd.clone(), 1.2),
            rate(gbp.clone(), eur.clone(), 0.8),
            usd.clone(),
            gbp.clone(),
            1.0416666666666667,
            0.96,
            104.16666666666667,
            96.0,
            eur.clone(),
        ),
        (
            rate(eur.clone(), usd.clone(), 1.2),
            rate(usd.clone(), gbp.clone(), 0.8),
            eur.clone(),
            gbp.clone(),
            0.96,
            1.0416666666666667,
            96.0,
            104.16666666666667,
            usd.clone(),
        ),
        (
            rate(eur.clone(), usd.clone(), 1.2),
            rate(gbp.clone(), usd.clone(), 0.8),
            eur.clone(),
            gbp.clone(),
            1.4999999999999998,
            0.6666666666666667,
            150.0,
            66.66666666666667,
            usd.clone(),
        ),
    ] {
        let chain = ExchangeRate::chain(&first, &second).unwrap();
        let reverse = ExchangeRate::chain(&second, &first).unwrap();
        assert_eq!(chain.source(), &source);
        assert_eq!(chain.target(), &target);
        assert_eq!(reverse.source(), &target);
        assert_eq!(reverse.target(), &source);
        close(chain.rate(), stored);
        close(reverse.rate(), reversed);
        for derived in [&chain, &reverse] {
            assert_eq!(derived.rate_type(), ExchangeRateType::Derived);
            for sign in [1.0, -1.0, 0.0] {
                let out = derived
                    .exchange(&Money::new(source.clone(), sign * 100.0))
                    .unwrap();
                assert_eq!(out.currency(), &target);
                close(out.value(), sign * forward);
                let out = derived
                    .exchange(&Money::new(target.clone(), sign * 100.0))
                    .unwrap();
                assert_eq!(out.currency(), &source);
                close(out.value(), sign * backward);
            }
            assert!(
                derived
                    .exchange(&Money::new(shared.clone(), 100.0))
                    .is_err()
            );
            assert!(
                derived
                    .exchange(&Money::new(Currency::jpy(), 100.0))
                    .is_err()
            );
        }
    }
}

#[test]
fn compiled_quantlib_nested_chain_is_owned_and_ordered() {
    let chain = {
        let eur_usd = rate(Currency::eur(), Currency::usd(), 1.2);
        let usd_gbp = rate(Currency::usd(), Currency::gbp(), 0.8);
        ExchangeRate::chain(&eur_usd, &usd_gbp).unwrap()
    };
    let gbp_jpy = rate(Currency::gbp(), Currency::jpy(), 150.0);
    let nested = ExchangeRate::chain(&chain, &gbp_jpy).unwrap();
    let reverse = ExchangeRate::chain(&gbp_jpy, &chain).unwrap();
    close(nested.rate(), 144.0);
    close(reverse.rate(), 0.006944444444444444);
    for derived in [nested.clone(), reverse] {
        for sign in [1.0, -1.0, 0.0] {
            let out = derived
                .exchange(&Money::new(Currency::eur(), sign * 100.0))
                .unwrap();
            assert_eq!(out.currency(), &Currency::jpy());
            close(out.value(), sign * 14400.0);
            let out = derived
                .exchange(&Money::new(Currency::jpy(), sign * 100.0))
                .unwrap();
            assert_eq!(out.currency(), &Currency::eur());
            close(out.value(), sign * 0.6944444444444444);
        }
        for currency in [Currency::usd(), Currency::gbp()] {
            assert!(derived.exchange(&Money::new(currency, 100.0)).is_err());
        }
    }
}

#[test]
fn compiled_quantlib_overlapping_endpoints_preserve_first_match() {
    let first = rate(Currency::eur(), Currency::usd(), 1.2);
    for (second, stored, euros, dollars) in [
        (
            rate(Currency::eur(), Currency::usd(), 1.3),
            1.0833333333333335,
            92.3076923076923,
            108.33333333333334,
        ),
        (
            rate(Currency::usd(), Currency::eur(), 1.3),
            0.641025641025641,
            156.0,
            64.1025641025641,
        ),
    ] {
        let chain = ExchangeRate::chain(&first, &second).unwrap();
        assert_eq!(chain.source(), &Currency::usd());
        assert_eq!(chain.target(), &Currency::usd());
        close(chain.rate(), stored);
        let eur = chain.exchange(&Money::new(Currency::eur(), 100.0)).unwrap();
        assert_eq!(eur.currency(), &Currency::eur());
        close(eur.value(), euros);
        let usd = chain.exchange(&Money::new(Currency::usd(), 100.0)).unwrap();
        assert_eq!(usd.currency(), &Currency::usd());
        close(usd.value(), dollars);
    }
}

#[test]
fn lineage_does_not_flatten_intermediate_underflow_or_overflow() {
    let underflow = ExchangeRate::chain(
        &rate(Currency::eur(), Currency::usd(), 1e-200),
        &rate(Currency::usd(), Currency::gbp(), 1e200),
    )
    .unwrap();
    close(underflow.rate(), 1.0);
    let out = underflow
        .exchange(&Money::new(Currency::eur(), 1e-200))
        .unwrap();
    assert_eq!(out.currency(), &Currency::gbp());
    assert_eq!(out.value().to_bits(), 0.0_f64.to_bits());
    let out = underflow
        .exchange(&Money::new(Currency::eur(), -1e-200))
        .unwrap();
    assert_eq!(out.value().to_bits(), (-0.0_f64).to_bits());
    let overflow = ExchangeRate::chain(
        &rate(Currency::eur(), Currency::usd(), 1e200),
        &rate(Currency::usd(), Currency::gbp(), 1e-200),
    )
    .unwrap();
    close(overflow.rate(), 1.0);
    assert!(
        overflow
            .exchange(&Money::new(Currency::eur(), 1e200))
            .is_err()
    );
    assert!(
        overflow
            .exchange(&Money::new(Currency::gbp(), 1e200))
            .is_err()
    );
}

#[test]
fn chaining_checks_numeric_inputs_results_and_currency_connectivity() {
    let valid = rate(Currency::usd(), Currency::gbp(), 0.8);
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let unchecked = ExchangeRate::new(Currency::eur(), Currency::usd(), invalid);
        assert!(ExchangeRate::chain(&unchecked, &valid).is_err());
        assert!(ExchangeRate::chain(&valid, &unchecked).is_err());
    }
    for extreme in [1e200, 1e-200] {
        assert!(
            ExchangeRate::chain(
                &rate(Currency::eur(), Currency::usd(), extreme),
                &rate(Currency::usd(), Currency::gbp(), extreme),
            )
            .is_err()
        );
    }
    let first = rate(Currency::eur(), Currency::usd(), 1.2);
    assert!(ExchangeRate::chain(&first, &rate(Currency::gbp(), Currency::jpy(), 150.0)).is_err());
    let chain = ExchangeRate::chain(&first, &valid).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            chain
                .exchange(&Money::new(Currency::eur(), invalid))
                .is_err()
        );
    }
    let smallest = f64::from_bits(1);
    let small = ExchangeRate::chain(
        &rate(Currency::eur(), Currency::usd(), smallest),
        &rate(Currency::usd(), Currency::gbp(), 1.0),
    )
    .unwrap();
    assert_eq!(small.rate().to_bits(), smallest.to_bits());
    assert_eq!(
        small
            .exchange(&Money::new(Currency::eur(), 1.0))
            .unwrap()
            .value()
            .to_bits(),
        smallest.to_bits()
    );
}
