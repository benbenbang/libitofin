//! Weighted mean-convergence diagnostics with atomic mutable state.

use crate::PyQlError;
use libitofin::math::statistics::{
    ConvergencePoint, ConvergenceStatistics, evaluate_convergence_batch,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

fn table(points: &[ConvergencePoint]) -> Vec<(usize, f64)> {
    points
        .iter()
        .map(|point| (point.samples, point.mean))
        .collect()
}

/// Cumulative weighted means at checkpoints 1, 3, 7, 15, and so on.
///
/// The incomplete final prefix is not recorded. Input order is preserved;
/// zero-weight observations count, but every checkpoint needs positive weight.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (observations, *, weights = None))]
pub(crate) fn convergence_table(
    observations: Vec<f64>,
    weights: Option<Vec<f64>>,
) -> PyResult<Vec<(usize, f64)>> {
    let points =
        evaluate_convergence_batch(&observations, weights.as_deref()).map_err(PyQlError::from)?;
    Ok(table(&points))
}

/// Atomic bounded accumulator for cumulative mean-convergence diagnostics.
#[gen_stub_pyclass]
#[pyclass(
    name = "ConvergenceStatistics",
    unsendable,
    module = "itofin.statistics"
)]
pub struct PyConvergenceStatistics {
    inner: ConvergenceStatistics,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyConvergenceStatistics {
    /// Construct an empty accumulator with checkpoints 1, 3, 7, 15, and so on.
    #[new]
    fn new() -> Self {
        Self {
            inner: ConvergenceStatistics::new(),
        }
    }

    /// Add one finite observation atomically with a nonnegative finite weight.
    #[pyo3(signature = (value, weight = 1.0))]
    fn add(&mut self, value: f64, weight: f64) -> PyResult<()> {
        self.inner
            .add_weighted(value, weight)
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }

    /// Add a complete ordered batch atomically; omitted weights are one.
    #[pyo3(signature = (observations, *, weights = None))]
    fn add_batch(&mut self, observations: Vec<f64>, weights: Option<Vec<f64>>) -> PyResult<()> {
        self.inner
            .add_batch(&observations, weights.as_deref())
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }

    /// Clear every sample and checkpoint.
    fn reset(&mut self) {
        self.inner.reset();
    }

    /// Number of accepted observations, including zero-weight observations.
    fn samples(&self) -> usize {
        self.inner.samples()
    }

    /// Sum of accepted weights.
    fn weight_sum(&self) -> f64 {
        self.inner.weight_sum()
    }

    /// Weighted mean of all accepted observations, including an incomplete prefix.
    fn mean(&self) -> PyResult<f64> {
        self.inner
            .mean()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }

    /// Return a fresh list of immutable sample-count and cumulative-mean tuples.
    fn convergence_table(&self) -> Vec<(usize, f64)> {
        table(self.inner.convergence_table())
    }
}
