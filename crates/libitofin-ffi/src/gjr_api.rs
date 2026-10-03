//! Live GJR-GARCH processes and stateless seeded paths.
use crate::boundary::*;
use crate::market_api::quote;
use crate::time_api::date;
use libitofin::handle::Handle;
use libitofin::math::array::Array;
use libitofin::methods::montecarlo::gjr_paths::{self, GjrRequest};
use libitofin::processes::{GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;

/// Daily variance parameters; live states contain annualized variance.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinGjrParameters {
    pub daily_variance: f64,
    pub omega: f64,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub lambda: f64,
    pub days_per_year: f64,
}
impl From<ItofinGjrParameters> for GjrGarchParameters {
    fn from(p: ItofinGjrParameters) -> Self {
        Self {
            v0: p.daily_variance,
            omega: p.omega,
            alpha: p.alpha,
            beta: p.beta,
            gamma: p.gamma,
            lambda: p.lambda,
            days_per_year: p.days_per_year,
        }
    }
}
fn scheme(value: i32) -> BindingResult<GjrGarchDiscretization> {
    match value {
        0 => Ok(GjrGarchDiscretization::PartialTruncation),
        1 => Ok(GjrGarchDiscretization::FullTruncation),
        2 => Ok(GjrGarchDiscretization::Reflection),
        _ => Err(BindingError::invalid("unknown GJR-GARCH discretization")),
    }
}
fn validate_error(error: *mut ItofinError) -> BindingResult<()> {
    if !error.is_null() {
        check_ptr(error)?;
    }
    Ok(())
}
fn validate_output(out: *mut f64, capacity: usize, count: usize) -> BindingResult<()> {
    if capacity < count || capacity > isize::MAX as usize / size_of::<f64>() {
        return Err(BindingError::invalid("invalid output capacity"));
    }
    check_ptr(out)
}

/// Retain a live spot quote and live risk-free/dividend curves.
/// # Safety
/// All pointers obey the crate-level C caller contract. Parameters are read
/// during this call only. Context and handles belong to the calling thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_process_new(
    ctx: *mut Context,
    spot: u64,
    risk_free: u64,
    dividend: u64,
    params: *const ItofinGjrParameters,
    discretization: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            validate_error(error)?;
            check_ptr(out)?;
            check_ptr(params)?;
            let process = GjrGarchProcess::new(
                c.get::<Handle<dyn YieldTermStructure>>(risk_free)?,
                c.get::<Handle<dyn YieldTermStructure>>(dividend)?,
                quote(c, spot)?,
                (*params).into(),
                scheme(discretization)?,
            )?;
            output(out, c.insert(shared(process))?)
        })
    }
}

/// Query kind 0 initial state (2), 1 drift (2), 2 row-major diffusion (4),
/// 3 daily parameters (7, ordered as ItofinGjrParameters), or 4 scheme (1).
/// # Safety
/// Follow the crate-level caller contract. State is exactly two doubles for
/// kinds 1/2; it is ignored for kinds 0/3/4. Output holds `capacity` doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_process_query(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    t: f64,
    state: *const f64,
    state_len: usize,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            validate_error(error)?;
            let count = match kind {
                0 | 1 => 2,
                2 => 4,
                3 => 7,
                4 => 1,
                _ => return Err(BindingError::invalid("unknown GJR-GARCH query kind")),
            };
            validate_output(out, capacity, count)?;
            let p = c.get::<Shared<GjrGarchProcess>>(process)?;
            let values = match kind {
                0 => p.initial_values()?.to_vec(),
                1 | 2 => {
                    if state_len != 2 {
                        return Err(BindingError::invalid("GJR-GARCH state must have length 2"));
                    }
                    let state = Array::from(input_slice(state, state_len)?.to_vec());
                    if kind == 1 {
                        p.drift(t, &state)?.to_vec()
                    } else {
                        let matrix = p.diffusion(t, &state)?;
                        vec![
                            matrix[(0, 0)],
                            matrix[(0, 1)],
                            matrix[(1, 0)],
                            matrix[(1, 1)],
                        ]
                    }
                }
                3 => {
                    let p = p.parameters();
                    vec![
                        p.v0,
                        p.omega,
                        p.alpha,
                        p.beta,
                        p.gamma,
                        p.lambda,
                        p.days_per_year,
                    ]
                }
                4 => vec![match p.discretization() {
                    GjrGarchDiscretization::PartialTruncation => 0.0,
                    GjrGarchDiscretization::FullTruncation => 1.0,
                    GjrGarchDiscretization::Reflection => 2.0,
                }],
                _ => unreachable!(),
            };
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
            Ok(())
        })
    }
}

/// Evolve a two-component state with two independent standard Gaussian draws.
/// # Safety
/// Follow the crate-level caller contract. State/draws contain their stated
/// lengths, each exactly 2. Output contains at least `capacity` doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_process_evolve(
    ctx: *mut Context,
    process: u64,
    t0: f64,
    state: *const f64,
    state_len: usize,
    dt: f64,
    draws: *const f64,
    draws_len: usize,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            validate_error(error)?;
            validate_output(out, capacity, 2)?;
            if state_len != 2 || draws_len != 2 {
                return Err(BindingError::invalid(
                    "GJR-GARCH state and draws must have length 2",
                ));
            }
            let p = c.get::<Shared<GjrGarchProcess>>(process)?;
            let state = Array::from(input_slice(state, state_len)?.to_vec());
            let draws = Array::from(input_slice(draws, draws_len)?.to_vec());
            let values = p.evolve(t0, &state, dt, &draws)?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
            Ok(())
        })
    }
}

/// Convert a date using the retained risk-free curve's reference date/day counter.
/// # Safety
/// Follow the crate-level context, pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_process_time(
    ctx: *mut Context,
    process: u64,
    date_serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            validate_error(error)?;
            check_ptr(out)?;
            let p = c.get::<Shared<GjrGarchProcess>>(process)?;
            output(out, p.time(&date(date_serial)?)?)
        })
    }
}

/// Flat-rate parameters and seeded path dimensions. Full output is
/// `[path,time,2]`; terminal output is `[path,2]`, with spot then annual variance.
#[repr(C)]
pub struct ItofinGjrInput {
    pub spot: f64,
    pub daily_variance: f64,
    pub risk_free_rate: f64,
    pub dividend_yield: f64,
    pub omega: f64,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub lambda: f64,
    pub days_per_year: f64,
    pub horizon: f64,
    pub steps: usize,
    pub paths: usize,
    pub seed: u32,
    /// 0 partial truncation, 1 full truncation, 2 reflection.
    pub discretization: i32,
    /// 0 full paths including initial values; 1 terminal states only.
    pub terminal_only: i32,
}

/// Generate deterministic seeded paths, preserving output on any failure.
/// # Safety
/// Follow the crate-level caller contract; no pointer is retained. Output holds
/// `capacity` doubles and does not overlap input or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_paths(
    input: *const ItofinGjrInput,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            validate_error(error)?;
            check_ptr(input)?;
            let i = &*input;
            if !matches!(i.terminal_only, 0 | 1) {
                return Err(BindingError::invalid("invalid terminal_only flag"));
            }
            let request = GjrRequest {
                spot: i.spot,
                daily_variance: i.daily_variance,
                risk_free_rate: i.risk_free_rate,
                dividend_yield: i.dividend_yield,
                omega: i.omega,
                alpha: i.alpha,
                beta: i.beta,
                gamma: i.gamma,
                lambda: i.lambda,
                days_per_year: i.days_per_year,
                horizon: i.horizon,
                steps: i.steps,
                paths: i.paths,
                seed: i.seed,
                discretization: scheme(i.discretization)?,
                terminal_only: i.terminal_only == 1,
            };
            let count = gjr_paths::output_len(&request)?;
            validate_output(out, capacity, count)?;
            let values = gjr_paths::gjr_paths(&request)?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
            Ok(())
        })
    }
}
