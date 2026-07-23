//! Facades for the Hull-White short-rate stack: [`PyHullWhite`] and the
//! [`PyEuribor`] index.

use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::curve::PyFlatForward;
use crate::option::PyOptionType;
use crate::settings::PySettings;
use crate::time::{PyDayCounter, PyPeriod};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::{Euribor, IborIndex};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::shortrate::SwaptionHelper;
use libitofin::models::{CalibratedModelHolder, HullWhite, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::VolatilityType;
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
    fn new(curve: PyRef<'_, PyFlatForward>, a: f64, sigma: f64) -> PyResult<Self> {
        let inner = HullWhite::new(curve.as_super().handle(), a, sigma).map_err(PyQlError::from)?;
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

    /// Calibrates the model to `helpers` with `method` under `end_criteria`,
    /// then writes the fitted `a`/`sigma` back (readable through the getters).
    ///
    /// Mirrors the core oracle (`hullwhite.rs:809-864`): one
    /// [`JamshidianSwaptionEngine`] is built on this model and installed on every
    /// helper, so all swaptions price through the same analytic engine the
    /// optimizer drives (keeping W2 independent of the user-facing engine facade).
    /// `fix_reversion` pins the mean reversion `a` and frees only `sigma`
    /// (`fix_parameters = [true, false]`, `hullwhite.rs:1043`); otherwise both are
    /// free. [`calibrate`](libitofin::models::calibrate) fails on an empty helper
    /// list.
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PySwaptionHelper>>,
        method: &mut PyLevenbergMarquardt,
        end_criteria: &PyEndCriteria,
        fix_reversion: bool,
    ) -> PyResult<()> {
        let engine = shared_mut(JamshidianSwaptionEngine::new(SharedMut::clone(&self.inner)))
            as SharedMut<dyn PricingEngine>;
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
        let fix_parameters = if fix_reversion {
            vec![true, false]
        } else {
            Vec::new()
        };
        calibrate(
            &self.inner,
            &dyn_helpers,
            method.inner_mut(),
            end_criteria.inner(),
            None,
            Vec::new(),
            fix_parameters,
        )
        .map_err(PyQlError::from)?;
        Ok(())
    }
}

impl PyHullWhite {
    /// A clone of the inner model handle for the calibration (W2) and
    /// Jamshidian-engine (X3) facades, which consume `SharedMut<HullWhite>`.
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
    fn six_months(curve: PyRef<'_, PyFlatForward>, settings: &PySettings) -> Self {
        PyEuribor {
            inner: shared(Euribor::six_months(
                curve.as_super().handle(),
                settings.inner(),
            )),
        }
    }
}

impl PyEuribor {
    /// A clone of the inner index for the swap/swaption facades.
    pub(crate) fn inner(&self) -> Shared<IborIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `SwaptionHelper`: a co-terminal swaption calibration instrument
/// (`models::shortrate::calibrationhelpers::swaptionhelper::SwaptionHelper`).
///
/// The helper builds its own European swaption from the maturity/length and the
/// index, so no swap or swaption facade is needed. The volatility is assembled
/// into a `Handle<dyn Quote>` internally from a `SimpleQuote`. The oracle's fixed
/// defaults are pinned here (`hullwhite.rs:839-844`): `strike = None` (struck at
/// the forward), `ShiftedLognormal` volatility with zero shift, the index's own
/// settlement days, and `Compound` averaging. Held as `SharedMut` so a
/// calibration can install the Jamshidian engine on it and upcast it to
/// `SharedMut<dyn CalibrationHelper>`.
#[pyclass(name = "SwaptionHelper", unsendable)]
pub struct PySwaptionHelper {
    inner: SharedMut<SwaptionHelper>,
}

#[pymethods]
impl PySwaptionHelper {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        maturity: &PyPeriod,
        length: &PyPeriod,
        volatility: f64,
        index: &PyEuribor,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_day_counter: &PyDayCounter,
        floating_leg_day_counter: &PyDayCounter,
        curve: PyRef<'_, PyFlatForward>,
        error_type: &PyCalibrationErrorType,
        nominal: f64,
    ) -> Self {
        let vol = Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
        PySwaptionHelper {
            inner: shared_mut(SwaptionHelper::new(
                maturity.inner(),
                length.inner(),
                vol,
                index.inner(),
                fixed_leg_tenor.inner(),
                fixed_leg_day_counter.inner(),
                floating_leg_day_counter.inner(),
                curve.as_super().handle(),
                error_type.inner(),
                None,
                nominal,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
                RateAveraging::Compound,
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
