//! Facade for the market cap/floor TERM-volatility surface
//! [`PyCapFloorTermVolSurface`].
//!
//! This is a `CapFloorTermVolatilityStructure`, not an
//! [`OptionletVolatilityStructure`](crate::optionletvol::PyOptionletVolatilityStructure):
//! it quotes the flat volatility of a WHOLE cap by cap length, which is how the
//! market quotes caps, whereas an optionlet surface quotes the individual
//! caplets a cap decomposes into. It is therefore the optionlet stripper's
//! input, not its output, and it does not extend the optionlet base.
//!
//! It is exposed standalone rather than under a shared base: it is the only
//! implementor of its trait in the core, so a Python base class would have a
//! single subclass and no second surface to type against (the
//! `SabrSmileSection` precedent). A base can be introduced if a second one ever
//! lands.
//!
//! All four core constructors are exposed: a fixed reference date or one
//! floating `settlement_days` off the evaluation date, each over fixed
//! volatilities or over the caller's quotes. The moving pair is not optional
//! polish here - it is what the optionlet stripping pipeline runs on. The
//! adapter that serves the stripped surface reads its settlement days from this
//! surface (`strippedoptionletadapter.rs:87` through
//! `optionletstripper.rs:241`), and a fixed-reference term structure has none,
//! so a surface built by the fixed forms fails the adapter with `"settlement
//! days not provided for this instance"`.

use crate::PyQlError;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::math::matrix::Matrix;
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    CapFloorTermVolSurface, CapFloorTermVolatilityStructure,
};
use libitofin::time::period::Period;
use pyo3::prelude::*;

/// The market cap/floor TERM-volatility surface, bicubic over an option-tenor
/// x strike grid.
///
/// This is the flat volatility of a WHOLE cap by cap length, which is how the
/// market quotes caps, not the volatility of the individual caplets it
/// decomposes into: it is the optionlet stripper's input, and it is not an
/// OptionletVolatilityStructure.
///
/// volatilities is a row per option tenor and a column per strike; both axes
/// must be strictly increasing.
///
/// All four constructors are exposed. __init__ and with_quotes pin the
/// reference date, so every query's option time runs from reference_date rather
/// than the evaluation date. moving and moving_with_quotes float it
/// settlement_days off the evaluation date, and are what the optionlet
/// stripping pipeline runs on: StrippedOptionletAdapter reads its settlement
/// days back off this surface, and a pinned-reference surface has none.
#[pyclass(name = "CapFloorTermVolSurface", unsendable)]
pub struct PyCapFloorTermVolSurface {
    inner: Shared<CapFloorTermVolSurface>,
}

#[pymethods]
impl PyCapFloorTermVolSurface {
    /// Build the surface on a pinned reference date over fixed volatilities.
    ///
    /// Every query's option time runs from reference_date, not from the
    /// evaluation date, and no later mutation can reach the grid.
    ///
    /// Args:
    ///     reference_date (Date): The date option times run from.
    ///     calendar (Calendar): The calendar cap tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The cap-length axis, one per grid
    ///         row; strictly increasing.
    ///     strikes (list[float]): The strike axis, one per grid column;
    ///         strictly increasing.
    ///     volatilities (list[list[float]]): The flat cap volatilities, one
    ///         row per option tenor.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged grid, on a grid whose shape does
    ///         not match the tenors and strikes, and on a non-increasing tenor
    ///         or strike axis.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, option_tenors, strikes, volatilities, day_counter))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        strikes: Vec<f64>,
        volatilities: Vec<Vec<f64>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<Self> {
        let volatilities = matrix_from_rows(&volatilities)?;
        let inner = shared(
            CapFloorTermVolSurface::with_reference_date_from_matrix(
                reference_date.inner(),
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                strikes,
                &volatilities,
                day_counter.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyCapFloorTermVolSurface { inner })
    }

    /// Build the pinned-reference surface over live quotes.
    ///
    /// Args:
    ///     reference_date (Date): The date option times run from.
    ///     calendar (Calendar): The calendar cap tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The cap-length axis, strictly
    ///         increasing.
    ///     strikes (list[float]): The strike axis, strictly increasing.
    ///     volatilities (list[list[SimpleQuote]]): The volatility quotes, one
    ///         row per option tenor; a later set_value rebuilds the
    ///         interpolation and notifies the surface's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///
    /// Returns:
    ///     CapFloorTermVolSurface: The surface over those quotes.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions __init__ reports.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, option_tenors, strikes, volatilities, day_counter))]
    fn with_quotes(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        strikes: Vec<f64>,
        volatilities: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<Self> {
        check_grid(&volatilities)?;
        let volatilities: Vec<Vec<Handle<dyn Quote>>> = volatilities
            .iter()
            .map(|row| row.iter().map(|quote| quote.handle()).collect())
            .collect();
        let inner = shared(
            CapFloorTermVolSurface::with_reference_date(
                reference_date.inner(),
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                strikes,
                volatilities,
                day_counter.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyCapFloorTermVolSurface { inner })
    }

    /// Build a surface whose reference date floats off the evaluation date.
    ///
    /// This is the form the optionlet stripping pipeline needs: unlike the
    /// pinned-reference constructors, it carries the settlement days
    /// StrippedOptionletAdapter reads back off the stripper.
    ///
    /// Args:
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar cap tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The cap-length axis, strictly
    ///         increasing.
    ///     strikes (list[float]): The strike axis, strictly increasing.
    ///     volatilities (list[list[float]]): The flat cap volatilities, one
    ///         row per option tenor.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date floats off.
    ///
    /// Returns:
    ///     CapFloorTermVolSurface: The floating-reference surface.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions __init__ reports.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, option_tenors, strikes, volatilities, day_counter, settings))]
    fn moving(
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        strikes: Vec<f64>,
        volatilities: Vec<Vec<f64>>,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let volatilities = matrix_from_rows(&volatilities)?;
        let inner = shared(
            CapFloorTermVolSurface::moving_from_matrix(
                settlement_days,
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                strikes,
                &volatilities,
                day_counter.inner(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyCapFloorTermVolSurface { inner })
    }

    /// Build the floating-reference surface over live quotes.
    ///
    /// Args:
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar cap tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     option_tenors (list[Period]): The cap-length axis, strictly
    ///         increasing.
    ///     strikes (list[float]): The strike axis, strictly increasing.
    ///     volatilities (list[list[SimpleQuote]]): The volatility quotes, one
    ///         row per option tenor.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date floats off.
    ///
    /// Returns:
    ///     CapFloorTermVolSurface: The floating-reference surface over those
    ///         quotes.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions __init__ reports.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, option_tenors, strikes, volatilities, day_counter, settings))]
    fn moving_with_quotes(
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        option_tenors: Vec<PyRef<'_, PyPeriod>>,
        strikes: Vec<f64>,
        volatilities: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Self> {
        check_grid(&volatilities)?;
        let volatilities: Vec<Vec<Handle<dyn Quote>>> = volatilities
            .iter()
            .map(|row| row.iter().map(|quote| quote.handle()).collect())
            .collect();
        let inner = shared(
            CapFloorTermVolSurface::moving(
                settlement_days,
                calendar.inner(),
                business_day_convention.inner(),
                tenors(&option_tenors),
                strikes,
                volatilities,
                day_counter.inner(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyCapFloorTermVolSurface { inner })
    }

    /// Return the flat cap volatility for a cap tenor and strike.
    ///
    /// The tenor form resolves against the surface's own calendar and
    /// business-day convention, so it is the one to reach for unless a date is
    /// already in hand.
    ///
    /// Args:
    ///     option_tenor (Period): The cap's length.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The flat cap volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn volatility(&self, option_tenor: &PyPeriod, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the flat cap volatility for a cap end date and strike.
    ///
    /// Args:
    ///     end_date (Date): The cap's end date.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The flat cap volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (end_date, strike, extrapolate = false))]
    fn volatility_date(&self, end_date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility_date(end_date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the flat cap volatility for a cap end time and strike.
    ///
    /// Args:
    ///     length (float): A year fraction off the reference date, in the
    ///         surface's own day count.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The flat cap volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (length, strike, extrapolate = false))]
    fn volatility_time(&self, length: f64, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility_time(length, strike, extrapolate)
            .map_err(PyQlError::from)?)
    }
}

impl PyCapFloorTermVolSurface {
    /// The wrapped core surface for the optionlet-stripper facade, which takes
    /// the concrete type rather than a handle.
    pub(crate) fn inner(&self) -> Shared<CapFloorTermVolSurface> {
        Shared::clone(&self.inner)
    }
}

fn tenors(periods: &[PyRef<'_, PyPeriod>]) -> Vec<Period> {
    periods.iter().map(|period| period.inner()).collect()
}

/// Rejects an empty or ragged `list[list[...]]` before it reaches the core's
/// dimension checks. Returns the common row length.
fn check_grid<T>(rows: &[Vec<T>]) -> PyResult<usize> {
    let Some(first) = rows.first() else {
        return Err(crate::ItofinError::new_err(
            "cap/floor term volatility grid must have at least one row",
        ));
    };
    let columns = first.len();
    if columns == 0 {
        return Err(crate::ItofinError::new_err(
            "cap/floor term volatility grid rows must have at least one column",
        ));
    }
    if rows.iter().any(|row| row.len() != columns) {
        return Err(crate::ItofinError::new_err(
            "cap/floor term volatility grid rows must all have the same length",
        ));
    }
    Ok(columns)
}

/// A `list[list[float]]` as a core [`Matrix`], one row per option tenor.
pub(crate) fn matrix_from_rows(rows: &[Vec<f64>]) -> PyResult<Matrix> {
    let columns = check_grid(rows)?;
    let mut matrix = Matrix::with_size(rows.len(), columns);
    for (i, row) in rows.iter().enumerate() {
        for (j, &value) in row.iter().enumerate() {
            matrix[(i, j)] = value;
        }
    }
    Ok(matrix)
}
