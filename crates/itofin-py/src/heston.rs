//! Facades for the Heston stack: [`PyHestonProcess`], [`PyHestonModel`] and
//! [`PyHestonModelHelper`].

use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::settings::PySettings;
use crate::time::{PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{HestonModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// Python `HestonProcess`: the square-root stochastic-variance process
/// (`processes::HestonProcess`).
///
/// The two flat yield curves and the spot quote are assembled behind their
/// `Handle`s internally so no `Handle` crosses the PyO3 boundary. The core
/// ctor takes `(risk_free_rate, dividend_yield, s0, ...)` in that order; the
/// two curves are bound by name and placed at the single call site.
#[pyclass(name = "HestonProcess", unsendable)]
pub struct PyHestonProcess {
    inner: Shared<HestonProcess>,
}

#[pymethods]
impl PyHestonProcess {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        risk_free_rate: f64,
        dividend_yield: f64,
        spot: f64,
        v0: f64,
        kappa: f64,
        theta: f64,
        sigma: f64,
        rho: f64,
        reference_date: &PyDate,
        day_counter: &PyDayCounter,
    ) -> Self {
        let ref_date = reference_date.inner();
        let dc = day_counter.inner();

        let risk_free_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            risk_free_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let dividend_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            dividend_yield,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let s0 = Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>);

        PyHestonProcess {
            inner: shared(HestonProcess::new(
                risk_free_curve,
                dividend_curve,
                s0,
                v0,
                kappa,
                theta,
                sigma,
                rho,
            )),
        }
    }

    /// The initial variance `v0`.
    fn v0(&self) -> f64 {
        self.inner.v0()
    }

    /// The mean-reversion speed `kappa`.
    fn kappa(&self) -> f64 {
        self.inner.kappa()
    }

    /// The long-run variance `theta`.
    fn theta(&self) -> f64 {
        self.inner.theta()
    }

    /// The volatility of variance `sigma`.
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }

    /// The spot/variance correlation `rho`.
    fn rho(&self) -> f64 {
        self.inner.rho()
    }
}

impl PyHestonProcess {
    /// A clone of the inner process for the model ctor.
    pub(crate) fn inner(&self) -> Shared<HestonProcess> {
        Shared::clone(&self.inner)
    }
}

/// Python `HestonModel`: the five-parameter calibrated Heston model
/// (`models::HestonModel`).
///
/// The ctor is fallible: it seeds its arguments from the process parameters
/// under their constraints (`theta`, `kappa`, `sigma`, `v0` strictly positive,
/// `rho` in `[-1, 1]`), so a violating parameter surfaces as an `ItofinError`.
#[pyclass(name = "HestonModel", unsendable)]
pub struct PyHestonModel {
    inner: SharedMut<HestonModel>,
}

#[pymethods]
impl PyHestonModel {
    #[new]
    fn new(process: &PyHestonProcess) -> PyResult<Self> {
        let inner = HestonModel::new(process.inner()).map_err(PyQlError::from)?;
        Ok(PyHestonModel { inner })
    }

    /// The long-run variance `theta`.
    fn theta(&self) -> f64 {
        self.inner.borrow().theta()
    }

    /// The mean-reversion speed `kappa`.
    fn kappa(&self) -> f64 {
        self.inner.borrow().kappa()
    }

    /// The volatility of variance `sigma`.
    fn sigma(&self) -> f64 {
        self.inner.borrow().sigma()
    }

    /// The spot/variance correlation `rho`.
    fn rho(&self) -> f64 {
        self.inner.borrow().rho()
    }

    /// The initial variance `v0`.
    fn v0(&self) -> f64 {
        self.inner.borrow().v0()
    }

    /// Calibrates the model to `helpers` with `method` under `end_criteria`,
    /// then writes the fitted parameters back (readable through the getters).
    ///
    /// Mirrors the core oracle (`hestonmodelhelper.rs:556-581`): one
    /// [`AnalyticHestonEngine`] of `integration_order` is built on this model and
    /// installed on every helper, so all helpers price through the same engine
    /// the optimizer drives. The engine ctor is fallible (order > 192 errors);
    /// [`calibrate`](libitofin::models::calibrate) fails on an empty helper list.
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PyHestonModelHelper>>,
        method: &mut PyLevenbergMarquardt,
        end_criteria: &PyEndCriteria,
        integration_order: usize,
    ) -> PyResult<()> {
        let engine = shared_mut(
            AnalyticHestonEngine::new(SharedMut::clone(&self.inner), integration_order)
                .map_err(PyQlError::from)?,
        ) as SharedMut<dyn PricingEngine>;
        for helper in &helpers {
            helper
                .inner
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(SharedMut::clone(&engine));
        }
        let dyn_helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
            .iter()
            .map(|helper| SharedMut::clone(&helper.inner) as SharedMut<dyn CalibrationHelper>)
            .collect();
        calibrate(
            &self.inner,
            &dyn_helpers,
            method.inner_mut(),
            end_criteria.inner(),
            None,
            Vec::new(),
            Vec::new(),
        )
        .map_err(PyQlError::from)?;
        Ok(())
    }
}

impl PyHestonModel {
    /// A clone of the inner model handle for the engine facade (H2 also calibrates).
    pub(crate) fn inner(&self) -> SharedMut<HestonModel> {
        SharedMut::clone(&self.inner)
    }
}

/// Python `HestonModelHelper`: a Black-vol calibration helper over a flat-vol
/// surface (`models::equity::HestonModelHelper`).
///
/// The core ctor takes the spot as a bare `Real`, the volatility as a
/// `Handle<dyn Quote>` and the two curves as `Handle<dyn YieldTermStructure>`.
/// This facade takes scalar market inputs plus a `reference_date`/`day_counter`
/// used only to assemble the vol quote handle and the two flat `FlatForward`
/// curves inline (the same `Continuous`/`Annual` convention the other facades
/// use); those two arguments are not forwarded to the core ctor. The helper is
/// held as `SharedMut` so a calibration can install a pricing engine on it and
/// upcast it to `SharedMut<dyn CalibrationHelper>`.
#[pyclass(name = "HestonModelHelper", unsendable)]
pub struct PyHestonModelHelper {
    inner: SharedMut<HestonModelHelper>,
}

#[pymethods]
impl PyHestonModelHelper {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        maturity: &PyPeriod,
        calendar: &PyCalendar,
        s0: f64,
        strike: f64,
        volatility: f64,
        risk_free_rate: f64,
        dividend_yield: f64,
        error_type: &PyCalibrationErrorType,
        reference_date: &PyDate,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> Self {
        let ref_date = reference_date.inner();
        let dc = day_counter.inner();

        let vol = Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
        let risk_free_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            risk_free_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let dividend_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            dividend_yield,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        PyHestonModelHelper {
            inner: shared_mut(HestonModelHelper::new(
                maturity.inner(),
                calendar.inner(),
                s0,
                strike,
                vol,
                risk_free_curve,
                dividend_curve,
                error_type.inner(),
                settings.inner(),
            )),
        }
    }

    /// The calibration error under the helper's error type, after a calibration
    /// has installed a pricing engine on it.
    fn calibration_error(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .calibration_error()
            .map_err(PyQlError::from)?)
    }
}
