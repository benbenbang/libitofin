//! Facades for the option instrument: [`PyOptionType`] and [`PyVanillaOption`].

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

/// Python `OptionType`: the call/put flag (core `option::OptionType`).
///
/// A fieldless pyo3 enum exposing `OptionType.Call` / `OptionType.Put`; the
/// signed discriminant convention lives in the core, so the facade only maps
/// the variant across.
#[pyclass(name = "OptionType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyOptionType {
    Call,
    Put,
}

impl PyOptionType {
    /// The core [`OptionType`] this variant stands for.
    pub(crate) fn inner(self) -> OptionType {
        match self {
            PyOptionType::Call => OptionType::Call,
            PyOptionType::Put => OptionType::Put,
        }
    }
}

/// Python `VanillaOption`: a single-asset vanilla option (core
/// `instruments::VanillaOption`, an alias of `OneAssetOption`).
///
/// The constructor builds the European-exercise option; the `american`
/// classmethod builds the American-exercise one.
///
/// Holds the option by value so the lazily-computed results can be produced
/// through `&mut self` accessors; the inner instrument is `Rc`/`RefCell`-based
/// and therefore `!Send`, hence `unsendable`.
#[pyclass(name = "VanillaOption", unsendable)]
pub struct PyVanillaOption {
    inner: VanillaOption,
}

#[pymethods]
impl PyVanillaOption {
    #[new]
    fn new(option_type: PyOptionType, strike: f64, expiry: &PyDate, settings: &PySettings) -> Self {
        let payoff = shared(PlainVanillaPayoff::new(option_type.inner(), strike))
            as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(expiry.inner())) as Shared<dyn Exercise>;
        PyVanillaOption {
            inner: VanillaOption::new(payoff, exercise, settings.inner()),
        }
    }

    /// The same option struck at `strike` but exercisable at any time over
    /// `[earliest, latest]`, paying on exercise rather than at expiry.
    ///
    /// This is the exercise the Monte Carlo American engine requires; the
    /// analytic European engine rejects it.
    ///
    /// Raises `ItofinError` when `earliest` is after `latest`.
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

    /// Attaches an analytic European engine built on `process`, threading in
    /// the exact same Black-Scholes process the Python object holds.
    fn set_engine(&mut self, process: &PyBlackScholesProcess) {
        let engine = shared_mut(AnalyticEuropeanEngine::new(process.inner()));
        self.inner
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    }

    /// Attaches an analytic Heston engine built on `model` with a Gauss-Laguerre
    /// integration of `integration_order` (fallible: order > 192 errors).
    ///
    /// The analytic Heston engine fills only `results.value`, so `npv()` works
    /// but the greeks (`delta()`, `gamma()`, ...) raise `ItofinError` ("not
    /// provided") on this path.
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

    /// Attaches the Monte Carlo European engine `engine`, which already holds
    /// the process it prices on.
    fn set_mc_engine(&mut self, engine: &PyMCEuropeanEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Attaches the Monte Carlo Heston engine `engine`, which already holds the
    /// Heston process it prices on.
    fn set_mc_heston_engine(&mut self, engine: &PyMCEuropeanHestonEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Attaches the Monte Carlo American engine `engine`, which already holds
    /// the process it prices on.
    ///
    /// The option must have been built through `american`: pricing a
    /// European-exercise option on this engine raises `ItofinError` ("wrong
    /// exercise given") from `npv()`.
    fn set_mc_american_engine(&mut self, engine: &PyMCAmericanEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Forces the valuation, so a later accessor reads a cache that is already
    /// warm.
    ///
    /// Idempotent: the core short-circuits on a valid cache, and the option
    /// only reprices once an observed input - the engine, or the settings
    /// evaluation date it registered with - has notified it.
    ///
    /// Fallible for everything a pricing accessor is: no engine attached, no
    /// evaluation date set, an engine that refuses the option.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self.inner.calculate().map_err(PyQlError::from)?)
    }

    /// Whether the cached results are currently valid, that is, whether the
    /// next accessor reads the cache or reprices.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }

    /// Attaches an analytic European engine on `process` and returns the NPV,
    /// the one-shot form of [`set_engine`](Self::set_engine) followed by
    /// [`npv`](Self::npv).
    ///
    /// The other engines have their own one-shots:
    /// [`price_heston`](Self::price_heston), [`price_mc`](Self::price_mc),
    /// [`price_mc_heston`](Self::price_mc_heston) and
    /// [`price_mc_american`](Self::price_mc_american).
    fn price(&mut self, process: &PyBlackScholesProcess) -> PyResult<f64> {
        self.set_engine(process);
        self.calculate()?;
        self.npv()
    }

    /// Attaches an analytic Heston engine on `model` at `integration_order` and
    /// returns the NPV, the one-shot form of
    /// [`set_heston_engine`](Self::set_heston_engine) followed by
    /// [`npv`](Self::npv).
    ///
    /// Fallible where the setter is: an `integration_order` above 192 raises
    /// `ItofinError` before any engine is attached. The greeks stay unavailable
    /// on this path.
    fn price_heston(&mut self, model: &PyHestonModel, integration_order: usize) -> PyResult<f64> {
        self.set_heston_engine(model, integration_order)?;
        self.calculate()?;
        self.npv()
    }

    /// Attaches the Monte Carlo European engine `engine` and returns the NPV,
    /// the one-shot form of [`set_mc_engine`](Self::set_mc_engine) followed by
    /// [`npv`](Self::npv).
    fn price_mc(&mut self, engine: &PyMCEuropeanEngine) -> PyResult<f64> {
        self.set_mc_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Attaches the Monte Carlo Heston engine `engine` and returns the NPV, the
    /// one-shot form of
    /// [`set_mc_heston_engine`](Self::set_mc_heston_engine) followed by
    /// [`npv`](Self::npv).
    fn price_mc_heston(&mut self, engine: &PyMCEuropeanHestonEngine) -> PyResult<f64> {
        self.set_mc_heston_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Attaches the Monte Carlo American engine `engine` and returns the NPV,
    /// the one-shot form of
    /// [`set_mc_american_engine`](Self::set_mc_american_engine) followed by
    /// [`npv`](Self::npv).
    ///
    /// The option must have been built through `american`: a European-exercise
    /// option raises `ItofinError` ("wrong exercise given").
    fn price_mc_american(&mut self, engine: &PyMCAmericanEngine) -> PyResult<f64> {
        self.set_mc_american_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// A frozen [`Results`] copy of the valuation, calculating first.
    ///
    /// The snapshot does not track the option: once taken, an evaluation-date
    /// or engine change reprices the live accessors and leaves it alone.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        Ok(Results::snapshot(self.inner.base()))
    }

    /// The present value, erroring when no evaluation date or engine is set.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }

    /// The option delta.
    fn delta(&mut self) -> PyResult<f64> {
        Ok(self.inner.delta().map_err(PyQlError::from)?)
    }

    /// The option gamma.
    fn gamma(&mut self) -> PyResult<f64> {
        Ok(self.inner.gamma().map_err(PyQlError::from)?)
    }

    /// The option theta.
    fn theta(&mut self) -> PyResult<f64> {
        Ok(self.inner.theta().map_err(PyQlError::from)?)
    }

    /// The option vega.
    fn vega(&mut self) -> PyResult<f64> {
        Ok(self.inner.vega().map_err(PyQlError::from)?)
    }

    /// The option rho.
    fn rho(&mut self) -> PyResult<f64> {
        Ok(self.inner.rho().map_err(PyQlError::from)?)
    }

    /// The option dividend rho.
    fn dividend_rho(&mut self) -> PyResult<f64> {
        Ok(self.inner.dividend_rho().map_err(PyQlError::from)?)
    }

    /// The standard error on the present value, raising `ItofinError` on the
    /// engines that do not produce one (every analytic engine here).
    fn error_estimate(&mut self) -> PyResult<f64> {
        Ok(self.inner.error_estimate().map_err(PyQlError::from)?)
    }

    /// The fraction of simulated paths exercised before expiry, raising
    /// `ItofinError` on every engine that does not report it - only the Monte
    /// Carlo American engine does.
    fn exercise_probability(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .result::<Real>("exerciseProbability")
            .map_err(PyQlError::from)?)
    }
}
