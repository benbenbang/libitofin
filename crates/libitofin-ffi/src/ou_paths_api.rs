//! Stateless OU simulation C ABI.

use crate::boundary::{BindingError, ItofinError, check_ptr, without_context};
use libitofin::methods::montecarlo::ou_paths::{self, OuRequest};
use libitofin::types::Real;

/// Scalar inputs for exact OU simulation.
#[repr(C)]
pub struct ItofinOuInput {
    pub initial: Real,
    pub level: Real,
    pub speed: Real,
    pub volatility: Real,
    pub horizon: Real,
    pub steps: usize,
    pub paths: usize,
    pub seed: u32,
    /// 0: full `[path,time]`; 1: terminal `[path]`.
    pub terminal_only: i32,
}

/// Generate OU paths into a caller-owned buffer.
///
/// # Safety
/// Input, output and error pointers obey the crate-level pointer contract;
/// `out` must hold at least `capacity` doubles and must not overlap `input`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_ou_paths(
    input: *const ItofinOuInput,
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
            let request = OuRequest {
                initial: i.initial,
                level: i.level,
                speed: i.speed,
                volatility: i.volatility,
                horizon: i.horizon,
                steps: i.steps,
                paths: i.paths,
                seed: i.seed,
                terminal_only: i.terminal_only == 1,
            };
            let count = ou_paths::output_len(&request)?;
            if capacity < count {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out)?;
            let values = ou_paths::ou_paths(&request)?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ItofinOuInput {
        ItofinOuInput {
            initial: 1.0,
            level: 3.0,
            speed: 0.5,
            volatility: 0.2,
            horizon: 1.0,
            steps: 2,
            paths: 3,
            seed: 42,
            terminal_only: 0,
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
        let mut i = ItofinOuInput {
            steps: 4,
            ..input()
        };
        let mut output = [0.0; 15];
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let status = unsafe { itofin_ou_paths(&i, output.as_mut_ptr(), output.len(), &mut error) };
        assert_eq!(status, 0);
        for (index, (&actual, &expected)) in output.iter().zip(&expected).enumerate() {
            assert!((actual - expected).abs() < 2e-15, "value {index}");
        }
        i.terminal_only = 1;
        let mut terminal = [0.0; 3];
        let status =
            unsafe { itofin_ou_paths(&i, terminal.as_mut_ptr(), terminal.len(), &mut error) };
        assert_eq!(status, 0);
        for (path, &actual) in terminal.iter().enumerate() {
            assert!((actual - expected[path * 5 + 4]).abs() < 2e-15);
            assert_eq!(actual, output[path * 5 + 4]);
        }
    }

    #[test]
    fn output_is_preserved_on_invalid_inputs() {
        let mut output = [91.0; 9];
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let mut i = input();
        let status = unsafe { itofin_ou_paths(&i, output.as_mut_ptr(), output.len(), &mut error) };
        assert_eq!(status, 0);
        let expected = ou_paths::ou_paths(&OuRequest {
            initial: i.initial,
            level: i.level,
            speed: i.speed,
            volatility: i.volatility,
            horizon: i.horizon,
            steps: i.steps,
            paths: i.paths,
            seed: i.seed,
            terminal_only: false,
        })
        .unwrap();
        assert_eq!(output.as_slice(), expected.as_slice());
        output.fill(91.0);
        i.speed = -1.0;
        let status = unsafe { itofin_ou_paths(&i, output.as_mut_ptr(), output.len(), &mut error) };
        assert_ne!(status, 0);
        assert_eq!(output, [91.0; 9]);
        i.speed = 0.5;
        let status = unsafe { itofin_ou_paths(&i, output.as_mut_ptr(), 8, &mut error) };
        assert_ne!(status, 0);
        assert_eq!(output, [91.0; 9]);
    }
}
