//! Finite-difference Black-Scholes vanilla pricing facade.

use crate::PyQlError;
use crate::market::PyBlackScholesProcess;
use crate::time::PyDate;
use libitofin::cashflows::{Dividend, dividend_vector};
use libitofin::methods::finitedifferences::solvers::FdmSchemeDesc;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::AnalyticDividendEuropeanEngine;
use libitofin::pricingengines::vanilla::fdblackscholesvanillaengine::FdBlackScholesVanillaEngine;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Supported rollback schemes for a one-dimensional Black-Scholes grid.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "FdScheme",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.pricingengines"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFdScheme {
    Douglas = 0,
    ImplicitEuler = 1,
}

impl PyFdScheme {
    fn inner(self) -> FdmSchemeDesc {
        match self {
            Self::Douglas => FdmSchemeDesc::douglas(),
            Self::ImplicitEuler => FdmSchemeDesc::implicit_euler(),
        }
    }
}

/// Finite-difference engine for European, American and Bermudan vanilla options.
#[gen_stub_pyclass]
#[pyclass(
    name = "FdBlackScholesVanillaEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyFdBlackScholesVanillaEngine {
    inner: SharedMut<FdBlackScholesVanillaEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyFdBlackScholesVanillaEngine {
    /// Build a grid engine retaining the Black-Scholes process.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The market process to price under.
    ///     t_grid (int): Positive number of time steps; defaults to 100.
    ///     x_grid (int): Spatial grid size, at least 3; defaults to 100.
    ///     damping_steps (int): Initial implicit-Euler steps; defaults to 0.
    ///     scheme (FdScheme): Douglas by default; ImplicitEuler is also supported.
    ///     dividend_dates (list[Date] | None): Payment dates of fixed cash
    ///         dividends, paid under the Spot model; dividends outside the
    ///         option life are dropped. The Escrowed model is not offered
    ///         (deferred to #828). A dividend on the evaluation date leaves
    ///         theta unavailable.
    ///     dividend_amounts (list[float] | None): The cash amount paid on each
    ///         of dividend_dates; given together with them or not at all.
    ///
    /// Raises:
    ///     ValueError: If the grid dimensions are invalid or only one of the
    ///         dividend lists is given.
    ///     OverflowError: If a grid integer is outside the accepted range.
    ///     ItofinError: If the dividend lists differ in length.
    #[new]
    #[pyo3(signature = (process, t_grid=100, x_grid=100, damping_steps=0, scheme=PyFdScheme::Douglas, dividend_dates=None, dividend_amounts=None))]
    fn new(
        process: &PyBlackScholesProcess,
        t_grid: usize,
        x_grid: usize,
        damping_steps: usize,
        scheme: PyFdScheme,
        dividend_dates: Option<Vec<PyRef<PyDate>>>,
        dividend_amounts: Option<Vec<f64>>,
    ) -> PyResult<Self> {
        if t_grid == 0 || x_grid < 3 || t_grid.checked_add(damping_steps).is_none() {
            return Err(PyValueError::new_err(
                "FD grid requires t_grid > 0, x_grid >= 3, and finite step count",
            ));
        }
        let dividends = match (dividend_dates, dividend_amounts) {
            (None, None) => Vec::new(),
            (Some(dates), Some(amounts)) => cash_dividends(&dates, &amounts)?,
            _ => {
                return Err(PyValueError::new_err(
                    "dividend_dates and dividend_amounts must be given together",
                ));
            }
        };
        Ok(Self {
            inner: shared_mut(FdBlackScholesVanillaEngine::with_cash_dividends(
                process.inner(),
                dividends,
                t_grid,
                x_grid,
                damping_steps,
                scheme.inner(),
            )),
        })
    }
}

impl PyFdBlackScholesVanillaEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

fn cash_dividends(dates: &[PyRef<PyDate>], amounts: &[f64]) -> PyResult<Vec<Shared<dyn Dividend>>> {
    let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
    Ok(dividend_vector(&dates, amounts).map_err(PyQlError::from)?)
}

/// Analytic engine for European vanilla options paying fixed cash dividends.
#[gen_stub_pyclass]
#[pyclass(
    name = "AnalyticDividendEuropeanEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyAnalyticDividendEuropeanEngine {
    inner: SharedMut<AnalyticDividendEuropeanEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyAnalyticDividendEuropeanEngine {
    /// Build the engine retaining the Black-Scholes process.
    ///
    /// The spot is reduced by the present value of the dividends paid up to
    /// the exercise date. Pricing a non-European exercise raises ItofinError.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The market process to price under.
    ///     dividend_dates (list[Date]): Payment dates of the cash dividends.
    ///     dividend_amounts (list[float]): The cash amount paid on each date.
    ///
    /// Raises:
    ///     ItofinError: If the dividend lists differ in length.
    #[new]
    fn new(
        process: &PyBlackScholesProcess,
        dividend_dates: Vec<PyRef<PyDate>>,
        dividend_amounts: Vec<f64>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(AnalyticDividendEuropeanEngine::new(
                process.inner(),
                cash_dividends(&dividend_dates, &dividend_amounts)?,
            )),
        })
    }
}

impl PyAnalyticDividendEuropeanEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
