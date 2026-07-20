//! Heston-model calibration helper.
//!
//! Port of `ql/models/equity/hestonmodelhelper.{hpp,cpp}`.
//! [`HestonModelHelper`] is a
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper):
//! it prices a European vanilla option's market value from a quoted Black
//! volatility over a flat-vol surface, and its model value through the model
//! pricing engine a calibration installs (an
//! [`AnalyticHestonEngine`](crate::pricingengines::AnalyticHestonEngine)).
//!
//! ## Ported surface
//!
//! Both constructors (`hestonmodelhelper.cpp:33-62`): [`new`](HestonModelHelper::new)
//! takes the spot as a bare [`Real`] and wraps it in a constant
//! [`SimpleQuote`] (`cpp:42`); [`with_spot_handle`](HestonModelHelper::with_spot_handle)
//! takes a [`Handle<Quote>`](Handle) spot directly (`cpp:48-62`).
//!
//! ## `addTimesTo`
//!
//! `addTimesTo` (`hestonmodelhelper.hpp:56`) is literally empty `{}` in C++ - the
//! helper has no tree/lattice pricing path. It is already omitted from the
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//! trait surface (`calibrationhelper.rs` deferral of `addTimesTo`), so there is
//! nothing to implement: the empty C++ body maps to no Rust method at all.
//!
//! ## Divergences from QuantLib
//!
//! - **`Settings` is an explicit constructor argument.** C++ reads the global
//!   `Settings::instance()` when the built [`VanillaOption`] checks expiry; per
//!   D5 the core has no global, so the settings are passed in and reused for the
//!   option. The Heston engine takes its maturity time from the process curves
//!   (not the settings), so the settings only gate the option's expiry check;
//!   pass the same [`Settings`] the curves and engine are anchored to.
//! - **`model_value` / `black_price` are `&self` and recompute their derived
//!   state.** C++'s are const and lean on a `LazyObject::calculate()` that caches
//!   `mutable exerciseDate_ / tau_ / type_ / option_`. This port's
//!   [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//!   fixes both as `&self`, so [`derive`](HestonModelHelper::derive) recomputes
//!   the exercise date, `tau`, the discounted strike/spot and the option type on
//!   each call (a few flat-curve discount lookups) and
//!   [`model_value`](HestonModelHelper::model_value) builds a fresh
//!   [`VanillaOption`]. `derive` is pure in the helper's inputs, so this is
//!   observationally identical to the C++ cache; observers are held weakly, so a
//!   fresh option per call does not leak.
//! - **A missing model engine is an explicit `Err`.** C++ `modelValue`
//!   dereferences a null `engine_`; the port returns an error (D4).
//! - **`black_price` uses the 6-arg [`black_formula`].** C++'s 4-arg overload
//!   (`cpp:89-91`) has discount `1.0` and displacement `0.0`; the strike and
//!   forward passed are already discounted, so this port passes `discount = 1.0`
//!   and `displacement = 0.0` (passing a curve discount here would double-count).

use crate::errors::QlResult;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::fail;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use crate::models::calibrationhelper::{
    BlackCalibrationHelper, BlackCalibrationHelperBase, CalibrationErrorType,
};
use crate::option::OptionType;
use crate::pricingengines::blackformula::black_formula;
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Real, Time};

/// The state derived from the term structures on each valuation
/// (`performCalculations`, `hestonmodelhelper.cpp:64-78`).
struct Derived {
    exercise_date: Date,
    tau: Time,
    option_type: OptionType,
    discounted_strike: Real,
    discounted_spot: Real,
}

/// Calibration helper for the Heston model (`hestonmodelhelper.hpp:32`).
pub struct HestonModelHelper {
    base: BlackCalibrationHelperBase,
    maturity: Period,
    calendar: Calendar,
    s0: Handle<dyn Quote>,
    strike_price: Real,
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
}

impl HestonModelHelper {
    /// Builds a helper from a bare spot (`hestonmodelhelper.cpp:33-45`): the spot
    /// is wrapped in a constant [`SimpleQuote`] and the helper delegates to
    /// [`with_spot_handle`](Self::with_spot_handle). C++ does not register with
    /// the constant spot; this port does (`with_spot_handle` registers all three
    /// handles), a no-op since a [`SimpleQuote`] built here never changes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        maturity: Period,
        calendar: Calendar,
        s0: Real,
        strike_price: Real,
        volatility: Handle<dyn Quote>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        settings: Shared<Settings<Date>>,
    ) -> HestonModelHelper {
        let s0: Handle<dyn Quote> = Handle::new(shared(SimpleQuote::new(s0)) as Shared<dyn Quote>);
        HestonModelHelper::with_spot_handle(
            maturity,
            calendar,
            s0,
            strike_price,
            volatility,
            risk_free_rate,
            dividend_yield,
            error_type,
            settings,
        )
    }

    /// Builds a helper from a quoted spot (`hestonmodelhelper.cpp:47-62`).
    ///
    /// Registers the base's observer with the spot, risk-free and dividend
    /// handles (C++ `registerWith(s0)`/`registerWith(riskFreeRate)`/
    /// `registerWith(dividendYield)`, `:58-60`), so a change to any invalidates
    /// the cached market value alongside the volatility handle the base
    /// registers. The base uses the default `ShiftedLognormal` volatility type
    /// and zero shift of the C++ 2-arg base constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn with_spot_handle(
        maturity: Period,
        calendar: Calendar,
        s0: Handle<dyn Quote>,
        strike_price: Real,
        volatility: Handle<dyn Quote>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        settings: Shared<Settings<Date>>,
    ) -> HestonModelHelper {
        let base = BlackCalibrationHelperBase::new(
            volatility,
            error_type,
            VolatilityType::ShiftedLognormal,
            0.0,
        );

        let observer = base.observer();
        s0.register_observer(&observer);
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);

        HestonModelHelper {
            base,
            maturity,
            calendar,
            s0,
            strike_price,
            risk_free_rate,
            dividend_yield,
            settings,
        }
    }

    /// The year fraction to the exercise date (`maturity()`,
    /// `hestonmodelhelper.hpp:60`): the `tau` of the current derived state.
    ///
    /// # Errors
    ///
    /// Propagates a failure of [`derive`](Self::derive) (an empty curve handle).
    pub fn maturity(&self) -> QlResult<Time> {
        Ok(self.derive()?.tau)
    }

    /// `performCalculations`'s derived state (`hestonmodelhelper.cpp:64-72`):
    /// advances the risk-free reference date by the maturity, times it, and
    /// selects the option type from forward moneyness. The type is
    /// [`Call`](OptionType::Call) when the discounted strike is at least the
    /// discounted spot, else [`Put`](OptionType::Put) (`cpp:68-71`).
    fn derive(&self) -> QlResult<Derived> {
        let risk_free = self.risk_free_rate.current_link()?;
        let reference_date = risk_free.reference_date()?;
        let exercise_date = self.calendar.advance_by_period(
            reference_date,
            self.maturity,
            BusinessDayConvention::Following,
            false,
        );
        let tau = risk_free.time_from_reference(exercise_date)?;

        let discounted_strike = self.strike_price * risk_free.discount(tau, false)?;
        let dividend = self.dividend_yield.current_link()?;
        let spot = self.s0.current_link()?.value()?;
        let discounted_spot = spot * dividend.discount(tau, false)?;

        let option_type = if discounted_strike >= discounted_spot {
            OptionType::Call
        } else {
            OptionType::Put
        };

        Ok(Derived {
            exercise_date,
            tau,
            option_type,
            discounted_strike,
            discounted_spot,
        })
    }

    /// Builds the vanilla option `performCalculations` assembles
    /// (`hestonmodelhelper.cpp:73-77`): a [`PlainVanillaPayoff`] of the derived
    /// type struck at `strikePrice_` over a [`EuropeanExercise`] on the exercise
    /// date.
    fn build_option(&self, derived: &Derived) -> SharedMut<VanillaOption> {
        let payoff = shared(PlainVanillaPayoff::new(
            derived.option_type,
            self.strike_price,
        )) as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(derived.exercise_date)) as Shared<dyn Exercise>;
        shared_mut(VanillaOption::new(
            payoff,
            exercise,
            Shared::clone(&self.settings),
        ))
    }
}

impl BlackCalibrationHelper for HestonModelHelper {
    fn base(&self) -> &BlackCalibrationHelperBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase {
        &mut self.base
    }

    /// `modelValue` (`hestonmodelhelper.cpp:80-84`): installs the model engine on
    /// the option and returns its NPV.
    fn model_value(&self) -> QlResult<Real> {
        let derived = self.derive()?;
        let option = self.build_option(&derived);
        let Some(engine) = self.base.pricing_engine() else {
            fail!("no model pricing engine set on the heston model helper");
        };
        option
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(engine));
        let value = option.borrow_mut().npv()?;
        Ok(value)
    }

    /// `blackPrice` (`hestonmodelhelper.cpp:86-92`): the Black 1976 value at the
    /// given volatility, with the strike and forward already discounted, so the
    /// discount and displacement of the 6-arg formula are `1.0` and `0.0`.
    fn black_price(&self, volatility: Real) -> QlResult<Real> {
        let derived = self.derive()?;
        let std_dev = volatility * derived.tau.sqrt();
        black_formula(
            derived.option_type,
            derived.discounted_strike,
            derived.discounted_spot,
            std_dev,
            1.0,
            0.0,
        )
    }
}
