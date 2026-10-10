//! Plain constant-notional, pay-base/receive-quote Ibor basis swap.
//!
//! The Ibor slice follows native constant-notional swap and engine laws.
use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{CashFlows, IborLeg, SimpleCashFlow};
use crate::errors::QlResult;
use crate::exchangerate::ExchangeRate;
use crate::handle::Handle;
use crate::indexes::{IborIndex, InterestRateIndex};
use crate::money::Money;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{
    businessdayconvention::BusinessDayConvention, date::Date, daycounter::DayCounter,
    schedule::Schedule,
};
/// Pay base coupons and final principal, receive quote coupons and final principal.
/// Initial exchanges have the opposite signs. Both notionals remain constant.
///
/// Each query rereads curves, fixings and settings without a cached engine.
/// The immutable spot snapshot is quote units per base unit at the discount
/// curve reference date, or an explicit spot settlement date. Curves must be supplied in the corresponding currencies; yield curves
/// themselves carry no currency metadata. Arbitrary positive notionals are
/// supported, including notionals not matched by the spot rate. Both discount curves must
/// have the same reference date, matching the native cross-currency engine.
pub struct XccyBasisSwap {
    base_leg: Leg,
    quote_leg: Leg,
    base_curve: Handle<dyn YieldTermStructure>,
    quote_curve: Handle<dyn YieldTermStructure>,
    spot: ExchangeRate,
    quote_basis_spread: f64,
    settings: Shared<Settings<Date>>,
    valuation: Option<(Date, Date, bool)>,
    spot_settlement: Option<Date>,
}
impl XccyBasisSwap {
    /// Builds two Ibor legs with mandatory initial and final notional exchanges.
    /// Exchanges use the earliest accrual start and latest coupon accrual maturity across
    /// both legs, adjusted on each schedule calendar/convention.
    /// Payment adjustment defaults to Following, matching native IborLeg.
    ///
    /// Empty curve handles are accepted for later linking. By default both the
    /// settlement cutoff and common NPV date follow the evaluation date; inclusion
    /// follows Settings. Spot must be refreshed by rebuilding for a new market date.
    /// # Errors
    /// Rejects invalid spot direction, identical/blank currencies, non-positive
    /// or non-finite notionals/spot, non-finite spread, mismatched settings and
    /// invalid legs. Leg-building errors are propagated.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_schedule: Schedule,
        base_index: Shared<IborIndex>,
        base_curve: Handle<dyn YieldTermStructure>,
        base_notional: f64,
        base_day_count: DayCounter,
        quote_schedule: Schedule,
        quote_index: Shared<IborIndex>,
        quote_curve: Handle<dyn YieldTermStructure>,
        quote_notional: f64,
        quote_day_count: DayCounter,
        quote_basis_spread: f64,
        spot: ExchangeRate,
        payment_convention: Option<BusinessDayConvention>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        for index in [&base_index, &quote_index] {
            crate::require!(
                index.tenor().units() != crate::time::timeunit::TimeUnit::Days,
                "day-tenor and overnight indexes are not supported"
            );
        }
        crate::require!(spot.source() != spot.target(), "currencies must differ");
        for currency in [spot.source(), spot.target()] {
            crate::require!(
                !currency.code().trim().is_empty() && !currency.name().trim().is_empty(),
                "currencies must not be blank"
            );
        }
        crate::require!(
            base_index.currency() == spot.source() && quote_index.currency() == spot.target(),
            "spot source/target must match base/quote index currencies"
        );
        crate::require!(
            Shared::ptr_eq(base_index.base().settings(), &settings)
                && Shared::ptr_eq(quote_index.base().settings(), &settings),
            "indexes must share swap settings"
        );
        for value in [base_notional, quote_notional, spot.rate()] {
            crate::require!(
                value.is_finite() && value > 0.0,
                "notionals and spot must be finite and positive"
            );
        }
        crate::require!(
            quote_basis_spread.is_finite(),
            "basis spread must be finite"
        );
        for schedule in [&base_schedule, &quote_schedule] {
            crate::require!(
                schedule.len() >= 2,
                "schedule must contain at least two dates"
            );
            for date in schedule.dates() {
                Self::valid_date(*date)?;
            }
            crate::require!(
                schedule.dates().windows(2).all(|pair| pair[0] < pair[1]),
                "schedule dates must increase"
            );
        }
        let base_calendar = base_schedule.calendar().clone();
        let quote_calendar = quote_schedule.calendar().clone();
        let base_convention = base_schedule.business_day_convention();
        let quote_convention = quote_schedule.business_day_convention();
        let start = base_schedule.start_date().min(quote_schedule.start_date());
        let mut base_leg = Self::build_leg(
            base_schedule,
            base_index,
            base_notional,
            0.0,
            base_day_count,
            payment_convention,
        )?;
        let mut quote_leg = Self::build_leg(
            quote_schedule,
            quote_index,
            quote_notional,
            quote_basis_spread,
            quote_day_count,
            payment_convention,
        )?;
        let end = CashFlows::maturity_date(&base_leg)?.max(CashFlows::maturity_date(&quote_leg)?);
        for (leg, calendar, convention, notional) in [
            (&mut base_leg, base_calendar, base_convention, base_notional),
            (
                &mut quote_leg,
                quote_calendar,
                quote_convention,
                quote_notional,
            ),
        ] {
            leg.insert(
                0,
                shared(SimpleCashFlow::new(
                    -notional,
                    calendar.adjust(start, convention),
                )?) as Shared<dyn CashFlow>,
            );
            leg.push(shared(SimpleCashFlow::new(
                notional,
                calendar.adjust(end, convention),
            )?) as Shared<dyn CashFlow>);
        }
        Ok(Self {
            base_leg,
            quote_leg,
            base_curve,
            quote_curve,
            spot,
            quote_basis_spread,
            settings,
            valuation: None,
            spot_settlement: None,
        })
    }
    fn build_leg(
        schedule: Schedule,
        index: Shared<IborIndex>,
        notional: f64,
        spread: f64,
        day_count: DayCounter,
        convention: Option<BusinessDayConvention>,
    ) -> QlResult<Leg> {
        let leg = IborLeg::new(schedule, index)
            .with_notional(notional)
            .with_spread(spread)
            .with_payment_day_counter(day_count)
            .with_payment_adjustment(convention.unwrap_or(BusinessDayConvention::Following))
            .build()?;
        crate::require!(!leg.is_empty(), "empty coupon leg");
        Ok(leg)
    }
    /// Uses fixed settlement/NPV dates and a settlement-flow inclusion flag.
    /// Settings' today-cashflow override still takes precedence on evaluation day.
    /// # Errors
    /// Rejects null or invalid dates. Curve-domain checks occur when pricing.
    pub fn with_valuation_dates(
        mut self,
        settlement: Date,
        npv_date: Date,
        include: bool,
    ) -> QlResult<Self> {
        Self::valid_date(settlement)?;
        Self::valid_date(npv_date)?;
        self.valuation = Some((settlement, npv_date, include));
        Ok(self)
    }

    /// Interprets spot at a fixed settlement date instead of the curve reference.
    /// Conversion uses `spot * quote_discount(date) / base_discount(date)`.
    /// # Errors
    /// Rejects invalid dates; curve-domain errors occur on valuation.
    pub fn with_spot_settlement_date(mut self, date: Date) -> QlResult<Self> {
        Self::valid_date(date)?;
        self.spot_settlement = Some(date);
        Ok(self)
    }

    fn effective_spot(&self) -> QlResult<f64> {
        let base = self.base_curve.current_link()?;
        let quote = self.quote_curve.current_link()?;
        let reference = base.reference_date()?;
        Self::valid_date(reference)?;
        crate::require!(
            reference == quote.reference_date()?,
            "discount curves must share a reference date"
        );
        let date = self.spot_settlement.unwrap_or(reference);
        Self::valid_date(date)?;
        crate::require!(
            date >= reference,
            "spot settlement precedes curve reference date"
        );
        let mut discounts = [0.0; 2];
        for (i, curve) in [&base, &quote].iter().enumerate() {
            discounts[i] = curve.discount_date(date, false)?;
            crate::require!(
                discounts[i].is_finite() && discounts[i] > 0.0,
                "spot settlement discounts must be finite and positive"
            );
        }
        let rate = self.spot.rate() * discounts[1] / discounts[0];
        crate::require!(
            rate.is_finite() && rate > 0.0,
            "effective spot must be finite and positive"
        );
        Ok(rate)
    }

    fn valid_date(date: Date) -> QlResult<()> {
        crate::require!(
            date >= Date::min_date() && date <= Date::max_date(),
            "date must be valid and non-null"
        );
        Ok(())
    }

    fn measure(&self, leg: &Leg, handle: &Handle<dyn YieldTermStructure>) -> QlResult<(f64, f64)> {
        let (settlement, npv_date, include) = match self.valuation {
            Some((s, n, i)) => (s, n, Some(i)),
            None => {
                let date = self.settings.evaluation_date().ok_or_else(|| {
                    crate::errors::QlError::new("evaluation date is unset", file!(), line!())
                })?;
                (date, date, None)
            }
        };
        Self::valid_date(settlement)?;
        Self::valid_date(npv_date)?;
        self.effective_spot()?;
        let curve = handle.current_link()?;
        crate::require!(
            curve.reference_date()? <= settlement && curve.reference_date()? <= npv_date,
            "curve reference date follows valuation dates"
        );
        let discount = curve.discount_date(npv_date, false)?;
        crate::require!(
            discount.is_finite() && discount > 0.0,
            "NPV date discount must be finite and positive"
        );
        let result = CashFlows::npvbps(
            leg,
            &*curve,
            &self.settings,
            include,
            Some(settlement),
            Some(npv_date),
        )?;
        crate::require!(
            result.0.is_finite() && result.1.is_finite(),
            "leg NPV and BPS must be finite"
        );
        Ok(result)
    }

    /// Unsigned contractual base-leg PV in base currency, including exchanges.
    /// # Errors
    /// Propagates curve-domain, missing-fixing and non-finite valuation errors.
    pub fn base_leg_pv(&self) -> QlResult<f64> {
        Ok(self.measure(&self.base_leg, &self.base_curve)?.0)
    }
    /// Unsigned contractual quote-leg PV in quote currency, including exchanges.
    /// # Errors
    /// Propagates curve-domain, missing-fixing and non-finite valuation errors.
    pub fn quote_leg_pv(&self) -> QlResult<f64> {
        Ok(self.measure(&self.quote_leg, &self.quote_curve)?.0)
    }
    /// Quote-currency spread sensitivity per basis point, excluding principal.
    /// # Errors
    /// Propagates valuation errors.
    pub fn quote_leg_bps(&self) -> QlResult<f64> {
        Ok(self.measure(&self.quote_leg, &self.quote_curve)?.1)
    }
    /// Base-currency NPV: quote PV divided by spot, minus base PV.
    /// # Errors
    /// Propagates valuation errors and rejects non-finite conversion.
    pub fn npv(&self) -> QlResult<Money> {
        let converted = self.quote_leg_pv()? / self.effective_spot()?;
        crate::require!(converted.is_finite(), "converted leg PV must be finite");
        Money::checked_new(self.spot.source().clone(), converted - self.base_leg_pv()?)
    }
    /// Quote-currency NPV: quote PV minus base PV multiplied by spot.
    /// # Errors
    /// Propagates valuation errors and rejects non-finite conversion.
    pub fn npv_in_quote(&self) -> QlResult<Money> {
        let converted = self.base_leg_pv()? * self.effective_spot()?;
        crate::require!(converted.is_finite(), "converted leg PV must be finite");
        Money::checked_new(self.spot.target().clone(), self.quote_leg_pv()? - converted)
    }
    /// Absolute quote spread which reprices the swap to zero, not a spread shift.
    /// # Errors
    /// Rejects zero/non-finite sensitivity and propagates valuation errors.
    pub fn fair_quote_basis_spread(&self) -> QlResult<f64> {
        let annuity = self.quote_leg_bps()? / 1e-4;
        crate::require!(
            annuity.is_finite() && annuity != 0.0,
            "null or non-finite quote spread sensitivity"
        );
        let fair = self.quote_basis_spread - self.npv_in_quote()?.value() / annuity;
        crate::require!(fair.is_finite(), "fair spread must be finite");
        Ok(fair)
    }
    /// Immutable quote-per-base snapshot at reference or explicit spot settlement.
    pub fn spot(&self) -> &ExchangeRate {
        &self.spot
    }
    /// Base flows before the paid-leg sign is applied.
    pub fn base_leg(&self) -> &Leg {
        &self.base_leg
    }
    /// Quote flows before the received-leg sign is applied.
    pub fn quote_leg(&self) -> &Leg {
        &self.quote_leg
    }
}
