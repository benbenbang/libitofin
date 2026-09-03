//! Facades for the option instrument: OptionType and VanillaOption.

use crate::PyQlError;
use crate::heston::PyHestonModel;
use crate::market::PyBlackScholesProcess;
use crate::mcengine::{PyMCAmericanEngine, PyMCEuropeanEngine, PyMCEuropeanHestonEngine};
use crate::results::Results;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::exercise::{AmericanExercise, EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::types::Real;
use pyo3::prelude::*;
use pyo3::types::PyType;

/// The call/put flag.
///
/// A fieldless enum mirroring the core option type; the signed discriminant
/// convention behind the two variants stays in the core.
#[pyclass(name = "OptionType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyOptionType {
    Call,
    Put,
}

impl PyOptionType {
    /// The core OptionType this variant stands for.
    pub(crate) fn inner(self) -> OptionType {
        match self {
            PyOptionType::Call => OptionType::Call,
            PyOptionType::Put => OptionType::Put,
        }
    }
}

/// A single-asset vanilla option: European by construction, American through american().
///
/// Valuation is lazy: an accessor reprices only once an observed input - the
/// attached engine, or the evaluation date on the Settings the option
/// registered with - has notified it.
#[pyclass(name = "VanillaOption", unsendable)]
pub struct PyVanillaOption {
    inner: VanillaOption,
}

#[pymethods]
impl PyVanillaOption {
    /// Build the European-exercise option, exercisable only at expiry.
    ///
    /// Args:
    ///     option_type (OptionType): Whether the payoff is a call or a put.
    ///     strike (float): The strike of the plain vanilla payoff.
    ///     expiry (Date): The single date the option may be exercised on.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the option prices against.
    #[new]
    fn new(option_type: PyOptionType, strike: f64, expiry: &PyDate, settings: &PySettings) -> Self {
        let payoff = shared(PlainVanillaPayoff::new(option_type.inner(), strike))
            as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(expiry.inner())) as Shared<dyn Exercise>;
        PyVanillaOption {
            inner: VanillaOption::new(payoff, exercise, settings.inner()),
        }
    }

    /// Build the option exercisable at any time over [earliest, latest].
    ///
    /// The option pays on exercise rather than at expiry. This is the exercise
    /// the Monte Carlo American engine requires; the analytic European engine
    /// rejects it.
    ///
    /// Args:
    ///     option_type (OptionType): Whether the payoff is a call or a put.
    ///     strike (float): The strike of the plain vanilla payoff.
    ///     earliest (Date): The first date the option may be exercised on.
    ///     latest (Date): The last date the option may be exercised on.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the option prices against.
    ///
    /// Returns:
    ///     VanillaOption: The American-exercise option.
    ///
    /// Raises:
    ///     ItofinError: If earliest is after latest.
    #[classmethod]
    fn american(
        _cls: &Bound<'_, PyType>,
        option_type: PyOptionType,
        strike: f64,
        earliest: &PyDate,
        latest: &PyDate,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let payoff = shared(PlainVanillaPayoff::new(option_type.inner(), strike))
            as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(
            AmericanExercise::over(earliest.inner(), latest.inner()).map_err(PyQlError::from)?,
        ) as Shared<dyn Exercise>;
        Ok(PyVanillaOption {
            inner: VanillaOption::new(payoff, exercise, settings.inner()),
        })
    }

    /// Attach an analytic European engine built on process.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The process the engine prices on;
    ///         the exact object this Python instance holds is threaded in.
    fn set_engine(&mut self, process: &PyBlackScholesProcess) {
        let engine = shared_mut(AnalyticEuropeanEngine::new(process.inner()));
        self.inner
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    }

    /// Attach an analytic Heston engine built on model.
    ///
    /// The analytic Heston engine fills only the value, so npv() works but the
    /// greeks raise on this path.
    ///
    /// Args:
    ///     model (HestonModel): The calibrated Heston model to price under.
    ///     integration_order (int): The order of the Gauss-Laguerre
    ///         integration.
    ///
    /// Raises:
    ///     ItofinError: If integration_order exceeds 192.
    fn set_heston_engine(
        &mut self,
        model: &PyHestonModel,
        integration_order: usize,
    ) -> PyResult<()> {
        let engine =
            AnalyticHestonEngine::new(model.inner(), integration_order).map_err(PyQlError::from)?;
        self.inner
            .base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        Ok(())
    }

    /// Attach the Monte Carlo European engine.
    ///
    /// Args:
    ///     engine (MCEuropeanEngine): The engine, which already holds the
    ///         process it prices on.
    fn set_mc_engine(&mut self, engine: &PyMCEuropeanEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Attach the Monte Carlo Heston engine.
    ///
    /// Args:
    ///     engine (MCEuropeanHestonEngine): The engine, which already holds
    ///         the Heston process it prices on.
    fn set_mc_heston_engine(&mut self, engine: &PyMCEuropeanHestonEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Attach the Monte Carlo American engine.
    ///
    /// The option must have been built through american(): a European-exercise
    /// option raises ItofinError ("wrong exercise given") from npv().
    ///
    /// Args:
    ///     engine (MCAmericanEngine): The engine, which already holds the
    ///         process it prices on.
    fn set_mc_american_engine(&mut self, engine: &PyMCAmericanEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Force the valuation, so a later accessor reads a warm cache.
    ///
    /// Idempotent: the core short-circuits on a valid cache, and the option
    /// reprices only once an observed input notified it.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the attached engine refuses the option.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self.inner.calculate().map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache rather than
    ///         repricing.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }

    /// Attach an analytic European engine on process and return the NPV.
    ///
    /// The one-shot form of set_engine followed by npv. The other engines have
    /// their own one-shots: price_heston, price_mc, price_mc_heston and
    /// price_mc_american.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The process the engine prices on.
    ///
    /// Returns:
    ///     float: The present value under the analytic European engine.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set or the engine refuses the
    ///         option.
    fn price(&mut self, process: &PyBlackScholesProcess) -> PyResult<f64> {
        self.set_engine(process);
        self.calculate()?;
        self.npv()
    }

    /// Attach an analytic Heston engine on model and return the NPV.
    ///
    /// The one-shot form of set_heston_engine followed by npv. The greeks stay
    /// unavailable on this path.
    ///
    /// Args:
    ///     model (HestonModel): The calibrated Heston model to price under.
    ///     integration_order (int): The order of the Gauss-Laguerre
    ///         integration.
    ///
    /// Returns:
    ///     float: The present value under the analytic Heston engine.
    ///
    /// Raises:
    ///     ItofinError: If integration_order exceeds 192, no evaluation date is
    ///         set, or the engine refuses the option.
    fn price_heston(&mut self, model: &PyHestonModel, integration_order: usize) -> PyResult<f64> {
        self.set_heston_engine(model, integration_order)?;
        self.calculate()?;
        self.npv()
    }

    /// Attach the Monte Carlo European engine and return the NPV.
    ///
    /// The one-shot form of set_mc_engine followed by npv.
    ///
    /// Args:
    ///     engine (MCEuropeanEngine): The engine, which already holds the
    ///         process it prices on.
    ///
    /// Returns:
    ///     float: The present value under the Monte Carlo European engine.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set or the engine refuses the
    ///         option.
    fn price_mc(&mut self, engine: &PyMCEuropeanEngine) -> PyResult<f64> {
        self.set_mc_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Attach the Monte Carlo Heston engine and return the NPV.
    ///
    /// The one-shot form of set_mc_heston_engine followed by npv.
    ///
    /// Args:
    ///     engine (MCEuropeanHestonEngine): The engine, which already holds
    ///         the Heston process it prices on.
    ///
    /// Returns:
    ///     float: The present value under the Monte Carlo Heston engine.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set or the engine refuses the
    ///         option.
    fn price_mc_heston(&mut self, engine: &PyMCEuropeanHestonEngine) -> PyResult<f64> {
        self.set_mc_heston_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Attach the Monte Carlo American engine and return the NPV.
    ///
    /// The one-shot form of set_mc_american_engine followed by npv. The option
    /// must have been built through american(): a European-exercise option
    /// raises ItofinError ("wrong exercise given").
    ///
    /// Args:
    ///     engine (MCAmericanEngine): The engine, which already holds the
    ///         process it prices on.
    ///
    /// Returns:
    ///     float: The present value under the Monte Carlo American engine.
    ///
    /// Raises:
    ///     ItofinError: If the option is not American-exercise, no evaluation
    ///         date is set, or the engine refuses the option.
    fn price_mc_american(&mut self, engine: &PyMCAmericanEngine) -> PyResult<f64> {
        self.set_mc_american_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// The snapshot does not track the option: once taken, an evaluation-date
    /// or engine change reprices the live accessors and leaves it alone.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        Ok(Results::snapshot(self.inner.base()))
    }

    /// Return the present value.
    ///
    /// Returns:
    ///     float: The option value under the attached engine.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date or no engine is set.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }

    /// Return the option delta.
    ///
    /// Returns:
    ///     float: The sensitivity to the underlying spot.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it, which the
    ///         analytic Heston engine does not.
    fn delta(&mut self) -> PyResult<f64> {
        Ok(self.inner.delta().map_err(PyQlError::from)?)
    }

    /// Return the option gamma.
    ///
    /// Returns:
    ///     float: The second-order sensitivity to the underlying spot.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it.
    fn gamma(&mut self) -> PyResult<f64> {
        Ok(self.inner.gamma().map_err(PyQlError::from)?)
    }

    /// Return the option theta.
    ///
    /// Returns:
    ///     float: The sensitivity to the passage of time.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it.
    fn theta(&mut self) -> PyResult<f64> {
        Ok(self.inner.theta().map_err(PyQlError::from)?)
    }

    /// Return the option vega.
    ///
    /// Returns:
    ///     float: The sensitivity to the volatility.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it.
    fn vega(&mut self) -> PyResult<f64> {
        Ok(self.inner.vega().map_err(PyQlError::from)?)
    }

    /// Return the option rho.
    ///
    /// Returns:
    ///     float: The sensitivity to the risk-free rate.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it.
    fn rho(&mut self) -> PyResult<f64> {
        Ok(self.inner.rho().map_err(PyQlError::from)?)
    }

    /// Return the option dividend rho.
    ///
    /// Returns:
    ///     float: The sensitivity to the dividend yield.
    ///
    /// Raises:
    ///     ItofinError: If the attached engine does not provide it.
    fn dividend_rho(&mut self) -> PyResult<f64> {
        Ok(self.inner.dividend_rho().map_err(PyQlError::from)?)
    }

    /// Return the standard error on the present value.
    ///
    /// Returns:
    ///     float: The Monte Carlo standard error.
    ///
    /// Raises:
    ///     ItofinError: On the engines that do not produce one, which is every
    ///         analytic engine here.
    fn error_estimate(&mut self) -> PyResult<f64> {
        Ok(self.inner.error_estimate().map_err(PyQlError::from)?)
    }

    /// Return the fraction of simulated paths exercised before expiry.
    ///
    /// Returns:
    ///     float: The exercise probability reported by the engine.
    ///
    /// Raises:
    ///     ItofinError: On every engine that does not report it - only
    ///         MCAmericanEngine does.
    fn exercise_probability(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .result::<Real>("exerciseProbability")
            .map_err(PyQlError::from)?)
    }
}
