//! Stock dividends.
//!
//! Port of `ql/cashflows/dividend.{hpp,cpp}`. A [`Dividend`] is a cash flow
//! whose amount may depend on the underlying's value, which is what
//! [`FractionalDividend`] uses it for.
//!
//! QuantLib marks a fractional dividend's missing nominal with the
//! `Null<Real>` sentinel and fails on it in `amount()`; the port stores
//! `Option<Real>` and returns the same failure as an `Err`, following the
//! [`SimpleQuote`](crate::quotes::SimpleQuote) precedent.

use crate::cashflow::{CashFlow, cash_flow_has_occurred};
use crate::errors::QlResult;
use crate::event::Event;
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::date::Date;
use crate::types::Real;
use crate::{fail, require};

/// A cash flow paid on a stock, whose amount may be read off the underlying.
pub trait Dividend: CashFlow {
    /// The amount paid when the underlying is worth `underlying`.
    fn amount_with_underlying(&self, underlying: Real) -> Real;
}

/// A dividend paying a predetermined amount, whatever the underlying is worth.
pub struct FixedDividend {
    amount: Real,
    date: Date,
    observable: Observable,
}

impl FixedDividend {
    /// Creates a dividend paying `amount` on `date`.
    pub fn new(amount: Real, date: Date) -> Self {
        FixedDividend {
            amount,
            date,
            observable: Observable::new(),
        }
    }
}

impl AsObservable for FixedDividend {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Event for FixedDividend {
    fn date(&self) -> Date {
        self.date
    }

    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool> {
        cash_flow_has_occurred(self.date, settings, ref_date, include_ref_date)
    }
}

impl CashFlow for FixedDividend {
    fn amount(&self) -> QlResult<Real> {
        Ok(self.amount)
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
}

impl Dividend for FixedDividend {
    fn amount_with_underlying(&self, _underlying: Real) -> Real {
        self.amount
    }
}

/// A dividend paying a fixed fraction of the underlying.
///
/// The nominal is optional: without one, [`CashFlow::amount`] has no underlying
/// to apply the rate to and fails, while
/// [`amount_with_underlying`](Dividend::amount_with_underlying) still works.
pub struct FractionalDividend {
    rate: Real,
    nominal: Option<Real>,
    date: Date,
    observable: Observable,
}

impl FractionalDividend {
    /// Creates a dividend paying `rate` times `nominal` on `date`; pass `None`
    /// for the nominal-less C++ two-argument constructor.
    pub fn new(rate: Real, nominal: impl Into<Option<Real>>, date: Date) -> Self {
        FractionalDividend {
            rate,
            nominal: nominal.into(),
            date,
            observable: Observable::new(),
        }
    }

    /// The fraction of the underlying that is paid.
    pub fn rate(&self) -> Real {
        self.rate
    }

    /// The nominal the rate applies to, when one was given.
    pub fn nominal(&self) -> Option<Real> {
        self.nominal
    }
}

impl AsObservable for FractionalDividend {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Event for FractionalDividend {
    fn date(&self) -> Date {
        self.date
    }

    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool> {
        cash_flow_has_occurred(self.date, settings, ref_date, include_ref_date)
    }
}

impl CashFlow for FractionalDividend {
    fn amount(&self) -> QlResult<Real> {
        match self.nominal {
            Some(nominal) => Ok(self.rate * nominal),
            None => fail!("no nominal given"),
        }
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
}

impl Dividend for FractionalDividend {
    fn amount_with_underlying(&self, underlying: Real) -> Real {
        self.rate * underlying
    }
}

/// Builds a sequence of [`FixedDividend`]s, one per date.
///
/// Mirrors `DividendVector`; the size mismatch it throws on is an `Err` here.
pub fn dividend_vector(dates: &[Date], amounts: &[Real]) -> QlResult<Vec<Shared<dyn Dividend>>> {
    require!(
        dates.len() == amounts.len(),
        "size mismatch between dividend dates and amounts"
    );
    Ok(dates
        .iter()
        .zip(amounts)
        .map(|(&date, &amount)| shared(FixedDividend::new(amount, date)) as Shared<dyn Dividend>)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    /// No test-suite case pins the dividends numerically; the header is the
    /// only oracle, and it makes a fixed dividend ignore the underlying.
    #[test]
    fn a_fixed_dividend_pays_its_amount_whatever_the_underlying() {
        let dividend = FixedDividend::new(3.5, today());

        assert_eq!(dividend.date(), today());
        assert_eq!(dividend.amount().unwrap(), 3.5);
        assert_eq!(dividend.amount_with_underlying(0.0), 3.5);
        assert_eq!(dividend.amount_with_underlying(100.0), 3.5);
    }

    #[test]
    fn a_fractional_dividend_pays_its_rate_times_the_underlying() {
        let dividend = FractionalDividend::new(0.02, None, today());

        assert_eq!(dividend.rate(), 0.02);
        assert_eq!(dividend.nominal(), None);
        assert_eq!(dividend.amount_with_underlying(150.0), 3.0);
    }

    /// The `QL_REQUIRE(nominal_ != Null<Real>(), "no nominal given")` of the
    /// header, as an `Err`.
    #[test]
    fn a_fractional_dividend_without_a_nominal_has_no_amount() {
        let without = FractionalDividend::new(0.02, None, today());
        let with = FractionalDividend::new(0.02, 150.0, today());

        assert!(without.amount().is_err());
        assert_eq!(with.nominal(), Some(150.0));
        assert_eq!(with.amount().unwrap(), 3.0);
    }

    #[test]
    fn a_dividend_uses_the_cash_flow_occurrence_rule() {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(false));

        let fixed = FixedDividend::new(3.5, today());
        let fractional = FractionalDividend::new(0.02, 150.0, today());

        assert!(fixed.has_occurred(&settings, None, None).unwrap());
        assert!(fractional.has_occurred(&settings, None, None).unwrap());
    }

    #[test]
    fn a_dividend_vector_pairs_each_date_with_its_amount() {
        let dates = [today(), today() + 90];
        let amounts = [1.0, 2.0];

        let dividends = dividend_vector(&dates, &amounts).unwrap();

        assert_eq!(dividends.len(), 2);
        assert_eq!(dividends[0].date(), today());
        assert_eq!(dividends[0].amount().unwrap(), 1.0);
        assert_eq!(dividends[1].date(), today() + 90);
        assert_eq!(dividends[1].amount_with_underlying(50.0), 2.0);
    }

    #[test]
    fn a_dividend_vector_rejects_a_size_mismatch() {
        assert!(dividend_vector(&[today()], &[1.0, 2.0]).is_err());
    }
}
