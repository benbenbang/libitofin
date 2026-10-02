//! Exact constant-parameter Merton jump-diffusion paths.
//!
//! `drift` is annualized expected arithmetic spot growth, including jumps.
//! Risk-neutral callers supply `r-q`; forecasts supply their own assumption.
//! The log drift compensates original event intensity, not pricing intensity.
//! Three MT19937 streams consume one diffusion normal, count uniform and jump
//! normal per step, including zero-volatility, zero-count and zero-horizon steps.

use crate::errors::{QlError, QlResult};
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::math::distributions::poisson::InverseCumulativePoisson;
use crate::math::distributions::{Probability, Quantile};
use crate::math::randomnumbers::{MersenneTwisterUniformRng, UniformRng};
use crate::require;
use crate::types::{Natural, Real, Size, Time};

/// Scalar Merton simulation parameters with time-zero-inclusive full output.
#[derive(Clone, Copy)]
pub struct MertonRequest {
    pub spot: Real,
    pub drift: Real,
    pub volatility: Real,
    pub jump_intensity: Real,
    pub log_mean_jump: Real,
    pub log_jump_volatility: Real,
    pub horizon: Time,
    pub steps: Size,
    pub paths: Size,
    pub seed: Natural,
    /// Return `[path]` instead of full `[path, time]` output.
    pub terminal_only: bool,
}

fn error(message: &str) -> QlError {
    QlError::new(message, file!(), line!())
}

/// Checked number of output values.
///
/// # Errors
/// Rejects overflowing full-output time and path dimensions.
pub fn output_len(request: &MertonRequest) -> QlResult<Size> {
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
        .ok_or_else(|| error("simulation dimensions overflow"))
}

fn shifted_seed(seed: Natural, offset: Natural) -> Natural {
    (1 + ((u64::from(seed) - 1 + u64::from(offset)) % u64::from(Natural::MAX))) as Natural
}

struct Draws {
    diffusion: MersenneTwisterUniformRng,
    counts: MersenneTwisterUniformRng,
    jumps: MersenneTwisterUniformRng,
    normal: InverseCumulativeNormal,
    poisson: Option<InverseCumulativePoisson>,
}

impl Draws {
    fn new(seed: Natural, poisson_mean: Real) -> QlResult<Self> {
        Ok(Self {
            diffusion: MersenneTwisterUniformRng::new(seed),
            counts: MersenneTwisterUniformRng::new(shifted_seed(seed, 0x9e37_79b9)),
            jumps: MersenneTwisterUniformRng::new(shifted_seed(seed, 0xbb67_ae85)),
            normal: InverseCumulativeNormal::standard(),
            poisson: if poisson_mean == 0.0 {
                None
            } else {
                Some(InverseCumulativePoisson::new(poisson_mean)?)
            },
        })
    }

    fn next(&mut self) -> QlResult<(Real, Real, Real)> {
        let diffusion = self.normal.value(self.diffusion.next_real())?;
        let uniform = self.counts.next_real();
        let jump = self.normal.value(self.jumps.next_real())?;
        let count = match &self.poisson {
            Some(poisson) => poisson.quantile(Probability::try_from(uniform)?)?,
            None => 0.0,
        };
        require!(count.is_finite() && count >= 0.0, "invalid jump count");
        Ok((diffusion, count, jump))
    }
}

struct Transition {
    dt: Real,
    log_growth: Real,
    diffusion_scale: Real,
    poisson_mean: Real,
}

fn validate(request: &MertonRequest) -> QlResult<Transition> {
    require!(
        request.spot.is_finite() && request.spot > 0.0,
        "spot must be finite and positive"
    );
    require!(request.drift.is_finite(), "drift must be finite");
    require!(
        request.log_mean_jump.is_finite(),
        "log-jump mean must be finite"
    );
    for (value, name) in [
        (request.volatility, "volatility"),
        (request.jump_intensity, "jump intensity"),
        (request.log_jump_volatility, "log-jump volatility"),
        (request.horizon, "horizon"),
    ] {
        require!(
            value.is_finite() && value >= 0.0,
            "{name} must be finite and non-negative"
        );
    }
    require!(
        request.steps > 0 && request.paths > 0,
        "steps and paths must be positive"
    );
    require!(request.seed != 0, "seed must be nonzero");
    request
        .steps
        .checked_mul(request.paths)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| error("simulation work dimensions overflow"))?;
    let diffusion_variance = request.volatility * request.volatility;
    let jump_variance = request.log_jump_volatility * request.log_jump_volatility;
    let jump_exponent = request.log_mean_jump + 0.5 * jump_variance;
    let multiplier = jump_exponent.exp();
    let compensation = request.jump_intensity * jump_exponent.exp_m1();
    require!(
        diffusion_variance.is_finite()
            && jump_variance.is_finite()
            && jump_exponent.is_finite()
            && multiplier.is_finite()
            && multiplier > 0.0
            && compensation.is_finite(),
        "jump or diffusion parameters are not representable"
    );
    let dt = request.horizon / request.steps as Real;
    require!(request.horizon == 0.0 || dt > 0.0, "time step underflows");
    let poisson_mean = request.jump_intensity * dt;
    require!(poisson_mean.is_finite(), "Poisson mean overflows");
    require!(
        request.jump_intensity == 0.0 || request.horizon == 0.0 || poisson_mean > 0.0,
        "Poisson mean underflows"
    );
    let (log_growth, diffusion_scale) = if request.horizon == 0.0 {
        (0.0, 0.0)
    } else {
        let log_drift = if request.jump_intensity == 0.0 {
            request.drift - 0.5 * request.volatility * request.volatility
        } else {
            request.drift - compensation - 0.5 * request.volatility * request.volatility
        };
        (log_drift * dt, request.volatility * dt.sqrt())
    };
    require!(
        log_growth.is_finite() && diffusion_scale.is_finite(),
        "transition parameters are not representable"
    );
    Ok(Transition {
        dt,
        log_growth,
        diffusion_scale,
        poisson_mean,
    })
}

/// Generate exact paths from a positive MT19937 seed, in `[path, time]` order.
///
/// Diffusion uses the supplied seed. Count and jump streams use
/// `1 + ((seed-1+offset) % u32::MAX)`, with offsets `0x9E3779B9` and
/// `0xBB67AE85`. Each uniform is `(word+0.5)/2^32`; normals use Acklam's
/// inverse without refinement and counts use the fallible inverse Poisson.
/// Zero intensity reproduces scalar GBM with the same seed and preserves its
/// deterministic zero-volatility time convention. Terminal output has the
/// same bits as each full path's last value. Zero horizon returns the spot.
///
/// # Errors
/// Rejects invalid parameters, overflowing work/output dimensions, allocation
/// failure, unrepresentable transition coefficients, positive time-step or
/// Poisson-mean underflow, inverse-Poisson domain/quantile errors, and any
/// nonfinite or nonpositive simulated price. The inverse-Poisson mean is
/// restricted to the normal recurrence domain, approximately at most 708.396.
pub fn merton_paths(request: &MertonRequest) -> QlResult<Vec<Real>> {
    let transition = validate(request)?;
    let mut draws = Draws::new(request.seed, transition.poisson_mean)?;
    let count = output_len(request)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| error("allocation failed"))?;
    for _ in 0..request.paths {
        let mut spot = request.spot;
        if !request.terminal_only {
            output.push(spot);
        }
        for step in 0..request.steps {
            let (diffusion, count, jump) = draws.next()?;
            if request.horizon == 0.0 {
                spot = request.spot;
            } else if request.jump_intensity == 0.0 && request.volatility == 0.0 {
                let time = if step + 1 == request.steps {
                    request.horizon
                } else {
                    (step + 1) as Real * transition.dt
                };
                spot = request.spot * (request.drift * time).exp();
            } else {
                let exponent = transition.log_growth + transition.diffusion_scale * diffusion;
                let exponent = if request.jump_intensity == 0.0 {
                    exponent
                } else {
                    exponent
                        + count * request.log_mean_jump
                        + count.sqrt() * request.log_jump_volatility * jump
                };
                spot *= exponent.exp();
            }
            require!(
                spot.is_finite() && spot > 0.0,
                "simulated price overflow or underflow"
            );
            if !request.terminal_only {
                output.push(spot);
            }
        }
        if request.terminal_only {
            output.push(spot);
        }
    }
    Ok(output)
}
