use super::*;
use itofin_optimize::ParticleSwarmOptions;

/// Particle-swarm controls. Presence flags preserve explicit zero coefficients.
/// Zero-initialized options select inertia .7, cognitive/social 1.4 and clamp .2.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinParticleSwarmOptions {
    pub global: ItofinGlobalOptions,
    pub inertia: f64,
    pub cognitive: f64,
    pub social: f64,
    pub velocity_clamp: f64,
    pub has_inertia: bool,
    pub has_cognitive: bool,
    pub has_social: bool,
    pub has_velocity_clamp: bool,
}

/// Minimize with seeded bounded particle swarm. Bounds must be finite,
/// contain x0 and have finite widths. Optional initial rows are preserved exactly;
/// zero rows and zero length mean automatic initialization. Row-major storage must
/// contain `initial_rows * n` values. A supplied gradient is not used.
/// # Safety
/// Objective/options/output follow the `itofin_optimize_lbfgsb` contract.
/// Each input pointer must be readable for its declared length. Output fields
/// and its x buffer remain untouched when the call returns an error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_particle_swarm(
    objective: *const ItofinObjective,
    x0: *const f64,
    n: usize,
    lower: *const f64,
    lower_len: usize,
    upper: *const f64,
    upper_len: usize,
    initial_population: *const f64,
    initial_rows: usize,
    initial_len: usize,
    options: *const ItofinParticleSwarmOptions,
    out_result: *mut ItofinOptimizeResult,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(objective)?;
            let mut objective = Released(*objective);
            if objective.0.value.is_none() {
                return Err(BindingError::invalid("objective value must not be null"));
            }
            check_ptr(options)?;
            check_ptr(out_result)?;
            let options = *options;
            if n == 0 || n > 256 {
                return Err(BindingError::invalid(
                    "global dimensions must be in 1..=256",
                ));
            }
            if lower_len != n || upper_len != n {
                return Err(BindingError::invalid("bounds length must match x0"));
            }
            let cells = initial_rows
                .checked_mul(n)
                .ok_or_else(|| BindingError::invalid("initial population shape overflows"))?;
            if cells != initial_len || cells > 1_000_000 {
                return Err(BindingError::invalid("invalid initial population shape"));
            }
            if initial_rows != 0 && !(4..=4096).contains(&initial_rows) {
                return Err(BindingError::invalid(
                    "initial population rows must be in 4..=4096",
                ));
            }
            let controls = options.global;
            if controls.population_size != 0 && !(4..=4096).contains(&controls.population_size) {
                return Err(BindingError::invalid("population_size must be in 4..=4096"));
            }
            if controls.population_size.saturating_mul(n) > 1_000_000
                || controls.maxiter > 1_000_000
                || controls.maxfev > 10_000_000
            {
                return Err(BindingError::invalid("global resource limit exceeded"));
            }
            check_ptr((*out_result).x)?;
            let initial_population = if initial_rows == 0 {
                None
            } else {
                Some(
                    input_slice(initial_population, initial_len)?
                        .chunks_exact(n)
                        .map(<[f64]>::to_vec)
                        .collect(),
                )
            };
            let problem = Problem {
                x0: input_slice(x0, n)?.to_vec(),
                bounds: Some(Bounds {
                    lower: input_slice(lower, lower_len)?.to_vec(),
                    upper: input_slice(upper, upper_len)?.to_vec(),
                }),
            };
            let (options, common) = decode_particle_swarm(options, initial_population)?;
            let method = Method::ParticleSwarm(options);
            let run =
                minimize(&mut objective, &problem, &method, &common).map_err(|e| match e {
                    MinimizeError::InvalidInput(e) => BindingError::invalid(e.to_string()),
                    MinimizeError::Objective(e) => BindingError {
                        code: CORE_ERROR,
                        message: e.0,
                    },
                })?;
            let run_status = status(run.status)?;
            let result = &mut *out_result;
            std::slice::from_raw_parts_mut(result.x, n).copy_from_slice(&run.x);
            result.fun = run.fun;
            result.nit = run.nit;
            result.nfev = run.nfev;
            result.njev = run.njev;
            result.status = run_status;
            result.success = run.success;
            Ok(())
        })
    }
}

pub(crate) fn decode_particle_swarm(
    options: ItofinParticleSwarmOptions,
    population: Option<Vec<Vec<f64>>>,
) -> BindingResult<(ParticleSwarmOptions, Common)> {
    let (shared, common) = decode_differential_evolution(
        ItofinDifferentialEvolutionOptions {
            global: options.global,
            ..Default::default()
        },
        population,
    )?;
    for (present, value, maximum, name) in [
        (options.has_inertia, options.inertia, 1.0, "inertia"),
        (options.has_cognitive, options.cognitive, 4.0, "cognitive"),
        (options.has_social, options.social, 4.0, "social"),
    ] {
        if present && (!value.is_finite() || !(0.0..=maximum).contains(&value)) {
            return Err(BindingError::invalid(format!(
                "{name} must be finite and in [0, {maximum}]"
            )));
        }
    }
    if options.has_velocity_clamp
        && (!options.velocity_clamp.is_finite()
            || options.velocity_clamp <= 0.0
            || options.velocity_clamp > 1.0)
    {
        return Err(BindingError::invalid("velocity_clamp must be in (0, 1]"));
    }
    let defaults = ParticleSwarmOptions::default();
    Ok((
        ParticleSwarmOptions {
            global: shared.global,
            inertia: if options.has_inertia {
                options.inertia
            } else {
                defaults.inertia
            },
            cognitive: if options.has_cognitive {
                options.cognitive
            } else {
                defaults.cognitive
            },
            social: if options.has_social {
                options.social
            } else {
                defaults.social
            },
            velocity_clamp: if options.has_velocity_clamp {
                options.velocity_clamp
            } else {
                defaults.velocity_clamp
            },
        },
        common,
    ))
}
