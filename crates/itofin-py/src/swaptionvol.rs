//! Facades for the swaption volatility stack: the [`PySwaptionVolatilityStructure`]
//! base, the [`PyVolatilityType`] flag, the constant surface
//! [`PyConstantSwaptionVolatility`], the at-the-money grid
//! [`PySwaptionVolatilityMatrix`], the spread cube over it,
//! [`PyInterpolatedSwaptionVolatilityCube`], and the calibrated
//! [`PySabrSwaptionVolatilityCube`].
//!
//! The base holds the erased `Handle<dyn SwaptionVolatilityStructure>` and
//! exposes the queries every concrete surface inherits; concrete surfaces
//! subclass it and supply only their constructor. They build the base through
//! [`from_handle`](PySwaptionVolatilityStructure::from_handle) rather than a
//! struct literal, so the later surfaces in this file (and the matrix/cube
//! facades stacking on it) never need access to the private field.
//!
//! Deferred (visible): the MOVING `ConstantSwaptionVolatility` constructors
//! (`moving` / `moving_with_quote`, whose reference date floats off the
//! evaluation date) are not exposed; only the fixed-reference-date `new` and
//! `with_quote` are. `BlackSwaptionEngine.with_flat_vol` builds a moving
//! surface internally, but that is the engine's business, not this facade's.

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

/// Python `SwaptionVolatilityStructure`: the shared base for every swaption
/// volatility surface (`termstructures::volatility::SwaptionVolatilityStructure`).
///
/// The option and swap axes are addressed by tenor, the form the surfaces are
/// quoted in; the core resolves each tenor against the surface's reference date
/// and calendar before reading the volatility.
#[pyclass(name = "SwaptionVolatilityStructure", subclass, unsendable)]
pub struct PySwaptionVolatilityStructure {
    inner: Handle<dyn SwaptionVolatilityStructure>,
}

#[pymethods]
impl PySwaptionVolatilityStructure {
    /// The volatility for an option tenor, swap tenor and strike.
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

    /// The Black variance (`vol^2 * option_time`) for an option tenor, swap
    /// tenor and strike.
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

    /// The lognormal shift for an option date and swap length in years.
    ///
    /// Taken in the date form because the core trait has no tenor overload for
    /// the shift (only `shift` and `shift_time`), unlike the volatility and
    /// variance queries above. Errors on a normal-volatility surface, where a
    /// shift has no meaning.
    #[pyo3(signature = (option_date, swap_length, extrapolate = false))]
    fn shift(&self, option_date: &PyDate, swap_length: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .shift(option_date.inner(), swap_length, extrapolate)
            .map_err(PyQlError::from)?)
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

/// Python `VolatilityType`: whether a surface quotes shifted-lognormal (Black)
/// or normal (Bachelier) volatilities
/// (`termstructures::volatility::VolatilityType`).
///
/// A fieldless pyo3 enum. The engine checks the surface it is handed against
/// its own formula and errors at pricing time on a mismatch, so a `Normal`
/// surface fed to `BlackSwaptionEngine` surfaces as an `ItofinError` from
/// `npv()`, not from the constructor.
#[pyclass(name = "VolatilityType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyVolatilityType {
    ShiftedLognormal,
    Normal,
}

impl PyVolatilityType {
    /// The core [`VolatilityType`] this variant stands for.
    pub(crate) fn inner(&self) -> VolatilityType {
        match self {
            PyVolatilityType::ShiftedLognormal => VolatilityType::ShiftedLognormal,
            PyVolatilityType::Normal => VolatilityType::Normal,
        }
    }
}

/// Python `ConstantSwaptionVolatility`: a single volatility with no option-time,
/// swap-length or strike dependence
/// (`termstructures::volatility::ConstantSwaptionVolatility`).
///
/// Extends [`PySwaptionVolatilityStructure`] and supplies only the constructors;
/// the query surface is inherited. Unbounded in time and strike, so queries
/// never need extrapolation enabled. Both forms pin the reference date, so the
/// option time every query measures runs from `reference_date`, not from the
/// evaluation date.
#[pyclass(name = "ConstantSwaptionVolatility", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PyConstantSwaptionVolatility;

#[pymethods]
impl PyConstantSwaptionVolatility {
    /// A constant surface at a fixed `volatility`, wrapped in an internal quote
    /// the caller cannot later mutate.
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

    /// A constant surface reading `volatility` from the caller's quote; a later
    /// `set_value` on that quote notifies the surface's observers.
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
}

/// Python `SwaptionVolatilityMatrix`: the at-the-money volatility grid,
/// bilinear over an option-tenor x swap-tenor lattice
/// (`termstructures::volatility::SwaptionVolatilityMatrix`).
///
/// Extends [`PySwaptionVolatilityStructure`] and supplies only the
/// constructors. Every grid is a row per option tenor and a column per swap
/// tenor; `shifts`, when given, must match that shape, and `None` means
/// all-zero shifts. The grid is at the money, so a query's strike argument is
/// range-checked and then ignored. `flat_extrapolation` selects C++'s
/// `flatExtrapolation = true`, under which a query past the grid clamps to the
/// nearest edge or corner vol instead of extending the boundary surface.
#[pyclass(name = "SwaptionVolatilityMatrix", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PySwaptionVolatilityMatrix;

#[pymethods]
impl PySwaptionVolatilityMatrix {
    /// A grid on a pinned reference date over fixed volatilities: every query's
    /// option time runs from `reference_date`, not from the evaluation date.
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

    /// A grid whose reference date floats off `settings`' evaluation date (zero
    /// settlement days), reading each node from the caller's quote: a later
    /// `set_value` on any of them rebuilds the interpolation and notifies the
    /// grid's observers.
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

/// Python `InterpolatedSwaptionVolatilityCube`: a smile cube adding
/// bilinearly-interpolated volatility spreads to an at-the-money surface
/// (`termstructures::volatility::InterpolatedSwaptionVolatilityCube`).
///
/// Extends [`PySwaptionVolatilityStructure`], so the inherited `volatility`
/// query now takes a real strike: the cube reads the at-the-money forward off
/// its base swap indexes, the at-the-money volatility off `atm_vol`, and adds
/// the spread interpolated at `strike - atm_strike`. `atm_strike` is served by
/// [`atm_strike_from_tenor`](PyInterpolatedSwaptionVolatilityCube::atm_strike_from_tenor),
/// which is what a caller needs to place a query on the smile.
///
/// `vol_spreads` is **row-major over the `(option tenor, swap tenor)` nodes**:
/// row `i * len(swap_tenors) + j` is the smile at `(option_tenors[i],
/// swap_tenors[j])`, holding one quote per entry of `strike_spreads`. The shape
/// is checked here rather than left to the core's dimension error. A later
/// `set_value` on any of those quotes rebuilds the per-strike interpolators.
///
/// `swap_index_base` is the long base index and `short_swap_index_base` the
/// short one; the cube picks between them per query by swap tenor, so the short
/// index's tenor must not exceed the long one's.
#[pyclass(name = "InterpolatedSwaptionVolatilityCube", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PyInterpolatedSwaptionVolatilityCube {
    concrete: Shared<InterpolatedSwaptionVolatilityCube>,
}

#[pymethods]
impl PyInterpolatedSwaptionVolatilityCube {
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

    /// The at-the-money strike for an option tenor and swap tenor: the fixing of
    /// whichever base swap index the swap tenor selects, off the option date the
    /// tenor resolves to against the cube's reference date and calendar.
    ///
    /// Lives on the concrete cube rather than the inherited base because it is
    /// the cube framework's, not the volatility structure trait's.
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

/// Python `SabrSwaptionVolatilityCube`: a smile cube whose every node is a SABR
/// smile fitted to the at-the-money volatility plus the market vol spreads
/// (`termstructures::volatility::SabrSwaptionVolatilityCube`).
///
/// Extends [`PySwaptionVolatilityStructure`], so the inherited `volatility`
/// query takes a real strike and answers off the fitted smile rather than an
/// interpolated spread. Construction is where the work happens: every node is
/// calibrated by Levenberg-Marquardt, and with `is_atm_calibrated` a second
/// dense pass re-anchors the fitted smiles on the at-the-money surface.
///
/// `vol_spreads` and `parameters_guess` are both **row-major over the
/// `(option tenor, swap tenor)` nodes**, as in
/// [`PyInterpolatedSwaptionVolatilityCube`]: row `i * len(swap_tenors) + j` is
/// the node at `(option_tenors[i], swap_tenors[j])`. A `vol_spreads` row holds
/// one quote per entry of `strike_spreads`; a `parameters_guess` row holds the
/// four SABR starting values `[alpha, beta, nu, rho]`. `is_parameter_fixed` pins
/// a parameter at its guess across every node, in that same order.
///
/// Deferred (visible): backward-flat interpolation is not ported (core #606) and
/// is not exposed; the optimisation method is always the core's default
/// Levenberg-Marquardt, since a trait object does not cross FFI (D7); a normal
/// (Bachelier) at-the-money surface needs the normal SABR formula, deferred to
/// core #586, and surfaces as an `ItofinError` from the constructor.
#[pyclass(name = "SabrSwaptionVolatilityCube", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PySabrSwaptionVolatilityCube {
    concrete: Shared<SabrSwaptionVolatilityCube>,
}

#[pymethods]
impl PySabrSwaptionVolatilityCube {
    /// A cube calibrating every node on construction. `end_criteria`, the
    /// maximum error tolerance, the optimisation method and the accepted error
    /// are left at the core's C++ defaults.
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

    /// The at-the-money strike for an option tenor and swap tenor: the fixing of
    /// whichever base swap index the swap tenor selects, off the option date the
    /// tenor resolves to against the cube's reference date and calendar.
    ///
    /// This is the strike the fitted smile is centred on, so it is what a caller
    /// needs to place a query at a given moneyness.
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

/// A `list[list[float]]` as a core [`Matrix`], one row per option tenor.
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
