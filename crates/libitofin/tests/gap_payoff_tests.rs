use std::any::Any;

use libitofin::instruments::{
    CashOrNothingPayoff, GapPayoff, PlainVanillaPayoff, StrikedTypePayoff,
};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;
use libitofin::pricingengines::BlackCalculator;

#[test]
fn separate_trigger_and_payment_strikes_match_quantlib() {
    let call = GapPayoff::new(OptionType::Call, 100.0, 110.0);
    let put = GapPayoff::new(OptionType::Put, 100.0, 90.0);
    for (price, call_value, put_value) in [
        (80.0, 0.0, 10.0),
        (95.0, 0.0, -5.0),
        (100.0, -10.0, -10.0),
        (105.0, -5.0, 0.0),
        (120.0, 10.0, 0.0),
    ] {
        assert_eq!(call.value(price), call_value);
        assert_eq!(put.value(price), put_value);
    }
    assert_eq!(
        GapPayoff::new(OptionType::Call, 100.0, 90.0).value(100.0),
        10.0
    );
    assert_eq!(
        GapPayoff::new(OptionType::Put, 100.0, 110.0).value(100.0),
        10.0
    );
}

#[test]
fn trigger_boundary_is_inclusive_without_a_tolerance_band() {
    let strike = 100.0_f64;
    let call = GapPayoff::new(OptionType::Call, strike, 110.0);
    let put = GapPayoff::new(OptionType::Put, strike, 90.0);
    assert_eq!(call.value(strike), -10.0);
    assert_eq!(put.value(strike), -10.0);
    assert_eq!(call.value(strike.next_up()), strike.next_up() - 110.0);
    assert_eq!(call.value(strike.next_down()), 0.0);
    assert_eq!(put.value(strike.next_down()), 90.0 - strike.next_down());
    assert_eq!(put.value(strike.next_up()), 0.0);
}

#[test]
fn equal_strikes_reduce_to_vanilla_for_finite_inputs() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let gap = GapPayoff::new(option_type, 100.0, 100.0);
        let vanilla = PlainVanillaPayoff::new(option_type, 100.0);
        for price in [-20.0, 0.0, 90.0, 100.0, 110.0] {
            assert_eq!(gap.value(price), vanilla.value(price));
        }
    }
}

#[test]
fn description_accessors_copy_and_dynamic_traits_are_available() {
    let payoff = GapPayoff::new(OptionType::Put, 80.0, 32.5);
    let copy = payoff;
    assert_eq!(copy, payoff);
    assert_eq!(payoff.name(), "Gap");
    assert_eq!(
        payoff.description(),
        "Gap Put, 80 strike, 32.5 strike payoff"
    );
    assert_eq!(payoff.second_strike(), 32.5);
    assert_eq!(
        GapPayoff::new(OptionType::Call, 100.0, 110.0).description(),
        "Gap Call, 100 strike, 110 strike payoff"
    );
    let dynamic: &dyn StrikedTypePayoff = &payoff;
    assert_eq!(dynamic.option_type(), OptionType::Put);
    assert_eq!(dynamic.strike(), 80.0);
    assert_eq!(dynamic.value(30.0), 2.5);
    let erased: &dyn Any = dynamic;
    assert_eq!(erased.downcast_ref::<GapPayoff>(), Some(&payoff));
}

#[test]
fn vanilla_and_cash_decomposition_matches_away_from_trigger() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let gap = GapPayoff::new(option_type, 100.0, 110.0);
        let vanilla = PlainVanillaPayoff::new(option_type, 100.0);
        let cash = CashOrNothingPayoff::new(option_type, 100.0, 1.0);
        let sign = match option_type {
            OptionType::Call => -1.0,
            OptionType::Put => 1.0,
        };
        for price in [50.0, 99.0, 101.0, 150.0] {
            assert_eq!(
                gap.value(price),
                vanilla.value(price) + sign * 10.0 * cash.value(price)
            );
        }
    }
}

#[test]
fn nonfinite_inputs_preserve_quantlib_difference_comparison_semantics() {
    for option_type in [OptionType::Call, OptionType::Put] {
        assert_eq!(
            GapPayoff::new(option_type, 100.0, 110.0).value(f64::NAN),
            0.0
        );
        assert_eq!(
            GapPayoff::new(option_type, f64::NAN, 110.0).value(100.0),
            0.0
        );
        for infinity in [f64::NEG_INFINITY, f64::INFINITY] {
            assert_eq!(
                GapPayoff::new(option_type, infinity, 110.0).value(infinity),
                0.0
            );
        }
        let payoff = GapPayoff::new(option_type, 100.0, f64::NAN);
        assert!(payoff.value(100.0).is_nan());
        let inactive_price = match option_type {
            OptionType::Call => 90.0,
            OptionType::Put => 110.0,
        };
        assert_eq!(payoff.value(inactive_price), 0.0);
    }
    assert_eq!(
        GapPayoff::new(OptionType::Call, 100.0, 110.0).value(f64::INFINITY),
        f64::INFINITY
    );
    assert_eq!(
        GapPayoff::new(OptionType::Put, 100.0, 90.0).value(f64::NEG_INFINITY),
        f64::INFINITY
    );
}

#[test]
fn analytic_calculator_rejects_unsupported_payoff_instead_of_pricing_as_vanilla() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let payoff = GapPayoff::new(option_type, 100.0, 110.0);
        let error = BlackCalculator::with_striked_payoff(&payoff, 105.0, 0.2, 0.95)
            .expect_err("this scalar payoff does not extend analytic visitor dispatch");
        assert_eq!(error.message(), "unsupported payoff type: Gap");
    }
}
