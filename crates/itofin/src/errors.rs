//! Error handling.
//!
//! Port of `ql/errors.{hpp,cpp}` (design decision D4). QuantLib throws
//! `QuantLib::Error` via the `QL_FAIL`, `QL_REQUIRE`, `QL_ASSERT` and
//! `QL_ENSURE` macros; we model failures as a [`QlError`] returned through
//! `Result<T, QlError>`, raised with the [`fail!`], [`require!`], [`assert_ql!`]
//! and [`ensure!`] macros.

use std::fmt;

use thiserror::Error;

/// Base error type, carrying the message and the source location that raised it.
#[derive(Debug, Clone, Error)]
pub struct QlError {
    message: String,
    file: &'static str,
    line: u32,
}

impl QlError {
    /// Builds an error from a message and the location it was raised at.
    pub fn new(message: impl Into<String>, file: &'static str, line: u32) -> Self {
        QlError {
            message: message.into(),
            file,
            line,
        }
    }

    /// The error message, without location information.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for QlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

/// Convenience alias for fallible core operations.
pub type QlResult<T> = Result<T, QlError>;

/// Raises a [`QlError`] by returning `Err`, capturing the call site.
///
/// Mirrors `QL_FAIL`. Accepts `format!`-style arguments.
#[macro_export]
macro_rules! fail {
    ($($arg:tt)*) => {
        return ::core::result::Result::Err($crate::errors::QlError::new(
            ::std::format!($($arg)*),
            ::core::file!(),
            ::core::line!(),
        ))
    };
}

/// Returns `Err` with a [`QlError`] if a pre-condition does not hold.
///
/// Mirrors `QL_REQUIRE`.
#[macro_export]
macro_rules! require {
    ($cond:expr, $($arg:tt)*) => {
        if !($cond) {
            $crate::fail!($($arg)*);
        }
    };
}

/// Returns `Err` with a [`QlError`] if a post-condition does not hold.
///
/// Mirrors `QL_ENSURE`.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !($cond) {
            $crate::fail!($($arg)*);
        }
    };
}

/// Returns `Err` with a [`QlError`] if an invariant does not hold.
///
/// Mirrors `QL_ASSERT`. Named `assert_ql!` to avoid clashing with the
/// std `assert!` macro.
#[macro_export]
macro_rules! assert_ql {
    ($cond:expr, $($arg:tt)*) => {
        if !($cond) {
            $crate::fail!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fails_with(value: i32) -> QlResult<i32> {
        fail!("bad value {value}");
    }

    fn requires_positive(value: i32) -> QlResult<i32> {
        require!(value > 0, "value must be positive, got {value}");
        Ok(value)
    }

    fn ensures_positive(value: i32) -> QlResult<i32> {
        ensure!(value > 0, "post-condition violated: {value}");
        Ok(value)
    }

    #[test]
    fn fail_returns_err_with_message() {
        let err = fails_with(7).unwrap_err();
        assert_eq!(err.message(), "bad value 7");
    }

    #[test]
    fn require_passes_and_fails() {
        assert_eq!(requires_positive(3).unwrap(), 3);
        let err = requires_positive(-1).unwrap_err();
        assert_eq!(err.message(), "value must be positive, got -1");
    }

    #[test]
    fn ensure_returns_err_on_violation() {
        assert!(ensures_positive(1).is_ok());
        assert!(ensures_positive(0).is_err());
    }

    #[test]
    fn display_includes_location() {
        let err = requires_positive(-1).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("errors.rs"));
        assert!(text.contains("value must be positive"));
    }
}
