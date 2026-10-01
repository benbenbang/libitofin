//! Batched stateless numerics: no native object survives the call.
use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::methods::montecarlo::simulation_kernel::{self, GbmRequest, HestonRequest};
use libitofin::processes::HestonDiscretization as Discretization;
use libitofin::types::Real;

/// GBM inputs; arrays each contain `assets` doubles, correlation `assets*assets`.
#[repr(C)]
pub struct ItofinGbmInput {
    pub initial: *const Real,
    pub drift: *const Real,
    pub volatility: *const Real,
    pub correlation: *const Real,
    pub assets: usize,
    pub horizon: Real,
    pub steps: usize,
    pub paths: usize,
    pub seed: u32,
    /// 0: full path/time/asset output; 1: terminal path/asset output.
    pub terminal_only: i32,
}

/// Heston inputs with flat rates. Scheme 0 is QEM; scheme 1 is QE.
#[repr(C)]
pub struct ItofinHestonInput {
    pub spot: Real,
    pub variance: Real,
    pub risk_free_rate: Real,
    pub dividend_yield: Real,
    pub kappa: Real,
    pub theta: Real,
    pub sigma: Real,
    pub rho: Real,
    pub horizon: Real,
    pub steps: usize,
    pub paths: usize,
    pub seed: u32,
    pub scheme: i32,
    pub terminal_only: i32,
}

/// # Safety
/// Follow crate-level pointer contract; `out` holds at least `capacity` doubles.
unsafe fn copy_output(out: *mut Real, capacity: usize, values: &[Real]) -> BindingResult<()> {
    if capacity < values.len() {
        return Err(BindingError::invalid("output capacity too small"));
    }
    if !values.is_empty() {
        check_ptr(out)?;
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
        }
    }
    Ok(())
}

/// Generate `count` standard normal values with a deterministic nonzero seed.
/// # Safety
/// Follow the crate-level pointer contract. Output contains `count` doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gaussian_draws(
    count: usize,
    seed: u32,
    out: *mut Real,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if capacity < count {
                return Err(BindingError::invalid("output capacity too small"));
            }
            if count > 0 {
                check_ptr(out)?;
            }
            copy_output(
                out,
                capacity,
                &simulation_kernel::gaussian_draws(count, seed)?,
            )
        })
    }
}

/// Generate correlated GBM paths, including initial values in full-path mode.
/// # Safety
/// Input arrays and output obey the crate-level pointer/non-overlap contract.
/// Full capacity is paths*(steps+1)*assets; terminal capacity is paths*assets.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gbm_paths(
    input: *const ItofinGbmInput,
    out: *mut Real,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(input)?;
            let i = &*input;
            if i.terminal_only != 0 && i.terminal_only != 1 {
                return Err(BindingError::invalid("invalid terminal_only flag"));
            }
            let correlation_len = i
                .assets
                .checked_mul(i.assets)
                .ok_or_else(|| BindingError::invalid("correlation size overflow"))?;
            let request = GbmRequest {
                initial: input_slice(i.initial, i.assets)?,
                drift: input_slice(i.drift, i.assets)?,
                volatility: input_slice(i.volatility, i.assets)?,
                correlation: input_slice(i.correlation, correlation_len)?,
                horizon: i.horizon,
                steps: i.steps,
                paths: i.paths,
                seed: i.seed,
                terminal_only: i.terminal_only == 1,
            };
            let length = simulation_kernel::output_len(&request)?;
            if capacity < length {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out)?;
            copy_output(out, capacity, &simulation_kernel::gbm_paths(&request)?)
        })
    }
}

/// Generate Heston spot/variance paths in path/time/component order.
/// # Safety
/// Input and output obey the crate-level pointer/non-overlap contract.
/// Full capacity is paths*(steps+1)*2; terminal capacity is paths*2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_heston_paths(
    input: *const ItofinHestonInput,
    out: *mut Real,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(input)?;
            let i = &*input;
            if i.terminal_only != 0 && i.terminal_only != 1 {
                return Err(BindingError::invalid("invalid terminal_only flag"));
            }
            let discretization = match i.scheme {
                0 => Discretization::QuadraticExponentialMartingale,
                1 => Discretization::QuadraticExponential,
                _ => return Err(BindingError::invalid("unsupported Heston discretization")),
            };
            let request = HestonRequest {
                spot: i.spot,
                variance: i.variance,
                risk_free_rate: i.risk_free_rate,
                dividend_yield: i.dividend_yield,
                kappa: i.kappa,
                theta: i.theta,
                sigma: i.sigma,
                rho: i.rho,
                horizon: i.horizon,
                steps: i.steps,
                paths: i.paths,
                seed: i.seed,
                discretization,
                terminal_only: i.terminal_only == 1,
            };
            let length = simulation_kernel::heston_output_len(&request)?;
            if capacity < length {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out)?;
            copy_output(out, capacity, &simulation_kernel::heston_paths(&request)?)
        })
    }
}
