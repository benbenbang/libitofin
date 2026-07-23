//! Facades for the Hull-White short-rate stack: [`PyHullWhite`] and the
//! [`PyEuribor`] index.

use crate::PyQlError;
use crate::curve::PyFlatForward;
use crate::option::PyOptionType;
use crate::settings::PySettings;
use libitofin::indexes::{Euribor, IborIndex};
use libitofin::models::{CalibratedModelHolder, HullWhite};
use libitofin::shared::{Shared, SharedMut, shared};
use pyo3::prelude::*;

/// Python `HullWhite`: the one-factor Hull-White short-rate model
/// (`models::shortrate::hullwhite::HullWhite`).
///
/// The ctor is fallible (`hullwhite.rs:188`): it reads the curve's forward rate
/// at `0` and applies the Vasicek positivity constraints on `a`/`sigma`, so an
/// empty curve or a constraint-violating parameter surfaces as an `ItofinError`.
#[pyclass(name = "HullWhite", unsendable)]
pub struct PyHullWhite {
    inner: SharedMut<HullWhite>,
}

#[pymethods]
impl PyHullWhite {
    #[new]
    fn new(curve: &PyFlatForward, a: f64, sigma: f64) -> PyResult<Self> {
        let inner = HullWhite::new(curve.handle(), a, sigma).map_err(PyQlError::from)?;
        Ok(PyHullWhite { inner })
    }

    /// The mean-reversion speed `a`, read as `params()[0]`.
    ///
    /// `HullWhite` exposes no direct `a()` (the Vasicek base field is private).
    /// The public route is the flattened calibrated-model parameters, whose
    /// `[0]`/`[1]` order (`a`, then `sigma`) is pinned by the core calibration
    /// oracle (`hullwhite.rs:892,898`).
    fn a(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[0]
    }

    /// The short-rate volatility `sigma`, read as `params()[1]`.
    fn sigma(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[1]
    }

    /// The fitted initial short rate `r0` (`hullwhite.rs:225`).
    fn r0(&self) -> f64 {
        self.inner.borrow().r0()
    }

    /// The price of a European option, exercised at `maturity`, on a zero-coupon
    /// bond maturing at `bond_maturity` (`hullwhite.rs:263`, the 4-argument
    /// overload). Fallible: the fitted curve must be linked and the underlying
    /// `black_formula` arguments valid.
    fn discount_bond_option(
        &self,
        option_type: PyOptionType,
        strike: f64,
        maturity: f64,
        bond_maturity: f64,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow()
            .discount_bond_option(option_type.inner(), strike, maturity, bond_maturity)
            .map_err(PyQlError::from)?)
    }
}

impl PyHullWhite {
    /// A clone of the inner model handle for the calibration (W2) and
    /// Jamshidian-engine (X3) facades, which consume `SharedMut<HullWhite>`.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> SharedMut<HullWhite> {
        SharedMut::clone(&self.inner)
    }
}

/// Python `Euribor`: the Euribor IBOR index family (`indexes::Euribor`).
///
/// Only the 6-month tenor is wired (`Euribor6M`), the tenor the swap/swaption
/// facades need. `Euribor::six_months` (`euribor.rs:98`) returns an `IborIndex`
/// by value; it is wrapped in `shared()` so downstream ctors that take a
/// `Shared<IborIndex>` (VanillaSwap/SwaptionHelper) can hold the same object.
#[pyclass(name = "Euribor", unsendable)]
pub struct PyEuribor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyEuribor {
    #[staticmethod]
    fn six_months(curve: &PyFlatForward, settings: &PySettings) -> Self {
        PyEuribor {
            inner: shared(Euribor::six_months(curve.handle(), settings.inner())),
        }
    }
}

impl PyEuribor {
    /// A clone of the inner index for the swap/swaption facades.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Shared<IborIndex> {
        Shared::clone(&self.inner)
    }
}
