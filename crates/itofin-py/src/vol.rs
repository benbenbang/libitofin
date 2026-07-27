//! Facade for the Black-volatility term-structure base:
//! [`PyBlackVolTermStructure`].

use crate::PyQlError;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::matrix::Matrix;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure,
    BlackVolTimeExtrapolation,
};
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
    pub(crate) fn handle(&self) -> Handle<dyn BlackVolTermStructure> {
        self.inner.clone()
    }
}

/// Python `BlackConstantVol`: a flat Black volatility, constant in strike and
/// time (`termstructures::volatility::BlackConstantVol`).
///
/// Extends [`PyBlackVolTermStructure`] and supplies only the constructor; the
/// query surface is inherited. Unbounded in both time and strike, so queries
/// never need extrapolation enabled.
#[pyclass(name = "BlackConstantVol", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackConstantVol;

#[pymethods]
impl PyBlackConstantVol {
    #[new]
    #[pyo3(signature = (reference_date, volatility, day_counter, calendar = None))]
    fn new(
        reference_date: &PyDate,
        volatility: f64,
        day_counter: &PyDayCounter,
        calendar: Option<&PyCalendar>,
    ) -> PyClassInitializer<Self> {
        let structure = shared(BlackConstantVol::new(
            reference_date.inner(),
            calendar.map(PyCalendar::inner),
            volatility,
            day_counter.inner(),
        )) as Shared<dyn BlackVolTermStructure>;
        PyClassInitializer::from(PyBlackVolTermStructure {
            inner: Handle::new(structure),
        })
        .add_subclass(PyBlackConstantVol)
    }
}

/// Python `BlackVolTimeExtrapolation`: how a variance curve extrapolates past
/// its last node (`termstructures::volatility::BlackVolTimeExtrapolation`).
///
/// A fieldless pyo3 enum. `UseInterpolator` is accepted at construction but
/// **fails on any extrapolating query**: delegating to the interpolation needs
/// it evaluated past its last node, which the interpolation layer cannot enable
/// generically (`blackvariancecurve.rs:22-25,156-160`). The core returns an
/// `Err` there rather than silently substituting another rule, so the Python
/// boundary surfaces an [`struct@crate::ItofinError`] from the *query*, not the
/// constructor - the D10 no-silent-fallback line.
#[pyclass(name = "BlackVolTimeExtrapolation", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyBlackVolTimeExtrapolation {
    FlatVolatility,
    UseInterpolator,
    LinearVariance,
}

impl PyBlackVolTimeExtrapolation {
    /// The core [`BlackVolTimeExtrapolation`] this variant stands for.
    fn inner(&self) -> BlackVolTimeExtrapolation {
        match self {
            PyBlackVolTimeExtrapolation::FlatVolatility => {
                BlackVolTimeExtrapolation::FlatVolatility
            }
            PyBlackVolTimeExtrapolation::UseInterpolator => {
                BlackVolTimeExtrapolation::UseInterpolator
            }
            PyBlackVolTimeExtrapolation::LinearVariance => {
                BlackVolTimeExtrapolation::LinearVariance
            }
        }
    }
}

/// Python `BlackVarianceCurve`: a term structure of Black volatility with no
/// strike dimension, interpolating linearly on variance
/// (`termstructures::volatility::BlackVarianceCurve<Linear>`).
///
/// Extends [`PyBlackVolTermStructure`]. Finite in time: the last date is the
/// maximum, so queries past it require `enable_extrapolation()`, and
/// `time_extrapolation` picks the rule applied there (default
/// `FlatVolatility`, the C++ default). The interpolation stays `Linear`: only
/// the extrapolation axis is exposed, which keeps the concrete
/// `BlackVarianceCurve<Linear>` handle retained alongside the erased base
/// handle for a future local-volatility curve facade.
///
/// Selecting `UseInterpolator` constructs fine and answers in-range queries,
/// then errors on an extrapolating one; see
/// [`PyBlackVolTimeExtrapolation`].
#[pyclass(name = "BlackVarianceCurve", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackVarianceCurve {
    #[allow(dead_code)]
    concrete: Handle<BlackVarianceCurve<Linear>>,
}

#[pymethods]
impl PyBlackVarianceCurve {
    #[new]
    #[pyo3(signature = (
        reference_date,
        dates,
        black_vol_curve,
        day_counter,
        force_monotone_variance,
        time_extrapolation = PyBlackVolTimeExtrapolation::FlatVolatility,
    ))]
    fn new(
        reference_date: &PyDate,
        dates: Vec<PyRef<PyDate>>,
        black_vol_curve: Vec<f64>,
        day_counter: &PyDayCounter,
        force_monotone_variance: bool,
        time_extrapolation: PyBlackVolTimeExtrapolation,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let curve = shared(
            BlackVarianceCurve::with_interpolator(
                reference_date.inner(),
                &dates,
                &black_vol_curve,
                day_counter.inner(),
                force_monotone_variance,
                time_extrapolation.inner(),
                Linear,
            )
            .map_err(PyQlError::from)?,
        );
        let concrete = Handle::new(Shared::clone(&curve));
        let erased = Handle::new(curve as Shared<dyn BlackVolTermStructure>);
        Ok(
            PyClassInitializer::from(PyBlackVolTermStructure { inner: erased })
                .add_subclass(PyBlackVarianceCurve { concrete }),
        )
    }
}

/// Python `BlackVarianceSurface`: a Black volatility surface in strike and
/// expiry, interpolating bilinearly on variance
/// (`termstructures::volatility::BlackVarianceSurface`).
///
/// Extends [`PyBlackVolTermStructure`]. The `black_vol_matrix` is a
/// `list[list[float]]` with **one row per strike and one column per date**;
/// the surface is finite in both time and strike, so out-of-grid queries
/// require `enable_extrapolation()`.
#[pyclass(name = "BlackVarianceSurface", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackVarianceSurface;

#[pymethods]
impl PyBlackVarianceSurface {
    #[new]
    #[pyo3(signature = (reference_date, dates, strikes, black_vol_matrix, day_counter, calendar = None))]
    fn new(
        reference_date: &PyDate,
        dates: Vec<PyRef<PyDate>>,
        strikes: Vec<f64>,
        black_vol_matrix: Vec<Vec<f64>>,
        day_counter: &PyDayCounter,
        calendar: Option<&PyCalendar>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let matrix = matrix_from_rows(&black_vol_matrix)?;
        let surface = shared(
            BlackVarianceSurface::new(
                reference_date.inner(),
                calendar.map(PyCalendar::inner),
                &dates,
                strikes,
                &matrix,
                day_counter.inner(),
            )
            .map_err(PyQlError::from)?,
        ) as Shared<dyn BlackVolTermStructure>;
        Ok(PyClassInitializer::from(PyBlackVolTermStructure {
            inner: Handle::new(surface),
        })
        .add_subclass(PyBlackVarianceSurface))
    }
}

/// Converts a Python `list[list[float]]` (row per strike, column per date)
/// into a core [`Matrix`], rejecting an empty or ragged grid before it reaches
/// the surface constructor's dimension checks.
fn matrix_from_rows(rows: &[Vec<f64>]) -> PyResult<Matrix> {
    let n_rows = rows.len();
    if n_rows == 0 {
        return Err(crate::ItofinError::new_err(
            "black vol matrix must have at least one row",
        ));
    }
    let n_cols = rows[0].len();
    if n_cols == 0 {
        return Err(crate::ItofinError::new_err(
            "black vol matrix rows must have at least one column",
        ));
    }
    if rows.iter().any(|row| row.len() != n_cols) {
        return Err(crate::ItofinError::new_err(
            "black vol matrix rows must all have the same length",
        ));
    }
    let mut matrix = Matrix::with_size(n_rows, n_cols);
    for (i, row) in rows.iter().enumerate() {
        for (j, &value) in row.iter().enumerate() {
            matrix[(i, j)] = value;
        }
    }
    Ok(matrix)
}
