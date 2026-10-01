//! Seeded Ornstein-Uhlenbeck paths using the exact process transition.

use crate::errors::{QlError, QlResult};
use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom, SequenceGenerator};
use crate::processes::OrnsteinUhlenbeckProcess;
use crate::require;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Natural, Real, Size, Time};

/// Scalar OU simulation parameters. Full output is `[path, time]`, including time zero.
pub struct OuRequest {
    pub initial: Real,
    pub level: Real,
    pub speed: Real,
    pub volatility: Real,
    pub horizon: Time,
    pub steps: Size,
    pub paths: Size,
    pub seed: Natural,
    /// Return `[path]` terminal values only.
    pub terminal_only: bool,
}

/// Checked number of values in the requested output.
pub fn output_len(request: &OuRequest) -> QlResult<Size> {
    let times = if request.terminal_only {
        1
    } else {
        request
            .steps
            .checked_add(1)
            .ok_or_else(|| QlError::new("step count overflows", file!(), line!()))?
    };
    request
        .paths
        .checked_mul(times)
        .ok_or_else(|| QlError::new("simulation dimensions overflow", file!(), line!()))
}

/// Generate exact OU paths from a nonzero MT19937/inverse-normal seed.
///
/// One normal is consumed for every path and step, even at zero horizon or
/// zero volatility. Full and terminal modes therefore share terminal bits.
///
/// # Errors
/// Rejects nonfinite parameters, negative speed/volatility/horizon, zero
/// steps/paths/seed, dimension overflow, allocation failure or nonfinite paths.
pub fn ou_paths(request: &OuRequest) -> QlResult<Vec<Real>> {
    require!(request.initial.is_finite(), "invalid initial value");
    require!(request.level.is_finite(), "invalid level");
    require!(
        request.speed.is_finite() && request.speed >= 0.0,
        "invalid speed"
    );
    require!(
        request.volatility.is_finite() && request.volatility >= 0.0,
        "invalid volatility"
    );
    require!(
        request.horizon.is_finite() && request.horizon >= 0.0,
        "invalid horizon"
    );
    require!(
        request.steps > 0 && request.paths > 0,
        "steps and paths must be positive"
    );
    require!(request.seed != 0, "seed must be nonzero");
    request
        .steps
        .checked_mul(request.paths)
        .ok_or_else(|| QlError::new("simulation dimensions overflow", file!(), line!()))?;
    let dt = request.horizon / request.steps as Real;
    require!(request.horizon == 0.0 || dt > 0.0, "time step underflows");
    let count = output_len(request)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| QlError::new("allocation failed", file!(), line!()))?;
    let process = OrnsteinUhlenbeckProcess::new(
        request.speed,
        request.volatility,
        request.initial,
        request.level,
    )?;
    let mut rng = PseudoRandom::make_sequence_generator(1, request.seed)?;
    for _ in 0..request.paths {
        let mut value = request.initial;
        if !request.terminal_only {
            output.push(value);
        }
        for step in 0..request.steps {
            let draw = rng.next_sequence().value[0];
            if dt != 0.0 {
                value = process.evolve(step as Real * dt, value, dt, draw)?;
                require!(value.is_finite(), "simulated value is nonfinite");
            }
            if !request.terminal_only {
                output.push(value);
            }
        }
        if request.terminal_only {
            output.push(value);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::montecarlo::simulation_kernel::gaussian_draws;

    fn request() -> OuRequest {
        OuRequest {
            initial: 1.0,
            level: 3.0,
            speed: 0.5,
            volatility: 0.2,
            horizon: 1.0,
            steps: 4,
            paths: 3,
            seed: 42,
            terminal_only: false,
        }
    }

    #[test]
    fn seeded_paths_match_pinned_multistep_fixture() {
        let expected = [
            1.0,
            1.2049197140571397,
            1.4938576175387845,
            1.8262101587088548,
            1.879255526298315,
            1.0,
            1.2932179159179402,
            1.5663072780654845,
            1.758274886469132,
            1.9272460691202662,
            1.0,
            1.139911950142801,
            1.3456668679224093,
            1.4449524117278607,
            1.50711446963386,
        ];
        let r = request();
        let full = ou_paths(&r).unwrap();
        assert_eq!(full.len(), expected.len());
        for (index, (&actual, &expected)) in full.iter().zip(&expected).enumerate() {
            assert!((actual - expected).abs() < 2e-15, "value {index}");
        }
        let terminal = ou_paths(&OuRequest {
            terminal_only: true,
            ..r
        })
        .unwrap();
        assert_eq!(terminal.len(), 3);
        for (path, &actual) in terminal.iter().enumerate() {
            assert!((actual - expected[path * 5 + 4]).abs() < 2e-15);
            assert_eq!(actual, full[path * 5 + 4]);
        }
    }

    #[test]
    fn first_transition_and_terminal_mode_share_exact_values() {
        let r = request();
        let values = ou_paths(&r).unwrap();
        let p = OrnsteinUhlenbeckProcess::new(r.speed, r.volatility, r.initial, r.level).unwrap();
        assert_eq!(
            values[1],
            p.evolve(0.0, r.initial, 0.25, gaussian_draws(1, 42).unwrap()[0])
                .unwrap()
        );
        let terminals = ou_paths(&OuRequest {
            terminal_only: true,
            ..r
        })
        .unwrap();
        for (i, terminal) in terminals.iter().enumerate() {
            assert_eq!(*terminal, values[i * 5 + 4]);
        }
    }

    #[test]
    fn zero_horizon_and_zero_volatility_are_deterministic() {
        let r = OuRequest {
            horizon: 0.0,
            ..request()
        };
        assert!(ou_paths(&r).unwrap().iter().all(|&v| v == r.initial));
        let r = OuRequest {
            volatility: 0.0,
            ..request()
        };
        assert_eq!(
            ou_paths(&r).unwrap(),
            ou_paths(&OuRequest { seed: 91, ..r }).unwrap()
        );
        let brownian = OuRequest {
            speed: 0.0,
            steps: 1,
            paths: 1,
            ..request()
        };
        let normal = gaussian_draws(1, brownian.seed).unwrap()[0];
        assert!(
            (ou_paths(&brownian).unwrap()[1]
                - (brownian.initial + brownian.volatility * brownian.horizon.sqrt() * normal))
                .abs()
                < 1e-15
        );
    }

    #[test]
    fn terminal_moments_match_analytic_transition() {
        let r = OuRequest {
            steps: 2,
            paths: 50_000,
            terminal_only: true,
            ..request()
        };
        let values = ou_paths(&r).unwrap();
        let process =
            OrnsteinUhlenbeckProcess::new(r.speed, r.volatility, r.initial, r.level).unwrap();
        let expected_mean = process.expectation(0.0, r.initial, r.horizon).unwrap();
        let expected_var = process.variance(0.0, r.initial, r.horizon).unwrap();
        let n = values.len() as Real;
        let mean = values.iter().sum::<Real>() / n;
        let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<Real>() / n;
        assert!((mean - expected_mean).abs() < 6.0 * (expected_var / n).sqrt());
        assert!((variance - expected_var).abs() < 6.0 * expected_var * (2.0 / n).sqrt());
    }

    #[test]
    fn invalid_requests_fail() {
        let cases = [
            OuRequest {
                initial: Real::NAN,
                ..request()
            },
            OuRequest {
                level: Real::INFINITY,
                ..request()
            },
            OuRequest {
                speed: -1.0,
                ..request()
            },
            OuRequest {
                volatility: -1.0,
                ..request()
            },
            OuRequest {
                horizon: -1.0,
                ..request()
            },
            OuRequest {
                horizon: Real::from_bits(1),
                ..request()
            },
            OuRequest {
                steps: 0,
                ..request()
            },
            OuRequest {
                paths: 0,
                ..request()
            },
            OuRequest {
                seed: 0,
                ..request()
            },
            OuRequest {
                steps: usize::MAX,
                ..request()
            },
            OuRequest {
                paths: usize::MAX,
                ..request()
            },
        ];
        for case in cases {
            assert!(ou_paths(&case).is_err());
        }
    }
}
