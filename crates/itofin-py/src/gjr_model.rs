//! Observable GJR-GARCH model, analytic approximation and Monte Carlo pricing.

use crate::PyQlError;
use crate::calibration::{PyEndCriteria, calibration_options, with_method};
use crate::gjr::PyGjrGarchProcess;
use crate::heston::PyHestonModelHelper;
use libitofin::math::array::Array;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::model::CalibratedModelHolder;
use libitofin::models::{GjrGarchModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{AnalyticGjrGarchEngine, MakeMcEuropeanGjrGarchEngine};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
use pyo3::types::PyAny;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Six-parameter GJR-GARCH model retaining live spot and yield curves.
///
/// Parameters and fixed masks are ordered omega, alpha, beta, gamma, lambda_,
/// daily_variance. Variance and omega use daily units, not annual units.
#[gen_stub_pyclass]
#[pyclass(name = "GJRGARCHModel", unsendable, module = "itofin.models")]
pub struct PyGjrGarchModel {
    inner: SharedMut<GjrGarchModel>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGjrGarchModel {
    /// Seed the model from a retained process with FullTruncation discretization.
    #[new]
    fn new(process: &PyGjrGarchProcess) -> PyResult<Self> {
        Ok(Self {
            inner: GjrGarchModel::new(process.inner()).map_err(PyQlError::from)?,
        })
    }

    /// Return the fitted initial daily variance.
    fn daily_variance(&self) -> f64 {
        self.inner.borrow().v0()
    }

    /// Return the fitted daily variance intercept.
    fn omega(&self) -> f64 {
        self.inner.borrow().omega()
    }

    /// Return the fitted squared innovation coefficient.
    fn alpha(&self) -> f64 {
        self.inner.borrow().alpha()
    }

    /// Return the fitted lagged variance coefficient.
    fn beta(&self) -> f64 {
        self.inner.borrow().beta()
    }

    /// Return the fitted asymmetric innovation coefficient.
    fn gamma(&self) -> f64 {
        self.inner.borrow().gamma()
    }

    /// Return the fitted innovation risk premium.
    fn lambda_(&self) -> f64 {
        self.inner.borrow().lambda()
    }

    /// Return omega, alpha, beta, gamma, lambda_, daily_variance in that order.
    fn params(&self) -> Vec<f64> {
        self.inner.borrow().calibrated_model().params().to_vec()
    }

    /// Atomically replace six validated parameters and invalidate pricing engines.
    fn set_params(&mut self, params: Vec<f64>) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .set_params(&Array::from(params))
            .map_err(PyQlError::from)?)
    }

    /// Return the current process snapshot, retaining live market inputs.
    ///
    /// Replacing model parameters creates a new FullTruncation process; an
    /// earlier returned process keeps its own parameter snapshot.
    fn process(&self) -> PyGjrGarchProcess {
        PyGjrGarchProcess::from_inner(self.inner.borrow().process())
    }

    /// Fit Heston Black-volatility helpers with the analytic GJR approximation.
    ///
    /// Weights correspond to helpers. The six-element fixed mask uses the
    /// same order as params(). Failed calibration restores the model parameters.
    #[pyo3(signature = (helpers, method, end_criteria, *, constraint=None, weights=None, fix_parameters=None))]
    #[allow(clippy::too_many_arguments)]
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PyHestonModelHelper>>,
        #[gen_stub(override_type(type_repr = "optimization.LevenbergMarquardt | optimization.Simplex | optimization.ConjugateGradient | optimization.SteepestDescent", imports = ("itofin.optimization")))]
        method: &Bound<'_, PyAny>,
        end_criteria: &PyEndCriteria,
        #[gen_stub(override_type(type_repr = "optimization.NoConstraint | optimization.PositiveConstraint | optimization.BoundaryConstraint | optimization.CompositeConstraint | None", imports = ("itofin.optimization")))]
        constraint: Option<&Bound<'_, PyAny>>,
        weights: Option<Vec<f64>>,
        fix_parameters: Option<Vec<bool>>,
    ) -> PyResult<()> {
        let options = calibration_options(constraint, weights, fix_parameters, false)?;
        let engine = shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(&self.inner)))
            as SharedMut<dyn PricingEngine>;
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

/// European plain-vanilla pricing with QuantLib's GJR-GARCH moment approximation.
///
/// This is an approximation, not an exact characteristic-function valuation.
/// Preserves the source convention: call values omit the dividend discount,
/// and put minus call is strike * risk-free discount / dividend discount - spot.
/// Only NPV is supplied; unsupported exercises, payoffs and Greeks raise errors.
#[gen_stub_pyclass]
#[pyclass(
    name = "AnalyticGJRGARCHEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyAnalyticGjrGarchEngine {
    inner: SharedMut<dyn PricingEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyAnalyticGjrGarchEngine {
    /// Retain the live model and its daily-unit parameters.
    #[new]
    fn new(model: &PyGjrGarchModel) -> Self {
        Self {
            inner: shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(&model.inner))),
        }
    }
}

impl PyAnalyticGjrGarchEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner)
    }
}

/// Seeded pseudo-random European pricing on any supported GJR process scheme.
///
/// Retains the supplied process snapshot: later model parameter updates do
/// not retarget an existing MC engine. Live spot and yield quotes still update.
/// Standard error is available through VanillaOption.error_estimate(). Brownian
/// bridge and control variates are not supported by this facade.
#[gen_stub_pyclass]
#[pyclass(
    name = "MCEuropeanGJRGARCHEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyMcEuropeanGjrGarchEngine {
    inner: SharedMut<dyn PricingEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMcEuropeanGjrGarchEngine {
    /// Configure the core factory, preserving its argument validation.
    ///
    /// Specify exactly one of steps and steps_per_year, and exactly one of
    /// samples and absolute_tolerance. max_samples caps tolerance-driven draws.
    #[new]
    #[pyo3(signature = (process, steps=None, steps_per_year=None, samples=None, absolute_tolerance=None, max_samples=None, seed=None, antithetic=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        process: &PyGjrGarchProcess,
        steps: Option<usize>,
        steps_per_year: Option<usize>,
        samples: Option<usize>,
        absolute_tolerance: Option<f64>,
        max_samples: Option<usize>,
        seed: Option<u32>,
        antithetic: Option<bool>,
    ) -> PyResult<Self> {
        let mut maker = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process.inner());
        if let Some(steps) = steps {
            maker = maker.with_steps(steps);
        }
        if let Some(steps_per_year) = steps_per_year {
            maker = maker.with_steps_per_year(steps_per_year);
        }
        if let Some(samples) = samples {
            maker = maker.with_samples(samples);
        }
        if let Some(tolerance) = absolute_tolerance {
            maker = maker.with_absolute_tolerance(tolerance);
        }
        if let Some(max_samples) = max_samples {
            maker = maker.with_max_samples(max_samples);
        }
        if let Some(seed) = seed {
            maker = maker.with_seed(seed);
        }
        if let Some(antithetic) = antithetic {
            maker = maker.with_antithetic_variate(antithetic);
        }
        Ok(Self {
            inner: shared_mut(maker.build().map_err(PyQlError::from)?),
        })
    }
}

impl PyMcEuropeanGjrGarchEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner)
    }
}
