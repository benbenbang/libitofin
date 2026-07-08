//! Null sentinel values.
//!
//! Port of `ql/utilities/null.hpp`. QuantLib's `Null<T>` yields a per-type
//! sentinel ("not set"): for floating-point types the largest `float`, for
//! integral types the largest `int`. We expose it as a [`Null`] trait so any
//! numeric type can provide and recognize its sentinel.

use crate::types::{BigInteger, BigNatural, Integer, Natural, Real, Size};

/// Provides a per-type "null"/unset sentinel value.
pub trait Null: Sized + PartialEq {
    /// The sentinel value for this type.
    fn null() -> Self;

    /// Whether `self` equals the sentinel.
    fn is_null(&self) -> bool {
        *self == Self::null()
    }
}

impl Null for Real {
    fn null() -> Self {
        // a specific, unlikely value that fits into any Real (largest float)
        f32::MAX as Real
    }
}

macro_rules! impl_null_integral {
    ($($t:ty),*) => {
        $(
            impl Null for $t {
                fn null() -> Self {
                    // fits into any Integer (largest int)
                    Integer::MAX as $t
                }
            }
        )*
    };
}

impl_null_integral!(Integer, BigInteger, Natural, BigNatural, Size);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_null_is_largest_float() {
        assert_eq!(Real::null(), f32::MAX as Real);
        assert!(Real::null().is_null());
        assert!(!(1.0 as Real).is_null());
    }

    #[test]
    fn integral_null_is_largest_int() {
        assert_eq!(Integer::null(), Integer::MAX);
        assert_eq!(BigInteger::null(), Integer::MAX as BigInteger);
        assert_eq!(Size::null(), Integer::MAX as Size);
        assert!(Integer::MAX.is_null());
        assert!(!0_i32.is_null());
    }
}
