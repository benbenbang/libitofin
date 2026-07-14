//! Volatility model tag.
//!
//! Port of `ql/termstructures/volatility/volatilitytype.hpp`. The variant
//! selects the pricing model a volatility quote is expressed in: a
//! (shifted) lognormal Black volatility or a normal Bachelier volatility.

/// The model a volatility is quoted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolatilityType {
    /// (Shifted) lognormal Black volatility.
    ShiftedLognormal,
    /// Normal (Bachelier) volatility.
    Normal,
}
