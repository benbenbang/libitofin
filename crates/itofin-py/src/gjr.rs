//! Observable GJR-GARCH process with spot and annual variance state.

use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::time::PyDate;
use crate::{ItofinError, PyQlError};
use libitofin::math::array::Array;
use libitofin::processes::{GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess};
use libitofin::shared::{Shared, shared};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

pub(crate) fn discretization(scheme: &str) -> PyResult<GjrGarchDiscretization> {
    match scheme {
        "PartialTruncation" => Ok(GjrGarchDiscretization::PartialTruncation),
        "FullTruncation" => Ok(GjrGarchDiscretization::FullTruncation),
        "Reflection" => Ok(GjrGarchDiscretization::Reflection),
        _ => Err(ItofinError::new_err("unsupported GJR-GARCH discretization")),
    }
}

/// GJR-GARCH diffusion approximation retaining live spot and yield curves.
///
/// Parameters `daily_variance` and `omega` use daily units. The state is
/// `[spot, annual_variance]`, with initial annual variance equal to
/// `daily_variance * days_per_year`. Truncation schemes can return a negative
/// raw variance state; this is not a daily GJR recursion or a pricing model.
#[gen_stub_pyclass]
#[pyclass(name = "GJRGARCHProcess", unsendable, module = "itofin.processes")]
pub struct PyGjrGarchProcess {
    inner: Shared<GjrGarchProcess>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGjrGarchProcess {
    /// Retain spot and curves and validate finite admissible GJR parameters.
    #[new]
    #[pyo3(signature = (spot, risk_free, dividend, daily_variance, omega, alpha, beta, gamma, lambda_, days_per_year = 252.0, scheme = "FullTruncation"))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        spot: &PySimpleQuote,
        risk_free: &PyYieldTermStructure,
        dividend: &PyYieldTermStructure,
        daily_variance: f64,
        omega: f64,
        alpha: f64,
        beta: f64,
        gamma: f64,
        lambda_: f64,
        days_per_year: f64,
        scheme: &str,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared(
                GjrGarchProcess::new(
                    risk_free.handle(),
                    dividend.handle(),
                    spot.handle(),
                    GjrGarchParameters {
                        v0: daily_variance,
                        omega,
                        alpha,
                        beta,
                        gamma,
                        lambda: lambda_,
                        days_per_year,
                    },
                    discretization(scheme)?,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Return initial daily variance, not the annualized state component.
    fn daily_variance(&self) -> f64 {
        self.inner.v0()
    }

    /// Return the daily variance intercept.
    fn omega(&self) -> f64 {
        self.inner.omega()
    }

    /// Return the squared innovation coefficient.
    fn alpha(&self) -> f64 {
        self.inner.alpha()
    }

    /// Return the lagged variance coefficient.
    fn beta(&self) -> f64 {
        self.inner.beta()
    }

    /// Return the asymmetric innovation coefficient.
    fn gamma(&self) -> f64 {
        self.inner.gamma()
    }

    /// Return the innovation risk premium.
    fn lambda_(&self) -> f64 {
        self.inner.lambda()
    }

    /// Return the daily-to-annual conversion factor.
    fn days_per_year(&self) -> f64 {
        self.inner.days_per_year()
    }

    /// Return the configured scheme name.
    fn discretization(&self) -> &'static str {
        match self.inner.discretization() {
            GjrGarchDiscretization::PartialTruncation => "PartialTruncation",
            GjrGarchDiscretization::FullTruncation => "FullTruncation",
            GjrGarchDiscretization::Reflection => "Reflection",
        }
    }

    /// Return current spot and initial annual variance.
    fn initial_values(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .initial_values()
            .map_err(PyQlError::from)?
            .to_vec())
    }

    /// Return log-spot and annual-variance drift at a two-component state.
    fn drift(&self, t: f64, state: Vec<f64>) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .drift(t, &Array::from(state))
            .map_err(PyQlError::from)?
            .to_vec())
    }

    /// Return the two-by-two diffusion matrix, one row per state component.
    fn diffusion(&self, t: f64, state: Vec<f64>) -> PyResult<Vec<Vec<f64>>> {
        let matrix = self
            .inner
            .diffusion(t, &Array::from(state))
            .map_err(PyQlError::from)?;
        Ok((0..matrix.rows())
            .map(|row| matrix.row(row).to_vec())
            .collect())
    }

    /// Evolve a state with two supplied independent standard-normal draws.
    fn evolve(&self, t0: f64, state: Vec<f64>, dt: f64, draws: Vec<f64>) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .evolve(t0, &Array::from(state), dt, &Array::from(draws))
            .map_err(PyQlError::from)?
            .to_vec())
    }

    /// Convert a date using the risk-free curve's reference date and day count.
    fn time(&self, date: &PyDate) -> PyResult<f64> {
        Ok(self.inner.time(&date.inner()).map_err(PyQlError::from)?)
    }
}
