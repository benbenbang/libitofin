//! Stateless Python facade for weighted vector statistics.

use crate::PyQlError;
use libitofin::math::statistics::{
    SequenceStatistic, evaluate_sequence_batch, validate_sequence_shape,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

fn evaluate(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
    measure: SequenceStatistic,
) -> PyResult<Vec<f64>> {
    let rows = samples.len();
    let dimension = samples.first().map_or(0, Vec::len);
    validate_sequence_shape(rows, dimension, measure).map_err(PyQlError::from)?;
    if samples.iter().any(|row| row.len() != dimension) {
        return Err(crate::ItofinError::new_err(
            "sample rows must be rectangular",
        ));
    }
    let values = samples.into_iter().flatten().collect::<Vec<_>>();
    evaluate_sequence_batch(&values, rows, dimension, weights.as_deref(), measure)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

fn matrix(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
    measure: SequenceStatistic,
) -> PyResult<Vec<Vec<f64>>> {
    let dimension = samples.first().map_or(0, Vec::len);
    let values = evaluate(samples, weights, measure)?;
    Ok(values.chunks(dimension).map(<[f64]>::to_vec).collect())
}

/// Weighted component means of rectangular row samples.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_mean(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::Mean)
}

/// Component variances with row-count correction n/(n-1), including zero-weight rows.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_variance(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::Variance)
}

/// Square roots of the count-corrected component variances.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_standard_deviation(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::StandardDeviation)
}

/// Component standard errors using the total row count, including zero-weight rows.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_error_estimate(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::ErrorEstimate)
}

/// Component minima over all rows, including zero-weight rows.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_minimum(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::Minimum)
}

/// Component maxima over all rows, including zero-weight rows.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn sequence_maximum(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<f64>> {
    evaluate(samples, weights, SequenceStatistic::Maximum)
}

/// Symmetric weighted covariance matrix with row-count correction n/(n-1).
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn covariance_matrix(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<Vec<f64>>> {
    matrix(samples, weights, SequenceStatistic::Covariance)
}

/// Correlations with unit diagonal, both constant components one, exactly one constant zero.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn correlation_matrix(
    samples: Vec<Vec<f64>>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<Vec<f64>>> {
    matrix(samples, weights, SequenceStatistic::Correlation)
}
