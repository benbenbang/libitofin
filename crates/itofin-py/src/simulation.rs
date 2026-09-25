//! Stateless Python simulation calls backed by the shared Rust/Go kernel.

use crate::{ItofinError, PyQlError};
use libitofin::methods::montecarlo::simulation_kernel::{self, GbmRequest};
use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayDyn, PyArrayMethods};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

pub(crate) const DEFAULT_MAX_OUTPUT_VALUES: usize = 16 * 1024 * 1024;

fn output_limit(requested: isize) -> PyResult<usize> {
    if requested < 0 {
        return Err(ItofinError::new_err(
            "max_output_values must be nonnegative",
        ));
    }
    Ok(if requested == 0 {
        DEFAULT_MAX_OUTPUT_VALUES
    } else {
        requested as usize
    })
}

/// Draw standard normal values in the same MT19937/inverse-normal order as Go.
///
/// Args:
///     count (int): Number of draws, at most 16,777,216.
///     seed (int): Nonzero 32-bit seed. Each call restarts the stream.
///
/// Returns:
///     numpy.ndarray: A float64 array of shape `(count,)`.
///
/// Raises:
///     ItofinError: If the count is outside the limit or the seed is zero.
#[gen_stub_pyfunction(module = "itofin")]
#[pyfunction]
pub(crate) fn gaussian_draws<'py>(
    py: Python<'py>,
    count: isize,
    seed: u32,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    if count < 0 || count as usize > DEFAULT_MAX_OUTPUT_VALUES {
        return Err(ItofinError::new_err("invalid draw count"));
    }
    let values =
        simulation_kernel::gaussian_draws(count as usize, seed).map_err(PyQlError::from)?;
    Ok(PyArray1::from_vec(py, values))
}

/// Generate exact-discretization correlated geometric Brownian paths.
///
/// Draws are consumed path, time, asset; equal nonzero seeds and inputs are
/// bit-identical to Go `SimulateGBM`. Initial values appear at time zero.
///
/// Args:
///     initial (list[float]): Positive initial asset values.
///     drift (list[float]): Annualized arithmetic drifts, one per asset.
///     volatility (list[float]): Nonnegative annualized volatilities.
///     horizon (float): Nonnegative time horizon in units matching the rates.
///     steps (int): Positive number of time intervals per path.
///     paths (int): Positive number of independent paths.
///     seed (int): Nonzero 32-bit seed.
///     correlation (list[float] | None): Positive-definite row-major matrix;
///         None selects identity.
///     terminal_only (bool): Return final asset values without intermediate rows.
///     max_output_values (int): Zero selects 16,777,216 float64 values (128 MiB);
///         a positive value overrides this limit.
///
/// Returns:
///     numpy.ndarray: Float64 values shaped `(paths, steps + 1, assets)` or,
///         with `terminal_only`, `(paths, assets)`.
///
/// Raises:
///     ItofinError: If inputs, correlation, or output size are invalid.
#[gen_stub_pyfunction(module = "itofin")]
#[pyfunction]
#[pyo3(signature = (initial, drift, volatility, horizon, steps, paths, seed, correlation = None, terminal_only = false, max_output_values = 0))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_gbm<'py>(
    py: Python<'py>,
    initial: Vec<f64>,
    drift: Vec<f64>,
    volatility: Vec<f64>,
    horizon: f64,
    steps: usize,
    paths: usize,
    seed: u32,
    correlation: Option<Vec<f64>>,
    terminal_only: bool,
    max_output_values: isize,
) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
    let assets = initial.len();
    if !(1..=1024).contains(&assets) {
        return Err(ItofinError::new_err(
            "asset count must be between 1 and 1024",
        ));
    }
    let identity;
    let correlation = if let Some(ref values) = correlation {
        values.as_slice()
    } else {
        identity = (0..assets * assets)
            .map(|index| f64::from(index % (assets + 1) == 0))
            .collect::<Vec<_>>();
        &identity
    };
    let request = GbmRequest {
        initial: &initial,
        drift: &drift,
        volatility: &volatility,
        correlation,
        horizon,
        steps,
        paths,
        seed,
        terminal_only,
    };
    let count = simulation_kernel::output_len(&request).map_err(PyQlError::from)?;
    let limit = output_limit(max_output_values)?;
    if count > limit || count > isize::MAX as usize / size_of::<f64>() {
        return Err(ItofinError::new_err(
            "result exceeds output limit; use terminal mode or smaller batches",
        ));
    }
    let values = simulation_kernel::gbm_paths(&request).map_err(PyQlError::from)?;
    let shape = if terminal_only {
        vec![paths, assets]
    } else {
        vec![paths, steps + 1, assets]
    };
    PyArray1::from_vec(py, values).reshape(IxDyn(&shape))
}
