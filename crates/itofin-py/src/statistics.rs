//! Stateless Python facade for weighted empirical statistics and risk.

use crate::PyQlError;
use libitofin::math::statistics::{BatchStatistic, evaluate_batch};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

fn evaluate(
    observations: Vec<f64>,
    weights: Option<Vec<f64>>,
    measure: BatchStatistic,
    probability: f64,
) -> PyResult<f64> {
    evaluate_batch(&observations, weights.as_deref(), measure, probability)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Weighted mean of signed observations. Negative observations represent losses.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn mean(observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::Mean, 0.0)
}

/// Weighted sample variance with the core's count-based n/(n-1) correction.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn variance(observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::Variance, 0.0)
}

/// Square root of weighted sample variance.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn standard_deviation(
    observations: Vec<f64>,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::StandardDeviation,
        0.0,
    )
}

/// Weighted empirical percentile for a probability in (0, 1].
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, probability, *, weights = None))]
pub(crate) fn percentile(
    observations: Vec<f64>,
    probability: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::Percentile,
        probability,
    )
}

/// Loss magnitude at confidence in [0.9, 1), capped at zero for gains.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, confidence, *, weights = None))]
pub(crate) fn value_at_risk(
    observations: Vec<f64>,
    confidence: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::ValueAtRisk,
        confidence,
    )
}

/// Weighted mean loss strictly below the VaR threshold, as a positive magnitude.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, confidence, *, weights = None))]
pub(crate) fn expected_shortfall(
    observations: Vec<f64>,
    confidence: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::ExpectedShortfall,
        confidence,
    )
}
