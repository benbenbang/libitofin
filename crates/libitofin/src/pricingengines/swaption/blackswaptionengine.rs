//! Black-formula swaption engine.
//!
//! Port of `ql/pricingengines/swaption/blackswaptionengine.{hpp}`:
//! [`BlackSwaptionEngine`] prices a [`Swaption`](crate::instruments::Swaption)
//! as a Black 1976 option on the underlying swap's fair rate, using a
//! [`SwaptionVolatilityStructure`] for the variance and shift and the swap's
//! annuity for the discounting (`blackswaptionengine.hpp:220-327`). It installs
//! a [`DiscountingSwapEngine`] on the swap, reads its `valuationDate`,
//! `fairRate` and leg BPS, and feeds those into [`black_formula`].
//!
//! Only the shifted-lognormal (`Black76`) instantiation of the C++ `Spec`
//! template is ported here; the Bachelier (`Normal`) engine and the swaption
//! delta oracle land with #365.
//!
//! ## Divergences from QuantLib
//!
//! - **The C++ engine mutates its own argument.** It installs the discounting
//!   engine on the swap it was handed, wrapped in
//!   `ObservableSettings::instance().disableUpdates()` /
//!   `enableUpdates()` (`blackswaptionengine.hpp:247-249`) so the swaption
//!   observing that swap is not invalidated mid-calculation. `ObservableSettings`
//!   is a global singleton with no Rust counterpart (deliberately not ported,
//!   D5). The port expresses the same suppression through
//!   [`InstrumentBase::set_pricing_engine_silent`](crate::instrument::InstrumentBase::set_pricing_engine_silent):
//!   a targeted non-broadcasting install that invalidates the swap locally (so
//!   it reprices on the engine's discount curve) without notifying the swaption.
//!   The swap is borrowed mutably only to install and price; no notification
//!   runs while that borrow is live, honouring the D1 re-entrancy rule.
//! - **No `firstCoupon` downcast.** C++ `dynamic_pointer_cast<FixedRateCoupon>`
//!   (`:236`) only calls `accrualStartDate()` and `dayCounter()` on the result,
//!   both `Coupon`-level, so the port reads them through
//!   [`CashFlow::as_coupon`](crate::cashflow::CashFlow::as_coupon).
//! - **`CashAnnuityModel` has no default.** The C++ header defaults to
//!   [`DiscountCurve`](CashAnnuityModel::DiscountCurve) (`:143`) but every
//!   ported test passes [`SwapRate`](CashAnnuityModel::SwapRate)
//!   (`swaption.cpp:85`), so every test takes the `valuation_date` branch and
//!   the `DiscountCurve` branch (first coupon accrual start) is UNPINNED by the
//!   oracle. It is ported but untested.
//! - **The `Cash && CollateralizedCashPrice` annuity arm** has no oracle in the
//!   swaption test file (only the delta helper at `:1057`, pinned by #365). It
//!   is ported but unexercised here.
//! - Only the `Handle<Quote>` and `Handle<SwaptionVolatilityStructure>`
//!   constructors are ported; the flat-`Volatility` convenience constructor
//!   (`:57`) is subsumed by [`with_flat_vol`](BlackSwaptionEngine::with_flat_vol)
//!   taking a quote handle, matching how the tests build the engine.

use std::any::Any;

use crate::cashflow::Leg;
use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::handle::Handle;
use crate::instrument::{Instrument, InstrumentResults};
use crate::instruments::{
    SettlementMethod, SettlementType, SwapType, SwaptionArguments, SwaptionEngine,
};
use crate::interestrate::{Compounding, InterestRate};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::DiscountingSwapEngine;
use crate::pricingengines::blackformula::{
    black_formula, black_formula_forward_derivative, black_formula_std_dev_derivative,
};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{
    ConstantSwaptionVolatility, SwaptionVolatilityStructure, VolatilityType,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::Real;
use crate::{fail, require};

/// One basis point, the unit the fixed-leg BPS annuity divides by
/// (`blackswaptionengine.hpp:222`).
const BASIS_POINT: Real = 1.0e-4;

/// How the cash-settled par-yield annuity picks its discount date
/// (`BlackStyleSwaptionEngine::CashAnnuityModel`, `blackswaptionengine.hpp:56`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CashAnnuityModel {
    /// Discount at the swap's valuation date (the fixture's choice,
    /// `swaption.cpp:85`).
    SwapRate,
    /// Discount at the first fixed coupon's accrual start date (the C++ header
    /// default; unpinned by the ported oracle).
    DiscountCurve,
}

/// Shifted-lognormal Black-formula swaption engine
/// (`blackswaptionengine.hpp:136`).
pub struct BlackSwaptionEngine {
    base: SwaptionEngine,
    discount_curve: Handle<dyn YieldTermStructure>,
    vol: Handle<dyn SwaptionVolatilityStructure>,
    model: CashAnnuityModel,
    settings: Shared<Settings<Date>>,
}

impl BlackSwaptionEngine {
    /// Builds the engine over a discount curve and a swaption volatility surface
    /// (the C++ `Handle<SwaptionVolatilityStructure>` constructor,
    /// `blackswaptionengine.hpp:149`).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn SwaptionVolatilityStructure>,
        model: CashAnnuityModel,
        settings: Shared<Settings<Date>>,
    ) -> BlackSwaptionEngine {
        let base = SwaptionEngine::new(SwaptionArguments::default(), InstrumentResults::default());
        discount_curve.register_observer(&base.observer());
        vol.register_observer(&base.observer());
        BlackSwaptionEngine {
            base,
            discount_curve,
            vol,
            model,
            settings,
        }
    }

    /// Builds the engine over a flat volatility quote, wrapping it in a moving
    /// [`ConstantSwaptionVolatility`] on a null calendar (the C++ `Handle<Quote>`
    /// constructor, `blackswaptionengine.hpp:144` / `:189`).
    pub fn with_flat_vol(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn Quote>,
        day_counter: DayCounter,
        displacement: Real,
        model: CashAnnuityModel,
        settings: Shared<Settings<Date>>,
    ) -> BlackSwaptionEngine {
        let surface = ConstantSwaptionVolatility::moving_with_quote(
            0,
            NullCalendar::new(),
            BusinessDayConvention::Following,
            vol,
            day_counter,
            VolatilityType::ShiftedLognormal,
            displacement,
            Shared::clone(&settings),
        );
        let vol = Handle::new(shared(surface) as Shared<dyn SwaptionVolatilityStructure>);
        BlackSwaptionEngine::new(discount_curve, vol, model, settings)
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for BlackSwaptionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BlackSwaptionEngine {
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
        // Read the swaption arguments, then release the borrow before pricing.
        let (exercise, swap, settlement_type, settlement_method) = {
            let arguments = self.base.arguments();
            let Some(exercise) = arguments.exercise.as_ref() else {
                fail!("exercise not set");
            };
            let Some(swap) = arguments.swap.as_ref() else {
                fail!("swap not set");
            };
            (
                Shared::clone(exercise),
                SharedMut::clone(swap),
                arguments.settlement_type,
                arguments.settlement_method,
            )
        };

        require!(
            exercise.exercise_type() == ExerciseType::European,
            "not a European option"
        );
        let exercise_date = exercise.dates()[0];

        let discount = self.discount_curve.current_link()?;
        let vol = self.vol.current_link()?;

        // Install a discounting engine on the swap and price it. The install is
        // silent so the swaption observing this swap is not invalidated
        // mid-calculation (the C++ disableUpdates() guard, expressed per D5).
        let discounting_engine = shared_mut(DiscountingSwapEngine::new(
            self.discount_curve.clone(),
            Some(false),
            None,
            None,
            Shared::clone(&self.settings),
        )) as SharedMut<dyn PricingEngine>;

        let (
            valuation_date,
            atm_forward,
            fixed_rate,
            swap_type,
            fixed_leg_bps,
            floating_leg_bps,
            spread,
            fixed_leg,
            first_accrual_start,
            first_day_count,
            fixed_frequency,
            floating_front,
            floating_back,
        ) = {
            let mut swap_ref = swap.borrow_mut();

            let (first_accrual_start, first_day_count) = {
                let leg = swap_ref.fixed_leg();
                let Some(first) = leg.first() else {
                    fail!("swap has no fixed leg");
                };
                let Some(coupon) = first.as_coupon() else {
                    fail!("first fixed cash flow is not a coupon");
                };
                (coupon.accrual_start_date(), coupon.day_counter())
            };
            require!(
                first_accrual_start >= exercise_date,
                "swap start ({first_accrual_start}) before exercise date ({exercise_date}) \
                 not supported in Black swaption engine"
            );

            let fixed_rate = swap_ref.fixed_rate();
            let spread = swap_ref.spread();
            let swap_type = swap_ref.swap_type();
            let fixed_leg: Leg = swap_ref.fixed_leg().to_vec();
            let fixed_frequency = if swap_ref.fixed_schedule().has_tenor() {
                swap_ref.fixed_schedule().tenor().frequency()
            } else {
                Frequency::Annual
            };
            let (floating_front, floating_back) = {
                let dates = swap_ref.floating_schedule().dates();
                (dates[0], dates[dates.len() - 1])
            };

            swap_ref
                .base_mut()
                .set_pricing_engine_silent(discounting_engine);
            let valuation_date = swap_ref.valuation_date()?;
            let atm_forward = swap_ref.fair_rate()?;
            let fixed_leg_bps = swap_ref.fixed_leg_bps()?;
            let floating_leg_bps = swap_ref.floating_leg_bps()?;

            (
                valuation_date,
                atm_forward,
                fixed_rate,
                swap_type,
                fixed_leg_bps,
                floating_leg_bps,
                spread,
                fixed_leg,
                first_accrual_start,
                first_day_count,
                fixed_frequency,
                floating_front,
                floating_back,
            )
        };

        // Volatilities are quoted for zero-spreaded swaps, so a floating-leg
        // spread is removed with a matching correction on the fixed rate
        // (`blackswaptionengine.hpp:253-265`).
        let mut strike = fixed_rate;
        let mut atm_forward = atm_forward;
        let spread_correction = if spread != 0.0 {
            let correction = spread * (floating_leg_bps / fixed_leg_bps).abs();
            strike -= correction;
            atm_forward -= correction;
            correction
        } else {
            0.0
        };

        let annuity = match (settlement_type, settlement_method) {
            (SettlementType::Physical, _)
            | (SettlementType::Cash, SettlementMethod::CollateralizedCashPrice) => {
                fixed_leg_bps.abs() / BASIS_POINT
            }
            (SettlementType::Cash, SettlementMethod::ParYieldCurve) => {
                // The cash settlement date is assumed equal to the swap start.
                let discount_date = match self.model {
                    CashAnnuityModel::DiscountCurve => first_accrual_start,
                    CashAnnuityModel::SwapRate => valuation_date,
                };
                let yield_rate = InterestRate::new(
                    atm_forward,
                    first_day_count,
                    Compounding::Compounded,
                    fixed_frequency,
                )?;
                // Positional C++ call: discountDate lands in the settlement-date
                // slot; the npv date defaults to it (`blackswaptionengine.hpp:288`).
                let fixed_leg_cash_bps = CashFlows::bps_at_yield(
                    &fixed_leg,
                    &yield_rate,
                    &self.settings,
                    Some(false),
                    Some(discount_date),
                    None,
                )?;
                (fixed_leg_cash_bps / BASIS_POINT).abs()
                    * discount.discount_date(discount_date, false)?
            }
            _ => fail!("invalid (settlementType, settlementMethod) pair"),
        };

        // swapLength is rounded to whole months, so it is floored at 1/12 to
        // ensure a variance and shift can be read (`blackswaptionengine.hpp:303-305`).
        let swap_length = vol
            .swap_length(floating_front, floating_back)?
            .max(1.0 / 12.0);
        let variance = vol.black_variance(exercise_date, swap_length, strike, false)?;
        let displacement = if vol.volatility_type() == VolatilityType::ShiftedLognormal {
            vol.shift(exercise_date, swap_length, false)?
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let option_type = if swap_type == SwapType::Payer {
            OptionType::Call
        } else {
            OptionType::Put
        };
        let value = black_formula(
            option_type,
            strike,
            atm_forward,
            std_dev,
            annuity,
            displacement,
        )?;

        let exercise_time = vol.time_from_reference(exercise_date)?;
        let vega = exercise_time.sqrt()
            * black_formula_std_dev_derivative(
                strike,
                atm_forward,
                std_dev,
                annuity,
                displacement,
            )?;
        let delta = black_formula_forward_derivative(
            option_type,
            strike,
            atm_forward,
            std_dev,
            annuity,
            displacement,
        )?;
        let implied_volatility = std_dev / exercise_time.sqrt();
        let forward_price = value / discount.discount_date(exercise_date, false)?;

        drop(vol);
        drop(discount);

        let results = self.base.results_mut();
        results.value = Some(value);
        results.error_estimate = None;
        results.valuation_date = Some(valuation_date);
        let extras = &mut results.additional_results;
        extras.insert(
            "spreadCorrection".into(),
            shared(spread_correction) as Shared<dyn Any>,
        );
        extras.insert("strike".into(), shared(strike) as Shared<dyn Any>);
        extras.insert("atmForward".into(), shared(atm_forward) as Shared<dyn Any>);
        extras.insert("annuity".into(), shared(annuity) as Shared<dyn Any>);
        extras.insert("swapLength".into(), shared(swap_length) as Shared<dyn Any>);
        extras.insert("stdDev".into(), shared(std_dev) as Shared<dyn Any>);
        extras.insert("vega".into(), shared(vega) as Shared<dyn Any>);
        extras.insert("delta".into(), shared(delta) as Shared<dyn Any>);
        extras.insert(
            "timeToExpiry".into(),
            shared(exercise_time) as Shared<dyn Any>,
        );
        extras.insert(
            "impliedVolatility".into(),
            shared(implied_volatility) as Shared<dyn Any>,
        );
        extras.insert(
            "forwardPrice".into(),
            shared(forward_price) as Shared<dyn Any>,
        );
        Ok(())
    }
}
