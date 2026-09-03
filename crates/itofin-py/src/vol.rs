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

/// Shared base for every Black-volatility surface: spot and forward vol/variance.
///
/// Concrete surfaces subclass this and supply only their constructor; the
/// whole query surface below is inherited, along with the strike domain and
/// the extrapolation toggles.
#[pyclass(name = "BlackVolTermStructure", subclass, unsendable)]
pub struct PyBlackVolTermStructure {
    inner: Handle<dyn BlackVolTermStructure>,
}

#[pymethods]
impl PyBlackVolTermStructure {
    /// Return the spot Black volatility at year-fraction t and strike.
    ///
    /// Args:
    ///     t (float): The year fraction, in the surface's own day count.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (t, strike, extrapolate = false))]
    fn black_vol(&self, t: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_vol(t, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the spot Black volatility at date and strike.
    ///
    /// Args:
    ///     date (Date): The expiry the volatility is read at.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (date, strike, extrapolate = false))]
    fn black_vol_date(&self, date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_vol_date(date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the spot Black variance at year-fraction t and strike.
    ///
    /// Args:
    ///     t (float): The year fraction, in the surface's own day count.
    ///     strike (float): The strike the variance is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black variance.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (t, strike, extrapolate = false))]
    fn black_variance(&self, t: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance(t, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the spot Black variance at date and strike.
    ///
    /// Args:
    ///     date (Date): The expiry the variance is read at.
    ///     strike (float): The strike the variance is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black variance.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (date, strike, extrapolate = false))]
    fn black_variance_date(&self, date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_date(date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the forward Black volatility between year-fractions t1 and t2.
    ///
    /// Args:
    ///     t1 (float): The start year fraction.
    ///     t2 (float): The end year fraction.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The forward Black volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (t1, t2, strike, extrapolate = false))]
    fn black_forward_vol(&self, t1: f64, t2: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_forward_vol(t1, t2, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the forward Black variance between year-fractions t1 and t2.
    ///
    /// Args:
    ///     t1 (float): The start year fraction.
    ///     t2 (float): The end year fraction.
    ///     strike (float): The strike the variance is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The forward Black variance.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
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

    /// Return the minimum strike for which the surface can return volatilities.
    ///
    /// Returns:
    ///     float: The lower bound of the strike domain.
    fn min_strike(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .min_strike())
    }

    /// Return the maximum strike for which the surface can return volatilities.
    ///
    /// Returns:
    ///     float: The upper bound of the strike domain.
    fn max_strike(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_strike())
    }

    /// Return the latest date for which the surface can return values.
    ///
    /// Returns:
    ///     Date: The surface's maximum date.
    fn max_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_date();
        Ok(PyDate::from_inner(date))
    }

    /// Return whether the surface answers dates and times beyond its maximum.
    ///
    /// Returns:
    ///     bool: True when extrapolation is enabled on the surface itself.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allow extrapolation past the maximum date and time.
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbid extrapolation past the maximum date and time.
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

/// A flat Black volatility, constant in strike and time.
///
/// Unbounded in both time and strike, so queries never need extrapolation
/// enabled.
#[pyclass(name = "BlackConstantVol", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackConstantVol;

#[pymethods]
impl PyBlackConstantVol {
    /// Build the flat surface.
    ///
    /// Args:
    ///     reference_date (Date): The date times are measured from.
    ///     volatility (float): The single volatility answered everywhere.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     calendar (Calendar | None): The surface's calendar, if any.
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

/// How a variance curve extrapolates past its last node.
///
/// ``UseInterpolator`` is accepted at construction but raises ItofinError on
/// any extrapolating query: the interpolation layer cannot be evaluated past
/// its last node, and the core errors rather than silently substituting
/// another rule.
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

/// A term structure of Black volatility with no strike dimension.
///
/// Interpolates linearly on variance. Finite in time: the last date is the
/// maximum, so queries past it require enable_extrapolation(), and
/// time_extrapolation picks the rule applied there. The interpolation itself
/// stays linear; only the extrapolation axis is exposed.
#[pyclass(name = "BlackVarianceCurve", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackVarianceCurve {
    #[allow(dead_code)]
    concrete: Handle<BlackVarianceCurve<Linear>>,
}

#[pymethods]
impl PyBlackVarianceCurve {
    /// Build the variance curve over its (date, volatility) nodes.
    ///
    /// Args:
    ///     reference_date (Date): The date times are measured from.
    ///     dates (list[Date]): The node dates.
    ///     black_vol_curve (list[float]): The Black volatility at each node.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     force_monotone_variance (bool): Whether to require the implied
    ///         variance to increase across the nodes.
    ///     time_extrapolation (BlackVolTimeExtrapolation): The rule applied
    ///         past the last node; defaults to FlatVolatility, the C++
    ///         default. Selecting UseInterpolator constructs fine and answers
    ///         in-range queries, then errors on an extrapolating one.
    ///
    /// Raises:
    ///     ItofinError: On whatever the core rejects about the nodes, a
    ///         non-monotone variance under force_monotone_variance included.
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

/// A Black volatility surface in strike and expiry, interpolating bilinearly.
///
/// Finite in both time and strike, so out-of-grid queries require
/// enable_extrapolation().
#[pyclass(name = "BlackVarianceSurface", extends = PyBlackVolTermStructure, unsendable)]
pub struct PyBlackVarianceSurface;

#[pymethods]
impl PyBlackVarianceSurface {
    /// Build the surface over its strike-by-expiry grid.
    ///
    /// Args:
    ///     reference_date (Date): The date times are measured from.
    ///     dates (list[Date]): The expiry grid, one per matrix column.
    ///     strikes (list[float]): The strike grid, one per matrix row.
    ///     black_vol_matrix (list[list[float]]): The volatilities, one row per
    ///         strike and one column per date.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     calendar (Calendar | None): The surface's calendar, if any.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged matrix, and on whatever the core
    ///         rejects about the grid dimensions.
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
