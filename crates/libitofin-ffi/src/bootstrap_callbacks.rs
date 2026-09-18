//! Synchronous snapshot callbacks with final-owner release.
use crate::boundary::*;
use libitofin::errors::{QlError, QlResult};
use std::cell::Cell;

thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }

pub(crate) fn ensure_not_active() -> BindingResult<()> {
    if ACTIVE.get() {
        Err(BindingError::invalid(
            "native context reentry is not allowed inside bootstrap callbacks",
        ))
    } else {
        Ok(())
    }
}
struct Guard(bool);
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Callback output, valid only during its callback. Never retain this pointer.
pub struct ItofinBootstrapOutput {
    values: Vec<f64>,
}

/// Copy one callback result into native storage. Dates use integer serial values.
/// # Safety
/// `out` must be the current callback's output; values must satisfy the slice contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bootstrap_output_set(
    out: *mut ItofinBootstrapOutput,
    values: *const f64,
    count: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            (*out).values = input_slice(values, count)?.to_vec();
            Ok(())
        })
    }
}

/// Borrowed arrays valid only during a penalty callback; consumers must copy them.
#[repr(C)]
pub struct ItofinBootstrapState {
    pub times: *const f64,
    pub data: *const f64,
    pub node_count: usize,
    pub quote_values: *const f64,
    pub quote_count: usize,
    pub helper_errors: *const f64,
    pub helper_count: usize,
}

pub type BootstrapCallback = unsafe extern "C" fn(
    usize,
    *const ItofinBootstrapState,
    *mut ItofinBootstrapOutput,
    *mut ItofinError,
) -> i32;

/// Functions and integer userdata remain valid until release is called exactly once.
/// Callbacks must not unwind, retain borrowed pointers, or call context APIs.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ItofinBootstrapCallbacks {
    pub userdata: usize,
    pub penalties: Option<
        unsafe extern "C" fn(
            usize,
            *const ItofinBootstrapState,
            *mut ItofinBootstrapOutput,
            *mut ItofinError,
        ) -> i32,
    >,
    pub dates: Option<
        unsafe extern "C" fn(
            usize,
            *const ItofinBootstrapState,
            *mut ItofinBootstrapOutput,
            *mut ItofinError,
        ) -> i32,
    >,
    pub release: Option<unsafe extern "C" fn(usize)>,
}

pub(crate) struct Callbacks(pub ItofinBootstrapCallbacks);
impl Drop for Callbacks {
    fn drop(&mut self) {
        if let Some(release) = self.0.release {
            let _guard = Guard(ACTIVE.replace(true));
            unsafe {
                release(self.0.userdata);
            }
        }
    }
}
impl Callbacks {
    pub fn call(
        &self,
        callback: Option<BootstrapCallback>,
        name: &str,
        state: Option<&ItofinBootstrapState>,
    ) -> QlResult<Vec<f64>> {
        let Some(callback) = callback else {
            return Ok(Vec::new());
        };
        if ACTIVE.replace(true) {
            return Err(QlError::new("nested bootstrap callback", file!(), line!()));
        }
        let _guard = Guard(false);
        let mut result = ItofinBootstrapOutput { values: Vec::new() };
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let status = unsafe {
            callback(
                self.0.userdata,
                state.map_or(std::ptr::null(), |v| v),
                &mut result,
                &mut error,
            )
        };
        if status != 0 {
            let bytes: Vec<_> = error
                .message
                .iter()
                .take_while(|v| **v != 0)
                .map(|v| *v as u8)
                .collect();
            return Err(QlError::new(
                format!(
                    "{name} callback failed: {}",
                    String::from_utf8_lossy(&bytes)
                ),
                file!(),
                line!(),
            ));
        }
        Ok(result.values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RELEASES: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn release(userdata: usize) {
        unsafe {
            assert_eq!(
                itofin_context_free(userdata as *mut Context, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
        }
        RELEASES.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn reject_reentry(
        userdata: usize,
        _: *const ItofinBootstrapState,
        _: *mut ItofinBootstrapOutput,
        _: *mut ItofinError,
    ) -> i32 {
        let ctx = userdata as *mut Context;
        let mut value = 0.0;
        unsafe {
            assert_eq!(
                crate::market_api::itofin_quote_value(ctx, 0, &mut value, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_context_free(ctx, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
        }
        0
    }

    #[test]
    fn callback_reentry_and_failed_construction_release_once() {
        let mut ctx = std::ptr::null_mut();
        unsafe {
            assert_eq!(itofin_context_new(&mut ctx, std::ptr::null_mut()), 0);
        }
        let config = ItofinBootstrapCallbacks {
            userdata: ctx as usize,
            penalties: Some(reject_reentry),
            dates: None,
            release: Some(release),
        };
        let owner = std::rc::Rc::new(Callbacks(config));
        let retained = owner.clone();
        owner.call(owner.0.penalties, "test", None).unwrap();
        assert!(ensure_not_active().is_ok());
        drop(owner);
        assert_eq!(RELEASES.load(Ordering::SeqCst), 0);
        drop(retained);
        assert_eq!(RELEASES.load(Ordering::SeqCst), 1);
        let mut adopted = false;
        let mut id = 0;
        let status = unsafe {
            crate::bootstrap_api::itofin_global_curve_new(
                ctx,
                i32::MIN,
                std::ptr::null(),
                0,
                0,
                0,
                std::ptr::null(),
                0,
                0,
                &config,
                &mut adopted,
                &mut id,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(status, 0);
        assert!(adopted);
        assert_eq!(RELEASES.load(Ordering::SeqCst), 2);
        unsafe {
            assert_eq!(itofin_context_free(ctx, std::ptr::null_mut()), 0);
        }
    }
}
