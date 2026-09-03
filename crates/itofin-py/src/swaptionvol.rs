//! Facades for the swaption volatility stack: the SwaptionVolatilityStructure
//! base, the VolatilityType flag, the constant surface
//! ConstantSwaptionVolatility, the at-the-money grid SwaptionVolatilityMatrix,
//! the spread cube over it, InterpolatedSwaptionVolatilityCube, and the
//! calibrated SabrSwaptionVolatilityCube.
//!
//! The base holds the surface type-erased and exposes the queries every
//! concrete surface inherits; concrete surfaces
//! subclass it and supply only their constructor. They build the base through
//! from_handle() rather than a struct literal, so the later surfaces in this
//! file (and the matrix/cube facades stacking on it) never need access to the
//! private field.
//!
//! The constant surface exposes both reference-date families (#627): `new` and
//! `with_quote` pin the reference date, while `moving` and `moving_with_quote`
//! float it off the `Settings` evaluation date by a settlement-day count on a
//! calendar, the shape live market data wants.

use crate::PyQlError;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::swapindex::PySwapIndex;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::math::matrix::Matrix;
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    ConstantSwaptionVolatility, InterpolatedSwaptionVolatilityCube, SabrSwaptionVolatilityCube,
    SwaptionVolatilityMatrix, SwaptionVolatilityStructure, VolatilityType,
};
use libitofin::time::period::Period;
use pyo3::prelude::*;

/// The four SABR parameters a guess row and the fixed-parameter flags carry,
/// in the order `[alpha, beta, nu, rho]`.
const SABR_PARAMETERS: usize = 4;

/// Shared base for every swaption volatility surface: volatility, Black
/// variance and lognormal shift, addressed by option and swap tenor.
#[pyclass(name = "SwaptionVolatilityStructure", subclass, unsendable)]
pub struct PySwaptionVolatilityStructure {
    inner: Handle<dyn SwaptionVolatilityStructure>,
}

#[pymethods]
impl PySwaptionVolatilityStructure {
    /// Return the volatility for an option tenor, swap tenor and strike.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor, resolved against the
    ///         surface's reference date and calendar.
    ///     swap_tenor (Period): The underlying swap's tenor.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The volatility, in whichever type the surface quotes.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (option_tenor, swap_tenor, strike, extrapolate = false))]
    fn volatility(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_tenors(
                option_tenor.inner(),
                swap_tenor.inner(),
                strike,
                extrapolate,
            )
            .map_err(PyQlError::from)?)
    }

    /// Return the Black variance, the squared volatility times option time.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor.
    ///     swap_tenor (Period): The underlying swap's tenor.
    ///     strike (float): The strike the variance is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black variance.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (option_tenor, swap_tenor, strike, extrapolate = false))]
    fn black_variance(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_tenors(
                option_tenor.inner(),
                swap_tenor.inner(),
                strike,
                extrapolate,
            )
            .map_err(PyQlError::from)?)
    }

    /// Return the lognormal shift, in the date form.
    ///
    /// Taken in the date form because the core trait has no tenor overload for
    /// the shift, unlike the volatility and variance queries above.
    ///
    /// Args:
    ///     option_date (Date): The option date the shift is read at.
    ///     swap_length (float): The underlying swap's length, in years.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The lognormal shift.
    ///
    /// Raises:
    ///     ItofinError: On a normal-volatility surface, where a shift has no
    ///         meaning, and on an out-of-grid query without extrapolation.
    #[pyo3(signature = (option_date, swap_length, extrapolate = false))]
    fn shift(&self, option_date: &PyDate, swap_length: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .shift(option_date.inner(), swap_length, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the date every option time is measured from.
    ///
    /// Pinned at construction on the fixed-reference surfaces; derived from
    /// the Settings evaluation date (settlement days on the calendar) on the
    /// moving ones, so it follows a later set_evaluation_date.
    ///
    /// Returns:
    ///     Date: The surface's reference date.
    ///
    /// Raises:
    ///     ItofinError: On a moving surface whose Settings has no evaluation
    ///         date set.
    fn reference_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .reference_date()
                .map_err(PyQlError::from)?,
        ))
    }
}

impl PySwaptionVolatilityStructure {
    /// A clone of the inner surface handle for the engine facades.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn SwaptionVolatilityStructure> {
        self.inner.clone()
    }

    /// The base a concrete surface's `#[new]` extends, built from its erased
    /// handle. The named constructor keeps the private field an implementation
    /// detail of this module.
    pub(crate) fn from_handle(inner: Handle<dyn SwaptionVolatilityStructure>) -> Self {
        PySwaptionVolatilityStructure { inner }
    }
}

/// Whether a surface quotes shifted-lognormal (Black) or normal (Bachelier)
/// volatilities. A mismatch with the engine's formula surfaces at pricing time.
#[pyclass(name = "VolatilityType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyVolatilityType {
    ShiftedLognormal,
    Normal,
}

impl PyVolatilityType {
    /// The core VolatilityType this variant stands for.
    pub(crate) fn inner(&self) -> VolatilityType {
        match self {
            PyVolatilityType::ShiftedLognormal => VolatilityType::ShiftedLognormal,
            PyVolatilityType::Normal => VolatilityType::Normal,
        }
    }
}

/// A single volatility with no option-time, swap-length or strike dependence.
///
/// The constructor and with_quote pin the reference date, so every query's
/// option time runs from reference_date rather than the evaluation date. The
/// moving and moving_with_quote forms float the reference date off the
/// Settings evaluation date instead (#627).
#[pyclass(name = "ConstantSwaptionVolatility", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PyConstantSwaptionVolatility;

#[pymethods]
impl PyConstantSwaptionVolatility {
    /// Build the surface at a fixed volatility.
    ///
    /// Args:
    ///     reference_date (Date): The date every query's option time runs
    ///         from.
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (float): The single volatility answered everywhere,
    ///         wrapped in an internal quote the caller cannot later mutate.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     shift (float): The lognormal shift.
    #[new]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, shift = 0.0))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        shift: f64,
    ) -> PyClassInitializer<Self> {
        let surface = shared(ConstantSwaptionVolatility::new(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            shift,
        )) as Shared<dyn SwaptionVolatilityStructure>;
        PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
            surface,
        )))
        .add_subclass(PyConstantSwaptionVolatility)
    }

    /// Build the surface reading its volatility from a live quote.
    ///
    /// Args:
    ///     reference_date (Date): The date every query's option time runs
    ///         from.
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (SimpleQuote): The volatility; a later set_value
    ///         notifies the surface's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     shift (float): The lognormal shift.
    ///
    /// Returns:
    ///     ConstantSwaptionVolatility: The surface over that quote.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, shift = 0.0))]
    fn with_quote(
        py: Python<'_>,
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        shift: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantSwaptionVolatility::with_quote(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            shift,
        )) as Shared<dyn SwaptionVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantSwaptionVolatility),
        )
    }

    /// Build the surface with a reference date floating off the evaluation date.
    ///
    /// The reference date is the evaluation date advanced by settlement_days
    /// business days on calendar, so it follows a later set_evaluation_date.
    ///
    /// Args:
    ///     settlement_days (int): Business days between the evaluation date
    ///         and the reference date.
    ///     calendar (Calendar): The calendar the reference date is derived on
    ///         and option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (float): The single volatility answered everywhere,
    ///         wrapped in an internal quote the caller cannot later mutate.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     settings (Settings): The evaluation context the reference date
    ///         floats off.
    ///     shift (float): The lognormal shift.
    ///
    /// Returns:
    ///     ConstantSwaptionVolatility: The moving surface.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, volatility, day_counter, volatility_type, settings, shift = 0.0))]
    fn moving(
        py: Python<'_>,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        settings: &PySettings,
        shift: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantSwaptionVolatility::moving(
            settlement_days,
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            shift,
            settings.inner(),
        )) as Shared<dyn SwaptionVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantSwaptionVolatility),
        )
    }

    /// Build the moving surface reading its volatility from a live quote.
    ///
    /// Args:
    ///     settlement_days (int): Business days between the evaluation date
    ///         and the reference date.
    ///     calendar (Calendar): The calendar the reference date is derived on
    ///         and option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (SimpleQuote): The volatility; a later set_value
    ///         notifies the surface's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     settings (Settings): The evaluation context the reference date
    ///         floats off.
    ///     shift (float): The lognormal shift.
    ///
    /// Returns:
    ///     ConstantSwaptionVolatility: The moving surface over that quote.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, volatility, day_counter, volatility_type, settings, shift = 0.0))]
    fn moving_with_quote(
        py: Python<'_>,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        settings: &PySettings,
        shift: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantSwaptionVolatility::moving_with_quote(
            settlement_days,
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            shift,
            settings.inner(),
        )) as Shared<dyn SwaptionVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantSwaptionVolatility),
        )
    }
}

/// An at-the-money volatility grid, bilinear over an option-tenor x
/// swap-tenor lattice.
///
/// Every grid is a row per option tenor and a column per swap tenor; shifts,
/// when given, must match that shape, and None means all-zero shifts. The grid
/// is at the money, so a query's strike is range-checked and then ignored.
/// flat_extrapolation clamps a query past the grid to the nearest edge or
/// corner vol instead of extending the boundary surface.
#[pyclass(name = "SwaptionVolatilityMatrix", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PySwaptionVolatilityMatrix;

#[pymethods]
impl PySwaptionVolatilityMatrix {
    /// Build the grid on a pinned reference date over fixed volatilities.
    ///
    /// Every query's option time runs from reference_date rather than from the
    /// evaluation date.
    ///
    /// Args:
    ///     reference_date (Date): The date option times run from.
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The option axis, one per grid row.
    ///     swap_tenors (list[Period]): The swap axis, one per grid column.
    ///     volatilities (list[list[float]]): The at-the-money volatilities,
    ///         one row per option tenor.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the grid is
    ///         shifted-lognormal or normal.
    ///     shifts (list[list[float]] | None): The lognormal shifts in the same
    ///         shape as volatilities; None means all-zero shifts.
    ///     flat_extrapolation (bool): Whether a query past the grid clamps to
    ///         the nearest edge or corner vol instead of extending the
    ///         boundary surface.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged grid, a shifts grid that does
    ///         not match the volatilities shape, and on whatever the core
    ///         rejects about the axes.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, option_tenors, swap_tenors, volatilities, day_counter, volatility_type, shifts = None, flat_extrapolation = false))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        swap_tenors: Vec<PyRef<'_, PyPeriod>>,
        volatilities: Vec<Vec<f64>>,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        shifts: Option<Vec<Vec<f64>>>,
        flat_extrapolation: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        let volatilities = matrix_from_rows(&volatilities, "volatility")?;
        let shifts = match shifts {
            Some(rows) => matrix_from_rows(&rows, "shift")?,
            None => Matrix::new(),
        };
        let build = if flat_extrapolation {
            SwaptionVolatilityMatrix::new_flat
        } else {
            SwaptionVolatilityMatrix::new
        };
        let surface = shared(
            build(
                reference_date.inner(),
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                tenors(&swap_tenors),
                &volatilities,
                day_counter.inner(),
                volatility_type.inner(),
                &shifts,
            )
            .map_err(PyQlError::from)?,
        ) as Shared<dyn SwaptionVolatilityStructure>;
        Ok(
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PySwaptionVolatilityMatrix),
        )
    }

    /// Build a grid whose reference date floats off the evaluation date.
    ///
    /// The reference date sits at zero settlement days from the evaluation
    /// date, and each node is read from the caller's quote.
    ///
    /// Args:
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The option axis, one per grid row.
    ///     swap_tenors (list[Period]): The swap axis, one per grid column.
    ///     volatilities (list[list[SimpleQuote]]): The at-the-money volatility
    ///         quotes; a later set_value on any of them rebuilds the
    ///         interpolation and notifies the grid's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the grid is
    ///         shifted-lognormal or normal.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date floats off.
    ///     shifts (list[list[float]] | None): The lognormal shifts in the same
    ///         shape as volatilities; None means all-zero shifts.
    ///     flat_extrapolation (bool): Whether a query past the grid clamps to
    ///         the nearest edge or corner vol.
    ///
    /// Returns:
    ///     SwaptionVolatilityMatrix: The moving grid.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged grid, a mismatched shifts shape,
    ///         and on whatever the core rejects about the axes.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (calendar, business_day_convention, option_tenors, swap_tenors, volatilities, day_counter, volatility_type, settings, shifts = None, flat_extrapolation = false))]
    fn moving(
        py: Python<'_>,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        swap_tenors: Vec<PyRef<'_, PyPeriod>>,
        volatilities: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        settings: &PySettings,
        shifts: Option<Vec<Vec<f64>>>,
        flat_extrapolation: bool,
    ) -> PyResult<Py<Self>> {
        check_grid(&volatilities, "volatility")?;
        let volatilities: Vec<Vec<Handle<dyn Quote>>> = volatilities
            .iter()
            .map(|row| row.iter().map(|quote| quote.handle()).collect())
            .collect();
        let shifts = match shifts {
            Some(rows) => {
                check_grid(&rows, "shift")?;
                rows
            }
            None => Vec::new(),
        };
        let build = if flat_extrapolation {
            SwaptionVolatilityMatrix::moving_flat
        } else {
            SwaptionVolatilityMatrix::moving
        };
        let surface = shared(
            build(
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                tenors(&swap_tenors),
                volatilities,
                day_counter.inner(),
                volatility_type.inner(),
                shifts,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        ) as Shared<dyn SwaptionVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PySwaptionVolatilityMatrix),
        )
    }
}

/// A smile cube adding bilinearly-interpolated volatility spreads to an
/// at-the-money surface.
///
/// The inherited volatility query now takes a real strike: the cube reads the
/// at-the-money forward off its base swap indexes, the at-the-money volatility
/// off atm_vol, and adds the spread interpolated at strike - atm_strike.
///
/// vol_spreads is row-major over the (option tenor, swap tenor) nodes: row
/// i * len(swap_tenors) + j is the smile at (option_tenors[i], swap_tenors[j]),
/// holding one quote per entry of strike_spreads. A later set_value on any of
/// those quotes rebuilds the per-strike interpolators.
///
/// swap_index_base is the long base index and short_swap_index_base the short
/// one; the cube picks between them per query by swap tenor.
#[pyclass(name = "InterpolatedSwaptionVolatilityCube", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PyInterpolatedSwaptionVolatilityCube {
    concrete: Shared<InterpolatedSwaptionVolatilityCube>,
}

#[pymethods]
impl PyInterpolatedSwaptionVolatilityCube {
    /// Build the cube over an at-the-money surface and its vol spreads.
    ///
    /// Args:
    ///     atm_vol (SwaptionVolatilityStructure): The at-the-money surface the
    ///         spreads are added to.
    ///     option_tenors (list[Period]): The option axis of the node grid.
    ///     swap_tenors (list[Period]): The swap axis of the node grid.
    ///     strike_spreads (list[float]): The moneyness offsets each smile is
    ///         quoted at.
    ///     vol_spreads (list[list[SimpleQuote]]): The spread quotes, row-major
    ///         over the nodes with one quote per strike spread; a later
    ///         set_value rebuilds the per-strike interpolators.
    ///     swap_index_base (SwapIndex): The long base swap index.
    ///     short_swap_index_base (SwapIndex): The short base swap index, whose
    ///         tenor must not exceed the long one's.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     vega_weighted_smile_fit (bool): Whether the smile fit is
    ///         vega-weighted.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged vol_spreads grid, on a row count
    ///         that is not one per node or a row length that is not one per
    ///         strike spread, and on whatever the core rejects.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (atm_vol, option_tenors, swap_tenors, strike_spreads, vol_spreads, swap_index_base, short_swap_index_base, settings, vega_weighted_smile_fit = false))]
    fn new(
        atm_vol: &PySwaptionVolatilityStructure,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        swap_tenors: Vec<PyRef<'_, PyPeriod>>,
        strike_spreads: Vec<f64>,
        vol_spreads: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        swap_index_base: &PySwapIndex,
        short_swap_index_base: &PySwapIndex,
        settings: &PySettings,
        vega_weighted_smile_fit: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        let columns = check_grid(&vol_spreads, "vol spread")?;
        let nodes = option_tenors.len() * swap_tenors.len();
        if vol_spreads.len() != nodes {
            return Err(crate::ItofinError::new_err(format!(
                "vol spread grid must have one row per (option tenor, swap tenor) node: \
                 expected {nodes}, got {}",
                vol_spreads.len()
            )));
        }
        if columns != strike_spreads.len() {
            return Err(crate::ItofinError::new_err(format!(
                "vol spread rows must have one column per strike spread: expected {}, got {columns}",
                strike_spreads.len()
            )));
        }
        let vol_spreads: Vec<Vec<Handle<dyn Quote>>> = vol_spreads
            .iter()
            .map(|row| row.iter().map(|quote| quote.handle()).collect())
            .collect();
        let cube = shared(
            InterpolatedSwaptionVolatilityCube::new(
                atm_vol.handle(),
                tenors(&option_tenors),
                tenors(&swap_tenors),
                strike_spreads,
                vol_spreads,
                swap_index_base.inner(),
                short_swap_index_base.inner(),
                vega_weighted_smile_fit,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        let erased = Handle::new(Shared::clone(&cube) as Shared<dyn SwaptionVolatilityStructure>);
        Ok(
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(erased))
                .add_subclass(PyInterpolatedSwaptionVolatilityCube { concrete: cube }),
        )
    }

    /// Return the at-the-money strike for an option tenor and swap tenor.
    ///
    /// The fixing of whichever base swap index the swap tenor selects, off the
    /// option date the tenor resolves to against the cube's reference date and
    /// calendar. It lives on the concrete cube rather than the inherited base
    /// because it belongs to the cube framework, not the volatility structure.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor.
    ///     swap_tenor (Period): The underlying swap's tenor, which selects the
    ///         base index.
    ///
    /// Returns:
    ///     float: The at-the-money strike a query is centred on.
    ///
    /// Raises:
    ///     ItofinError: On whatever the selected index's fixing reports, an
    ///         unset evaluation date or an unlinked forwarding curve included.
    fn atm_strike_from_tenor(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
    ) -> PyResult<f64> {
        Ok(self
            .concrete
            .cube()
            .atm_strike_from_tenor(option_tenor.inner(), swap_tenor.inner())
            .map_err(PyQlError::from)?)
    }
}

/// A smile cube whose every node is a SABR smile fitted to the at-the-money
/// volatility plus the market vol spreads.
///
/// The inherited volatility query takes a real strike and answers off the fitted
/// smile rather than an interpolated spread. Construction is where the work
/// happens: every node is calibrated by Levenberg-Marquardt, and with
/// is_atm_calibrated a second dense pass re-anchors the fitted smiles on the
/// at-the-money surface.
///
/// vol_spreads and parameters_guess are both row-major over the (option tenor,
/// swap tenor) nodes: row i * len(swap_tenors) + j is the node at
/// (option_tenors[i], swap_tenors[j]). A vol_spreads row holds one quote per
/// entry of strike_spreads; a parameters_guess row holds the four SABR starting
/// values [alpha, beta, nu, rho]. is_parameter_fixed pins a parameter at its
/// guess across every node, in that same order.
///
/// The end criteria, maximum error tolerance, optimisation method and accepted
/// error are left at the core's C++ defaults. Backward-flat interpolation
/// (core #606) is not exposed, and the optimisation method is always
/// Levenberg-Marquardt, since a trait object does not cross FFI. ZABR and the
/// generic XABR cube are a separate core track (#597), and the section-
/// recalibration API is unported in the core: re-fit by bumping the guess or
/// vol-spread quotes.
#[pyclass(name = "SabrSwaptionVolatilityCube", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PySabrSwaptionVolatilityCube {
    concrete: Shared<SabrSwaptionVolatilityCube>,
}

#[pymethods]
impl PySabrSwaptionVolatilityCube {
    /// Build the cube, calibrating every node on construction.
    ///
    /// The end criteria, the maximum error tolerance, the optimisation method
    /// and the accepted error are left at the core's C++ defaults.
    ///
    /// Args:
    ///     atm_vol (SwaptionVolatilityStructure): The at-the-money surface the
    ///         fitted smiles are anchored on.
    ///     option_tenors (list[Period]): The option axis of the node grid.
    ///     swap_tenors (list[Period]): The swap axis of the node grid.
    ///     strike_spreads (list[float]): The moneyness offsets each smile is
    ///         quoted at.
    ///     vol_spreads (list[list[SimpleQuote]]): The spread quotes, row-major
    ///         over the nodes with one quote per strike spread.
    ///     swap_index_base (SwapIndex): The long base swap index.
    ///     short_swap_index_base (SwapIndex): The short base swap index.
    ///     parameters_guess (list[list[SimpleQuote]]): The SABR starting
    ///         values, row-major over the nodes, each row holding alpha, beta,
    ///         nu and rho in that order.
    ///     is_parameter_fixed (list[bool]): Which of the four parameters are
    ///         pinned at their guess across every node, in that same order.
    ///     is_atm_calibrated (bool): Whether a second dense pass re-anchors
    ///         the fitted smiles on the at-the-money surface.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     vega_weighted_smile_fit (bool): Whether the smile fit is
    ///         vega-weighted.
    ///     use_max_error (bool): Whether the fit is judged on the maximum
    ///         error rather than the aggregate one.
    ///     max_guesses (int): How many starting guesses a node may try.
    ///     cutoff_strike (float): The strike floor the fit is evaluated above.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged vol_spreads or parameters_guess
    ///         grid, on a row count that is not one per node, on an
    ///         is_parameter_fixed list that is not four entries long, on a
    ///         normal at-the-money surface, which needs the deferred normal
    ///         SABR formula, and on a calibration failure.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (atm_vol, option_tenors, swap_tenors, strike_spreads, vol_spreads, swap_index_base, short_swap_index_base, parameters_guess, is_parameter_fixed, is_atm_calibrated, settings, vega_weighted_smile_fit = false, use_max_error = false, max_guesses = 50, cutoff_strike = 0.0001))]
    fn new(
        atm_vol: &PySwaptionVolatilityStructure,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        swap_tenors: Vec<PyRef<'_, PyPeriod>>,
        strike_spreads: Vec<f64>,
        vol_spreads: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        swap_index_base: &PySwapIndex,
        short_swap_index_base: &PySwapIndex,
        parameters_guess: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        is_parameter_fixed: Vec<bool>,
        is_atm_calibrated: bool,
        settings: &PySettings,
        vega_weighted_smile_fit: bool,
        use_max_error: bool,
        max_guesses: usize,
        cutoff_strike: f64,
    ) -> PyResult<PyClassInitializer<Self>> {
        let nodes = option_tenors.len() * swap_tenors.len();
        let vol_spreads = node_grid(
            &vol_spreads,
            nodes,
            strike_spreads.len(),
            "vol spread",
            "strike spread",
        )?;
        let parameters_guess = node_grid(
            &parameters_guess,
            nodes,
            SABR_PARAMETERS,
            "parameters guess",
            "SABR parameter (alpha, beta, nu, rho)",
        )?;
        let flags = is_parameter_fixed.len();
        let is_parameter_fixed: [bool; SABR_PARAMETERS] =
            is_parameter_fixed.try_into().map_err(|_| {
                crate::ItofinError::new_err(format!(
                    "is_parameter_fixed must hold one flag per SABR parameter \
                     (alpha, beta, nu, rho): expected {SABR_PARAMETERS}, got {flags}"
                ))
            })?;
        let cube = shared(
            SabrSwaptionVolatilityCube::new(
                atm_vol.handle(),
                tenors(&option_tenors),
                tenors(&swap_tenors),
                strike_spreads,
                vol_spreads,
                swap_index_base.inner(),
                short_swap_index_base.inner(),
                vega_weighted_smile_fit,
                parameters_guess,
                is_parameter_fixed,
                is_atm_calibrated,
                None,
                None,
                None,
                None,
                use_max_error,
                max_guesses,
                false,
                cutoff_strike,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        let erased = Handle::new(Shared::clone(&cube) as Shared<dyn SwaptionVolatilityStructure>);
        Ok(
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(erased))
                .add_subclass(PySabrSwaptionVolatilityCube { concrete: cube }),
        )
    }

    /// Return the at-the-money strike for an option tenor and swap tenor.
    ///
    /// The fixing of whichever base swap index the swap tenor selects, and the
    /// strike the fitted smile is centred on, so it is what a caller needs to
    /// place a query at a given moneyness.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor.
    ///     swap_tenor (Period): The underlying swap's tenor, which selects the
    ///         base index.
    ///
    /// Returns:
    ///     float: The at-the-money strike.
    ///
    /// Raises:
    ///     ItofinError: On whatever the selected index's fixing reports, an
    ///         unset evaluation date or an unlinked forwarding curve included.
    fn atm_strike_from_tenor(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
    ) -> PyResult<f64> {
        Ok(self
            .concrete
            .cube()
            .atm_strike_from_tenor(option_tenor.inner(), swap_tenor.inner())
            .map_err(PyQlError::from)?)
    }
}

/// The wrapped core periods, in order.
fn tenors(periods: &[PyRef<'_, PyPeriod>]) -> Vec<Period> {
    periods.iter().map(|period| period.inner()).collect()
}

/// The shape check both grid constructors share, rejecting an empty or ragged
/// `list[list[...]]` before it reaches the core's dimension checks. Returns the
/// common row length.
fn check_grid<T>(rows: &[Vec<T>], label: &str) -> PyResult<usize> {
    let Some(first) = rows.first() else {
        return Err(crate::ItofinError::new_err(format!(
            "swaption {label} grid must have at least one row"
        )));
    };
    let columns = first.len();
    if columns == 0 {
        return Err(crate::ItofinError::new_err(format!(
            "swaption {label} grid rows must have at least one column"
        )));
    }
    if rows.iter().any(|row| row.len() != columns) {
        return Err(crate::ItofinError::new_err(format!(
            "swaption {label} grid rows must all have the same length"
        )));
    }
    Ok(columns)
}

/// A node-indexed quote grid as core handles, after checking it is `nodes` rows
/// of `columns`. The row count is the `(option tenor, swap tenor)` node count
/// and the column count is fixed by what the row holds, so both are checked
/// here, before the core's dimension errors.
fn node_grid(
    rows: &[Vec<PyRef<'_, PySimpleQuote>>],
    nodes: usize,
    columns: usize,
    label: &str,
    column_label: &str,
) -> PyResult<Vec<Vec<Handle<dyn Quote>>>> {
    let got = check_grid(rows, label)?;
    if rows.len() != nodes {
        return Err(crate::ItofinError::new_err(format!(
            "{label} grid must have one row per (option tenor, swap tenor) node: \
             expected {nodes}, got {}",
            rows.len()
        )));
    }
    if got != columns {
        return Err(crate::ItofinError::new_err(format!(
            "{label} rows must have one column per {column_label}: expected {columns}, got {got}"
        )));
    }
    Ok(rows
        .iter()
        .map(|row| row.iter().map(|quote| quote.handle()).collect())
        .collect())
}

/// A `list[list[float]]` as a core Matrix, one row per option tenor.
fn matrix_from_rows(rows: &[Vec<f64>], label: &str) -> PyResult<Matrix> {
    let columns = check_grid(rows, label)?;
    let mut matrix = Matrix::with_size(rows.len(), columns);
    for (i, row) in rows.iter().enumerate() {
        for (j, &value) in row.iter().enumerate() {
            matrix[(i, j)] = value;
        }
    }
    Ok(matrix)
}
