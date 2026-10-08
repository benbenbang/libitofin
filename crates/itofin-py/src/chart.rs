//! Python chart indicators backed by the shared Rust calculations.

use crate::PyQlError;
use libitofin::math::chart::{self, BollingerBands, ChartSeries, Kd, Macd, VolumeBars};
use libitofin::math::volatility;
use numpy::PyArray1;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Values aligned with input bars. Entries before `first_valid` are warmup slots.
#[gen_stub_pyclass]
#[pyclass(
    name = "ChartSeries",
    frozen,
    skip_from_py_object,
    module = "itofin.chart"
)]
#[derive(Clone)]
pub(crate) struct PyChartSeries {
    inner: ChartSeries,
}

impl PyChartSeries {
    pub(crate) fn from_core(inner: ChartSeries) -> Self {
        Self { inner }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyChartSeries {
    /// A float64 copy of all bars; warmup slots contain zero placeholders.
    #[getter]
    fn values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_slice(py, &self.inner.values)
    }

    /// Index of the first usable result, or the series length if none is usable.
    #[getter]
    fn first_valid(&self) -> usize {
        self.inner.first_valid
    }

    /// Copy values with `None` in warmup slots for JSON or chart libraries.
    fn to_list(&self) -> Vec<Option<f64>> {
        self.inner
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| (index >= self.inner.first_valid).then_some(*value))
            .collect()
    }
}

/// Raw volume and the per-bar direction relative to the open.
#[gen_stub_pyclass]
#[pyclass(name = "VolumeBars", frozen, module = "itofin.chart")]
pub(crate) struct PyVolumeBars {
    volume: PyChartSeries,
    direction: Vec<i8>,
}

impl From<VolumeBars> for PyVolumeBars {
    fn from(bars: VolumeBars) -> Self {
        Self {
            volume: PyChartSeries::from_core(bars.volume),
            direction: bars.direction,
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyVolumeBars {
    /// Raw nonnegative volume, aligned with input bars.
    #[getter]
    fn volume(&self) -> PyChartSeries {
        self.volume.clone()
    }

    /// Per-bar direction: -1 for down, 0 for flat, 1 for up.
    #[getter]
    fn direction<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<i8>> {
        PyArray1::from_slice(py, &self.direction)
    }
}

/// Bollinger middle, upper, and lower bands aligned with input closes.
#[gen_stub_pyclass]
#[pyclass(name = "BollingerBands", frozen, module = "itofin.chart")]
pub(crate) struct PyBollingerBands {
    middle: PyChartSeries,
    upper: PyChartSeries,
    lower: PyChartSeries,
}

impl From<BollingerBands> for PyBollingerBands {
    fn from(bands: BollingerBands) -> Self {
        Self {
            middle: PyChartSeries::from_core(bands.middle),
            upper: PyChartSeries::from_core(bands.upper),
            lower: PyChartSeries::from_core(bands.lower),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBollingerBands {
    /// Rolling arithmetic mean.
    #[getter]
    fn middle(&self) -> PyChartSeries {
        self.middle.clone()
    }

    /// Middle plus population standard deviation times the multiplier.
    #[getter]
    fn upper(&self) -> PyChartSeries {
        self.upper.clone()
    }

    /// Middle minus population standard deviation times the multiplier.
    #[getter]
    fn lower(&self) -> PyChartSeries {
        self.lower.clone()
    }
}

/// Taiwan stochastic oscillator lines aligned with input bars.
#[gen_stub_pyclass]
#[pyclass(name = "Kd", frozen, module = "itofin.chart")]
pub(crate) struct PyKd {
    rsv: PyChartSeries,
    k: PyChartSeries,
    d: PyChartSeries,
}

impl From<Kd> for PyKd {
    fn from(values: Kd) -> Self {
        Self {
            rsv: PyChartSeries::from_core(values.rsv),
            k: PyChartSeries::from_core(values.k),
            d: PyChartSeries::from_core(values.d),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyKd {
    /// Raw stochastic value, with a flat range set to 50.
    #[getter]
    fn rsv(&self) -> PyChartSeries {
        self.rsv.clone()
    }

    /// Smoothed K line, seeded at 50.
    #[getter]
    fn k(&self) -> PyChartSeries {
        self.k.clone()
    }

    /// Smoothed D line, seeded at 50.
    #[getter]
    fn d(&self) -> PyChartSeries {
        self.d.clone()
    }
}

/// MACD line, signal, and histogram aligned with input closes.
#[gen_stub_pyclass]
#[pyclass(name = "Macd", frozen, module = "itofin.chart")]
pub(crate) struct PyMacd {
    line: PyChartSeries,
    signal: PyChartSeries,
    histogram: PyChartSeries,
}

impl From<Macd> for PyMacd {
    fn from(values: Macd) -> Self {
        Self {
            line: PyChartSeries::from_core(values.line),
            signal: PyChartSeries::from_core(values.signal),
            histogram: PyChartSeries::from_core(values.histogram),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMacd {
    /// Difference between SMA-seeded fast and slow EMAs.
    #[getter]
    fn line(&self) -> PyChartSeries {
        self.line.clone()
    }

    /// SMA-seeded EMA of valid MACD line values.
    #[getter]
    fn signal(&self) -> PyChartSeries {
        self.signal.clone()
    }

    /// Line minus signal.
    #[getter]
    fn histogram(&self) -> PyChartSeries {
        self.histogram.clone()
    }
}

/// Simple moving average, seeded after `period` closing prices.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn sma(close: Vec<f64>, period: usize) -> PyResult<PyChartSeries> {
    chart::sma(&close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Exponential moving average seeded by the first `period`-bar SMA.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ema(close: Vec<f64>, period: usize) -> PyResult<PyChartSeries> {
    chart::ema(&close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Cumulative VWAP of supplied prices; a separate call starts a new session.
/// Zero-volume prefixes are missing; later zero volume carries the last value.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn vwap(price: Vec<f64>, volume: Vec<f64>) -> PyResult<PyChartSeries> {
    chart::vwap(&price, &volume)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Zero-seeded OBV; rises add volume, falls subtract it, equal closes preserve it.
/// The initial volume is validated but does not contribute to the zero seed.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn obv(close: Vec<f64>, volume: Vec<f64>) -> PyResult<PyChartSeries> {
    chart::obv(&close, &volume)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Validate OHLCV bars and return raw volume with close-versus-open direction.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn volume_bars(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    volume: Vec<f64>,
) -> PyResult<PyVolumeBars> {
    chart::volume_bars(&open, &high, &low, &close, &volume)
        .map(PyVolumeBars::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Bollinger bands using a population standard deviation over each window.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (close, period = 20, multiplier = 2.0))]
pub(crate) fn bollinger_bands(
    close: Vec<f64>,
    period: usize,
    multiplier: f64,
) -> PyResult<PyBollingerBands> {
    chart::bollinger_bands(&close, period, multiplier)
        .map(PyBollingerBands::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Wilder RSI, seeded after `period` closing-price changes.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (close, period = 14))]
pub(crate) fn rsi(close: Vec<f64>, period: usize) -> PyResult<PyChartSeries> {
    chart::rsi(&close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Taiwan KD with RSV and recursive K/D smoothing seeded at 50.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (high, low, close, period = 9, k_smooth = 3, d_smooth = 3))]
pub(crate) fn kd(
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
    k_smooth: usize,
    d_smooth: usize,
) -> PyResult<PyKd> {
    chart::kd(&high, &low, &close, period, k_smooth, d_smooth)
        .map(PyKd::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// MACD with SMA-seeded fast, slow, and signal exponential averages.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (close, fast_period = 12, slow_period = 26, signal_period = 9))]
pub(crate) fn macd(
    close: Vec<f64>,
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> PyResult<PyMacd> {
    chart::macd(&close, fast_period, slow_period, signal_period)
        .map(PyMacd::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Absolute log returns annualized by each bar's year fraction.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn simple_local_volatility(
    close: Vec<f64>,
    year_fractions: Vec<f64>,
) -> PyResult<PyChartSeries> {
    volatility::simple_local_volatility(&close, &year_fractions)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Absolute log returns annualized by one year fraction for every bar.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn simple_local_volatility_constant_fraction(
    close: Vec<f64>,
    year_fraction: f64,
) -> PyResult<PyChartSeries> {
    volatility::simple_local_volatility_constant_fraction(&close, year_fraction)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Rolling volatility from preceding valid local values, excluding the current bar.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn constant_volatility(
    input: PyRef<PyChartSeries>,
    window: i64,
) -> PyResult<PyChartSeries> {
    let window = usize::try_from(window).unwrap_or(0);
    volatility::constant_volatility(&input.inner, window)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_converts_to_missing_without_changing_valid_zero() {
        let series = PyChartSeries::from_core(ChartSeries {
            values: vec![0.0, 0.0, 2.0, 0.0],
            first_valid: 2,
        });
        assert_eq!(series.to_list(), vec![None, None, Some(2.0), Some(0.0)]);
    }

    #[test]
    fn invalid_period_maps_to_python_error() {
        assert!(sma(vec![1.0], 0).is_err());
        assert!(ema(vec![1.0], 0).is_err());
    }
}
