//! Facades for the option instrument: [`PyOptionType`] and [`PyVanillaOption`].

use crate::PyQlError;
use crate::heston::PyHestonModel;
use crate::market::PyBlackScholesProcess;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::exercise::{EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;

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

/// Python `VanillaOption`: a single-asset European option (core
/// `instruments::VanillaOption`, an alias of `OneAssetOption`).
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
}
