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

/// Count-corrected conditional variance below the weighted mean.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn semi_variance(observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::SemiVariance, 0.0)
}

/// Square root of semi variance.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn semi_deviation(observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::SemiDeviation, 0.0)
}

/// Count-corrected conditional variance below zero.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn downside_variance(
    observations: Vec<f64>,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::DownsideVariance, 0.0)
}

/// Square root of downside variance.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn downside_deviation(
    observations: Vec<f64>,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::DownsideDeviation,
        0.0,
    )
}

/// Count-corrected conditional variance below a finite target.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, target, *, weights = None))]
pub(crate) fn regret(
    observations: Vec<f64>,
    target: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::Regret, target)
}

/// Nonnegative upper percentile at confidence in [0.9, 1).
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, confidence, *, weights = None))]
pub(crate) fn potential_upside(
    observations: Vec<f64>,
    confidence: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::PotentialUpside,
        confidence,
    )
}

/// Weighted probability of observations strictly below a finite target.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, target, *, weights = None))]
pub(crate) fn shortfall(
    observations: Vec<f64>,
    target: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(observations, weights, BatchStatistic::Shortfall, target)
}

/// Weighted mean of target minus observations strictly below target.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, target, *, weights = None))]
pub(crate) fn average_shortfall(
    observations: Vec<f64>,
    target: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::AverageShortfall,
        target,
    )
}

/// Weighted empirical percentile traversing observations from high to low.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, probability, *, weights = None))]
pub(crate) fn top_percentile(
    observations: Vec<f64>,
    probability: f64,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    evaluate(
        observations,
        weights,
        BatchStatistic::TopPercentile,
        probability,
    )
}
