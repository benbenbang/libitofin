use std::any::Any;

use libitofin::instruments::{FloatingTypePayoff, PlainVanillaPayoff, TypePayoff};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;

fn from_bits(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).expect("fixture contains f64 bits"))
}

#[test]
fn two_input_values_match_native_quantlib_143_with_documented_zero_sign_exception() {
    let fixture = include_str!("fixtures/medium5_floating/quantlib-1.43.tsv");
    let mut count = 0;
    for row in fixture.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 4);
        let option_type = match fields[0] {
            "Call" => OptionType::Call,
            "Put" => OptionType::Put,
            other => panic!("unknown fixture option type: {other}"),
        };
        let price = from_bits(fields[1]);
        let strike = from_bits(fields[2]);
        let expected = from_bits(fields[3]);
        let actual = FloatingTypePayoff::new(option_type).value_with_strike(price, strike);
        if expected.is_nan() {
            assert!(actual.is_nan(), "{row}");
        } else if expected == 0.0 {
            assert_eq!(actual, 0.0, "native wheel canonicalizes zero sign: {row}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 56);
}

#[test]
fn strike_is_supplied_fresh_for_every_evaluation() {
    let call = FloatingTypePayoff::new(OptionType::Call);
    let put = FloatingTypePayoff::new(OptionType::Put);
    for (price, strike, call_value, put_value) in [
        (120.0, 100.0, 20.0, 0.0),
        (120.0, 130.0, 0.0, 10.0),
        (120.0, 120.0, 0.0, 0.0),
        (-20.0, -30.0, 10.0, 0.0),
        (-20.0, -10.0, 0.0, 10.0),
    ] {
        assert_eq!(call.value_with_strike(price, strike), call_value);
        assert_eq!(put.value_with_strike(price, strike), put_value);
    }
    assert_eq!(call.value_with_strike(120.0, 100.0), 20.0);
}

#[test]
fn floating_values_equal_fixed_vanilla_and_obey_put_call_parity() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = FloatingTypePayoff::new(option_type);
        for price in [-100.0, -0.01, 0.0, 75.0, 100.0, 110.0] {
            for strike in [-200.0, -0.01, 0.0, 100.0, 200.0] {
                let expected = PlainVanillaPayoff::new(option_type, strike).value(price);
                assert_eq!(payoff.value_with_strike(price, strike), expected);
                let call = FloatingTypePayoff::new(OptionType::Call);
                let put = FloatingTypePayoff::new(OptionType::Put);
                assert_eq!(
                    call.value_with_strike(price, strike) - put.value_with_strike(price, strike),
                    price - strike
                );
            }
        }
    }
}

#[test]
fn option_type_description_copy_and_any_remain_available_without_a_stored_strike() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = FloatingTypePayoff::new(option_type);
        let copy = payoff;
        assert_eq!(copy, payoff);
        assert_eq!(payoff.name(), "FloatingType");
        assert_eq!(payoff.description(), format!("FloatingType {option_type}"));
        let dynamic: &dyn TypePayoff = &payoff;
        assert_eq!(dynamic.option_type(), option_type);
        assert_eq!(dynamic.name(), payoff.name());
        assert_eq!(dynamic.description(), payoff.description());
        let erased: &dyn Any = &payoff;
        assert_eq!(erased.downcast_ref::<FloatingTypePayoff>(), Some(&payoff));
    }
}

#[test]
#[should_panic(expected = "floating payoff not handled")]
fn one_input_evaluation_is_explicitly_unsupported() {
    FloatingTypePayoff::new(OptionType::Call).value(100.0);
}

#[test]
#[should_panic(expected = "floating payoff not handled")]
fn erased_payoff_does_not_silently_supply_a_fixed_strike() {
    let payoff = FloatingTypePayoff::new(OptionType::Put);
    let dynamic: &dyn Payoff = &payoff;
    dynamic.value(f64::NAN);
}

#[test]
fn unsupported_one_input_panic_preserves_native_error_message_for_both_types() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = FloatingTypePayoff::new(option_type);
        for price in [0.0, -100.0, f64::INFINITY, f64::NAN] {
            let error = std::panic::catch_unwind(|| payoff.value(price))
                .expect_err("the strike is missing");
            assert_eq!(
                error.downcast_ref::<&str>(),
                Some(&"floating payoff not handled")
            );
        }
    }
}

#[test]
fn source_intrinsic_first_max_preserves_negative_zero_and_nan() {
    let call = FloatingTypePayoff::new(OptionType::Call);
    let put = FloatingTypePayoff::new(OptionType::Put);
    assert_eq!(
        call.value_with_strike(-0.0, 0.0).to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        put.value_with_strike(0.0, -0.0).to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        call.value_with_strike(0.0, -0.0).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        put.value_with_strike(-0.0, 0.0).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        call.value_with_strike(-1.0, 0.0).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(put.value_with_strike(1.0, 0.0).to_bits(), 0.0_f64.to_bits());
    for payoff in [call, put] {
        assert!(payoff.value_with_strike(f64::NAN, 0.0).is_nan());
        assert!(payoff.value_with_strike(0.0, f64::NAN).is_nan());
    }
}
