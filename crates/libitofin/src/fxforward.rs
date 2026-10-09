//! Snapshot-spot FX forwards with live discount curves and explicit dates.
//!
//! Settlement-normalized pricing follows the algebra in QuantLib's
//! `DiscountingFxForwardEngine`. This is not its instrument/engine API: there
//! is no rolling spot lag, calendar, observer cache, or live spot quote.

use crate::errors::QlResult;
use crate::exchangerate::ExchangeRate;
use crate::handle::Handle;
use crate::money::Money;
use crate::require;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::Real;

/// Receive base currency and pay quote currency when long; reverse when short.
///
/// Spot is an immutable target-per-source snapshot for `spot_settlement_date`.
/// Each calculation rereads both curve handles, including relinks. Quote NPV
/// is discounted to the quote curve's reference date, not the evaluation date.
/// Rebuild with a fresh spot snapshot/date for a new market valuation.
pub struct FxForward {
    spot: ExchangeRate,
    base_curve: Handle<dyn YieldTermStructure>,
    quote_curve: Handle<dyn YieldTermStructure>,
    base_notional: Real,
    strike: Real,
    spot_settlement_date: Date,
    delivery_date: Date,
    long: bool,
}

impl FxForward {
    /// Builds a forward with explicit, fixed spot-settlement and delivery dates.
    ///
    /// Curves correspond to spot source/base and target/quote respectively.
    /// A derived exchange rate is used only as a numeric endpoint-rate snapshot.
    /// Empty curve handles are allowed for later relinking, but cannot price.
    ///
    /// # Errors
    /// Rejects identical/blank currencies, invalid or non-positive spot,
    /// notional or strike, invalid dates, delivery before spot settlement, and
    /// overflow/underflow of the contracted quote notional.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spot: ExchangeRate,
        base_curve: Handle<dyn YieldTermStructure>,
        quote_curve: Handle<dyn YieldTermStructure>,
        base_notional: Real,
        strike: Real,
        spot_settlement_date: Date,
        delivery_date: Date,
        long: bool,
    ) -> QlResult<Self> {
        require!(spot.source() != spot.target(), "currencies must differ");
        for currency in [spot.source(), spot.target()] {
            require!(
                !currency.name().trim().is_empty() && !currency.code().trim().is_empty(),
                "currencies must not be blank"
            );
        }
        Self::positive(spot.rate(), "spot rate")?;
        Self::positive(base_notional, "base notional")?;
        Self::positive(strike, "strike")?;
        Self::positive(base_notional * strike, "quote notional")?;
        Self::date(spot_settlement_date)?;
        Self::date(delivery_date)?;
        require!(
            delivery_date >= spot_settlement_date,
            "delivery precedes spot settlement"
        );
        Ok(Self {
            spot,
            base_curve,
            quote_curve,
            base_notional,
            strike,
            spot_settlement_date,
            delivery_date,
            long,
        })
    }

    fn positive(value: Real, name: &str) -> QlResult<Real> {
        require!(
            value.is_finite() && value > 0.0,
            "{name} must be finite and positive"
        );
        Ok(value)
    }

    fn date(date: Date) -> QlResult<()> {
        require!(
            date >= Date::min_date() && date <= Date::max_date(),
            "date must be valid and non-null"
        );
        Ok(())
    }

    fn discounts(&self) -> QlResult<(Real, Real, Real)> {
        let base = self.base_curve.current_link()?;
        let quote = self.quote_curve.current_link()?;
        for curve in [&base, &quote] {
            let reference = curve.reference_date()?;
            Self::date(reference)?;
            require!(
                reference <= self.spot_settlement_date,
                "curve reference date follows spot settlement"
            );
        }
        let base_settlement = Self::positive(
            base.discount_date(self.spot_settlement_date, false)?,
            "base settlement discount",
        )?;
        let quote_settlement = Self::positive(
            quote.discount_date(self.spot_settlement_date, false)?,
            "quote settlement discount",
        )?;
        let base_delivery = Self::positive(
            base.discount_date(self.delivery_date, false)?,
            "base delivery discount",
        )?;
        let quote_delivery = Self::positive(
            quote.discount_date(self.delivery_date, false)?,
            "quote delivery discount",
        )?;
        let base_forward =
            Self::positive(base_delivery / base_settlement, "base forward discount")?;
        let quote_forward =
            Self::positive(quote_delivery / quote_settlement, "quote forward discount")?;
        Ok((base_forward, quote_forward, quote_settlement))
    }

    /// Fair quote-per-base rate for the fixed contractual dates.
    ///
    /// Does not apply expiry: no evaluation date is implied by this query.
    /// # Errors
    /// Rejects empty handles, inadmissible dates/curve domains, non-positive or
    /// non-finite discounts and non-representable intermediate/final rates.
    pub fn fair_forward_rate(&self) -> QlResult<Real> {
        let (base, quote, _) = self.discounts()?;
        let numerator = Self::positive(self.spot.rate() * base, "forward numerator")?;
        Self::positive(numerator / quote, "forward rate")
    }

    /// Quote-currency NPV as of the current quote curve reference date.
    ///
    /// `evaluation_date` controls only expiry. Delivery before evaluation is
    /// expired and returns zero without reading curves. Delivery on evaluation
    /// remains active, independent of reference-event/cashflow settings.
    /// # Errors
    /// Rejects an invalid evaluation date, invalid discounts, or any non-finite
    /// intermediate/final cash amount, even if cancellation could recover it.
    pub fn npv(&self, evaluation_date: Date) -> QlResult<Money> {
        if self.is_expired(evaluation_date)? {
            return Money::checked_new(self.spot.target().clone(), 0.0);
        }
        let (base, quote, quote_settlement) = self.discounts()?;
        let base_pv = Self::positive(self.base_notional * base, "base leg PV")?;
        let quote_pv = Self::positive((self.base_notional * self.strike) * quote, "quote leg PV")?;
        let quote_in_base = Self::positive(quote_pv / self.spot.rate(), "converted quote leg PV")?;
        let settlement_base = if self.long {
            base_pv - quote_in_base
        } else {
            -base_pv + quote_in_base
        };
        require!(settlement_base.is_finite(), "settlement NPV must be finite");
        let settlement_quote = settlement_base * self.spot.rate();
        require!(
            settlement_quote.is_finite(),
            "converted settlement NPV must be finite"
        );
        Money::checked_new(
            self.spot.target().clone(),
            settlement_quote * quote_settlement,
        )
    }

    /// Whether delivery is strictly before the explicit evaluation date.
    /// # Errors
    /// Rejects null or invalid evaluation dates.
    pub fn is_expired(&self, evaluation_date: Date) -> QlResult<bool> {
        Self::date(evaluation_date)?;
        Ok(self.delivery_date < evaluation_date)
    }

    /// Immutable endpoint-rate snapshot, quoted target per source.
    pub fn spot(&self) -> &ExchangeRate {
        &self.spot
    }
    /// Contractual quote-per-base rate.
    pub fn strike(&self) -> Real {
        self.strike
    }
    /// Positive source-currency notional.
    pub fn base_notional(&self) -> Real {
        self.base_notional
    }
    /// Fixed value date of the supplied spot snapshot.
    pub fn spot_settlement_date(&self) -> Date {
        self.spot_settlement_date
    }
    /// Unadjusted delivery date.
    pub fn delivery_date(&self) -> Date {
        self.delivery_date
    }
    /// Whether this side receives base currency and pays quote currency.
    pub fn is_long(&self) -> bool {
        self.long
    }
}
