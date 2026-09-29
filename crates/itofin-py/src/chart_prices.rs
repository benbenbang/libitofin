//! Python facade for dated interval prices.

use crate::PyQlError;
use crate::time::PyDate;
use libitofin::prices::{IntervalPrice, PriceSeries};
use libitofin::time::date::Date;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// One validated OHLC bar and its calendar date.
#[gen_stub_pyclass]
#[pyclass(name = "DatedIntervalPrice", frozen, module = "itofin.chart")]
pub(crate) struct PyDatedIntervalPrice {
    date: Date,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl PyDatedIntervalPrice {
    fn from_core(date: Date, price: &IntervalPrice) -> Self {
        Self {
            date,
            open: price.open(),
            high: price.high(),
            low: price.low(),
            close: price.close(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyDatedIntervalPrice {
    /// Calendar date of this bar.
    #[getter]
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.date)
    }

    /// Opening price.
    #[getter]
    fn open(&self) -> f64 {
        self.open
    }

    /// Highest price.
    #[getter]
    fn high(&self) -> f64 {
        self.high
    }

    /// Lowest price.
    #[getter]
    fn low(&self) -> f64 {
        self.low
    }

    /// Closing price.
    #[getter]
    fn close(&self) -> f64 {
        self.close
    }
}

/// Build sorted dated OHLC bars; duplicate dates retain their last input bar.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn interval_prices(
    dates: Vec<PyRef<PyDate>>,
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
) -> PyResult<Vec<PyDatedIntervalPrice>> {
    let dates: Vec<Date> = dates.iter().map(|date| date.inner()).collect();
    PriceSeries::from_slices(&dates, &open, &high, &low, &close)
        .map(|series| {
            series
                .iter()
                .map(|(date, price)| PyDatedIntervalPrice::from_core(date, price))
                .collect()
        })
        .map_err(PyQlError::from)
        .map_err(Into::into)
}
