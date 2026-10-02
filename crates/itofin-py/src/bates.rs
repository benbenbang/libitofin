//! Constant-intensity Bates market, calibration model and European engine.

use crate::PyQlError;
use crate::calibration::{PyEndCriteria, calibration_options, with_method};
use crate::curve::PyYieldTermStructure;
use crate::heston::PyHestonModelHelper;
use crate::market::PySimpleQuote;
use crate::time::PyDate;
use libitofin::math::array::Array;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::model::CalibratedModelHolder;
use libitofin::models::{BatesModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::BatesEngine;
use libitofin::processes::BatesProcess;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;
use pyo3::types::PyAny;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Heston variance with independent constant-intensity lognormal jumps.
///
/// Retains live spot and yield curves for analytic pricing, not path generation.
#[gen_stub_pyclass]
#[pyclass(name = "BatesProcess", unsendable, module = "itofin.processes")]
pub struct PyBatesProcess {
    inner: Shared<BatesProcess>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBatesProcess {
    /// Retain the market and validate eight finite Bates parameters.
    ///
    /// Variance, mean reversion and variance volatility must be positive;
    /// correlation is in [-1, 1], and lambda_ and delta are nonnegative.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        spot: &PySimpleQuote,
        risk_free: &PyYieldTermStructure,
        dividend: &PyYieldTermStructure,
        v0: f64,
        kappa: f64,
        theta: f64,
        sigma: f64,
        rho: f64,
        lambda_: f64,
        nu: f64,
        delta: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared(
                BatesProcess::new(
                    risk_free.handle(),
                    dividend.handle(),
                    spot.handle(),
                    v0,
                    kappa,
                    theta,
                    sigma,
                    rho,
                    lambda_,
                    nu,
                    delta,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Return the initial variance.
    fn v0(&self) -> f64 {
        self.inner.v0()
    }

    /// Return the variance mean-reversion speed.
    fn kappa(&self) -> f64 {
        self.inner.kappa()
    }

    /// Return the long-run variance.
    fn theta(&self) -> f64 {
        self.inner.theta()
    }

    /// Return the volatility of variance.
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }

    /// Return the spot/variance correlation.
    fn rho(&self) -> f64 {
        self.inner.rho()
    }

    /// Return the Poisson jump intensity.
    fn lambda_(&self) -> f64 {
        self.inner.lambda()
    }

    /// Return the logarithmic jump mean.
    fn nu(&self) -> f64 {
        self.inner.nu()
    }

    /// Return the logarithmic jump standard deviation.
    fn delta(&self) -> f64 {
        self.inner.delta()
    }

    /// Return the current retained spot quote.
    fn spot(&self) -> PyResult<f64> {
        Ok(self.inner.initial_values().map_err(PyQlError::from)?[0])
    }

    /// Return the current spot and initial variance.
    fn initial_values(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .initial_values()
            .map_err(PyQlError::from)?
            .to_vec())
    }

    /// Convert a date using the risk-free curve's clock.
    fn time(&self, date: &PyDate) -> PyResult<f64> {
        Ok(self.inner.time(&date.inner()).map_err(PyQlError::from)?)
    }
}

/// Eight-parameter Bates model retaining the original observable market.
#[gen_stub_pyclass]
#[pyclass(name = "BatesModel", unsendable, module = "itofin.models")]
pub struct PyBatesModel {
    inner: SharedMut<BatesModel>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBatesModel {
    /// Seed the calibrated model from its process.
    #[new]
    fn new(process: &PyBatesProcess) -> PyResult<Self> {
        Ok(Self {
            inner: BatesModel::new(Shared::clone(&process.inner)).map_err(PyQlError::from)?,
        })
    }

    /// Return the fitted initial variance.
    fn v0(&self) -> f64 {
        self.inner.borrow().v0()
    }

    /// Return the fitted variance mean-reversion speed.
    fn kappa(&self) -> f64 {
        self.inner.borrow().kappa()
    }

    /// Return the fitted long-run variance.
    fn theta(&self) -> f64 {
        self.inner.borrow().theta()
    }

    /// Return the fitted volatility of variance.
    fn sigma(&self) -> f64 {
        self.inner.borrow().sigma()
    }

    /// Return the fitted spot/variance correlation.
    fn rho(&self) -> f64 {
        self.inner.borrow().rho()
    }

    /// Return the fitted Poisson jump intensity.
    fn lambda_(&self) -> f64 {
        self.inner.borrow().lambda()
    }

    /// Return the fitted logarithmic jump mean.
    fn nu(&self) -> f64 {
        self.inner.borrow().nu()
    }

    /// Return the fitted logarithmic jump standard deviation.
    fn delta(&self) -> f64 {
        self.inner.borrow().delta()
    }

    /// Return theta, kappa, sigma, rho, v0, nu, delta, lambda_ in that order.
    fn params(&self) -> Vec<f64> {
        self.inner.borrow().calibrated_model().params().to_vec()
    }

    /// Atomically replace eight validated parameters and invalidate engines.
    fn set_params(&mut self, params: Vec<f64>) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .set_params(&Array::from(params))
            .map_err(PyQlError::from)?)
    }

    /// Fit existing Heston helpers with a retained Bates engine.
    ///
    /// Optional weights correspond to helpers. The eight-element fixed mask is
    /// ordered theta, kappa, sigma, rho, v0, nu, delta, lambda_.
    #[pyo3(signature = (helpers, method, end_criteria, integration_order=144, *, constraint=None, weights=None, fix_parameters=None))]
    #[allow(clippy::too_many_arguments)]
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PyHestonModelHelper>>,
        #[gen_stub(override_type(type_repr = "optimization.LevenbergMarquardt | optimization.Simplex | optimization.ConjugateGradient | optimization.SteepestDescent", imports = ("itofin.optimization")))]
        method: &Bound<'_, PyAny>,
        end_criteria: &PyEndCriteria,
        integration_order: usize,
        #[gen_stub(override_type(type_repr = "optimization.NoConstraint | optimization.PositiveConstraint | optimization.BoundaryConstraint | optimization.CompositeConstraint | None", imports = ("itofin.optimization")))]
        constraint: Option<&Bound<'_, PyAny>>,
        weights: Option<Vec<f64>>,
        fix_parameters: Option<Vec<bool>>,
    ) -> PyResult<()> {
        let options = calibration_options(constraint, weights, fix_parameters, false)?;
        let engine = shared_mut(
            BatesEngine::new(SharedMut::clone(&self.inner), integration_order)
                .map_err(PyQlError::from)?,
        ) as SharedMut<dyn PricingEngine>;
        with_method(method, |method| {
            let dyn_helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
                .iter()
                .map(|helper| {
                    let inner = helper.inner();
                    inner
                        .borrow_mut()
                        .base_mut()
                        .set_pricing_engine(SharedMut::clone(&engine));
                    inner as SharedMut<dyn CalibrationHelper>
                })
                .collect();
            calibrate(
                &self.inner,
                &dyn_helpers,
                method,
                end_criteria.inner(),
                options.constraint,
                options.weights,
                options.fix_parameters,
            )
            .map_err(PyQlError::from)?;
            Ok(())
        })
    }
}

/// European plain-vanilla Bates pricing by Gauss-Laguerre integration.
#[gen_stub_pyclass]
#[pyclass(name = "BatesEngine", unsendable, module = "itofin.pricingengines")]
pub struct PyBatesEngine {
    inner: SharedMut<dyn PricingEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBatesEngine {
    /// Retain the live model with an integration order from 1 through 192.
    ///
    /// Only NPV is supplied. Unsupported exercises and Greeks raise ItofinError.
    #[new]
    #[pyo3(signature = (model, integration_order=144))]
    fn new(model: &PyBatesModel, integration_order: usize) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                BatesEngine::new(SharedMut::clone(&model.inner), integration_order)
                    .map_err(PyQlError::from)?,
            ),
        })
    }
}

impl PyBatesEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner)
    }
}
