//! Stateless, seeded Monte Carlo building blocks for foreign-language bindings.
//!
//! Draws use the core Mersenne Twister / inverse-normal policy, consumed in
//! path, time, asset order. Nonzero seeds reproduce the native sequence; zero
//! is rejected because the native RNG interprets it as a nondeterministic seed.

use libitofin::errors::{QlError, QlResult};
use libitofin::math::matrix::Matrix;
use libitofin::math::matrixutilities::choleskydecomposition::cholesky_decomposition;
use libitofin::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom, SequenceGenerator};
use libitofin::require;
use libitofin::types::{Natural, Rate, Real, Size, Time, Volatility};

fn error(message: &str) -> QlError {
    QlError::new(message, file!(), line!())
}

fn zeros(count: Size) -> QlResult<Vec<Real>> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| error("allocation failed"))?;
    result.resize(count, 0.0);
    Ok(result)
}

fn product(a: Size, b: Size) -> QlResult<Size> {
    a.checked_mul(b).ok_or_else(|| error("simulation dimensions overflow"))
}

/// Independent standard-normal draws in native sequence order.
///
/// An empty request returns an empty vector, but still requires a nonzero seed.
pub fn gaussian_draws(count: Size, seed: Natural) -> QlResult<Vec<Real>> {
    require!(seed != 0, "seed must be nonzero");
    let mut result = zeros(count)?;
    // A scalar sequence keeps native internal allocations constant-sized.
    let mut rng = PseudoRandom::make_sequence_generator(1, seed)?;
    for value in &mut result {
        *value = rng.next_sequence().value[0];
    }
    Ok(result)
}

/// Parameters for exact-discretization geometric Brownian motion.
///
/// `drift` is the annualized arithmetic drift (risk-neutral callers supply
/// their own rate minus yield); volatility and horizon use matching time units.
/// Correlation is a dense row-major, symmetric, unit-diagonal **positive
/// definite** matrix. Singular positive-semidefinite matrices are rejected;
/// no identity fallback, jitter, or eigenvalue repair is performed.
pub struct GbmRequest<'a> {
    pub initial: &'a [Real],
    pub drift: &'a [Rate],
    pub volatility: &'a [Volatility],
    pub correlation: &'a [Real],
    pub horizon: Time,
    pub steps: Size,
    pub paths: Size,
    pub seed: Natural,
    /// Return only `[path, asset]`, with the same terminal values as full mode.
    pub terminal_only: bool,
}

/// Number of result elements, with overflow checked before allocation.
pub fn output_len(request: &GbmRequest<'_>) -> QlResult<Size> {
    let times = if request.terminal_only {
        1
    } else {
        request.steps.checked_add(1).ok_or_else(|| error("step count overflows"))?
    };
    product(product(request.paths, times)?, request.initial.len())
}

fn correlation_factor(values: &[Real], assets: Size) -> QlResult<Matrix> {
    require!(values.len() == product(assets, assets)?, "correlation shape mismatch");
    // Strict input checks prevent the native decomposition's flexible mode
    // from silently interpreting an asymmetric matrix using its upper half.
    for i in 0..assets {
        require!(values[i * assets + i] == 1.0, "correlation diagonal must equal one");
        for j in 0..assets {
            let value = values[i * assets + j];
            require!(value.is_finite() && value.abs() <= 1.0, "invalid correlation entry");
            require!(value == values[j * assets + i], "correlation must be symmetric");
        }
    }
    let mut matrix = Matrix::with_size(assets, assets);
    for i in 0..assets {
        matrix.row_mut(i).copy_from_slice(&values[i * assets..(i + 1) * assets]);
    }
    // Flexible mode avoids the core strict-mode panic. Require every pivot
    // positive and verify reconstruction so this remains an SPD-only API.
    let factor = cholesky_decomposition(&matrix, true);
    let tolerance = 64.0 * Real::EPSILON * assets as Real;
    for i in 0..assets {
        require!(factor[(i, i)] > 0.0, "correlation must be positive definite");
        for j in 0..=i {
            let rebuilt: Real = (0..=j).map(|k| factor[(i, k)] * factor[(j, k)]).sum();
            require!(
                rebuilt.is_finite() && (rebuilt - matrix[(i, j)]).abs() <= tolerance,
                "correlation factorization failed"
            );
        }
    }
    Ok(factor)
}

/// Generate GBM paths, laid out `[path, time, asset]`, including time zero.
///
/// Uses `dt = horizon / steps` and `S *= exp((mu - sigma²/2)dt +
/// sigma*sqrt(dt)*(L*z))`. All paths consume the same number of draws,
/// including zero-volatility assets. Zero horizon returns initial values
/// exactly. Zero volatility follows deterministic `initial*exp(mu*t)`.
///
/// # Errors
/// Rejects nonfinite parameters, nonpositive spots, negative volatilities or
/// horizon, zero steps/paths/seed, inconsistent shapes, non-SPD correlation,
/// and nonfinite or underflowed prices. Supports at most 1,024 assets to bound
/// the core Cholesky routine's infallibly allocated matrix workspace. Output
/// and binding-owned scratch allocations use `try_reserve_exact`.
pub fn gbm_paths(request: &GbmRequest<'_>) -> QlResult<Vec<Real>> {
    let assets = request.initial.len();
    require!((1..=1024).contains(&assets), "asset count must be between 1 and 1024");
    require!(request.steps > 0 && request.paths > 0, "steps and paths must be positive");
    require!(request.seed != 0, "seed must be nonzero");
    require!(request.horizon.is_finite() && request.horizon >= 0.0, "invalid horizon");
    require!(request.drift.len() == assets && request.volatility.len() == assets, "asset shape mismatch");
    for i in 0..assets {
        require!(request.initial[i].is_finite() && request.initial[i] > 0.0, "invalid initial value");
        require!(request.drift[i].is_finite(), "invalid drift");
        require!(request.volatility[i].is_finite() && request.volatility[i] >= 0.0, "invalid volatility");
    }
    // Validate dimension products even when terminal mode does not store time.
    product(product(request.paths, request.steps)?, assets)?;
    let count = output_len(request)?;
    let factor = correlation_factor(request.correlation, assets)?;
    let mut output = zeros(count)?;
    let mut current = zeros(assets)?;
    let mut normals = zeros(assets)?;
    let mut rng = PseudoRandom::make_sequence_generator(1, request.seed)?;
    let dt = request.horizon / request.steps as Real;
    let sqrt_dt = dt.sqrt();
    let mut cursor = 0;
    for _ in 0..request.paths {
        current.copy_from_slice(request.initial);
        if !request.terminal_only {
            output[cursor..cursor + assets].copy_from_slice(&current);
            cursor += assets;
        }
        for step in 0..request.steps {
            for z in &mut normals {
                *z = rng.next_sequence().value[0];
            }
            for i in 0..assets {
                let sigma = request.volatility[i];
                let mu = request.drift[i];
                if request.horizon == 0.0 {
                    current[i] = request.initial[i];
                } else if sigma == 0.0 {
                    let time = if step + 1 == request.steps {
                        request.horizon
                    } else {
                        (step + 1) as Real * dt
                    };
                    current[i] = request.initial[i] * (mu * time).exp();
                } else {
                    let z: Real = (0..=i).map(|j| factor[(i, j)] * normals[j]).sum();
                    current[i] *= ((mu - 0.5 * sigma * sigma) * dt + sigma * sqrt_dt * z).exp();
                }
                require!(current[i].is_finite() && current[i] > 0.0, "simulated price overflow or underflow");
            }
            if !request.terminal_only {
                output[cursor..cursor + assets].copy_from_slice(&current);
                cursor += assets;
            }
        }
        if request.terminal_only {
            output[cursor..cursor + assets].copy_from_slice(&current);
            cursor += assets;
        }
    }
    Ok(output)
}
