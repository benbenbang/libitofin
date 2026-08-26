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

/// Python `CapFloorTermVolSurface`: the market cap/floor term-volatility
/// surface, bicubic over an option-tenor x strike grid
/// (`termstructures::volatility::CapFloorTermVolSurface`).
///
/// The grid is a row per option tenor and a column per strike, both strictly
/// increasing. The three query forms address the same surface by cap tenor, by
/// cap end date and by cap end time; the tenor form resolves against the
/// surface's own calendar and business-day convention, so it is the one to
/// reach for unless a date is already in hand.
#[pyclass(name = "CapFloorTermVolSurface", unsendable)]
pub struct PyCapFloorTermVolSurface {
    inner: Shared<CapFloorTermVolSurface>,
}

#[pymethods]
impl PyCapFloorTermVolSurface {
    /// A surface on a pinned reference date over fixed volatilities: every
    /// query's option time runs from `reference_date`, not from the evaluation
    /// date, and no later mutation can reach the grid.
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

    /// The same pinned-reference surface reading each node from the caller's
    /// quote: a later `set_value` on any of them rebuilds the interpolation and
    /// notifies the surface's observers.
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

    /// A surface whose reference date floats `settlement_days` business days
    /// off the evaluation date, over fixed volatilities.
    ///
    /// This is the form the optionlet stripping pipeline needs: unlike the
    /// pinned-reference constructors, it carries the settlement days
    /// `StrippedOptionletAdapter` reads back off the stripper.
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

    /// The same floating-reference surface reading each node from the caller's
    /// quote.
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

    /// The cap volatility for a cap tenor and strike.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn volatility(&self, option_tenor: &PyPeriod, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The cap volatility for a cap end date and strike.
    #[pyo3(signature = (end_date, strike, extrapolate = false))]
    fn volatility_date(&self, end_date: &PyDate, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility_date(end_date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The cap volatility for a cap end time (a year fraction off the reference
    /// date, in the surface's own day count) and strike.
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
