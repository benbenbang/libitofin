//! Stateless constant-parameter Merton jump-diffusion simulation.
use crate::boundary::{BindingError, ItofinError, check_ptr, without_context};
use libitofin::methods::montecarlo::merton_paths::{self, MertonRequest};

/// Scalar annualized Merton parameters and seeded path dimensions.
/// Drift includes jumps; risk-neutral callers supply risk-free minus dividend.
#[repr(C)]
pub struct ItofinMertonInput {
    pub spot: f64,
    pub drift: f64,
    pub volatility: f64,
    pub jump_intensity: f64,
    pub log_mean_jump: f64,
    pub log_jump_volatility: f64,
    pub horizon: f64,
    pub steps: usize,
    pub paths: usize,
    pub seed: u32,
    /// 0 full `[path,time]` including initial spot; 1 terminal `[path]`.
    pub terminal_only: i32,
}

/// Generate exact constant-parameter paths, preserving outputs on any error.
/// # Safety
/// All pointers obey the crate-level C caller contract. Output must hold
/// `capacity` doubles and must not overlap input or error. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_merton_paths(
    input: *const ItofinMertonInput,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(input)?;
            let i = &*input;
            if i.terminal_only != 0 && i.terminal_only != 1 {
                return Err(BindingError::invalid("invalid terminal_only flag"));
            }
            let request = MertonRequest {
                spot: i.spot,
                drift: i.drift,
                volatility: i.volatility,
                jump_intensity: i.jump_intensity,
                log_mean_jump: i.log_mean_jump,
                log_jump_volatility: i.log_jump_volatility,
                horizon: i.horizon,
                steps: i.steps,
                paths: i.paths,
                seed: i.seed,
                terminal_only: i.terminal_only == 1,
            };
            let count = merton_paths::output_len(&request)?;
            if capacity < count {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out)?;
            let values = merton_paths::merton_paths(&request)?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
            Ok(())
        })
    }
}
