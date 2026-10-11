//! Black valuation of a single European call or put on a fixed-rate bond.
//!
//! Constant forward-yield volatility follows QuantLib's callable Black engine,
//! with settlement PV converted back to the curve reference date before
//! forming the forward price. This corrects QuantLib's missing settlement
//! discount factor when settlement follows the curve reference date. This is not
//! a callable volatility surface or a multiple-exercise model.

use crate::cashflow::Leg;
use crate::cashflows::{CashFlows, Duration};
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::instruments::{BondResults, CallabilityType, CallableBondArguments};
use crate::interestrate::{Compounding, InterestRate};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::pricingengines::blackformula::black_formula;
use crate::quotes::Quote;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;
use crate::types::Real;

/// The additional inputs needed by the single-exercise Black bond engine.
///
/// The existing [`CallableBondArguments`] layout remains unchanged. Both
/// callable fixed-rate and zero-coupon products populate this bundle.
pub struct BlackCallableBondArguments {
    /// Existing settlement and exercise inputs.
    pub bond: CallableBondArguments,
    /// Full cash-flow leg, including flows before settlement.
    pub cashflows: Leg,
    /// Day counter for the forward compounded yield and modified duration.
    pub payment_day_counter: Option<DayCounter>,
    /// Coupon frequency; once/no-frequency are interpreted as annual.
    pub frequency: Frequency,
    /// Number of exercise rights in the original, unfiltered schedule.
    pub callability_count: usize,
}

impl Default for BlackCallableBondArguments {
    fn default() -> Self {
        Self {
            bond: CallableBondArguments::default(),
            cashflows: Vec::new(),
            payment_day_counter: None,
            frequency: Frequency::NoFrequency,
            callability_count: 0,
        }
    }
}

impl Arguments for BlackCallableBondArguments {
    fn validate(&self) -> QlResult<()> {
        self.bond.validate()?;
        require!(
            self.callability_count == 1 && self.bond.callability_dates.len() == 1,
            "Black callable engine requires exactly one unexpired call/put"
        );
        require!(
            self.payment_day_counter.is_some(),
            "missing payment day counter"
        );
        require!(
            !self.cashflows.is_empty(),
            "empty Black callable cash flows"
        );
        require!(
            self.cashflows
                .windows(2)
                .all(|pair| pair[0].date() <= pair[1].date()),
            "Black callable cash flows must be ordered"
        );
        for flow in &self.cashflows {
            require!(
                flow.date() != Date::null(),
                "null Black callable cash-flow date"
            );
            require!(
                flow.amount()?.is_finite(),
                "nonfinite Black callable cash flow"
            );
        }
        Ok(())
    }
}

/// Black engine for a single hard European call or put on a callable bond.
///
/// The quoted volatility is relative forward-yield volatility, not price
/// volatility. Its exercise clock is Actual/365 Fixed from the evaluation date.
/// Negative forward yields with positive volatility cannot use this lognormal
/// model. Discounting, yield conversion and settlement follow QuantLib's engine.
/// Quote, curve-handle and evaluation-date changes invalidate attached bonds.
pub struct BlackCallableFixedRateBondEngine {
    base: GenericEngine<BlackCallableBondArguments, BondResults>,
    yield_volatility: Handle<dyn Quote>,
    discount_curve: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
}

impl BlackCallableFixedRateBondEngine {
    /// Creates a constant forward-yield-volatility engine.
    ///
    /// Empty handles and invalid market inputs are rejected at calculation.
    pub fn new(
        yield_volatility: Handle<dyn Quote>,
        discount_curve: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        let base = GenericEngine::new(
            BlackCallableBondArguments::default(),
            BondResults::default(),
        );
        yield_volatility.register_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        settings.register_eval_date_observer(&base.observer());
        Self {
            base,
            yield_volatility,
            discount_curve,
            settings,
        }
    }

    fn forward_price_volatility(
        &self,
        curve: &dyn YieldTermStructure,
        exercise: Date,
        volatility: Real,
    ) -> QlResult<Real> {
        let args = self.base.arguments();
        let day_counter = args.payment_day_counter.clone().ok_or_else(|| {
            crate::errors::QlError::new("missing payment day counter", file!(), line!())
        })?;
        let frequency = match args.frequency {
            Frequency::Once | Frequency::NoFrequency => Frequency::Annual,
            other => other,
        };
        let forward_npv = CashFlows::npv(
            &args.cashflows,
            curve,
            &self.settings,
            Some(false),
            Some(exercise),
            Some(exercise),
        )?;
        require!(
            forward_npv.is_finite() && forward_npv > 0.0,
            "invalid forward bond NPV"
        );
        let yield_rate = CashFlows::solve_yield(
            &args.cashflows,
            forward_npv,
            day_counter.clone(),
            Compounding::Compounded,
            frequency,
            &self.settings,
            Some(false),
            Some(exercise),
            Some(exercise),
            None,
            None,
            None,
        )?;
        let rate = InterestRate::new(yield_rate, day_counter, Compounding::Compounded, frequency)?;
        let duration = CashFlows::duration(
            &args.cashflows,
            &rate,
            Duration::Modified,
            &self.settings,
            Some(false),
            Some(exercise),
            Some(exercise),
        )?;
        let price_volatility = volatility * duration * yield_rate;
        require!(
            price_volatility.is_finite() && price_volatility >= 0.0,
            "invalid lognormal forward-price volatility"
        );
        Ok(price_volatility)
    }
}

impl AsObservable for BlackCallableFixedRateBondEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BlackCallableFixedRateBondEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }
    fn results(&self) -> &dyn Results {
        self.base.results()
    }
    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        self.base.reset();
        self.base.arguments().validate()?;
        let evaluation = self.settings.evaluation_date().ok_or_else(|| {
            crate::errors::QlError::new("evaluation date is unset", file!(), line!())
        })?;
        require!(evaluation != Date::null(), "null evaluation date");
        let curve = self.discount_curve.current_link()?;
        let volatility = self.yield_volatility.current_link()?.value()?;
        require!(
            volatility.is_finite() && volatility >= 0.0,
            "invalid forward-yield volatility"
        );
        let args = self.base.arguments();
        let settlement = args
            .bond
            .settlement_date
            .ok_or_else(|| crate::errors::QlError::new("null settlement date", file!(), line!()))?;
        let reference = curve.reference_date()?;
        let exercise = args.bond.callability_dates[0];
        require!(
            settlement >= reference,
            "settlement before curve reference date"
        );
        require!(
            exercise >= evaluation,
            "exercise before volatility reference date"
        );
        let settlement_discount = curve.discount_date(settlement, false)?;
        let exercise_discount = curve.discount_date(exercise, false)?;
        require!(
            settlement_discount.is_finite()
                && settlement_discount > 0.0
                && exercise_discount.is_finite()
                && exercise_discount > 0.0,
            "invalid Black callable discount"
        );
        for flow in &args.cashflows {
            if flow.date() >= reference {
                let discount = curve.discount_date(flow.date(), false)?;
                require!(
                    discount.is_finite() && discount > 0.0,
                    "invalid cash-flow discount"
                );
            }
        }
        let value = CashFlows::npv(
            &args.cashflows,
            &*curve,
            &self.settings,
            Some(false),
            Some(settlement),
            Some(settlement),
        )?;
        let npv = CashFlows::npv(
            &args.cashflows,
            &*curve,
            &self.settings,
            Some(false),
            Some(reference),
            Some(reference),
        )?;
        let mut income = 0.0;
        for flow in args.cashflows.iter().take(args.cashflows.len() - 1) {
            if flow.date() > settlement
                && flow.date() <= exercise
                && !flow.trading_ex_coupon(&self.settings, Some(settlement))?
            {
                income += flow.amount()? * curve.discount_date(flow.date(), false)?;
            }
        }
        income /= settlement_discount;
        let forward = (value - income) * settlement_discount / exercise_discount;
        let strike = args.bond.callability_prices[0] * args.bond.face_amount / 100.0;
        require!(
            forward.is_finite() && forward > 0.0 && strike.is_finite() && strike >= 0.0,
            "invalid Black callable forward or strike"
        );
        let price_volatility = self.forward_price_volatility(&*curve, exercise, volatility)?;
        let exercise_time = Actual365Fixed::new().year_fraction(evaluation, exercise);
        let standard_deviation = price_volatility * exercise_time.sqrt();
        require!(
            standard_deviation.is_finite(),
            "Black callable standard deviation overflow"
        );
        let option_type = match args.bond.callability_types[0] {
            CallabilityType::Call => OptionType::Call,
            CallabilityType::Put => OptionType::Put,
        };
        let option = black_formula(option_type, strike, forward, standard_deviation, 1.0, 0.0)?;
        let sign = if option_type == OptionType::Call {
            -1.0
        } else {
            1.0
        };
        let result = npv + sign * option * exercise_discount;
        let settlement_value = value + sign * option * exercise_discount / settlement_discount;
        require!(
            result.is_finite() && settlement_value.is_finite(),
            "Black callable value overflow"
        );
        let results = self.base.results_mut();
        results.instrument.value = Some(result);
        results.instrument.valuation_date = Some(reference);
        results.settlement_value = Some(settlement_value);
        Ok(())
    }
}
