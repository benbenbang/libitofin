//! Context-free C API for `itofin-optimize`.
//!
//! No `ItofinContext` is involved, so an objective may call any other entry
//! point, including context APIs, while the solver runs.
use crate::boundary::*;
use itofin_optimize::{
    BfgsOptions, Bounds, Common, Converged, FiniteDifference, Flow, IterationState, LbfgsbOptions,
    Method, MinimizeError, NelderMeadOptions, Objective, Problem, Termination, minimize,
};
use std::ffi::c_char;
use std::fmt;

/// Why a run stopped. Values are fixed and append-only: `8` (infeasible)
/// is reserved for a constrained solver.
pub type ItofinOptimizeStatus = i32;
pub const ITOFIN_OPTIMIZE_CONVERGED_XTOL: ItofinOptimizeStatus = 0;
pub const ITOFIN_OPTIMIZE_CONVERGED_FTOL: ItofinOptimizeStatus = 1;
pub const ITOFIN_OPTIMIZE_CONVERGED_GTOL: ItofinOptimizeStatus = 2;
pub const ITOFIN_OPTIMIZE_MAX_ITERATIONS: ItofinOptimizeStatus = 3;
pub const ITOFIN_OPTIMIZE_MAX_EVALUATIONS: ItofinOptimizeStatus = 4;
pub const ITOFIN_OPTIMIZE_CANCELLED: ItofinOptimizeStatus = 5;
pub const ITOFIN_OPTIMIZE_NONFINITE: ItofinOptimizeStatus = 6;
pub const ITOFIN_OPTIMIZE_LINE_SEARCH_FAILED: ItofinOptimizeStatus = 7;

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
/// either optimizer returns whenever `objective` is non-null. `gradient` is
/// appended to preserve the original field offsets; Nelder-Mead only reads
/// the original prefix for compatibility with existing compiled callers.
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
    pub gradient:
        Option<unsafe extern "C" fn(usize, *const f64, usize, *mut f64, *mut ItofinError) -> i32>,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct LegacyObjective {
    userdata: usize,
    value:
        Option<unsafe extern "C" fn(usize, *const f64, usize, *mut f64, *mut ItofinError) -> i32>,
    callback: Option<
        unsafe extern "C" fn(
            usize,
            *const ItofinIterationState,
            *mut bool,
            *mut ItofinError,
        ) -> i32,
    >,
    release: Option<unsafe extern "C" fn(usize)>,
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

/// BFGS options. Zero values select defaults. `finite_difference` is 0 for
/// forward and 1 for central differences; any other value is rejected.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinBfgsOptions {
    pub gtol: f64,
    pub eps: f64,
    pub finite_difference: i32,
    pub maxiter: usize,
}

/// L-BFGS-B options. Zero values select solver defaults. Bounds are supplied
/// separately to `itofin_optimize_lbfgsb` as two equally sized arrays.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ItofinLbfgsbOptions {
    pub maxcor: usize,
    pub ftol: f64,
    pub gtol: f64,
    pub eps: f64,
    pub maxiter: usize,
    pub maxfev: usize,
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

    fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, CallbackError> {
        let Some(gradient) = self.0.gradient else {
            return Ok(false);
        };
        let mut error = blank();
        match unsafe {
            gradient(
                self.0.userdata,
                x.as_ptr(),
                x.len(),
                out.as_mut_ptr(),
                &mut error,
            )
        } {
            0 => Ok(true),
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
        Termination::LineSearchFailed => ITOFIN_OPTIMIZE_LINE_SEARCH_FAILED,
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
            let old = &*objective.cast::<LegacyObjective>();
            let mut objective = Released(ItofinObjective {
                userdata: old.userdata,
                value: old.value,
                callback: old.callback,
                release: old.release,
                gradient: None,
            });
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

/// Minimize `objective` from `x0` with BFGS. The optional gradient writes `n`
/// components; without it, the selected finite difference is used.
/// # Safety
/// Pointers must satisfy the same contract as `itofin_optimize_nelder_mead`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_bfgs(
    objective: *const ItofinObjective,
    x0: *const f64,
    n: usize,
    options: *const ItofinBfgsOptions,
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
            let finite_difference = match options.finite_difference {
                0 => FiniteDifference::Forward,
                1 => FiniteDifference::Central,
                _ => return Err(BindingError::invalid("invalid finite_difference")),
            };
            let problem = Problem {
                x0: input_slice(x0, n)?.to_vec(),
                bounds: None,
            };
            if n > 0 {
                check_ptr((*out_result).x)?;
            }
            let method = Method::Bfgs(BfgsOptions {
                gtol: nonzero(options.gtol),
                eps: nonzero(options.eps),
                finite_difference,
                ..BfgsOptions::default()
            });
            let common = Common {
                maxiter: nonzero(options.maxiter),
                ..Common::default()
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

/// Minimize with box-constrained L-BFGS-B. `lower` and `upper` each contain
/// `n` values, with `-INFINITY` or `INFINITY` marking an open lower or upper
/// side respectively. Both null pointers with zero lengths mean no bounds.
/// The optional gradient writes `n` components; otherwise bounded finite
/// differences are used. Zero option fields select solver defaults.
/// # Safety
/// `objective`, `x0`, `options` and `out_result` follow the same contract as
/// `itofin_optimize_bfgs`. Each non-null bound pointer must be readable for
/// its declared length. `lower_len` and `upper_len` are checked against `n`
/// before either array is read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optimize_lbfgsb(
    objective: *const ItofinObjective,
    x0: *const f64,
    n: usize,
    lower: *const f64,
    lower_len: usize,
    upper: *const f64,
    upper_len: usize,
    options: *const ItofinLbfgsbOptions,
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
            let bounds = if lower.is_null() && upper.is_null() && lower_len == 0 && upper_len == 0 {
                None
            } else {
                if lower_len != n || upper_len != n {
                    return Err(BindingError::invalid("bounds length must match x0"));
                }
                Some(Bounds {
                    lower: input_slice(lower, lower_len)?.to_vec(),
                    upper: input_slice(upper, upper_len)?.to_vec(),
                })
            };
            let problem = Problem {
                x0: input_slice(x0, n)?.to_vec(),
                bounds,
            };
            if n > 0 {
                check_ptr((*out_result).x)?;
            }
            let method = Method::Lbfgsb(LbfgsbOptions {
                maxcor: nonzero(options.maxcor),
                ftol: nonzero(options.ftol),
                gtol: nonzero(options.gtol),
                eps: nonzero(options.eps),
                ..LbfgsbOptions::default()
            });
            let common = Common {
                maxiter: nonzero(options.maxiter),
                maxfev: nonzero(options.maxfev),
                ..Common::default()
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
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static RELEASES: AtomicUsize = AtomicUsize::new(0);
    static RELEASE_LOCK: Mutex<()> = Mutex::new(());

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

    unsafe extern "C" fn rosenbrock_gradient(
        _: usize,
        x: *const f64,
        n: usize,
        out: *mut f64,
        _: *mut ItofinError,
    ) -> i32 {
        let x = unsafe { std::slice::from_raw_parts(x, n) };
        let out = unsafe { std::slice::from_raw_parts_mut(out, n) };
        out[0] = -400.0 * x[0] * (x[1] - x[0] * x[0]) + 2.0 * (x[0] - 1.0);
        out[1] = 200.0 * (x[1] - x[0] * x[0]);
        0
    }

    fn objective(value: bool) -> ItofinObjective {
        ItofinObjective {
            userdata: 3,
            value: if value { Some(rosenbrock) } else { None },
            gradient: None,
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
        let _guard = RELEASE_LOCK.lock().unwrap();
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

    #[test]
    fn nelder_mead_accepts_the_pre_gradient_objective_layout() {
        let legacy = LegacyObjective {
            userdata: 0,
            value: Some(rosenbrock),
            callback: None,
            release: None,
        };
        let x0 = [-1.2, 1.0];
        let mut x = [0.0; 2];
        let mut result = ItofinOptimizeResult {
            x: x.as_mut_ptr(),
            fun: 0.0,
            nit: 0,
            nfev: 0,
            njev: 0,
            status: 0,
            success: false,
        };
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_nelder_mead(
                (&legacy as *const LegacyObjective).cast(),
                x0.as_ptr(),
                2,
                &ItofinOptimizeOptions::default(),
                &mut result,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert!(result.success && result.fun < 1e-6);
    }

    #[test]
    fn bfgs_gradient_and_finite_difference_accounting() {
        let _guard = RELEASE_LOCK.lock().unwrap();
        let options = ItofinBfgsOptions::default();
        let x0 = [-1.2, 1.0];
        let before = RELEASES.load(Ordering::SeqCst);
        let mut gradient = objective(true);
        gradient.gradient = Some(rosenbrock_gradient);
        let mut analytic_x = [0.0; 2];
        let mut analytic = ItofinOptimizeResult {
            x: analytic_x.as_mut_ptr(),
            fun: 0.0,
            nit: 0,
            nfev: 0,
            njev: 0,
            status: 0,
            success: false,
        };
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_bfgs(
                &gradient,
                x0.as_ptr(),
                2,
                &options,
                &mut analytic,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert!(analytic.success && analytic.fun < 1e-8 && analytic.njev > 0);

        let mut numeric_x = [0.0; 2];
        let mut numeric = ItofinOptimizeResult {
            x: numeric_x.as_mut_ptr(),
            ..analytic
        };
        let code = unsafe {
            itofin_optimize_bfgs(
                &objective(true),
                x0.as_ptr(),
                2,
                &options,
                &mut numeric,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert!(numeric.success && numeric.nfev > analytic.nfev);
        let bad = ItofinBfgsOptions {
            finite_difference: 2,
            ..options
        };
        let code = unsafe {
            itofin_optimize_bfgs(
                &objective(true),
                x0.as_ptr(),
                2,
                &bad,
                &mut numeric,
                &mut error,
            )
        };
        assert_eq!(code, INVALID_ARGUMENT);
        assert_eq!(RELEASES.load(Ordering::SeqCst) - before, 3);
    }

    #[test]
    fn lbfgsb_bounds_length_and_single_release() {
        let _guard = RELEASE_LOCK.lock().unwrap();
        let x0 = [0.0, 0.0];
        let lower = [0.0, -f64::INFINITY];
        let upper = [0.5, f64::INFINITY];
        let mut x = [0.0; 2];
        let mut result = ItofinOptimizeResult {
            x: x.as_mut_ptr(),
            fun: 0.0,
            nit: 0,
            nfev: 0,
            njev: 0,
            status: 0,
            success: false,
        };
        let options = ItofinLbfgsbOptions::default();
        let mut error = blank();
        let before = RELEASES.load(Ordering::SeqCst);
        let code = unsafe {
            itofin_optimize_lbfgsb(
                &objective(true),
                x0.as_ptr(),
                2,
                lower.as_ptr(),
                2,
                upper.as_ptr(),
                2,
                &options,
                &mut result,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert!(result.success && x[0] >= 0.0 && x[0] <= 0.5);
        let code = unsafe {
            itofin_optimize_lbfgsb(
                &objective(true),
                x0.as_ptr(),
                2,
                lower.as_ptr(),
                1,
                upper.as_ptr(),
                2,
                &options,
                &mut result,
                &mut error,
            )
        };
        assert_eq!(code, INVALID_ARGUMENT);
        assert_eq!(message(&error), "bounds length must match x0");
        let mut rejected = |lo, lo_n, hi, hi_n, n| unsafe {
            itofin_optimize_lbfgsb(
                &objective(true),
                x0.as_ptr(),
                n,
                lo,
                lo_n,
                hi,
                hi_n,
                &options,
                &mut result,
                &mut error,
            )
        };
        assert_eq!(
            rejected(lower.as_ptr(), 2, upper.as_ptr(), 1, 2),
            INVALID_ARGUMENT
        );
        assert_eq!(
            rejected(std::ptr::null(), 2, upper.as_ptr(), 2, 2),
            INVALID_ARGUMENT
        );
        let inverted = [-1.0, -1.0];
        assert_eq!(
            rejected(lower.as_ptr(), 2, inverted.as_ptr(), 2, 2),
            INVALID_ARGUMENT
        );
        assert_eq!(
            rejected(std::ptr::null(), 0, std::ptr::null(), 0, 0),
            INVALID_ARGUMENT
        );
        assert_eq!(RELEASES.load(Ordering::SeqCst) - before, 6);
    }
}
