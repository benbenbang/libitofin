//! Direct and ordered, derived two-currency exchange rates.
//!
//! Matches the conversion ordering of `ql/exchangerate.cpp`. Unlike QuantLib,
//! checked construction, chaining and exchange reject invalid numeric values.

use std::sync::Arc;

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::money::Money;
use crate::types::Real;
use crate::{fail, require};

/// Whether a rate is supplied directly or derived by chaining.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExchangeRateType {
    /// A supplied two-currency rate.
    Direct,
    /// A rate derived from two ordered child rates.
    Derived,
}

/// One unit of `source` is worth `rate` units of `target`.
#[derive(Clone, Debug)]
pub struct ExchangeRate {
    source: Currency,
    target: Currency,
    rate: Real,
    chain: Option<(Arc<Self>, Arc<Self>)>,
}

impl ExchangeRate {
    /// Builds a direct rate. Invalid numeric values are rejected by exchange.
    pub fn new(source: Currency, target: Currency, rate: Real) -> Self {
        Self {
            source,
            target,
            rate,
            chain: None,
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
        if self.chain.is_some() {
            ExchangeRateType::Derived
        } else {
            ExchangeRateType::Direct
        }
    }

    /// The target amount per unit of source.
    pub fn rate(&self) -> Real {
        self.rate
    }

    /// Chains two rates sharing a currency, retaining their conversion order.
    ///
    /// The first matching orientation follows QuantLib: common source,
    /// first source/second target, first target/second source, then common target.
    /// The stored rate is descriptive; exchange applies the retained children,
    /// not a flattened multiplication. Child rates may themselves be derived.
    ///
    /// # Errors
    /// Returns an error for rates without a shared currency, invalid input rates,
    /// or a computed rate that is non-finite or non-positive, including underflow.
    pub fn chain(first: &Self, second: &Self) -> QlResult<Self> {
        require!(
            first.rate.is_finite()
                && first.rate > 0.0
                && second.rate.is_finite()
                && second.rate > 0.0,
            "exchange rates must be finite and positive"
        );
        let (source, target, rate) = if first.source == second.source {
            (&first.target, &second.target, second.rate / first.rate)
        } else if first.source == second.target {
            (
                &first.target,
                &second.source,
                1.0 / (first.rate * second.rate),
            )
        } else if first.target == second.source {
            (&first.source, &second.target, first.rate * second.rate)
        } else if first.target == second.target {
            (&first.source, &second.source, first.rate / second.rate)
        } else {
            fail!("exchange rates not chainable");
        };
        let mut result = Self::checked_new(source.clone(), target.clone(), rate)?;
        result.chain = Some((Arc::new(first.clone()), Arc::new(second.clone())));
        Ok(result)
    }

    /// Converts in either direction, preserving the amount's sign.
    ///
    /// Derived rates apply the first child followed by the second when the
    /// amount matches either endpoint of the first child, otherwise vice versa.
    /// Intermediate underflow can yield zero; intermediate overflow is an error.
    ///
    /// # Errors
    /// Returns an error for unrelated currencies, invalid rate/amount, or overflow.
    pub fn exchange(&self, amount: &Money) -> QlResult<Money> {
        require!(
            self.rate.is_finite() && self.rate > 0.0,
            "exchange rate must be finite and positive"
        );
        require!(amount.value().is_finite(), "money amount must be finite");
        if let Some((first, second)) = &self.chain {
            if amount.currency() == first.source() || amount.currency() == first.target() {
                return second.exchange(&first.exchange(amount)?);
            } else if amount.currency() == second.source() || amount.currency() == second.target() {
                return first.exchange(&second.exchange(amount)?);
            } else {
                fail!(
                    "exchange rate not applicable: money is {}",
                    amount.currency().code()
                );
            }
        }
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
