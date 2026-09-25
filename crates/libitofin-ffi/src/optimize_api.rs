//! Context-free C API for `itofin-optimize`: Nelder-Mead over a C objective.
//!
//! No `ItofinContext` is involved, so an objective may call any other entry
//! point, including context APIs, while the solver runs.
use crate::boundary::*;
use itofin_optimize::{
    Common, Converged, Flow, IterationState, Method, MinimizeError, NelderMeadOptions, Objective,
    Problem, Termination, minimize,
};
use std::ffi::c_char;
use std::fmt;

/// Why a run stopped. Values are fixed and append-only: `7` (line search
/// failed) and `8` (infeasible) are reserved for later solvers.
pub type ItofinOptimizeStatus = i32;
pub const ITOFIN_OPTIMIZE_CONVERGED_XTOL: ItofinOptimizeStatus = 0;
pub const ITOFIN_OPTIMIZE_CONVERGED_FTOL: ItofinOptimizeStatus = 1;
pub const ITOFIN_OPTIMIZE_CONVERGED_GTOL: ItofinOptimizeStatus = 2;
pub const ITOFIN_OPTIMIZE_MAX_ITERATIONS: ItofinOptimizeStatus = 3;
pub const ITOFIN_OPTIMIZE_MAX_EVALUATIONS: ItofinOptimizeStatus = 4;
pub const ITOFIN_OPTIMIZE_CANCELLED: ItofinOptimizeStatus = 5;
pub const ITOFIN_OPTIMIZE_NONFINITE: ItofinOptimizeStatus = 6;

/// Borrowed solver state, valid only during an iteration callback.
#[repr(C)]
pub struct ItofinIterationState {
    pub x: *const f64,
    pub n: usize,
    pub fun: f64,
    pub nit: usize,
    pub nfev: usize,
    pub njev: usize,
}

/// A caller-supplied objective. `value` writes f(x) to its output and returns
/// zero, or returns nonzero after filling the error. The optional `callback`
/// runs after every iteration and sets `*stop` to cancel the run; a nonzero
/// return fails it. `release`, when set, is called exactly once before
/// `itofin_optimize_nelder_mead` returns whenever `objective` is non-null.
/// Callbacks must not unwind.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ItofinObjective {
    pub userdata: usize,
    pub value:
        Option<unsafe extern "C" fn(usize, *const f64, usize, *mut f64, *mut ItofinError) -> i32>,
    pub callback: Option<
        unsafe extern "C" fn(
            usize,
            *const ItofinIterationState,
            *mut bool,
            *mut ItofinError,
        ) -> i32,
    >,
    pub release: Option<unsafe extern "C" fn(usize)>,
}

/// Nelder-Mead options. A zero field keeps the solver default, so a
/// zero-initialized struct is valid; a zero tolerance is therefore not
/// expressible here.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinOptimizeOptions {
    pub maxiter: usize,
    pub maxfev: usize,
    pub xatol: f64,
    pub fatol: f64,
    pub adaptive: bool,
}

/// Run outcome. The caller sets `x` to a writable buffer of `n` values before
/// the call; every other field is written by it.
#[repr(C)]
pub struct ItofinOptimizeResult {
    pub x: *mut f64,
    pub fun: f64,
    pub nit: usize,
    pub nfev: usize,
    pub njev: usize,
    pub status: ItofinOptimizeStatus,
    pub success: bool,
}

#[derive(Debug)]
struct CallbackError(String);

impl fmt::Display for CallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CallbackError {}

struct Released(ItofinObjective);

impl Drop for Released {
    fn drop(&mut self) {
        if let Some(release) = self.0.release {
            unsafe { release(self.0.userdata) }
        }
    }
}

fn message(error: &ItofinError) -> String {
    let bytes: Vec<u8> = error
        .message
        .iter()
        .take_while(|v| **v != 0)
        .map(|v| *v as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn blank() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0 as c_char; 1024],
    }
}

impl Objective for Released {
    type Error = CallbackError;

    fn value(&mut self, x: &[f64]) -> Result<f64, CallbackError> {
        let value = self
            .0
            .value
            .ok_or_else(|| CallbackError("objective value is null".into()))?;
        let mut out = f64::NAN;
        let mut error = blank();
        match unsafe { value(self.0.userdata, x.as_ptr(), x.len(), &mut out, &mut error) } {
            0 => Ok(out),
            _ => Err(CallbackError(message(&error))),
        }
    }

    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, CallbackError> {
        let Some(callback) = self.0.callback else {
            return Ok(Flow::Continue);
        };
        let view = ItofinIterationState {
            x: state.x.as_ptr(),
            n: state.x.len(),
            fun: state.fun,
            nit: state.nit,
            nfev: state.nfev,
            njev: state.njev,
        };
        let mut stop = false;
        let mut error = blank();
        match unsafe { callback(self.0.userdata, &view, &mut stop, &mut error) } {
            0 if stop => Ok(Flow::Stop),
            0 => Ok(Flow::Continue),
            _ => Err(CallbackError(message(&error))),
        }
    }
}

fn status(termination: Termination) -> BindingResult<ItofinOptimizeStatus> {
    Ok(match termination {
        Termination::Converged(Converged::XTol) => ITOFIN_OPTIMIZE_CONVERGED_XTOL,
        Termination::Converged(Converged::FTol) => ITOFIN_OPTIMIZE_CONVERGED_FTOL,
        Termination::Converged(Converged::GTol) => ITOFIN_OPTIMIZE_CONVERGED_GTOL,
        Termination::MaxIterations => ITOFIN_OPTIMIZE_MAX_ITERATIONS,
        Termination::MaxEvaluations => ITOFIN_OPTIMIZE_MAX_EVALUATIONS,
        Termination::Cancelled => ITOFIN_OPTIMIZE_CANCELLED,
        Termination::Nonfinite => ITOFIN_OPTIMIZE_NONFINITE,
        other => {
            return Err(BindingError {
                code: CORE_ERROR,
                message: format!("unmapped termination: {other}"),
            });
        }
    })
}

fn nonzero<T: PartialEq + Default>(value: T) -> Option<T> {
    (value != T::default()).then_some(value)
}

/// Minimize `objective` from `x0` (length `n`) with Nelder-Mead.
///
/// Returns zero with `out_result` filled when the run reached the solver,
/// whatever its status. A rejected input returns `ITOFIN_INVALID_ARGUMENT`;
/// a failing `value` or `callback` returns `ITOFIN_CORE_ERROR` carrying its
/// message, truncated to 1023 bytes.
/// # Safety
/// `objective`, `x0`, `options` and `out_result` must satisfy the C caller
/// contract, and `out_result->x` must be writable for `n` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_nelder_mead(
    objective: *const ItofinObjective,
    x0: *const f64,
    n: usize,
    options: *const ItofinOptimizeOptions,
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
            let problem = Problem {
                x0: input_slice(x0, n)?.to_vec(),
                bounds: None,
            };
            if n > 0 {
                check_ptr((*out_result).x)?;
            }
            let method = Method::NelderMead(NelderMeadOptions {
                xatol: nonzero(options.xatol),
                fatol: nonzero(options.fatol),
                adaptive: options.adaptive,
                initial_simplex: None,
            });
            let common = Common {
                maxiter: nonzero(options.maxiter),
                maxfev: nonzero(options.maxfev),
                tol: None,
            };
            let run =
                minimize(&mut objective, &problem, &method, &common).map_err(|e| match e {
                    MinimizeError::InvalidInput(e) => BindingError::invalid(e.to_string()),
                    MinimizeError::Objective(e) => BindingError {
                        code: CORE_ERROR,
                        message: e.0,
                    },
                })?;
            let result = &mut *out_result;
            std::slice::from_raw_parts_mut(result.x, n).copy_from_slice(&run.x);
            result.fun = run.fun;
            result.nit = run.nit;
            result.nfev = run.nfev;
            result.njev = run.njev;
            result.status = status(run.status)?;
            result.success = run.success;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static RELEASES: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn release(_: usize) {
        RELEASES.fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "C" fn rosenbrock(
        _: usize,
        x: *const f64,
        n: usize,
        out: *mut f64,
        _: *mut ItofinError,
    ) -> i32 {
        let x = unsafe { std::slice::from_raw_parts(x, n) };
        unsafe { *out = 100.0 * (x[1] - x[0] * x[0]).powi(2) + (1.0 - x[0]).powi(2) };
        0
    }

    unsafe extern "C" fn failing(
        _: usize,
        _: *const f64,
        _: usize,
        _: *mut f64,
        error: *mut ItofinError,
    ) -> i32 {
        let error = unsafe { &mut *error };
        for (dst, src) in error.message.iter_mut().zip(b"objective exploded") {
            *dst = *src as c_char;
        }
        1
    }

    fn objective(value: bool) -> ItofinObjective {
        ItofinObjective {
            userdata: 3,
            value: if value { Some(rosenbrock) } else { None },
            callback: None,
            release: Some(release),
        }
    }

    fn run(
        objective: &ItofinObjective,
        x0: &[f64],
        options: &ItofinOptimizeOptions,
    ) -> (i32, ItofinOptimizeResult, Vec<f64>, ItofinError) {
        let mut x = vec![0.0; x0.len()];
        let mut result: ItofinOptimizeResult = unsafe { std::mem::zeroed() };
        result.x = x.as_mut_ptr();
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_nelder_mead(
                objective,
                x0.as_ptr(),
                x0.len(),
                options,
                &mut result,
                &mut error,
            )
        };
        (code, result, x, error)
    }

    #[test]
    fn nelder_mead_statuses_errors_and_single_release() {
        let defaults = ItofinOptimizeOptions::default();
        let before = RELEASES.load(Ordering::SeqCst);
        let (code, result, _, _) = run(
            &objective(true),
            &[-1.2, 1.0],
            &ItofinOptimizeOptions {
                maxiter: 5,
                ..defaults
            },
        );
        assert_eq!(
            (code, result.status, result.nit, result.success),
            (0, ITOFIN_OPTIMIZE_MAX_ITERATIONS, 5, false)
        );

        let mut fail = objective(true);
        fail.value = Some(failing);
        let (code, _, _, error) = run(&fail, &[1.0], &defaults);
        assert_eq!((code, error.code), (CORE_ERROR, CORE_ERROR));
        assert_eq!(message(&error), "objective exploded");

        let (code, _, _, error) = run(&objective(true), &[], &defaults);
        assert_eq!(
            (code, message(&error).as_str()),
            (INVALID_ARGUMENT, "x0 must not be empty")
        );
        let (code, _, _, _) = run(&objective(false), &[1.0], &defaults);
        assert_eq!(code, INVALID_ARGUMENT);
        assert_eq!(RELEASES.load(Ordering::SeqCst) - before, 4);
    }
}
