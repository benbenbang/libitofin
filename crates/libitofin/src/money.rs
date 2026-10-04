//! Cash amount in a given currency.
//!
//! Port of the value-type core of `ql/money.{hpp,cpp}` (conversion settings
//! deferred with the exchange-rate manager).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;
use std::fmt;
use std::ops::{Div, Mul, Neg};

/// An amount of cash denominated in a [`Currency`].
#[derive(Clone, Debug, PartialEq)]
pub struct Money {
    currency: Currency,
    value: Real,
}

impl Money {
    /// Builds a money amount.
    pub fn new(currency: Currency, value: Real) -> Self {
        Self { currency, value }
    }

    /// Builds a finite money amount.
    ///
    /// # Errors
    /// Returns an error for non-finite amounts.
    pub fn checked_new(currency: Currency, value: Real) -> QlResult<Self> {
        require!(value.is_finite(), "money amount must be finite");
        Ok(Self::new(currency, value))
    }

    /// The currency.
    pub fn currency(&self) -> &Currency {
        &self.currency
    }

    /// The numeric value.
    pub fn value(&self) -> Real {
        self.value
    }

    /// Adds two amounts of the same currency.
    pub fn checked_add(&self, rhs: &Money) -> QlResult<Money> {
        require!(
            self.currency == rhs.currency,
            "currency mismatch: {} vs {}",
            self.currency.code(),
            rhs.currency.code()
        );
        require!(
            self.value.is_finite() && rhs.value.is_finite(),
            "money amounts must be finite"
        );
        Money::checked_new(self.currency.clone(), self.value + rhs.value)
    }

    /// Multiplies by a finite scalar, rejecting overflow.
    pub fn checked_mul(&self, rhs: Real) -> QlResult<Money> {
        require!(
            self.value.is_finite() && rhs.is_finite(),
            "money and scalar must be finite"
        );
        Money::checked_new(self.currency.clone(), self.value * rhs)
    }

    /// Divides by a finite nonzero scalar, rejecting overflow.
    pub fn checked_div(&self, rhs: Real) -> QlResult<Money> {
        require!(
            rhs.is_finite() && rhs != 0.0,
            "divisor must be finite and nonzero"
        );
        require!(self.value.is_finite(), "money amount must be finite");
        Money::checked_new(self.currency.clone(), self.value / rhs)
    }

    /// Returns the ratio of same-currency amounts.
    pub fn checked_ratio(&self, rhs: &Money) -> QlResult<Real> {
        require!(self.currency == rhs.currency, "currency mismatch");
        Ok(self.checked_div(rhs.value)?.value())
    }

    /// Subtracts two amounts of the same currency.
    pub fn checked_sub(&self, rhs: &Money) -> QlResult<Money> {
        self.checked_add(&Money::new(rhs.currency.clone(), -rhs.value))
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money::new(self.currency, -self.value)
    }
}

impl Mul<Real> for Money {
    type Output = Money;
    fn mul(self, rhs: Real) -> Money {
        Money::new(self.currency, self.value * rhs)
    }
}

impl Div<Real> for Money {
    type Output = Money;
    fn div(self, rhs: Real) -> Money {
        Money::new(self.currency, self.value / rhs)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.value, self.currency.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantlib_no_conversion_arithmetic() {
        let m1 = Money::new(Currency::eur(), 50000.0);
        let m2 = Money::new(Currency::eur(), 100000.0);
        let m3 = Money::new(Currency::eur(), 500000.0);
        let result = m1
            .checked_mul(3.0)
            .unwrap()
            .checked_add(&m2.checked_mul(2.5).unwrap())
            .unwrap()
            .checked_sub(&m3.checked_div(5.0).unwrap())
            .unwrap()
            .checked_add(&m1.checked_mul(m2.checked_ratio(&m3).unwrap()).unwrap())
            .unwrap();
        assert_eq!(result.value(), 310000.0);
    }

    #[test]
    fn checked_operations_reject_invalid_and_overflowing_amounts() {
        let amount = Money::new(Currency::eur(), Real::MAX);
        assert!(amount.checked_add(&amount).is_err());
        assert!(amount.checked_mul(2.0).is_err());
        assert!(amount.checked_div(0.0).is_err());
        assert!(amount.checked_mul(Real::NAN).is_err());
        assert!(Money::checked_new(Currency::eur(), Real::INFINITY).is_err());
        assert!(
            Money::new(Currency::eur(), Real::NAN)
                .checked_add(&amount)
                .is_err()
        );
        assert!(
            amount
                .checked_ratio(&Money::new(Currency::usd(), 1.0))
                .is_err()
        );
    }

    #[test]
    fn same_currency_amounts_add() {
        let a = Money::new(Currency::eur(), 10.0);
        let b = Money::new(Currency::eur(), 2.5);
        let sum = a.checked_add(&b).unwrap();
        assert_eq!(sum.value(), 12.5);
        assert_eq!(sum.currency().code(), "EUR");
    }

    #[test]
    fn mismatched_currencies_error() {
        let a = Money::new(Currency::eur(), 1.0);
        let b = Money::new(Currency::usd(), 1.0);
        assert!(a.checked_add(&b).is_err());
    }
}
