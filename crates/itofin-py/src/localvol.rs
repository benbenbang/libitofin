//! Facades for local volatility: LocalVolTermStructure, LocalVolSurface and
//! LocalVolCurve.
//!
//! Deferred: only the linear `LocalVolCurve` is bound, the one interpolation
//! `BlackVarianceCurve` exposes (#624); FD local-vol pricing is #828.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::vol::{PyBlackVarianceCurve, PyBlackVolTermStructure};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    LocalVolCurve, LocalVolSurface, LocalVolTermStructure,
};
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// Shared base for every local-volatility structure.
///
/// Concrete structures subclass this and supply only their constructor; the
/// local volatility query below is inherited.
#[gen_stub_pyclass]
#[pyclass(
    name = "LocalVolTermStructure",
    subclass,
    unsendable,
    module = "itofin.termstructures"
)]
pub struct PyLocalVolTermStructure {
    inner: Handle<dyn LocalVolTermStructure>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyLocalVolTermStructure {
    /// Return the local volatility at year-fraction t and underlying level.
    ///
    /// Args:
    ///     t (float): The year fraction, in the structure's own day count.
    ///     strike (float): The underlying level the volatility is read at.
    ///     extrapolate (bool): Whether to answer past the structure's maximum
    ///         time.
    ///
    /// Returns:
    ///     float: The local volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls past the maximum time and
    ///         extrapolation is not allowed, or the core rejects the inputs.
    #[pyo3(signature = (t, strike, extrapolate = false))]
    fn local_vol(&self, t: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .local_vol(t, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }
}

impl PyLocalVolTermStructure {
    /// A clone of the inner handle for the process facade.
    pub(crate) fn handle(&self) -> Handle<dyn LocalVolTermStructure> {
        self.inner.clone()
    }

    fn base(inner: Shared<dyn LocalVolTermStructure>) -> Self {
        PyLocalVolTermStructure {
            inner: Handle::new(inner),
        }
    }
}

/// The Dupire local volatility derived from a Black volatility surface.
///
/// Observes the spot quote and every input, so setting the quote reprices
/// the processes built over it. Over a bilinear BlackVarianceSurface the
/// local volatility dips at the strike nodes, where the finite-difference
/// curvature meets the interpolation kink.
#[gen_stub_pyclass]
#[pyclass(name = "LocalVolSurface", extends = PyLocalVolTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyLocalVolSurface;

#[gen_stub_pymethods]
#[pymethods]
impl PyLocalVolSurface {
    /// Build the local volatility surface.
    ///
    /// Args:
    ///     black_vol (BlackVolTermStructure): The Black volatility surface.
    ///     risk_free (YieldTermStructure): The risk-free discount curve.
    ///     dividend (YieldTermStructure): The dividend curve.
    ///     spot (SimpleQuote): The spot quote the surface observes.
    #[gen_stub(override_return_type(type_repr = "LocalVolSurface"))]
    #[new]
    fn new(
        black_vol: &PyBlackVolTermStructure,
        risk_free: &PyYieldTermStructure,
        dividend: &PyYieldTermStructure,
        spot: &PySimpleQuote,
    ) -> PyClassInitializer<Self> {
        let surface = shared(LocalVolSurface::new(
            black_vol.handle(),
            risk_free.handle(),
            dividend.handle(),
            spot.handle(),
        ));
        PyClassInitializer::from(PyLocalVolTermStructure::base(surface)).add_subclass(Self)
    }
}

/// The local volatility of a Black variance curve: time-only, unbounded in
/// strike.
#[gen_stub_pyclass]
#[pyclass(name = "LocalVolCurve", extends = PyLocalVolTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyLocalVolCurve;

#[gen_stub_pymethods]
#[pymethods]
impl PyLocalVolCurve {
    /// Build the local volatility curve.
    ///
    /// Args:
    ///     curve (BlackVarianceCurve): The Black variance curve it derives from.
    #[gen_stub(override_return_type(type_repr = "LocalVolCurve"))]
    #[new]
    fn new(curve: PyRef<PyBlackVarianceCurve>) -> PyClassInitializer<Self> {
        let local = shared(LocalVolCurve::new(curve.concrete()));
        PyClassInitializer::from(PyLocalVolTermStructure::base(local)).add_subclass(Self)
    }
}
