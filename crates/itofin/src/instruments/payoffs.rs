//! Payoffs for various options.
//!
//! Port of the plain-vanilla subset of `ql/instruments/payoffs.{hpp,cpp}`:
//! the [`TypePayoff`] and [`StrikedTypePayoff`] intermediate contracts and
//! the [`PlainVanillaPayoff`]. The remaining payoffs (`NullPayoff`,
//! `FloatingTypePayoff`, `PercentageStrikePayoff`, `AssetOrNothingPayoff`,
//! `CashOrNothingPayoff`, `GapPayoff`, `SuperFundPayoff`,
//! `SuperSharePayoff`) are follow-up work.

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
pub trait StrikedTypePayoff: TypePayoff {
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
        match self.option_type {
            OptionType::Call => (price - self.strike).max(0.0),
            OptionType::Put => (self.strike - price).max(0.0),
        }
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
}
