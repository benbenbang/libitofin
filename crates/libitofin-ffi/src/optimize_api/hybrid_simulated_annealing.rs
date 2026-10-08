use super::*;
use itofin_optimize::HybridSimulatedAnnealingOptions;

/// Single-chain annealing controls. Presence flags preserve explicit values;
/// a zero-initialized record selects deterministic seed zero and solver defaults.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinHybridSimulatedAnnealingOptions {
    pub seed: u64,
    pub maxiter: usize,
    pub maxfev: usize,
    pub xatol: f64,
    pub fatol: f64,
    pub initial_temperature: f64,
    pub cooling_rate: f64,
    pub step_size: f64,
    pub local_search_interval: usize,
    pub local_search_steps: usize,
    pub reanneal_interval: usize,
    pub has_xatol: bool,
    pub has_fatol: bool,
    pub has_initial_temperature: bool,
    pub has_cooling_rate: bool,
    pub has_step_size: bool,
    pub has_local_search_interval: bool,
    pub has_local_search_steps: bool,
    pub has_reanneal_interval: bool,
}

/// Minimize a signed scalar cost using seeded bounded hybrid annealing.
/// Bounds are mandatory, finite, contain x0 and have finite widths.
/// A null options pointer selects defaults. A supplied gradient is not used.
/// # Safety
/// The objective and output follow `itofin_optimize_lbfgsb` ownership rules.
/// Inputs must be readable for their lengths; result.x must hold n doubles.
/// Errors leave the result and its x buffer untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_hybrid_simulated_annealing(
    objective: *const ItofinObjective,
    x0: *const f64,
    n: usize,
    lower: *const f64,
    lower_len: usize,
    upper: *const f64,
    upper_len: usize,
    options: *const ItofinHybridSimulatedAnnealingOptions,
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
            if n == 0 || n > 256 || lower_len != n || upper_len != n {
                return Err(BindingError::invalid(
                    "dimensions must be in 1..=256 and bounds must match x0",
                ));
            }
            let options = if options.is_null() {
                ItofinHybridSimulatedAnnealingOptions::default()
            } else {
                check_ptr(options)?;
                *options
            };
            let (options, common) = decode_hybrid_simulated_annealing(options)?;
            check_ptr((*out_result).x)?;
            let problem = Problem {
                x0: input_slice(x0, n)?.to_vec(),
                bounds: Some(Bounds {
                    lower: input_slice(lower, lower_len)?.to_vec(),
                    upper: input_slice(upper, upper_len)?.to_vec(),
                }),
            };
            let run = minimize(
                &mut objective,
                &problem,
                &Method::HybridSimulatedAnnealing(options),
                &common,
            )
            .map_err(|e| match e {
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

pub(crate) fn decode_hybrid_simulated_annealing(
    options: ItofinHybridSimulatedAnnealingOptions,
) -> BindingResult<(HybridSimulatedAnnealingOptions, Common)> {
    if options.maxiter > 1_000_000 || options.maxfev > 10_000_000 {
        return Err(BindingError::invalid("annealing resource limit exceeded"));
    }
    for (present, value) in [
        (options.has_xatol, options.xatol),
        (options.has_fatol, options.fatol),
    ] {
        if present && (!value.is_finite() || value < 0.0) {
            return Err(BindingError::invalid(
                "annealing tolerances must be finite and nonnegative",
            ));
        }
    }
    if options.has_initial_temperature
        && (!options.initial_temperature.is_finite() || options.initial_temperature <= 0.0)
    {
        return Err(BindingError::invalid(
            "initial_temperature must be finite and positive",
        ));
    }
    if options.has_cooling_rate
        && (!options.cooling_rate.is_finite()
            || options.cooling_rate <= 0.0
            || options.cooling_rate >= 1.0)
    {
        return Err(BindingError::invalid("cooling_rate must be in (0, 1)"));
    }
    if options.has_step_size
        && (!options.step_size.is_finite() || options.step_size <= 0.0 || options.step_size > 1.0)
    {
        return Err(BindingError::invalid("step_size must be in (0, 1]"));
    }
    for (present, value, maximum, name) in [
        (
            options.has_local_search_interval,
            options.local_search_interval,
            1_000_000,
            "local_search_interval",
        ),
        (
            options.has_local_search_steps,
            options.local_search_steps,
            256,
            "local_search_steps",
        ),
        (
            options.has_reanneal_interval,
            options.reanneal_interval,
            1_000_000,
            "reanneal_interval",
        ),
    ] {
        if present && !(1..=maximum).contains(&value) {
            return Err(BindingError::invalid(format!(
                "{name} must be in 1..={maximum}"
            )));
        }
    }
    let defaults = HybridSimulatedAnnealingOptions::default();
    Ok((
        HybridSimulatedAnnealingOptions {
            seed: options.seed,
            xatol: options.has_xatol.then_some(options.xatol),
            fatol: options.has_fatol.then_some(options.fatol),
            initial_temperature: if options.has_initial_temperature {
                options.initial_temperature
            } else {
                defaults.initial_temperature
            },
            cooling_rate: if options.has_cooling_rate {
                options.cooling_rate
            } else {
                defaults.cooling_rate
            },
            step_size: if options.has_step_size {
                options.step_size
            } else {
                defaults.step_size
            },
            local_search_interval: if options.has_local_search_interval {
                options.local_search_interval
            } else {
                defaults.local_search_interval
            },
            local_search_steps: if options.has_local_search_steps {
                options.local_search_steps
            } else {
                defaults.local_search_steps
            },
            reanneal_interval: if options.has_reanneal_interval {
                options.reanneal_interval
            } else {
                defaults.reanneal_interval
            },
        },
        Common {
            maxiter: nonzero(options.maxiter),
            maxfev: nonzero(options.maxfev),
            tol: None,
        },
    ))
}

#[cfg(test)]
mod tests;
