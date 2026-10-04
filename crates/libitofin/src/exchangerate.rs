//! Direct two-currency exchange rates, without manager or derived chaining.
//!
//! Matches the direct-rate core of `ql/exchangerate.cpp`. Unlike QuantLib,
//! checked construction and exchange reject non-finite or non-positive rates.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::money::Money;
use crate::types::Real;
use crate::{fail, require};

/// Whether a rate is direct or derived. Only direct construction is supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExchangeRateType {
    /// A supplied two-currency rate.
    Direct,
    /// A chained rate, reserved for the exchange-rate manager.
    Derived,
}

/// One unit of `source` is worth `rate` units of `target`.
#[derive(Clone, Debug)]
pub struct ExchangeRate {
    source: Currency,
    target: Currency,
    rate: Real,
}

impl ExchangeRate {
    /// Builds a direct rate. Invalid numeric values are rejected by exchange.
    pub fn new(source: Currency, target: Currency, rate: Real) -> Self {
        Self {
            source,
            target,
            rate,
        }
    }

    /// Builds a finite positive direct rate.
    ///
    /// # Errors
    /// Returns an error unless the rate is finite and positive.
    pub fn checked_new(source: Currency, target: Currency, rate: Real) -> QlResult<Self> {
        require!(
            rate.is_finite() && rate > 0.0,
            "exchange rate must be finite and positive"
        );
        Ok(Self::new(source, target, rate))
    }

    /// The source currency.
    pub fn source(&self) -> &Currency {
        &self.source
    }

    /// The target currency.
    pub fn target(&self) -> &Currency {
        &self.target
    }

    /// The rate type.
    pub fn rate_type(&self) -> ExchangeRateType {
        ExchangeRateType::Direct
    }

    /// The target amount per unit of source.
    pub fn rate(&self) -> Real {
        self.rate
    }

    /// Converts in either direction, preserving the amount's sign.
    ///
    /// # Errors
    /// Returns an error for unrelated currencies, invalid rate/amount, or overflow.
    pub fn exchange(&self, amount: &Money) -> QlResult<Money> {
        require!(
            self.rate.is_finite() && self.rate > 0.0,
            "exchange rate must be finite and positive"
        );
        require!(amount.value().is_finite(), "money amount must be finite");
        if amount.currency() == &self.source {
            Money::checked_new(self.target.clone(), amount.value() * self.rate)
        } else if amount.currency() == &self.target {
            Money::checked_new(self.source.clone(), amount.value() / self.rate)
        } else {
            fail!(
                "exchange rate not applicable: money is {}, rate is {}/{}",
                amount.currency().code(),
                self.source.code(),
                self.target.code()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantlib_direct_rate_exchanges_in_both_directions() {
        let rate = ExchangeRate::new(Currency::eur(), Currency::usd(), 1.2042);
        for amount in [100000.0, -100000.0, 0.0] {
            let usd = rate.exchange(&Money::new(Currency::eur(), amount)).unwrap();
            assert_eq!(usd.currency(), &Currency::usd());
            assert!((usd.value() - amount * 1.2042).abs() < 1e-10);
            let eur = rate.exchange(&usd).unwrap();
            assert!((eur.value() - amount).abs() < 1e-10);
            assert_eq!(eur.currency(), &Currency::eur());
        }
        assert_eq!(rate.rate_type(), ExchangeRateType::Direct);
    }

    #[test]
    fn exchange_rejects_unrelated_currency_and_invalid_numbers() {
        for rate in [0.0, -1.0, Real::NAN, Real::INFINITY] {
            assert!(ExchangeRate::checked_new(Currency::eur(), Currency::usd(), rate).is_err());
            assert!(
                ExchangeRate::new(Currency::eur(), Currency::usd(), rate)
                    .exchange(&Money::new(Currency::eur(), 1.0))
                    .is_err()
            );
        }
        let rate = ExchangeRate::new(Currency::eur(), Currency::usd(), 2.0);
        assert!(rate.exchange(&Money::new(Currency::gbp(), 1.0)).is_err());
        assert!(
            rate.exchange(&Money::new(Currency::eur(), Real::MAX))
                .is_err()
        );
        assert!(
            rate.exchange(&Money::new(Currency::eur(), Real::NAN))
                .is_err()
        );
    }
}
