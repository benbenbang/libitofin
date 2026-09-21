use crate::PyQlError;
use libitofin::termstructures::iterativebootstrap::{
    IterativeBootstrap, IterativeBootstrapOptions,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Immutable controls for iterative yield-curve bootstrap.
///
/// Bounds are initial guesses for the solver bracket, not constraints on the
/// final curve. Attempts widen those bounds by the specified factors.
/// dont_throw explicitly accepts an approximate fallback when solving fails;
/// helper evaluation errors still propagate. None accuracy uses curve accuracy.
#[gen_stub_pyclass]
#[pyclass(
    name = "IterativeBootstrapOptions",
    frozen,
    module = "itofin.termstructures"
)]
pub struct PyIterativeBootstrapOptions {
    inner: IterativeBootstrapOptions,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyIterativeBootstrapOptions {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (accuracy=None, min_value=None, max_value=None, max_attempts=1, max_factor=2.0, min_factor=2.0, dont_throw=false, dont_throw_steps=10, max_evaluations=100))]
    fn new(
        accuracy: Option<f64>,
        min_value: Option<f64>,
        max_value: Option<f64>,
        max_attempts: usize,
        max_factor: f64,
        min_factor: f64,
        dont_throw: bool,
        dont_throw_steps: usize,
        max_evaluations: usize,
    ) -> PyResult<Self> {
        let inner = IterativeBootstrapOptions {
            accuracy,
            min_value,
            max_value,
            max_attempts,
            max_factor,
            min_factor,
            dont_throw,
            dont_throw_steps,
            max_evaluations,
        };
        inner.validate().map_err(PyQlError::from)?;
        Ok(Self { inner })
    }
}

pub(crate) fn strategy(
    options: Option<&PyIterativeBootstrapOptions>,
) -> PyResult<IterativeBootstrap> {
    IterativeBootstrap::with_options(
        options.map_or_else(IterativeBootstrapOptions::default, |x| x.inner),
    )
    .map_err(|e| PyQlError::from(e).into())
}
