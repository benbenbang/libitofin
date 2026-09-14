//! Library identity, separate from the C layout compatibility version.
use std::ffi::c_char;

/// Returns the package version as a static, NUL-terminated UTF-8 string.
/// The caller must not modify or free it. Valid for the loaded library lifetime.
#[unsafe(no_mangle)]
pub extern "C" fn itofin_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}
