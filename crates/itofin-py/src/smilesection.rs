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

/// One option expiry's volatility smile, read off the closed-form Hagan SABR
/// formula at fixed parameters.
///
/// There is no calibration here: the four parameters are inputs. A fitted smile
/// is what SabrSwaptionVolatilityCube serves; this class is for querying a smile
/// whose parameters are already known.
#[pyclass(name = "SabrSmileSection", unsendable)]
pub struct PySabrSmileSection {
    inner: SabrSmileSection,
}

#[pymethods]
impl PySabrSmileSection {
    /// Build the smile at a given exercise time and forward.
    ///
    /// Only the exercise-time form is wrapped; the date-anchored one differs
    /// from it only in computing that time from a reference date and a day
    /// counter, which a caller can do with DayCounter.year_fraction.
    ///
    /// Args:
    ///     exercise_time (float): The option's exercise time, in years.
    ///     forward (float): The forward the smile is centred on.
    ///     alpha (float): The SABR alpha, which must be positive.
    ///     beta (float): The SABR beta, which must lie in [0, 1].
    ///     nu (float): The SABR nu, which must be non-negative.
    ///     rho (float): The SABR rho, whose square must be below 1.
    ///     shift (float): The lognormal shift; a non-zero shift is deferred
    ///         and refused.
    ///     volatility_type (VolatilityType): The quoting convention; Normal is
    ///         deferred and refused.
    ///
    /// Raises:
    ///     ItofinError: On a non-zero shift or a Normal volatility_type, both
    ///         deferred to #586; on a non-positive shifted forward; and on
    ///         SABR parameters outside their admissible ranges.
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

    /// Return the volatility at strike.
    ///
    /// Args:
    ///     strike (float): The strike; strikes below the shifted domain floor
    ///         are clamped to it rather than rejected, as the core does.
    ///
    /// Returns:
    ///     float: The Hagan SABR volatility.
    ///
    /// Raises:
    ///     ItofinError: On whatever the closed-form evaluation rejects.
    fn volatility(&self, strike: f64) -> PyResult<f64> {
        Ok(self.inner.volatility(strike).map_err(PyQlError::from)?)
    }

    /// Return the Black variance at strike.
    ///
    /// Args:
    ///     strike (float): The strike the variance is read at.
    ///
    /// Returns:
    ///     float: The squared volatility times the exercise time.
    ///
    /// Raises:
    ///     ItofinError: On whatever the closed-form evaluation rejects.
    fn variance(&self, strike: f64) -> PyResult<f64> {
        Ok(self.inner.variance(strike).map_err(PyQlError::from)?)
    }

    /// The exercise time the smile was built for.
    ///
    /// Returns:
    ///     float: The exercise time, in years.
    #[getter]
    fn exercise_time(&self) -> f64 {
        self.inner.exercise_time()
    }

    /// The at-the-money level.
    ///
    /// Returns:
    ///     float: The forward the smile is centred on.
    #[getter]
    fn atm_level(&self) -> f64 {
        self.inner
            .atm_level()
            .expect("a SABR smile section always carries its forward as the atm level")
    }

    /// The SABR alpha parameter.
    ///
    /// Returns:
    ///     float: The alpha the smile was built with.
    #[getter]
    fn alpha(&self) -> f64 {
        self.inner.alpha()
    }

    /// The SABR beta parameter.
    ///
    /// Returns:
    ///     float: The beta the smile was built with.
    #[getter]
    fn beta(&self) -> f64 {
        self.inner.beta()
    }

    /// The SABR nu parameter.
    ///
    /// Returns:
    ///     float: The nu the smile was built with.
    #[getter]
    fn nu(&self) -> f64 {
        self.inner.nu()
    }

    /// The SABR rho parameter.
    ///
    /// Returns:
    ///     float: The rho the smile was built with.
    #[getter]
    fn rho(&self) -> f64 {
        self.inner.rho()
    }
}
