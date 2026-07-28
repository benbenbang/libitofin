//! Payoffs for various options.
//!
//! Port of the plain-vanilla subset of `ql/instruments/payoffs.{hpp,cpp}`:
//! the [`TypePayoff`] and [`StrikedTypePayoff`] intermediate contracts, the
//! [`PlainVanillaPayoff`] and the [`CashOrNothingPayoff`]. The remaining
//! payoffs (`NullPayoff`, `FloatingTypePayoff`, `PercentageStrikePayoff`,
//! `AssetOrNothingPayoff`, `GapPayoff`, `SuperFundPayoff`,
//! `SuperSharePayoff`) are follow-up work.

use std::any::Any;

use crate::option::OptionType;
use crate::payoff::Payoff;
use crate::types::Real;

/// Intermediate contract for put/call payoffs (QuantLib's `TypePayoff`).
pub trait TypePayoff: Payoff {
    /// The option type the payoff is written on.
    fn option_type(&self) -> OptionType;
}

/// Intermediate contract for payoffs based on a fixed strike (QuantLib's
/// `StrikedTypePayoff`).
///
/// The [`Any`] supertrait ports the C++ engines' `dynamic_pointer_cast`
/// dispatch on the concrete payoff (visiting by dynamic type); it is
/// auto-satisfied by every `'static` implementor.
pub trait StrikedTypePayoff: TypePayoff + Any {
    /// The strike the payoff is based on.
    fn strike(&self) -> Real;
}

/// Plain-vanilla payoff: `max(price - strike, 0)` for a call,
/// `max(strike - price, 0)` for a put.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlainVanillaPayoff {
    option_type: OptionType,
    strike: Real,
}

impl PlainVanillaPayoff {
    /// Builds a plain-vanilla payoff of the given type and strike.
    pub fn new(option_type: OptionType, strike: Real) -> PlainVanillaPayoff {
        PlainVanillaPayoff {
            option_type,
            strike,
        }
    }
}

impl Payoff for PlainVanillaPayoff {
    fn name(&self) -> String {
        "Vanilla".to_string()
    }

    fn description(&self) -> String {
        format!(
            "{} {}, {} strike",
            self.name(),
            self.option_type,
            self.strike
        )
    }

    fn value(&self, price: Real) -> Real {
        let intrinsic = match self.option_type {
            OptionType::Call => price - self.strike,
            OptionType::Put => self.strike - price,
        };
        if intrinsic < 0.0 { 0.0 } else { intrinsic }
    }
}

impl TypePayoff for PlainVanillaPayoff {
    fn option_type(&self) -> OptionType {
        self.option_type
    }
}

impl StrikedTypePayoff for PlainVanillaPayoff {
    fn strike(&self) -> Real {
        self.strike
    }
}

/// Binary cash-or-nothing payoff: a fixed `cash_payoff` when the price ends
/// strictly beyond the strike, nothing otherwise.
///
/// Ports `CashOrNothingPayoff` (`ql/instruments/payoffs.hpp:148`,
/// `payoffs.cpp:154-163`). The comparison is strict on both sides, so a price
/// exactly at the strike pays nothing for either option type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CashOrNothingPayoff {
    option_type: OptionType,
    strike: Real,
    cash_payoff: Real,
}

impl CashOrNothingPayoff {
    /// Builds a cash-or-nothing payoff of the given type, strike and cash
    /// amount.
    pub fn new(option_type: OptionType, strike: Real, cash_payoff: Real) -> CashOrNothingPayoff {
        CashOrNothingPayoff {
            option_type,
            strike,
            cash_payoff,
        }
    }

    /// The amount paid when the payoff is in the money.
    pub fn cash_payoff(&self) -> Real {
        self.cash_payoff
    }
}

impl Payoff for CashOrNothingPayoff {
    fn name(&self) -> String {
        "CashOrNothing".to_string()
    }

    fn description(&self) -> String {
        format!(
            "{} {}, {} strike, {} cash payoff",
            self.name(),
            self.option_type,
            self.strike,
            self.cash_payoff
        )
    }

    fn value(&self, price: Real) -> Real {
        let moneyness = match self.option_type {
            OptionType::Call => price - self.strike,
            OptionType::Put => self.strike - price,
        };
        if moneyness > 0.0 {
            self.cash_payoff
        } else {
            0.0
        }
    }
}

impl TypePayoff for CashOrNothingPayoff {
    fn option_type(&self) -> OptionType {
        self.option_type
    }
}

impl StrikedTypePayoff for CashOrNothingPayoff {
    fn strike(&self) -> Real {
        self.strike
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_pays_excess_over_strike() {
        let payoff = PlainVanillaPayoff::new(OptionType::Call, 100.0);
        assert_eq!(payoff.value(110.0), 10.0);
        assert_eq!(payoff.value(100.0), 0.0);
        assert_eq!(payoff.value(90.0), 0.0);
    }

    #[test]
    fn put_pays_shortfall_under_strike() {
        let payoff = PlainVanillaPayoff::new(OptionType::Put, 100.0);
        assert_eq!(payoff.value(90.0), 10.0);
        assert_eq!(payoff.value(100.0), 0.0);
        assert_eq!(payoff.value(110.0), 0.0);
    }

    #[test]
    fn nan_price_propagates_like_cpp_std_max() {
        let call = PlainVanillaPayoff::new(OptionType::Call, 100.0);
        assert!(call.value(Real::NAN).is_nan());
        let put = PlainVanillaPayoff::new(OptionType::Put, 100.0);
        assert!(put.value(Real::NAN).is_nan());
    }

    #[test]
    fn accessors_expose_type_and_strike() {
        let payoff = PlainVanillaPayoff::new(OptionType::Put, 32.5);
        assert_eq!(payoff.option_type(), OptionType::Put);
        assert_eq!(payoff.strike(), 32.5);
    }

    #[test]
    fn name_and_description_match_quantlib() {
        let call = PlainVanillaPayoff::new(OptionType::Call, 100.0);
        assert_eq!(call.name(), "Vanilla");
        assert_eq!(call.description(), "Vanilla Call, 100 strike");

        let put = PlainVanillaPayoff::new(OptionType::Put, 32.5);
        assert_eq!(put.description(), "Vanilla Put, 32.5 strike");
    }

    #[test]
    fn usable_as_trait_object() {
        let payoff = PlainVanillaPayoff::new(OptionType::Call, 100.0);
        let dynamic: &dyn StrikedTypePayoff = &payoff;
        assert_eq!(dynamic.value(107.0), 7.0);
        assert_eq!(dynamic.option_type(), OptionType::Call);
        assert_eq!(dynamic.strike(), 100.0);
    }

    #[test]
    fn digital_pays_the_cash_amount_beyond_the_strike() {
        let call = CashOrNothingPayoff::new(OptionType::Call, 100.0, 10.0);
        assert_eq!(call.value(110.0), 10.0);
        assert_eq!(call.value(90.0), 0.0);

        let put = CashOrNothingPayoff::new(OptionType::Put, 100.0, 10.0);
        assert_eq!(put.value(90.0), 10.0);
        assert_eq!(put.value(110.0), 0.0);
    }

    /// `payoffs.cpp:156,158` compares `> 0.0`, so the strike itself is out of
    /// the money for both types - a non-strict comparison would pay twice.
    #[test]
    fn digital_at_the_strike_pays_nothing_either_way() {
        let call = CashOrNothingPayoff::new(OptionType::Call, 100.0, 10.0);
        let put = CashOrNothingPayoff::new(OptionType::Put, 100.0, 10.0);
        assert_eq!(call.value(100.0), 0.0);
        assert_eq!(put.value(100.0), 0.0);
    }

    #[test]
    fn digital_accessors_and_description_match_quantlib() {
        let put = CashOrNothingPayoff::new(OptionType::Put, 80.0, 10.0);
        assert_eq!(put.option_type(), OptionType::Put);
        assert_eq!(put.strike(), 80.0);
        assert_eq!(put.cash_payoff(), 10.0);
        assert_eq!(put.name(), "CashOrNothing");
        assert_eq!(
            put.description(),
            "CashOrNothing Put, 80 strike, 10 cash payoff"
        );
    }

    #[test]
    fn digital_usable_as_trait_object() {
        let payoff = CashOrNothingPayoff::new(OptionType::Call, 100.0, 10.0);
        let dynamic: &dyn StrikedTypePayoff = &payoff;
        assert_eq!(dynamic.value(107.0), 10.0);
        assert_eq!(dynamic.option_type(), OptionType::Call);
        assert_eq!(dynamic.strike(), 100.0);
    }
}
