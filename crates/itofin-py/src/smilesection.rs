//! Facade for [`PySabrSmileSection`], a single option expiry's volatility smile
//! at fixed Hagan SABR parameters
//! (`termstructures::volatility::SabrSmileSection`).
//!
//! The core `SmileSection` trait is not exposed as a Python base class: the SABR
//! section is the only implementor a caller of this crate has a constructor for,
//! so the queries it needs are re-exported here directly rather than through an
//! abstract parent that would have exactly one child.
//!
//! Only the exercise-time constructor is wrapped, as the SABR cube's own
//! sections are: the date-anchored form differs from it only in computing the
//! exercise time from a reference date and a day counter, which a caller can do
//! with `DayCounter.year_fraction`.
//!
//! Deferred (visible): normal (Bachelier) SABR volatility and a non-zero
//! lognormal shift are rejected by the core constructor, both under #586, so the
//! arguments are accepted here and the core's error surfaces as an
//! `ItofinError`.

use crate::PyQlError;
use crate::swaptionvol::PyVolatilityType;
use libitofin::termstructures::volatility::{SabrSmileSection, SmileSection};
use pyo3::prelude::*;

/// Python `SabrSmileSection`: the volatility smile of one option expiry, read
/// off the closed-form Hagan SABR formula at fixed parameters
/// (`termstructures::volatility::SabrSmileSection`).
///
/// There is no calibration here: the four parameters are inputs. A fitted smile
/// is what the SABR swaption vol cube serves; this class is for querying a smile
/// whose parameters are already known.
#[pyclass(name = "SabrSmileSection", unsendable)]
pub struct PySabrSmileSection {
    inner: SabrSmileSection,
}

#[pymethods]
impl PySabrSmileSection {
    /// A smile at `exercise_time` years, centred on `forward`.
    ///
    /// Raises `ItofinError` on a non-zero `shift` or a `Normal`
    /// `volatility_type` (both deferred to #586), on a non-positive shifted
    /// forward, and on SABR parameters outside their admissible ranges
    /// (`alpha > 0`, `beta` in `[0, 1]`, `nu >= 0`, `rho^2 < 1`).
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (exercise_time, forward, alpha, beta, nu, rho, shift = 0.0, volatility_type = PyVolatilityType::ShiftedLognormal))]
    fn new(
        exercise_time: f64,
        forward: f64,
        alpha: f64,
        beta: f64,
        nu: f64,
        rho: f64,
        shift: f64,
        volatility_type: PyVolatilityType,
    ) -> PyResult<Self> {
        Ok(PySabrSmileSection {
            inner: SabrSmileSection::with_exercise_time(
                exercise_time,
                forward,
                alpha,
                beta,
                nu,
                rho,
                shift,
                volatility_type.inner(),
            )
            .map_err(PyQlError::from)?,
        })
    }

    /// The volatility at `strike`. Strikes below the shifted domain floor are
    /// clamped to it rather than rejected, as the core does.
    fn volatility(&self, strike: f64) -> PyResult<f64> {
        Ok(self.inner.volatility(strike).map_err(PyQlError::from)?)
    }

    /// The Black variance at `strike`: `volatility(strike)^2 * exercise_time`.
    fn variance(&self, strike: f64) -> PyResult<f64> {
        Ok(self.inner.variance(strike).map_err(PyQlError::from)?)
    }

    /// The exercise time the smile was built for, in years.
    #[getter]
    fn exercise_time(&self) -> f64 {
        self.inner.exercise_time()
    }

    /// The at-the-money level: the forward the smile is centred on.
    #[getter]
    fn atm_level(&self) -> f64 {
        self.inner
            .atm_level()
            .expect("a SABR smile section always carries its forward as the atm level")
    }

    /// The SABR `alpha` parameter.
    #[getter]
    fn alpha(&self) -> f64 {
        self.inner.alpha()
    }

    /// The SABR `beta` parameter.
    #[getter]
    fn beta(&self) -> f64 {
        self.inner.beta()
    }

    /// The SABR `nu` parameter.
    #[getter]
    fn nu(&self) -> f64 {
        self.inner.nu()
    }

    /// The SABR `rho` parameter.
    #[getter]
    fn rho(&self) -> f64 {
        self.inner.rho()
    }
}
