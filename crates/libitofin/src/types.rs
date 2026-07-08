//! Custom numeric types.
//!
//! Port of `ql/types.hpp`. QuantLib defines these as `typedef`s over the
//! configurable macros `QL_REAL` (default `double`), `QL_INTEGER` (default
//! `int`) and `QL_BIG_INTEGER` (default `long`); we pin them to their default
//! widths.

/// Integer number (`QL_INTEGER`, default `int`).
pub type Integer = i32;

/// Large integer number (`QL_BIG_INTEGER`, default `long`).
pub type BigInteger = i64;

/// Positive integer (`unsigned QL_INTEGER`).
pub type Natural = u32;

/// Large positive integer (`unsigned QL_BIG_INTEGER`).
pub type BigNatural = u64;

/// Real number (`QL_REAL`, default `double`).
pub type Real = f64;

/// Complex number over [`Real`] (`std::complex<Real>`), backed by the
/// `num-complex` crate (design decision D9).
pub type Complex = num_complex::Complex<Real>;

/// Decimal number.
pub type Decimal = Real;

/// Size of a container (`std::size_t`).
pub type Size = usize;

/// Continuous quantity with 1-year units.
pub type Time = Real;

/// Discount factor between dates.
pub type DiscountFactor = Real;

/// Interest rate.
pub type Rate = Real;

/// Spread on an interest rate.
pub type Spread = Real;

/// Volatility.
pub type Volatility = Real;

/// Probability.
pub type Probability = Real;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_widths_match_quantlib_defaults() {
        assert_eq!(size_of::<Real>(), 8);
        assert_eq!(size_of::<Integer>(), 4);
        assert_eq!(size_of::<BigInteger>(), 8);
        assert_eq!(size_of::<Natural>(), 4);
        assert_eq!(size_of::<BigNatural>(), 8);
        assert_eq!(size_of::<Size>(), size_of::<usize>());
    }

    #[test]
    fn semantic_aliases_are_real() {
        let _t: Time = 1.0;
        let _r: Rate = 0.05;
        let _s: Spread = 0.01;
        let _v: Volatility = 0.2;
        let _d: Decimal = 2.5;
        let _df: DiscountFactor = 0.99;
        let _p: Probability = 0.5;
    }
}
