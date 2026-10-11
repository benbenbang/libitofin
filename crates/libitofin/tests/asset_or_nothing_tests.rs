use std::any::Any;

use libitofin::instruments::{
    AssetOrNothingPayoff, CashOrNothingPayoff, PlainVanillaPayoff, StrikedTypePayoff, TypePayoff,
};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;
use libitofin::pricingengines::BlackCalculator;

#[test]
fn calls_and_puts_match_quantlib_terminal_payments() {
    let call = AssetOrNothingPayoff::new(OptionType::Call, 100.0);
    let put = AssetOrNothingPayoff::new(OptionType::Put, 100.0);
    for (price, call_value, put_value) in [
        (-20.0, 0.0, -20.0),
        (0.0, 0.0, 0.0),
        (90.0, 0.0, 90.0),
        (100.0, 0.0, 0.0),
        (110.0, 110.0, 0.0),
    ] {
        assert_eq!(call.value(price), call_value);
        assert_eq!(put.value(price), put_value);
    }
}

#[test]
fn strike_boundary_is_strict_without_a_tolerance_band() {
    let strike = 100.0_f64;
    let call = AssetOrNothingPayoff::new(OptionType::Call, strike);
    let put = AssetOrNothingPayoff::new(OptionType::Put, strike);
    assert_eq!(call.value(strike.next_up()), strike.next_up());
    assert_eq!(call.value(strike.next_down()), 0.0);
    assert_eq!(put.value(strike.next_down()), strike.next_down());
    assert_eq!(put.value(strike.next_up()), 0.0);
    assert_eq!(call.value(strike), 0.0);
    assert_eq!(put.value(strike), 0.0);
}

#[test]
fn description_accessors_copy_and_dynamic_traits_are_available() {
    let payoff = AssetOrNothingPayoff::new(OptionType::Put, 32.5);
    let copy = payoff;
    assert_eq!(copy, payoff);
    assert_eq!(payoff.name(), "AssetOrNothing");
    assert_eq!(payoff.description(), "AssetOrNothing Put, 32.5 strike");
    assert_eq!(
        AssetOrNothingPayoff::new(OptionType::Call, 100.0).description(),
        "AssetOrNothing Call, 100 strike"
    );
    let dynamic: &dyn StrikedTypePayoff = &payoff;
    assert_eq!(dynamic.option_type(), OptionType::Put);
    assert_eq!(dynamic.strike(), 32.5);
    assert_eq!(dynamic.value(30.0), 30.0);
    let erased: &dyn Any = dynamic;
    assert_eq!(erased.downcast_ref::<AssetOrNothingPayoff>(), Some(&payoff));
}

#[test]
fn decomposition_into_vanilla_and_cash_digital_matches_away_from_strike() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let asset = AssetOrNothingPayoff::new(option_type, 100.0);
        let vanilla = PlainVanillaPayoff::new(option_type, 100.0);
        let cash = CashOrNothingPayoff::new(option_type, 100.0, 1.0);
        for price in [50.0, 99.0, 101.0, 150.0] {
            let expected = match option_type {
                OptionType::Call => vanilla.value(price) + 100.0 * cash.value(price),
                OptionType::Put => 100.0 * cash.value(price) - vanilla.value(price),
            };
            assert_eq!(asset.value(price), expected);
        }
        assert_eq!(asset.option_type(), option_type);
    }
}

#[test]
fn nonfinite_inputs_preserve_quantlib_difference_comparison_semantics() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = AssetOrNothingPayoff::new(option_type, 100.0);
        assert_eq!(payoff.value(f64::NAN), 0.0);
        assert_eq!(
            AssetOrNothingPayoff::new(option_type, f64::NAN).value(100.0),
            0.0
        );
        for infinity in [f64::NEG_INFINITY, f64::INFINITY] {
            assert_eq!(
                AssetOrNothingPayoff::new(option_type, infinity).value(infinity),
                0.0
            );
        }
    }
    assert_eq!(
        AssetOrNothingPayoff::new(OptionType::Call, 100.0).value(f64::INFINITY),
        f64::INFINITY
    );
    assert_eq!(
        AssetOrNothingPayoff::new(OptionType::Put, 100.0).value(f64::NEG_INFINITY),
        f64::NEG_INFINITY
    );
}

#[test]
fn analytic_calculator_rejects_unsupported_payoff_instead_of_pricing_as_vanilla() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = AssetOrNothingPayoff::new(option_type, 100.0);
        let error = BlackCalculator::with_striked_payoff(&payoff, 105.0, 0.2, 0.95)
            .expect_err("this scalar payoff does not extend analytic visitor dispatch");
        assert_eq!(error.message(), "unsupported payoff type: AssetOrNothing");
    }
}
