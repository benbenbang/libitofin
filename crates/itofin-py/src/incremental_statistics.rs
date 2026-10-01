//! Python facade for bounded-memory weighted statistics.

use crate::PyQlError;
use libitofin::errors::{QlError, QlResult};
use libitofin::math::statistics::{IncrementalStatistics, MeanStdDev, Statistics};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

fn invalid(message: &str) -> PyErr {
    PyQlError::from(QlError::new(message, file!(), line!())).into()
}

fn sample(value: f64, weight: f64) -> PyResult<()> {
    if !value.is_finite() || !weight.is_finite() || weight < 0.0 {
        return Err(invalid(
            "statistics values and nonnegative weights must be finite",
        ));
    }
    Ok(())
}

fn finite(value: QlResult<f64>) -> PyResult<f64> {
    let value = value.map_err(PyQlError::from)?;
    if !value.is_finite() {
        return Err(invalid("statistics result is nonfinite"));
    }
    Ok(value)
}

/// Weighted streaming moments with fixed memory use, independent of sample count.
#[gen_stub_pyclass]
#[pyclass(
    name = "IncrementalStatistics",
    unsendable,
    module = "itofin.statistics"
)]
pub(crate) struct PyIncrementalStatistics {
    inner: IncrementalStatistics,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyIncrementalStatistics {
    /// Create an empty accumulator.
    #[new]
    fn new() -> Self {
        Self {
            inner: IncrementalStatistics::new(),
        }
    }

    /// Append one finite observation and finite nonnegative weight.
    #[pyo3(signature = (value, weight = 1.0))]
    fn add(&mut self, value: f64, weight: f64) -> PyResult<()> {
        sample(value, weight)?;
        let mut next = self.inner.clone();
        next.add_weighted(value, weight).map_err(PyQlError::from)?;
        if !next.has_finite_state() {
            return Err(invalid("statistics accumulator state is nonfinite"));
        }
        self.inner = next;
        Ok(())
    }

    /// Append a batch atomically; omitted weights give unit weights.
    #[pyo3(signature = (values, *, weights = None))]
    fn add_batch(&mut self, values: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<()> {
        if values.is_empty() {
            return Err(invalid("statistics batch must not be empty"));
        }
        if weights
            .as_ref()
            .is_some_and(|weights| weights.len() != values.len())
        {
            return Err(invalid("observation and weight lengths differ"));
        }
        for (index, &value) in values.iter().enumerate() {
            sample(
                value,
                weights.as_ref().map_or(1.0, |weights| weights[index]),
            )?;
        }
        let mut next = self.inner.clone();
        for (index, value) in values.into_iter().enumerate() {
            next.add_weighted(
                value,
                weights.as_ref().map_or(1.0, |weights| weights[index]),
            )
            .map_err(PyQlError::from)?;
            if !next.has_finite_state() {
                return Err(invalid("statistics accumulator state is nonfinite"));
            }
        }
        self.inner = next;
        Ok(())
    }

    /// Clear all observations while retaining this object.
    fn reset(&mut self) {
        self.inner.reset();
    }

    /// Number of observations, including zero-weight observations.
    fn samples(&self) -> usize {
        self.inner.samples()
    }

    /// Number of strictly negative observations.
    fn downside_samples(&self) -> usize {
        self.inner.downside_samples()
    }

    /// Sum of all observation weights.
    fn weight_sum(&self) -> PyResult<f64> {
        finite(Ok(self.inner.weight_sum()))
    }

    /// Sum of weights on strictly negative observations.
    fn downside_weight_sum(&self) -> PyResult<f64> {
        finite(Ok(self.inner.downside_weight_sum()))
    }

    /// Lowest observation.
    fn min(&self) -> PyResult<f64> {
        finite(self.inner.min())
    }

    /// Highest observation.
    fn max(&self) -> PyResult<f64> {
        finite(self.inner.max())
    }

    /// Weighted arithmetic mean.
    fn mean(&self) -> PyResult<f64> {
        finite(self.inner.mean())
    }

    /// Weighted variance corrected by the sample count N/(N-1).
    fn variance(&self) -> PyResult<f64> {
        finite(self.inner.variance())
    }

    /// Square root of the corrected variance.
    fn standard_deviation(&self) -> PyResult<f64> {
        finite(self.inner.standard_deviation())
    }

    /// Standard error of the mean.
    fn error_estimate(&self) -> PyResult<f64> {
        finite(self.inner.error_estimate())
    }

    /// Bias-corrected weighted skewness.
    fn skewness(&self) -> PyResult<f64> {
        finite(self.inner.skewness())
    }

    /// Bias-corrected excess kurtosis.
    fn kurtosis(&self) -> PyResult<f64> {
        finite(self.inner.kurtosis())
    }

    /// Corrected second moment of strictly negative observations.
    fn downside_variance(&self) -> PyResult<f64> {
        finite(self.inner.downside_variance())
    }

    /// Square root of the downside variance.
    fn downside_deviation(&self) -> PyResult<f64> {
        finite(self.inner.downside_deviation())
    }
}
