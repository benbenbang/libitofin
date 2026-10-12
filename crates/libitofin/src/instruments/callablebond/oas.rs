//! Pure Hull-White spread analytics, with settlement-normalized bond quotes.

use super::{
    CallabilitySchedule, CallableBondArguments, CallableFixedRateBond, CallableZeroCouponBond,
};
use crate::errors::QlResult;
use crate::instruments::{Bond, BondPrice};
use crate::interestrate::{Compounding, InterestRate};
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::pricingengine::Arguments;
use crate::pricingengines::bond::TreeCallableFixedRateBondEngine;
use crate::require;
use crate::settings::Settings;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::Real;

fn settlement_date(bond: &Bond, settings: &Settings<Date>, date: Option<Date>) -> QlResult<Date> {
    if let Some(date) = date {
        require!(date != Date::null(), "null settlement date");
        require!(
            bond.issue_date().is_none_or(|issue| date >= issue),
            "settlement before issue date"
        );
        return Ok(date);
    }
    let mut date = settings
        .evaluation_date()
        .ok_or_else(|| crate::errors::QlError::new("evaluation date is unset", file!(), line!()))?;
    require!(date != Date::null(), "null evaluation date");
    let mut remaining = bond.settlement_days();
    while remaining > 0 || bond.calendar().is_holiday(date) {
        require!(
            date < Date::max_date(),
            "settlement exceeds supported date range"
        );
        date += 1;
        if bond.calendar().is_business_day(date) {
            remaining = remaining.saturating_sub(1);
        }
    }
    Ok(bond.issue_date().map_or(date, |issue| date.max(issue)))
}

fn arguments(
    bond: &Bond,
    face_amount: Real,
    calls: &CallabilitySchedule,
    settings: &Settings<Date>,
    settlement: Date,
) -> QlResult<CallableBondArguments> {
    let redemption = bond
        .redemptions()
        .first()
        .ok_or_else(|| crate::errors::QlError::new("missing redemption", file!(), line!()))?;
    require!(
        settlement < redemption.date(),
        "settlement must precede redemption"
    );
    let mut args = CallableBondArguments {
        settlement_date: Some(settlement),
        face_amount,
        redemption: redemption.amount()?,
        redemption_date: Some(redemption.date()),
        ..CallableBondArguments::default()
    };
    for flow in bond.cashflows() {
        if flow.as_coupon().is_some()
            && flow.date() > settlement
            && !flow.trading_ex_coupon(settings, Some(settlement))?
        {
            args.coupon_dates.push(flow.date());
            args.coupon_amounts.push(flow.amount()?);
        }
    }
    for call in calls.iter().filter(|call| call.date() > settlement) {
        let mut price = call.price().amount();
        if let BondPrice::Clean(_) = call.price()
            && let Some(flow) = bond
                .cashflows()
                .iter()
                .find(|flow| flow.date() > call.date())
            && let Some(coupon) = flow.as_coupon()
        {
            let mut accrued = coupon.accrued_amount(call.date())?;
            if coupon.trades_ex_coupon_on(call.date()) {
                accrued += flow.amount()?;
            }
            let notional = bond.notional(Some(call.date()))?;
            require!(
                notional.is_finite() && notional > 0.0,
                "invalid notional at call date"
            );
            price += accrued / notional * 100.0;
        }
        args.callability_dates.push(call.date());
        args.callability_prices.push(price);
        args.callability_types.push(call.call_type());
    }
    args.validate()?;
    Ok(args)
}

fn converted_spread(
    spread: Real,
    curve: &dyn YieldTermStructure,
    maturity: Date,
    day_counter: DayCounter,
    compounding: Compounding,
    frequency: Frequency,
    to_continuous: bool,
) -> QlResult<Real> {
    require!(spread.is_finite(), "nonfinite callable bond spread");
    let reference = curve.reference_date()?;
    require!(
        maturity > reference,
        "maturity must follow curve reference date"
    );
    let time = day_counter.year_fraction(reference, maturity);
    require!(
        time.is_finite() && time > 0.0,
        "invalid spread conversion interval"
    );
    let (source_comp, source_freq, target_comp, target_freq) = if to_continuous {
        (
            compounding,
            frequency,
            Compounding::Continuous,
            Frequency::NoFrequency,
        )
    } else {
        (
            Compounding::Continuous,
            Frequency::NoFrequency,
            compounding,
            frequency,
        )
    };
    let base = curve.zero_rate_date(
        maturity,
        day_counter.clone(),
        source_comp,
        source_freq,
        false,
    )?;
    let spreaded = InterestRate::new(
        base.rate() + spread,
        day_counter.clone(),
        source_comp,
        source_freq,
    )?;
    let rate = spreaded
        .equivalent_rate_between(
            day_counter.clone(),
            target_comp,
            target_freq,
            reference,
            maturity,
        )?
        .rate()
        - base
            .equivalent_rate_between(day_counter, target_comp, target_freq, reference, maturity)?
            .rate();
    require!(rate.is_finite(), "spread conversion overflow");
    Ok(rate)
}

macro_rules! impl_oas {
    ($bond:ty) => {
        impl $bond {
            /// The clean settlement quote per 100 at a conventionally quoted OAS.
            ///
            /// Uses an explicit Hull-White engine as an immutable pricing probe,
            /// without replacing or updating the bond's installed engine/cache.
            /// `settlement` defaults to the bond's settlement date. The spread is
            /// converted at maturity using the model curve and supplied conventions.
            /// Unlike QuantLib's helper, reference-date NPV is normalized by the
            /// unspread curve discount at settlement before converting to a quote.
            ///
            /// # Errors
            /// Rejects invalid spreads, conventions, settlement dates or zero
            /// notionals; propagates cash-flow, curve and lattice pricing errors.
            #[allow(clippy::too_many_arguments)]
            pub fn clean_price_oas(
                &self,
                spread: Real,
                engine: &TreeCallableFixedRateBondEngine,
                day_counter: DayCounter,
                compounding: Compounding,
                frequency: Frequency,
                settlement: Option<Date>,
            ) -> QlResult<Real> {
                let settlement = settlement_date(self.bond(), &self.settings, settlement)?;
                let args = arguments(
                    self.bond(),
                    self.face_amount,
                    &self.put_call_schedule,
                    &self.settings,
                    settlement,
                )?;
                let curve = engine.discount_curve()?;
                let spread = converted_spread(
                    spread,
                    &*curve,
                    self.bond().maturity_date()?,
                    day_counter,
                    compounding,
                    frequency,
                    true,
                )?;
                let results = engine.price_with_spread(&args, spread)?;
                let value = results.settlement_value.ok_or_else(|| {
                    crate::errors::QlError::new("missing settlement value", file!(), line!())
                })?;
                let notional = self.bond().notional(Some(settlement))?;
                require!(
                    notional.is_finite() && notional > 0.0,
                    "invalid settlement notional"
                );
                let price =
                    value / notional * 100.0 - self.bond().accrued_amount(Some(settlement))?;
                require!(price.is_finite(), "callable clean price overflow");
                Ok(price)
            }

            /// Recovers conventionally quoted OAS from a clean settlement quote.
            ///
            /// `guess` and `accuracy` are continuous short-rate spread units, not
            /// basis points or conventionally compounded rates. Uses Brent with
            /// an initial continuous spread step of 0.001. Does not alter either
            /// the explicit engine or the bond's installed engine/cache on error.
            ///
            /// # Errors
            /// Rejects nonfinite targets/guesses, nonpositive accuracy or budgets;
            /// propagates price/convention failures and root-finding exhaustion.
            #[allow(clippy::too_many_arguments)]
            pub fn oas(
                &self,
                clean_price: Real,
                engine: &TreeCallableFixedRateBondEngine,
                day_counter: DayCounter,
                compounding: Compounding,
                frequency: Frequency,
                settlement: Option<Date>,
                accuracy: Real,
                max_evaluations: usize,
                guess: Real,
            ) -> QlResult<Real> {
                require!(
                    clean_price.is_finite() && guess.is_finite(),
                    "nonfinite OAS target or guess"
                );
                require!(
                    accuracy.is_finite() && accuracy > 0.0 && max_evaluations > 0,
                    "invalid OAS solver configuration"
                );
                let settlement = settlement_date(self.bond(), &self.settings, settlement)?;
                let args = arguments(
                    self.bond(),
                    self.face_amount,
                    &self.put_call_schedule,
                    &self.settings,
                    settlement,
                )?;
                let notional = self.bond().notional(Some(settlement))?;
                require!(
                    notional.is_finite() && notional > 0.0,
                    "invalid settlement notional"
                );
                let target = clean_price + self.bond().accrued_amount(Some(settlement))?;
                require!(target.is_finite(), "OAS target overflow");
                converted_spread(
                    0.0,
                    &*engine.discount_curve()?,
                    self.bond().maturity_date()?,
                    day_counter.clone(),
                    compounding,
                    frequency,
                    false,
                )?;
                let mut error = None;
                let mut evaluations = 0;
                let result = Brent::new().with_max_evaluations(max_evaluations).solve(
                    |spread| {
                        if error.is_some() {
                            return Real::NAN;
                        }
                        if evaluations >= max_evaluations {
                            error = Some(crate::errors::QlError::new(
                                "maximum OAS pricing evaluations exceeded",
                                file!(),
                                line!(),
                            ));
                            return Real::NAN;
                        }
                        evaluations += 1;
                        match engine.price_with_spread(&args, spread) {
                            Ok(results) => results
                                .settlement_value
                                .map_or(Real::NAN, |price| target - price / notional * 100.0),
                            Err(err) => {
                                error = Some(err);
                                Real::NAN
                            }
                        }
                    },
                    accuracy,
                    guess,
                    0.001,
                );
                if let Some(error) = error {
                    return Err(error);
                }
                let spread = result?;
                converted_spread(
                    spread,
                    &*engine.discount_curve()?,
                    self.bond().maturity_date()?,
                    day_counter,
                    compounding,
                    frequency,
                    false,
                )
            }
        }
    };
}

impl_oas!(CallableFixedRateBond);
impl_oas!(CallableZeroCouponBond);
