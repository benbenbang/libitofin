//! Python facade for stored-sample empirical statistics.

use crate::PyQlError;
use libitofin::math::statistics::{
    EmpiricalStatistics, GeneralStatistics, MeanStdDev, RiskStatistics, Statistics,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

fn check(value: f64, weight: f64) -> PyResult<()> {
    if !value.is_finite() || !weight.is_finite() || weight < 0.0 {
        return Err(PyValueError::new_err(
            "observations must be finite and weights finite and nonnegative",
        ));
    }
    Ok(())
}

fn finite(value: libitofin::errors::QlResult<f64>) -> PyResult<f64> {
    let value = value.map_err(PyQlError::from)?;
    if !value.is_finite() {
        return Err(PyValueError::new_err("statistic result is nonfinite"));
    }
    Ok(value)
}

fn positive_tail(state: &GeneralStatistics, target: f64) -> PyResult<()> {
    if !state
        .data()
        .iter()
        .any(|&(value, weight)| value < target && weight > 0.0)
    {
        return Err(PyValueError::new_err(
            "no positive-weight data below the target",
        ));
    }
    Ok(())
}

/// Stores weighted observations and exposes empirical moments and risk measures.
/// A zero-weight observation counts toward count-based moment corrections.
#[gen_stub_pyclass]
#[pyclass(name = "GeneralStatistics", unsendable, module = "itofin.statistics")]
pub(crate) struct PyGeneralStatistics {
    inner: GeneralStatistics,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGeneralStatistics {
    /// Create an empty stored-sample accumulator.
    #[new]
    fn new() -> Self {
        Self {
            inner: GeneralStatistics::new(),
        }
    }

    /// Add one finite observation with a nonnegative weight.
    #[pyo3(signature = (value, weight = 1.0))]
    fn add(&mut self, value: f64, weight: f64) -> PyResult<()> {
        check(value, weight)?;
        if !(self.inner.weight_sum() + weight).is_finite() {
            return Err(PyValueError::new_err("total weight would be nonfinite"));
        }
        self.inner
            .add_weighted(value, weight)
            .map_err(PyQlError::from)?;
        Ok(())
    }

    /// Atomically append a sequence and optional matching weights.
    #[pyo3(signature = (observations, *, weights = None))]
    fn add_batch(&mut self, observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<()> {
        if let Some(ref weights) = weights
            && weights.len() != observations.len()
        {
            return Err(PyValueError::new_err(
                "observation and weight lengths differ",
            ));
        }
        let mut next = self.inner.clone();
        next.reserve(observations.len());
        let mut total = next.weight_sum();
        for (index, value) in observations.into_iter().enumerate() {
            let weight = weights.as_ref().map_or(1.0, |items| items[index]);
            check(value, weight)?;
            total += weight;
            if !total.is_finite() {
                return Err(PyValueError::new_err("total weight would be nonfinite"));
            }
            next.add_weighted(value, weight).map_err(PyQlError::from)?;
        }
        self.inner = next;
        Ok(())
    }

    /// Remove all observations.
    fn reset(&mut self) {
        self.inner.reset();
    }

    /// Number of observations, including zero-weight observations.
    fn samples(&self) -> usize {
        self.inner.samples()
    }

    /// Sum of observation weights.
    fn weight_sum(&self) -> f64 {
        self.inner.weight_sum()
    }

    fn min(&self) -> PyResult<f64> {
        finite(self.inner.min())
    }
    fn max(&self) -> PyResult<f64> {
        finite(self.inner.max())
    }
    fn mean(&self) -> PyResult<f64> {
        finite(self.inner.mean())
    }
    fn variance(&self) -> PyResult<f64> {
        finite(self.inner.variance())
    }
    fn standard_deviation(&self) -> PyResult<f64> {
        finite(self.inner.standard_deviation())
    }
    fn error_estimate(&self) -> PyResult<f64> {
        finite(self.inner.error_estimate())
    }
    fn skewness(&self) -> PyResult<f64> {
        finite(self.inner.skewness())
    }
    fn kurtosis(&self) -> PyResult<f64> {
        finite(self.inner.kurtosis())
    }

    fn percentile(&mut self, probability: f64) -> PyResult<f64> {
        if !(probability.is_finite() && probability > 0.0 && probability <= 1.0) {
            return Err(PyValueError::new_err("percentile must be in (0, 1]"));
        }
        finite(self.inner.percentile(probability))
    }
    fn top_percentile(&mut self, probability: f64) -> PyResult<f64> {
        if !(probability.is_finite() && probability > 0.0 && probability <= 1.0) {
            return Err(PyValueError::new_err("percentile must be in (0, 1]"));
        }
        finite(self.inner.top_percentile(probability))
    }
    fn semi_variance(&self) -> PyResult<f64> {
        positive_tail(&self.inner, finite(self.inner.mean())?)?;
        finite(self.inner.semi_variance())
    }
    fn semi_deviation(&self) -> PyResult<f64> {
        positive_tail(&self.inner, finite(self.inner.mean())?)?;
        finite(self.inner.semi_deviation())
    }
    fn downside_variance(&self) -> PyResult<f64> {
        positive_tail(&self.inner, 0.0)?;
        finite(self.inner.downside_variance())
    }
    fn downside_deviation(&self) -> PyResult<f64> {
        positive_tail(&self.inner, 0.0)?;
        finite(self.inner.downside_deviation())
    }

    fn regret(&self, target: f64) -> PyResult<f64> {
        if !target.is_finite() {
            return Err(PyValueError::new_err("target must be finite"));
        }
        positive_tail(&self.inner, target)?;
        finite(self.inner.regret(target))
    }
    fn potential_upside(&mut self, confidence: f64) -> PyResult<f64> {
        finite(self.inner.potential_upside(confidence))
    }
    fn value_at_risk(&mut self, confidence: f64) -> PyResult<f64> {
        finite(self.inner.value_at_risk(confidence))
    }
    fn expected_shortfall(&mut self, confidence: f64) -> PyResult<f64> {
        let threshold = -finite(self.inner.value_at_risk(confidence))?;
        positive_tail(&self.inner, threshold)?;
        finite(self.inner.expected_shortfall(confidence))
    }
    fn shortfall(&self, target: f64) -> PyResult<f64> {
        if !target.is_finite() {
            return Err(PyValueError::new_err("target must be finite"));
        }
        finite(self.inner.shortfall(target))
    }
    fn average_shortfall(&self, target: f64) -> PyResult<f64> {
        if !target.is_finite() {
            return Err(PyValueError::new_err("target must be finite"));
        }
        positive_tail(&self.inner, target)?;
        finite(self.inner.average_shortfall(target))
    }
}
