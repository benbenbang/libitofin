//! Ownership, error transport, and panic containment shared by C entry points.
use std::any::Any;
use std::collections::HashMap;
use std::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, ThreadId};

use libitofin::errors::QlError;

pub const INVALID_ARGUMENT: i32 = 1;
pub const INVALID_HANDLE: i32 = 2;
pub const CORE_ERROR: i32 = 3;
pub const WRONG_THREAD: i32 = 4;
pub const PANIC: i32 = 5;
pub const POISONED: i32 = 6;

/// Caller-owned error. Zero code means success; message is NUL-terminated UTF-8.
#[repr(C)]
pub struct ItofinError {
    pub code: i32,
    pub message: [c_char; 1024],
}

#[derive(Debug)]
pub struct BindingError {
    pub code: i32,
    pub message: String,
}
pub type BindingResult<T> = Result<T, BindingError>;
impl BindingError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: INVALID_ARGUMENT,
            message: message.into(),
        }
    }
}
impl From<QlError> for BindingError {
    fn from(value: QlError) -> Self {
        Self {
            code: CORE_ERROR,
            message: value.to_string(),
        }
    }
}

/// Opaque thread-confined owner of live native objects. Never copy this value.
pub struct Context {
    owner: ThreadId,
    poisoned: bool,
    objects: HashMap<u64, Box<dyn Any>>,
}
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

impl Context {
    pub fn new() -> Self {
        Self {
            owner: thread::current().id(),
            poisoned: false,
            objects: HashMap::new(),
        }
    }
    pub fn insert<T: 'static>(&mut self, value: T) -> BindingResult<u64> {
        self.objects
            .try_reserve(1)
            .map_err(|_| BindingError::invalid("object allocation failed"))?;
        let id = NEXT_HANDLE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| BindingError::invalid("handle space exhausted"))?;
        self.objects.insert(id, Box::new(value));
        Ok(id)
    }
    pub fn get<T: Clone + 'static>(&self, id: u64) -> BindingResult<T> {
        self.objects
            .get(&id)
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
            .ok_or_else(|| BindingError {
                code: INVALID_HANDLE,
                message: "unknown, released, foreign, or wrong-type handle".into(),
            })
    }
    fn check(&self) -> BindingResult<()> {
        if self.owner != thread::current().id() {
            return Err(BindingError {
                code: WRONG_THREAD,
                message: "context belongs to another thread".into(),
            });
        }
        if self.poisoned {
            return Err(BindingError {
                code: POISONED,
                message: "context was invalidated by a panic; close it".into(),
            });
        }
        Ok(())
    }
}
impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a single C pointer before dereferencing; allocation validity is the caller's obligation.
pub fn check_ptr<T>(ptr: *const T) -> BindingResult<()> {
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<T>()) {
        Err(BindingError::invalid("null or misaligned pointer"))
    } else {
        Ok(())
    }
}

/// # Safety
/// `ptr` must reference `len` initialized values for the returned borrow's lifetime.
pub unsafe fn input_slice<'a, T>(ptr: *const T, len: usize) -> BindingResult<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    check_ptr(ptr)?;
    if len > (isize::MAX as usize) / std::mem::size_of::<T>().max(1) {
        return Err(BindingError::invalid("slice length overflow"));
    }
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// # Safety
/// `ptr` must reference a writable, non-overlapping `T`.
pub unsafe fn output<T>(ptr: *mut T, value: T) -> BindingResult<()> {
    check_ptr(ptr)?;
    unsafe {
        ptr.write(value);
    }
    Ok(())
}

/// # Safety
/// `error`, when non-null, must be writable and aligned.
unsafe fn report(error: *mut ItofinError, result: BindingResult<()>) -> i32 {
    let (code, message) = match result {
        Ok(()) => (0, String::new()),
        Err(e) => (e.code, e.message),
    };
    if !error.is_null() {
        // Error pointers must satisfy the caller contract, too.
        if check_ptr(error).is_err() {
            return INVALID_ARGUMENT;
        }
        let mut buf = [0 as c_char; 1024];
        let mut end = message.len().min(1023);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        for (dst, src) in buf.iter_mut().zip(message.as_bytes()[..end].iter()) {
            *dst = if *src == 0 {
                b'?' as c_char
            } else {
                *src as c_char
            };
        }
        unsafe {
            error.write(ItofinError { code, message: buf });
        }
    }
    code
}

/// # Safety
/// Context and error must satisfy the crate-level C caller contract.
pub unsafe fn with_context(
    ctx: *mut Context,
    error: *mut ItofinError,
    f: impl FnOnce(&mut Context) -> BindingResult<()>,
) -> i32 {
    let result = (|| {
        crate::bootstrap_callbacks::ensure_not_active()?;
        check_ptr(ctx)?;
        let context = unsafe { &mut *ctx };
        context.check()?;
        match catch_unwind(AssertUnwindSafe(|| f(context))) {
            Ok(result) => result,
            Err(_) => {
                context.poisoned = true;
                Err(BindingError {
                    code: PANIC,
                    message: "Rust panic contained; context invalidated".into(),
                })
            }
        }
    })();
    unsafe { report(error, result) }
}

/// # Safety
/// Error and pointers used by `f` must satisfy the C caller contract.
pub unsafe fn without_context(
    error: *mut ItofinError,
    f: impl FnOnce() -> BindingResult<()>,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(BindingError {
            code: PANIC,
            message: "Rust panic contained".into(),
        })
    });
    unsafe { report(error, result) }
}

/// ABI major version. Increment for incompatible layouts or calling conventions.
#[unsafe(no_mangle)]
pub extern "C" fn itofin_abi_version() -> u32 {
    1
}

/// # Safety
/// `out` must be writable. Destroy the returned context on this same thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_context_new(
    out: *mut *mut Context,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            output(out, Box::into_raw(Box::new(Context::new())))
        })
    }
}

/// Destroy the context and every remaining object, including a poisoned context.
/// # Safety
/// `ctx` must be a live context created here, on its owner thread, with no active calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_context_free(ctx: *mut Context, error: *mut ItofinError) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(ctx)?;
            crate::bootstrap_callbacks::ensure_not_active()?;
            if (*ctx).owner != thread::current().id() {
                return Err(BindingError {
                    code: WRONG_THREAD,
                    message: "context belongs to another thread".into(),
                });
            }
            drop(Box::from_raw(ctx));
            Ok(())
        })
    }
}

/// Release one external reference; dependent native objects retain their references.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_handle_release(
    ctx: *mut Context,
    handle: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.objects.remove(&handle).ok_or_else(|| BindingError {
                code: INVALID_HANDLE,
                message: "unknown, released, or foreign handle".into(),
            })?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn typed_context_handles_are_unique_scoped_and_released() {
        let mut first = Context::new();
        let mut second = Context::new();
        let id = first.insert(42_u64).unwrap();
        let other = second.insert(42_u64).unwrap();
        assert_ne!(id, other);
        assert_eq!(first.get::<u64>(id).unwrap(), 42);
        assert_eq!(first.get::<String>(id).unwrap_err().code, INVALID_HANDLE);
        assert_eq!(second.get::<u64>(id).unwrap_err().code, INVALID_HANDLE);
        unsafe {
            assert_eq!(
                itofin_handle_release(&mut second, id, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(itofin_handle_release(&mut first, id, null_mut()), 0);
            assert_eq!(
                itofin_handle_release(&mut first, id, null_mut()),
                INVALID_HANDLE
            );
        }
        assert_eq!(first.get::<u64>(id).unwrap_err().code, INVALID_HANDLE);
        assert_ne!(first.insert(99_u64).unwrap(), id);
        assert_eq!(second.get::<u64>(other).unwrap(), 42);
    }

    #[test]
    fn wrong_thread_calls_and_destruction_preserve_owner_context() {
        let mut context = null_mut();
        unsafe {
            assert_eq!(itofin_context_new(&mut context, null_mut()), 0);
        }
        let address = context as usize;
        // Parent makes no calls until join. The context allocation stays live;
        // the wrong-thread checks run before any thread-confined object access.
        let status = std::thread::spawn(move || unsafe {
            let ptr = address as *mut Context;
            let operation = with_context(ptr, null_mut(), |_| {
                panic!("wrong-thread closure must not execute")
            });
            let destruction = itofin_context_free(ptr, null_mut());
            (operation, destruction)
        })
        .join()
        .unwrap();
        assert_eq!(status, (WRONG_THREAD, WRONG_THREAD));
        unsafe {
            assert_eq!(
                with_context(context, null_mut(), |c| {
                    let id = c.insert(7_u64)?;
                    assert_eq!(c.get::<u64>(id)?, 7);
                    Ok(())
                }),
                0
            );
            assert_eq!(itofin_context_free(context, null_mut()), 0);
        }
    }

    #[test]
    fn panic_poisoning_blocks_further_calls_but_allows_destruction() {
        let mut context = null_mut();
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        unsafe {
            assert_eq!(itofin_context_new(&mut context, &mut error), 0);
            assert_eq!(
                with_context(context, &mut error, |c| {
                    c.insert(1_u64)?;
                    panic!("intentional boundary test panic")
                }),
                PANIC
            );
            assert_eq!(error.code, PANIC);
            assert_eq!(
                with_context(context, &mut error, |_| {
                    panic!("poisoned context closure must not execute")
                }),
                POISONED
            );
            assert_eq!(error.code, POISONED);
            assert_eq!(itofin_context_free(context, &mut error), 0);
            assert_eq!(error.code, 0);
        }
    }

    #[test]
    fn errors_are_utf8_truncated_and_success_clears_them() {
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        unsafe {
            assert_eq!(
                without_context(&mut error, || {
                    Err(BindingError::invalid("é".repeat(1024)))
                }),
                INVALID_ARGUMENT
            );
            let bytes = error
                .message
                .iter()
                .map(|x| *x as u8)
                .take_while(|x| *x != 0)
                .collect::<Vec<_>>();
            assert_eq!(bytes.len(), 1022);
            assert!(std::str::from_utf8(&bytes).is_ok());
            assert_eq!(without_context(&mut error, || Ok(())), 0);
            assert_eq!(error.code, 0);
            assert_eq!(error.message[0], 0);
            assert_eq!(
                with_context(null_mut(), &mut error, |_| Ok(())),
                INVALID_ARGUMENT
            );
        }
    }
}
