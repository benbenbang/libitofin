use std::any::Any;

use libitofin::instruments::{PercentageStrikePayoff, StrikedTypePayoff};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;
use libitofin::pricingengines::BlackCalculator;

#[test]
fn native_quantlib_143_vectors_match_bits_except_nan_payloads() {
    let fixture = include_str!("fixtures/medium5_percentage/quantlib-1.43.tsv");
    let mut count = 0;
    for row in fixture.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 4);
        let option_type = match fields[0] {
            "Call" => OptionType::Call,
            "Put" => OptionType::Put,
            other => panic!("unexpected option type: {other}"),
        };
        let decode = |s| f64::from_bits(u64::from_str_radix(s, 16).unwrap());
        let expected = decode(fields[3]);
        let actual =
            PercentageStrikePayoff::new(option_type, decode(fields[1])).value(decode(fields[2]));
        if expected.is_nan() {
            assert!(actual.is_nan(), "{row}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 44);
}

#[test]
fn moneyness_is_a_ratio_and_negative_price_is_not_floored() {
    let call = PercentageStrikePayoff::new(OptionType::Call, 0.5);
    let put = PercentageStrikePayoff::new(OptionType::Put, 1.5);
    for payoff in [call, put] {
        assert_eq!(payoff.value(80.0), 40.0);
        assert_eq!(payoff.value(-80.0), -40.0);
        assert_eq!(payoff.value(-0.0).to_bits(), (-0.0_f64).to_bits());
    }
    assert_eq!(
        PercentageStrikePayoff::new(OptionType::Call, 50.0).value(80.0),
        0.0
    );
    assert_eq!(
        PercentageStrikePayoff::new(OptionType::Put, 50.0).value(80.0),
        3920.0
    );
}

#[test]
fn strict_factor_boundary_preserves_next_float_rounding() {
    let below = 1.0_f64.next_down();
    let above = 1.0_f64.next_up();
    let call = PercentageStrikePayoff::new(OptionType::Call, below);
    let put = PercentageStrikePayoff::new(OptionType::Put, above);
    assert_eq!(call.value(1.0).to_bits(), (1.0 - below).to_bits());
    assert_eq!(put.value(1.0).to_bits(), (above - 1.0).to_bits());
    assert_eq!(
        PercentageStrikePayoff::new(OptionType::Call, above).value(1.0),
        0.0
    );
    assert_eq!(
        PercentageStrikePayoff::new(OptionType::Put, below).value(1.0),
        0.0
    );
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = PercentageStrikePayoff::new(option_type, 1.0);
        assert_eq!(payoff.value(-3.0).to_bits(), (-0.0_f64).to_bits());
        assert!(payoff.value(f64::INFINITY).is_nan());
    }
}

#[test]
fn nonfinite_inputs_remain_infallible_and_propagate_like_cpp_std_max() {
    for option_type in [OptionType::Call, OptionType::Put] {
        assert!(
            PercentageStrikePayoff::new(option_type, f64::NAN)
                .value(2.0)
                .is_nan()
        );
        assert!(
            PercentageStrikePayoff::new(option_type, 1.0)
                .value(f64::NAN)
                .is_nan()
        );
    }
    let call = PercentageStrikePayoff::new(OptionType::Call, f64::NEG_INFINITY);
    assert_eq!(call.value(-2.0), f64::NEG_INFINITY);
    assert!(call.value(0.0).is_nan());
    let put = PercentageStrikePayoff::new(OptionType::Put, f64::INFINITY);
    assert_eq!(put.value(2.0), f64::INFINITY);
    assert!(put.value(0.0).is_nan());
}

#[test]
fn description_accessors_copy_and_dynamic_downcast_match_contract() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = PercentageStrikePayoff::new(option_type, 0.5);
        let copy = payoff;
        assert_eq!(copy, payoff);
        assert_eq!(payoff.name(), "PercentageStrike");
        assert_eq!(
            payoff.description(),
            format!("PercentageStrike {option_type}, 0.5 strike")
        );
        let dynamic: &dyn StrikedTypePayoff = &payoff;
        assert_eq!(dynamic.option_type(), option_type);
        assert_eq!(dynamic.strike(), 0.5);
        assert_eq!(dynamic.value(80.0), payoff.value(80.0));
        let erased: &dyn Any = dynamic;
        assert_eq!(
            erased.downcast_ref::<PercentageStrikePayoff>(),
            Some(&payoff)
        );
    }
}

#[test]
fn analytic_calculator_rejects_percentage_strike_without_new_dispatch() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = PercentageStrikePayoff::new(option_type, 0.5);
        let error = BlackCalculator::with_striked_payoff(&payoff, 100.0, 0.2, 0.95)
            .expect_err("a scalar payoff does not enable an analytic engine");
        assert_eq!(error.message(), "unsupported payoff type: PercentageStrike");
    }
}
