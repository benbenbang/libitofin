//! Seeded constant-parameter GJR-GARCH diffusion paths.

use crate::errors::{QlError, QlResult};
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::math::randomnumbers::{MersenneTwisterUniformRng, UniformRng};
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters};
use crate::require;
use crate::types::{Natural, Rate, Real, Size, Time};

/// Constant daily GJR parameters and an annual-time simulation grid.
///
/// `daily_variance` is multiplied by `days_per_year` at time zero. Output
/// variance is annualized and retains negative raw Euler states; the selected
/// discretization determines how each subsequent transition handles them.
#[derive(Clone, Copy)]
pub struct GjrRequest {
    pub spot: Real,
    pub daily_variance: Real,
    pub risk_free_rate: Rate,
    pub dividend_yield: Rate,
    pub omega: Real,
    pub alpha: Real,
    pub beta: Real,
    pub gamma: Real,
    pub lambda: Real,
    pub days_per_year: Real,
    pub horizon: Time,
    pub steps: Size,
    pub paths: Size,
    pub seed: Natural,
    pub discretization: GjrGarchDiscretization,
    /// Return `[path, component]` instead of time-zero-inclusive full output.
    pub terminal_only: bool,
}

fn error(message: &str) -> QlError {
    QlError::new(message, file!(), line!())
}

/// Checked output size: `[path, time, component]`, with spot then variance.
///
/// # Errors
/// Rejects overflowing time, path or two-component output dimensions.
pub fn output_len(request: &GjrRequest) -> QlResult<Size> {
    let times = if request.terminal_only {
        1
    } else {
        request
            .steps
            .checked_add(1)
            .ok_or_else(|| error("step count overflows"))?
    };
    request
        .paths
        .checked_mul(times)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| error("simulation output dimensions overflow"))
}

/// Generate GJR-GARCH paths using the shared process transition.
///
/// A single MT19937 stream consumes two uniforms per path and step, in spot
/// factor then variance factor order. Each uniform is `(word+0.5)/2^32` and
/// normals use the native inverse normal without refinement. Draws are consumed
/// even at zero horizon, when all states remain the initial state. Terminal
/// mode has exactly the same bits as the last state of each full path.
///
/// # Errors
/// Rejects invalid daily GJR parameters, nonfinite rates, nonpositive spot,
/// negative or nonfinite horizon, zero steps/paths/seed, overflowing work or
/// output dimensions, allocation failure, positive time-step underflow, and
/// any transition whose spot or variance is not representable.
pub fn gjr_paths(request: &GjrRequest) -> QlResult<Vec<Real>> {
    require!(
        request.spot.is_finite() && request.spot > 0.0,
        "spot must be finite and positive"
    );
    require!(request.risk_free_rate.is_finite(), "rate must be finite");
    require!(request.dividend_yield.is_finite(), "yield must be finite");
    let drift = request.risk_free_rate - request.dividend_yield;
    require!(drift.is_finite(), "rate minus yield overflows");
    require!(
        request.horizon.is_finite() && request.horizon >= 0.0,
        "horizon must be finite and non-negative"
    );
    require!(
        request.steps > 0 && request.paths > 0,
        "steps and paths must be positive"
    );
    require!(request.seed != 0, "seed must be nonzero");
    request
        .steps
        .checked_mul(request.paths)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| error("simulation work dimensions overflow"))?;
    let dt = request.horizon / request.steps as Real;
    require!(request.horizon == 0.0 || dt > 0.0, "time step underflows");
    let parameters = GjrGarchParameters {
        v0: request.daily_variance,
        omega: request.omega,
        alpha: request.alpha,
        beta: request.beta,
        gamma: request.gamma,
        lambda: request.lambda,
        days_per_year: request.days_per_year,
    };
    let coefficients = parameters.coefficients()?;
    let initial = [request.spot, coefficients.initial_variance()];
    let count = output_len(request)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| error("allocation failed"))?;
    let mut rng = MersenneTwisterUniformRng::new(request.seed);
    let normal = InverseCumulativeNormal::standard();
    for _ in 0..request.paths {
        let mut state = initial;
        if !request.terminal_only {
            output.extend_from_slice(&state);
        }
        for _ in 0..request.steps {
            let dw = [
                normal.value(rng.next_real())?,
                normal.value(rng.next_real())?,
            ];
            if dt != 0.0 {
                state = coefficients.evolve(state, dt, drift, dw, request.discretization)?;
            }
            if !request.terminal_only {
                output.extend_from_slice(&state);
            }
        }
        if request.terminal_only {
            output.extend_from_slice(&state);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod validation;

#[cfg(test)]
mod oracle;
