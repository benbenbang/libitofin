//! Floating-point proximity tests.
//!
//! Port of `ql/math/comparison.hpp`: [`close`] and [`close_enough`] decide
//! whether two reals are "equal" to within a small multiple of the machine
//! epsilon, the tolerance QuantLib uses throughout its numerical routines (root
//! finders, interpolators). They differ only in how the two magnitudes combine:
//! [`close`] requires the difference to be within tolerance of *both* (stricter),
//! [`close_enough`] of *either* (looser). The default strength `n = 42` matches
//! QuantLib.

use crate::types::Real;

/// `true` if `x` and `y` are within `42` epsilons of each other.
///
/// See [`close_n`] for the general form and the exact tolerance rule.
pub fn close(x: Real, y: Real) -> bool {
    close_n(x, y, 42)
}

/// `true` if `x` and `y` are within `n` epsilons of each other.
///
/// The comparison is relative to the operands' magnitudes, except when one is
/// exactly zero, where an absolute `(n * eps)^2` floor is used instead (a
/// relative test against zero is meaningless). Exact equality short-circuits, so
/// equal infinities compare equal.
pub fn close_n(x: Real, y: Real, n: usize) -> bool {
    // Exact equality is intentional here (QuantLib's `x == y` / `x == 0`): it
    // short-circuits the relative test and handles matching infinities.
    #[allow(clippy::float_cmp)]
    if x == y {
        return true;
    }
    let diff = (x - y).abs();
    let tolerance = n as Real * Real::EPSILON;
    #[allow(clippy::float_cmp)]
    if x == 0.0 || y == 0.0 {
        return diff < tolerance * tolerance;
    }
    diff <= tolerance * x.abs() && diff <= tolerance * y.abs()
}

/// `true` if `x` and `y` are within `42` epsilons of *either* operand.
///
/// The looser companion to [`close`]. See [`close_enough_n`] for the general
/// form.
pub fn close_enough(x: Real, y: Real) -> bool {
    close_enough_n(x, y, 42)
}

/// `true` if `x` and `y` are within `n` epsilons of *either* operand.
///
/// Identical to [`close_n`] but accepts the difference being within tolerance of
/// *either* magnitude rather than both, so it is the more permissive test.
pub fn close_enough_n(x: Real, y: Real, n: usize) -> bool {
    // Exact equality is intentional (QuantLib's `x == y` / `x == 0`), as in `close_n`.
    #[allow(clippy::float_cmp)]
    if x == y {
        return true;
    }
    let diff = (x - y).abs();
    let tolerance = n as Real * Real::EPSILON;
    #[allow(clippy::float_cmp)]
    if x == 0.0 || y == 0.0 {
        return diff < tolerance * tolerance;
    }
    diff <= tolerance * x.abs() || diff <= tolerance * y.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_values_are_close() {
        for x in [0.0, 1.0, -3.5, 1e300, Real::INFINITY] {
            assert!(close(x, x));
        }
    }

    #[test]
    fn neighbouring_floats_are_close_but_distinct_ones_are_not() {
        let x = 1.0;
        assert!(close(x, x + Real::EPSILON));
        assert!(!close(1.0, 1.0 + 1e-6));
    }

    #[test]
    fn zero_uses_an_absolute_floor() {
        // against zero the tolerance is (42*eps)^2 ~= 8.7e-29, so 1e-30 is close
        // but 1e-20 is not.
        assert!(close(0.0, 1e-30));
        assert!(!close(0.0, 1e-20));
    }

    #[test]
    fn strength_widens_the_tolerance() {
        let x = 1.0;
        let y = 1.0 + 50.0 * Real::EPSILON;
        assert!(!close_n(x, y, 42));
        assert!(close_n(x, y, 100));
    }

    #[test]
    fn close_enough_is_the_looser_or_variant() {
        // At the boundary the difference (42*eps) is within tolerance of x = 1 but
        // not of the slightly smaller y, so the OR-form accepts where AND rejects.
        let x = 1.0;
        let y = 1.0 - 42.0 * Real::EPSILON;
        assert!(!close(x, y));
        assert!(close_enough(x, y));
        // It still agrees with close on the ordinary cases.
        assert!(close_enough(3.5, 3.5));
        assert!(close_enough(0.0, 1e-30));
        assert!(!close_enough(1.0, 1.0 + 1e-6));
    }
}
