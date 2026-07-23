//! Facades for the Heston stack: [`PyHestonProcess`] and [`PyHestonModel`].

use crate::PyQlError;
use crate::time::{PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::models::HestonModel;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared};
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
}

impl PyHestonModel {
    /// A clone of the inner model handle for the engine facade (H2 also calibrates).
    pub(crate) fn inner(&self) -> SharedMut<HestonModel> {
        SharedMut::clone(&self.inner)
    }
}
