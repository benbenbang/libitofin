//! Bounded batch facade for unit-weight star L2 discrepancy.

use crate::PyQlError;
use libitofin::math::statistics::{evaluate_discrepancy_batch, validate_discrepancy_shape};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Star L2 discrepancy of rectangular samples in the closed unit cube.
///
/// Only omitted weights or exact unit weights are supported. Shape and work
/// bounds are checked before flattening; Python argument extraction occurs first.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (samples, *, weights = None))]
pub(crate) fn discrepancy(samples: Vec<Vec<f64>>, weights: Option<Vec<f64>>) -> PyResult<f64> {
    let rows = samples.len();
    let dimension = samples.first().map_or(0, Vec::len);
    validate_discrepancy_shape(rows, dimension).map_err(PyQlError::from)?;
    if samples.iter().any(|row| row.len() != dimension) {
        return Err(crate::ItofinError::new_err(
            "sample rows must be rectangular",
        ));
    }
    let values = samples.into_iter().flatten().collect::<Vec<_>>();
    evaluate_discrepancy_batch(&values, rows, dimension, weights.as_deref())
        .map_err(PyQlError::from)
        .map_err(Into::into)
}
