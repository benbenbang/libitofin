use super::*;
use itofin_optimize::FireflyOptions;

/// Firefly controls with presence flags preserving explicit zero alpha/gamma.
/// Zero-initialized options select alpha .25, beta0 1, gamma 1 and decay .97.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinFireflyOptions {
    pub global: ItofinGlobalOptions,
    pub alpha: f64,
    pub beta0: f64,
    pub gamma: f64,
    pub alpha_decay: f64,
    pub has_alpha: bool,
    pub has_beta0: bool,
    pub has_gamma: bool,
    pub has_alpha_decay: bool,
}

/// Minimize with seeded bounded firefly optimization. Bounds must be finite,
/// contain x0 and have finite widths. Optional initial rows are preserved exactly;
/// Zero rows and zero length mean automatic initialization; null options select
/// defaults. Row-major storage must contain `initial_rows * n` values. A supplied gradient is not used.
/// # Safety
/// Objective/options/output follow the `itofin_optimize_lbfgsb` contract.
/// Each input pointer must be readable for its declared length. Output fields
/// and its x buffer remain untouched when the call returns an error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_firefly(
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
    options: *const ItofinFireflyOptions,
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
            check_ptr(out_result)?;
            let options = if options.is_null() {
                ItofinFireflyOptions::default()
            } else {
                check_ptr(options)?;
                *options
            };
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
            decode_firefly(options, None)?;
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
            let (options, common) = decode_firefly(options, initial_population)?;
            let method = Method::Firefly(options);
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

pub(crate) fn decode_firefly(
    options: ItofinFireflyOptions,
    population: Option<Vec<Vec<f64>>>,
) -> BindingResult<(FireflyOptions, Common)> {
    let (shared, common) = decode_differential_evolution(
        ItofinDifferentialEvolutionOptions {
            global: options.global,
            ..Default::default()
        },
        population,
    )?;
    for (present, value, maximum, allow_zero, name) in [
        (options.has_alpha, options.alpha, 1.0, true, "alpha"),
        (options.has_beta0, options.beta0, 1.0, false, "beta0"),
        (options.has_gamma, options.gamma, 1_000_000.0, true, "gamma"),
        (
            options.has_alpha_decay,
            options.alpha_decay,
            1.0,
            false,
            "alpha_decay",
        ),
    ] {
        if present
            && (!value.is_finite()
                || value < 0.0
                || value > maximum
                || (!allow_zero && value == 0.0))
        {
            return Err(BindingError::invalid(format!("invalid firefly {name}")));
        }
    }
    let defaults = FireflyOptions::default();
    Ok((
        FireflyOptions {
            global: shared.global,
            alpha: if options.has_alpha {
                options.alpha
            } else {
                defaults.alpha
            },
            beta0: if options.has_beta0 {
                options.beta0
            } else {
                defaults.beta0
            },
            gamma: if options.has_gamma {
                options.gamma
            } else {
                defaults.gamma
            },
            alpha_decay: if options.has_alpha_decay {
                options.alpha_decay
            } else {
                defaults.alpha_decay
            },
        },
        common,
    ))
}

#[cfg(test)]
mod tests;
