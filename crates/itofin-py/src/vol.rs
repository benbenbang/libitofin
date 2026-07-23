//! Facade for the Black-volatility term-structure base:
//! [`PyBlackVolTermStructure`].

use crate::PyQlError;
use crate::time::PyDate;
use libitofin::handle::Handle;
use libitofin::termstructures::volatility::BlackVolTermStructure;
use pyo3::prelude::*;

/// Python `BlackVolTermStructure`: the shared base for every Black-volatility
/// surface (`termstructures::volatility::BlackVolTermStructure`).
///
/// Holds the erased `Handle<dyn BlackVolTermStructure>` and exposes the spot
/// and forward volatility/variance queries every concrete surface inherits,
/// plus the strike domain and the extrapolation toggles. Concrete surfaces
/// subclass this and supply only their constructor.
#[pyclass(name = "BlackVolTermStructure", subclass, unsendable)]
pub struct PyBlackVolTermStructure {
    inner: Handle<dyn BlackVolTermStructure>,
}

#[pymethods]
impl PyBlackVolTermStructure {
    /// The spot Black volatility at year-fraction `t` and `strike`.
    #[pyo3(signature = (t, strike, extrapolate = false))]
    fn black_vol(&self, t: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_vol(t, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The spot Black volatility at `date` and `strike`.
    #[pyo3(signature = (date, strike, extrapolate = false))]
    fn black_vol_date(&self, date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_vol_date(date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The spot Black variance at year-fraction `t` and `strike`.
    #[pyo3(signature = (t, strike, extrapolate = false))]
    fn black_variance(&self, t: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance(t, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The spot Black variance at `date` and `strike`.
    #[pyo3(signature = (date, strike, extrapolate = false))]
    fn black_variance_date(&self, date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_date(date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The forward Black volatility between year-fractions `t1` and `t2`.
    #[pyo3(signature = (t1, t2, strike, extrapolate = false))]
    fn black_forward_vol(&self, t1: f64, t2: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_forward_vol(t1, t2, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The forward Black variance between year-fractions `t1` and `t2`.
    #[pyo3(signature = (t1, t2, strike, extrapolate = false))]
    fn black_forward_variance(
        &self,
        t1: f64,
        t2: f64,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_forward_variance(t1, t2, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The minimum strike for which the surface can return volatilities.
    fn min_strike(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .min_strike())
    }

    /// The maximum strike for which the surface can return volatilities.
    fn max_strike(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_strike())
    }

    /// The latest date for which the surface can return values.
    fn max_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_date();
        Ok(PyDate::from_inner(date))
    }

    /// Whether the surface answers dates/times beyond its maximum.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allows extrapolation past the maximum date/time.
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbids extrapolation past the maximum date/time.
    fn disable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .disable_extrapolation();
        Ok(())
    }
}

impl PyBlackVolTermStructure {
    /// A clone of the inner surface handle for the pricing facades.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn BlackVolTermStructure> {
        self.inner.clone()
    }
}
