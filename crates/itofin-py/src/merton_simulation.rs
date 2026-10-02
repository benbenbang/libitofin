//! Seeded exact lognormal-jump transitions through the shared Rust kernel.

use crate::{ItofinError, PyQlError};
use libitofin::methods::montecarlo::merton_paths::{self, MertonRequest};
use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayDyn, PyArrayMethods};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Generate exact constant-parameter Merton jump-diffusion paths.
///
/// `drift` is annualized expected arithmetic spot growth including jumps:
/// risk-neutral callers supply risk-free rate minus dividend yield; forecast
/// callers supply their own assumption. The kernel subtracts the jump
/// compensator internally. It uses original event intensity, rather than the
/// adjusted intensity used by the European pricing engine.
///
/// Separate deterministic MT19937 streams supply diffusion normals, Poisson
/// uniforms and jump normals. Every path step consumes all three draws, even
/// when the horizon or a coefficient is zero. Equal seeds and inputs reproduce
/// the same paths; zero jump intensity matches scalar `simulate_gbm` exactly.
///
/// Args:
///     spot (float): Finite positive initial spot.
///     drift (float): Finite annualized expected arithmetic spot growth.
///     volatility (float): Finite nonnegative diffusion volatility.
///     jump_intensity (float): Finite nonnegative event intensity.
///     log_mean_jump (float): Finite mean of logarithmic jump size.
///     log_jump_volatility (float): Finite nonnegative log-jump dispersion.
///     horizon (float): Finite nonnegative horizon in matching annual units.
///     steps (int): Positive intervals per path.
///     paths (int): Positive independent paths.
///     seed (int): Nonzero 32-bit seed.
///     terminal_only (bool): Return final spots instead of complete paths.
///     max_output_values (int): Zero selects 16,777,216 values (128 MiB).
///
/// Returns:
///     numpy.ndarray: Owned contiguous float64 values shaped
///         `(paths, steps + 1)` including the initial spot, or `(paths,)` for
///         terminal output. Terminal spots match the last full column exactly.
///
/// Raises:
///     ItofinError: If inputs, numerical representation, output size, Poisson
///         recurrence domain or simulated positive finite spots are invalid.
#[gen_stub_pyfunction(module = "itofin")]
#[pyfunction]
#[pyo3(signature = (spot, drift, volatility, jump_intensity, log_mean_jump, log_jump_volatility, horizon, steps, paths, seed, terminal_only = false, max_output_values = 0))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_merton<'py>(
    py: Python<'py>,
    spot: f64,
    drift: f64,
    volatility: f64,
    jump_intensity: f64,
    log_mean_jump: f64,
    log_jump_volatility: f64,
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
    let request = MertonRequest {
        spot,
        drift,
        volatility,
        jump_intensity,
        log_mean_jump,
        log_jump_volatility,
        horizon,
        steps,
        paths,
        seed,
        terminal_only,
    };
    let count = merton_paths::output_len(&request).map_err(PyQlError::from)?;
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
    let values = merton_paths::merton_paths(&request).map_err(PyQlError::from)?;
    let shape = if terminal_only {
        vec![paths]
    } else {
        vec![paths, steps + 1]
    };
    PyArray1::from_vec(py, values).reshape(IxDyn(&shape))
}
