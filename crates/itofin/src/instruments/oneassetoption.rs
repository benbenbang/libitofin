//! Options on a single asset.
//!
//! Port of the vanilla-option slice of `ql/option.hpp`,
//! `ql/instruments/oneassetoption.{hpp,cpp}`, `ql/instruments/vanillaoption.hpp`
//! and `ql/instruments/europeanoption.hpp`: [`OptionArguments`] (the C++
//! `Option::arguments`), the [`Greeks`] and [`MoreGreeks`] result mix-ins,
//! [`OneAssetOptionResults`] and the [`OneAssetOption`] instrument with its
//! greek accessors.
//!
//! Deviations, all by existing design decisions:
//! - C++ stores the payoff as the base `Payoff` and engines `dynamic_cast` it
//!   to `StrikedTypePayoff`; trait objects do not cross-cast, so the arguments
//!   carry [`StrikedTypePayoff`] directly - every consumer in scope (the
//!   analytic engine, the greeks) needs the strike.
//! - `VanillaOption` and `EuropeanOption` add only constructor sugar on top of
//!   `OneAssetOption` until `impliedVolatility` (solver-backed, follow-up) and
//!   early-exercise instruments arrive, so both are aliases of it here.
//! - `isExpired` reads the evaluation date from the `Settings` handle the
//!   option is constructed with (per D5 there is no singleton). QuantLib's
//!   singleton always falls back to today's clock date, which the core lacks:
//!   with no evaluation date set (or the settings locked mid-notification)
//!   the option is treated as not expired and downstream pricing decides.
//!   The general `Event`/`simple_event` machinery of `ql/event.hpp` is
//!   follow-up work; its `hasOccurred` date comparison is ported inline.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::Exercise;
use crate::fail;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::instruments::StrikedTypePayoff;
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::types::Real;

/// Basic option arguments (the C++ `Option::arguments`).
#[derive(Default)]
pub struct OptionArguments {
    /// The payoff the option is written on.
    pub payoff: Option<Shared<dyn StrikedTypePayoff>>,
    /// The exercise schedule.
    pub exercise: Option<Shared<dyn Exercise>>,
}

impl Arguments for OptionArguments {
    fn validate(&self) -> QlResult<()> {
        if self.payoff.is_none() {
            fail!("no payoff given");
        }
        if self.exercise.is_none() {
            fail!("no exercise given");
        }
        Ok(())
    }
}

/// Additional option results (the C++ `Greeks` mix-in).
#[derive(Clone, Copy, Debug, Default)]
pub struct Greeks {
    pub delta: Option<Real>,
    pub gamma: Option<Real>,
    pub theta: Option<Real>,
    pub vega: Option<Real>,
    pub rho: Option<Real>,
    pub dividend_rho: Option<Real>,
}

impl Greeks {
    /// Clears all greeks (the C++ `reset`).
    pub fn reset(&mut self) {
        *self = Greeks::default();
    }
}

/// More additional option results (the C++ `MoreGreeks` mix-in).
#[derive(Clone, Copy, Debug, Default)]
pub struct MoreGreeks {
    pub itm_cash_probability: Option<Real>,
    pub delta_forward: Option<Real>,
    pub elasticity: Option<Real>,
    pub theta_per_day: Option<Real>,
    pub strike_sensitivity: Option<Real>,
}

impl MoreGreeks {
    /// Clears all greeks (the C++ `reset`).
    pub fn reset(&mut self) {
        *self = MoreGreeks::default();
    }
}

/// Results from a single-asset option calculation (the C++
/// `OneAssetOption::results`: instrument results plus both greek mix-ins).
#[derive(Default)]
pub struct OneAssetOptionResults {
    pub instrument: InstrumentResults,
    pub greeks: Greeks,
    pub more_greeks: MoreGreeks,
}

impl Results for OneAssetOptionResults {
    fn reset(&mut self) {
        self.instrument.reset();
        self.greeks.reset();
        self.more_greeks.reset();
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        Some(&self.instrument)
    }
}

/// Engine base for single-asset options (the C++ `OneAssetOption::engine`).
pub type OneAssetOptionEngine = GenericEngine<OptionArguments, OneAssetOptionResults>;

/// Option on a single asset.
///
/// Holds a payoff and an exercise schedule, prices through the attached
/// engine and exposes the fetched greeks. The greek accessors trigger the
/// lazy calculation and, as in C++, fail when the engine did not provide the
/// requested value.
pub struct OneAssetOption {
    base: InstrumentBase,
    payoff: Shared<dyn StrikedTypePayoff>,
    exercise: Shared<dyn Exercise>,
    settings: SharedMut<Settings<Date>>,
    greeks: Greeks,
    more_greeks: MoreGreeks,
}

/// Vanilla option (no discrete dividends, no barriers) on a single asset.
///
/// `impliedVolatility` is follow-up work (it needs the solver layer and a
/// Black-Scholes process); until it lands the C++ `VanillaOption` adds
/// nothing to `OneAssetOption`.
pub type VanillaOption = OneAssetOption;

/// European option on a single asset.
pub type EuropeanOption = VanillaOption;

impl OneAssetOption {
    /// Builds the option from its payoff and exercise schedule.
    ///
    /// The C++ `Instrument` constructor registers with the `Settings`
    /// evaluation date; per D5 the settings are passed explicitly and the
    /// registration happens here, so an evaluation-date change invalidates
    /// the cached results.
    pub fn new(
        payoff: Shared<dyn StrikedTypePayoff>,
        exercise: Shared<dyn Exercise>,
        settings: SharedMut<Settings<Date>>,
    ) -> QlResult<OneAssetOption> {
        let base = InstrumentBase::new();
        {
            let Ok(guard) = settings.try_borrow() else {
                fail!("evaluation-date settings are locked during notification");
            };
            guard.register_eval_date_observer(&base.observer());
        }
        Ok(OneAssetOption {
            base,
            payoff,
            exercise,
            settings,
            greeks: Greeks::default(),
            more_greeks: MoreGreeks::default(),
        })
    }

    /// The payoff the option is written on.
    pub fn payoff(&self) -> &Shared<dyn StrikedTypePayoff> {
        &self.payoff
    }

    /// The exercise schedule.
    pub fn exercise(&self) -> &Shared<dyn Exercise> {
        &self.exercise
    }

    fn greek(value: Option<Real>, description: &str) -> QlResult<Real> {
        let Some(value) = value else {
            fail!("{description} not provided");
        };
        Ok(value)
    }

    /// The option delta.
    pub fn delta(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.delta, "delta")
    }

    /// The option forward delta.
    pub fn delta_forward(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.delta_forward, "forward delta")
    }

    /// The option elasticity.
    pub fn elasticity(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.elasticity, "elasticity")
    }

    /// The option gamma.
    pub fn gamma(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.gamma, "gamma")
    }

    /// The option theta.
    pub fn theta(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.theta, "theta")
    }

    /// The option per-day theta.
    pub fn theta_per_day(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.theta_per_day, "theta per-day")
    }

    /// The option vega.
    pub fn vega(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.vega, "vega")
    }

    /// The option rho.
    pub fn rho(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.rho, "rho")
    }

    /// The option dividend rho.
    pub fn dividend_rho(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.dividend_rho, "dividend rho")
    }

    /// The sensitivity of the option value to the strike.
    pub fn strike_sensitivity(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.strike_sensitivity, "strike sensitivity")
    }

    /// The probability of the option expiring in the money.
    pub fn itm_cash_probability(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(
            self.more_greeks.itm_cash_probability,
            "in-the-money cash probability",
        )
    }
}

impl Instrument for OneAssetOption {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    /// Whether the last exercise date has occurred relative to the evaluation
    /// date (`detail::simple_event(exercise_->lastDate()).hasOccurred()`).
    fn is_expired(&self) -> bool {
        let Ok(settings) = self.settings.try_borrow() else {
            return false;
        };
        let Some(evaluation_date) = settings.evaluation_date().copied() else {
            return false;
        };
        let last_date = self.exercise.last_date();
        if settings.include_reference_date_events() {
            last_date < evaluation_date
        } else {
            last_date <= evaluation_date
        }
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<OptionArguments>() else {
            fail!("wrong argument type");
        };
        arguments.payoff = Some(Shared::clone(&self.payoff));
        arguments.exercise = Some(Shared::clone(&self.exercise));
        Ok(())
    }

    fn setup_expired(&mut self) {
        let expired = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&expired);
        self.greeks = Greeks {
            delta: Some(0.0),
            gamma: Some(0.0),
            theta: Some(0.0),
            vega: Some(0.0),
            rho: Some(0.0),
            dividend_rho: Some(0.0),
        };
        self.more_greeks = MoreGreeks {
            itm_cash_probability: Some(0.0),
            delta_forward: Some(0.0),
            elasticity: Some(0.0),
            theta_per_day: Some(0.0),
            strike_sensitivity: Some(0.0),
        };
    }

    /// Reads the greeks back alongside the instrument-level results.
    ///
    /// As in C++, the values are copied without null checks: what to do about
    /// missing greeks is decided by the accessors (throw) or derived options.
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<OneAssetOptionResults>() else {
            fail!("no greeks returned from pricing engine");
        };
        self.greeks = results.greeks;
        self.more_greeks = results.more_greeks;
        self.base_mut().store_results(&results.instrument);
        Ok(())
    }
}
