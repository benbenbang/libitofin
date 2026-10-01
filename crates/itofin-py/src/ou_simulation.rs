//! Seeded OU paths through the shared Rust simulation kernel.

use crate::{ItofinError, PyQlError};
use libitofin::methods::montecarlo::ou_paths::{self, OuRequest};
use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayDyn, PyArrayMethods};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Generate exact Ornstein-Uhlenbeck paths with a deterministic seed.
///
/// One standard normal is consumed per path and step. Full output has shape
/// `(paths, steps + 1)` and includes time zero; terminal output has shape
/// `(paths,)`. Zero horizon and zero volatility remain deterministic.
///
/// Args:
///     initial (float): Initial state.
///     level (float): Long-run mean.
///     speed (float): Nonnegative mean-reversion speed.
///     volatility (float): Nonnegative diffusion volatility.
///     horizon (float): Nonnegative simulation horizon.
///     steps (int): Positive intervals per path.
///     paths (int): Positive independent paths.
///     seed (int): Nonzero 32-bit seed.
///     terminal_only (bool): Return only final states.
///     max_output_values (int): Zero selects 16,777,216 values (128 MiB).
///
/// Returns:
///     numpy.ndarray: Contiguous float64 paths.
///
/// Raises:
///     ItofinError: If inputs, output size or simulated values are invalid.
#[gen_stub_pyfunction(module = "itofin")]
#[pyfunction]
#[pyo3(signature = (initial, level, speed, volatility, horizon, steps, paths, seed, terminal_only = false, max_output_values = 0))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_ou<'py>(
    py: Python<'py>,
    initial: f64,
    level: f64,
    speed: f64,
    volatility: f64,
    horizon: f64,
    steps: usize,
    paths: usize,
    seed: u32,
    terminal_only: bool,
    max_output_values: isize,
) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
    if max_output_values < 0 {
        return Err(ItofinError::new_err(
            "max_output_values must be nonnegative",
        ));
    }
    let request = OuRequest {
        initial,
        level,
        speed,
        volatility,
        horizon,
        steps,
        paths,
        seed,
        terminal_only,
    };
    let count = ou_paths::output_len(&request).map_err(PyQlError::from)?;
    let limit = if max_output_values == 0 {
        crate::simulation::DEFAULT_MAX_OUTPUT_VALUES
    } else {
        max_output_values as usize
    };
    if count > limit || count > isize::MAX as usize / size_of::<f64>() {
        return Err(ItofinError::new_err(
            "result exceeds output limit; use terminal mode or smaller batches",
        ));
    }
    let values = ou_paths::ou_paths(&request).map_err(PyQlError::from)?;
    let shape = if terminal_only {
        vec![paths]
    } else {
        vec![paths, steps + 1]
    };
    PyArray1::from_vec(py, values).reshape(IxDyn(&shape))
}
