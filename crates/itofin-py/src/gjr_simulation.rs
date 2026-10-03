//! Seeded GJR-GARCH paths through the shared Rust kernel.

use crate::{ItofinError, PyQlError};
use libitofin::methods::montecarlo::gjr_paths::{self, GjrRequest};
use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayDyn, PyArrayMethods};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Generate seeded GJR-GARCH diffusion-approximation spot/variance paths.
///
/// Daily `daily_variance` and `omega` parameters are converted internally.
/// Output component zero is spot; component one is raw annual variance,
/// which may be negative with truncation schemes. This is not the daily
/// GJR recursion. Rates and horizon are annualized, continuously compounded.
///
/// `scheme` is `PartialTruncation`, `FullTruncation` or `Reflection`. Each
/// step consumes two MT19937/inverse-normal draws in path/time/factor order,
/// even for zero horizon. Equal seeds and inputs reproduce the same paths
/// across Python and Go. Zero seed is rejected.
///
/// Returns:
///     numpy.ndarray: Owned contiguous float64 values shaped
///         `(paths, steps + 1, 2)` including initial state, or `(paths, 2)`
///         with `terminal_only`. Terminal rows match final full rows exactly.
///
/// Raises:
///     ItofinError: If parameters, time grid, simulation or output size are
///         invalid. `max_output_values=0` selects 16,777,216 values (128 MiB).
#[gen_stub_pyfunction(module = "itofin")]
#[pyfunction]
#[pyo3(signature = (spot, daily_variance, risk_free_rate, dividend_yield, omega, alpha, beta, gamma, lambda_, horizon, steps, paths, seed, days_per_year = 252.0, scheme = "FullTruncation", terminal_only = false, max_output_values = 0))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_gjr<'py>(
    py: Python<'py>,
    spot: f64,
    daily_variance: f64,
    risk_free_rate: f64,
    dividend_yield: f64,
    omega: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
    lambda_: f64,
    horizon: f64,
    steps: usize,
    paths: usize,
    seed: u32,
    days_per_year: f64,
    scheme: &str,
    terminal_only: bool,
    max_output_values: isize,
) -> PyResult<Bound<'py, PyArrayDyn<f64>>> {
    if max_output_values < 0 {
        return Err(ItofinError::new_err(
            "max_output_values must be nonnegative",
        ));
    }
    let request = GjrRequest {
        spot,
        daily_variance,
        risk_free_rate,
        dividend_yield,
        omega,
        alpha,
        beta,
        gamma,
        lambda: lambda_,
        days_per_year,
        horizon,
        steps,
        paths,
        seed,
        discretization: crate::gjr::discretization(scheme)?,
        terminal_only,
    };
    let count = gjr_paths::output_len(&request).map_err(PyQlError::from)?;
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
    let values = gjr_paths::gjr_paths(&request).map_err(PyQlError::from)?;
    let shape = if terminal_only {
        vec![paths, 2]
    } else {
        vec![paths, steps + 1, 2]
    };
    PyArray1::from_vec(py, values).reshape(IxDyn(&shape))
}
