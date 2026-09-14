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
        Self { code: INVALID_ARGUMENT, message: message.into() }
    }
}
impl From<QlError> for BindingError {
    fn from(value: QlError) -> Self {
        Self { code: CORE_ERROR, message: value.to_string() }
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
        Self { owner: thread::current().id(), poisoned: false, objects: HashMap::new() }
    }
    pub fn insert<T: 'static>(&mut self, value: T) -> BindingResult<u64> {
        self.objects.try_reserve(1).map_err(|_| BindingError::invalid("object allocation failed"))?;
        let id = NEXT_HANDLE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| BindingError::invalid("handle space exhausted"))?;
        self.objects.insert(id, Box::new(value));
        Ok(id)
    }
    pub fn get<T: Clone + 'static>(&self, id: u64) -> BindingResult<T> {
        self.objects.get(&id).and_then(|v| v.downcast_ref::<T>()).cloned()
            .ok_or_else(|| BindingError { code: INVALID_HANDLE, message: "unknown, released, foreign, or wrong-type handle".into() })
    }
    fn check(&self) -> BindingResult<()> {
        if self.owner != thread::current().id() {
            return Err(BindingError { code: WRONG_THREAD, message: "context belongs to another thread".into() });
        }
        if self.poisoned {
            return Err(BindingError { code: POISONED, message: "context was invalidated by a panic; close it".into() });
        }
        Ok(())
    }
}
impl Default for Context { fn default() -> Self { Self::new() } }

/// Validate a single C pointer before dereferencing; allocation validity is the caller's obligation.
pub fn check_ptr<T>(ptr: *const T) -> BindingResult<()> {
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<T>()) {
        Err(BindingError::invalid("null or misaligned pointer"))
    } else { Ok(()) }
}

/// # Safety
/// `ptr` must reference `len` initialized values for the returned borrow's lifetime.
pub unsafe fn input_slice<'a, T>(ptr: *const T, len: usize) -> BindingResult<&'a [T]> {
    if len == 0 { return Ok(&[]); }
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
    unsafe { ptr.write(value); }
    Ok(())
}

/// # Safety
/// `error`, when non-null, must be writable and aligned.
unsafe fn report(error: *mut ItofinError, result: BindingResult<()>) -> i32 {
    let (code, message) = match result { Ok(()) => (0, String::new()), Err(e) => (e.code, e.message) };
    if !error.is_null() {
        // Error pointers must satisfy the caller contract, too.
        if check_ptr(error).is_err() { return INVALID_ARGUMENT; }
        let mut buf = [0 as c_char; 1024];
        let mut end = message.len().min(1023);
        while !message.is_char_boundary(end) { end -= 1; }
        for (dst, src) in buf.iter_mut().zip(message.as_bytes()[..end].iter()) {
            *dst = if *src == 0 { b'?' as c_char } else { *src as c_char };
        }
        unsafe { error.write(ItofinError { code, message: buf }); }
    }
    code
}

/// # Safety
/// Context and error must satisfy the crate-level C caller contract.
pub unsafe fn with_context(
    ctx: *mut Context, error: *mut ItofinError,
    f: impl FnOnce(&mut Context) -> BindingResult<()>,
) -> i32 {
    let result = (|| {
        check_ptr(ctx)?;
        let context = unsafe { &mut *ctx };
        context.check()?;
        match catch_unwind(AssertUnwindSafe(|| f(context))) {
            Ok(result) => result,
            Err(_) => {
                context.poisoned = true;
                Err(BindingError { code: PANIC, message: "Rust panic contained; context invalidated".into() })
            }
        }
    })();
    unsafe { report(error, result) }
}

/// # Safety
/// Error and pointers used by `f` must satisfy the C caller contract.
pub unsafe fn without_context(error: *mut ItofinError, f: impl FnOnce() -> BindingResult<()>) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(BindingError { code: PANIC, message: "Rust panic contained".into() })
    });
    unsafe { report(error, result) }
}

/// ABI major version. Increment for incompatible layouts or calling conventions.
#[unsafe(no_mangle)]
pub extern "C" fn itofin_abi_version() -> u32 { 1 }

/// # Safety
/// `out` must be writable. Destroy the returned context on this same thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_context_new(out: *mut *mut Context, error: *mut ItofinError) -> i32 {
    unsafe { without_context(error, || {
        check_ptr(out)?;
        output(out, Box::into_raw(Box::new(Context::new())))
    }) }
}

/// Destroy the context and every remaining object, including a poisoned context.
/// # Safety
/// `ctx` must be a live context created here, on its owner thread, with no active calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_context_free(ctx: *mut Context, error: *mut ItofinError) -> i32 {
    unsafe { without_context(error, || {
        check_ptr(ctx)?;
        if (*ctx).owner != thread::current().id() {
            return Err(BindingError { code: WRONG_THREAD, message: "context belongs to another thread".into() });
        }
        drop(Box::from_raw(ctx));
        Ok(())
    }) }
}

/// Release one external reference; dependent native objects retain their references.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_handle_release(ctx: *mut Context, handle: u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        c.objects.remove(&handle).ok_or_else(|| BindingError {
            code: INVALID_HANDLE, message: "unknown, released, or foreign handle".into(),
        })?;
        Ok(())
    }) }
}
