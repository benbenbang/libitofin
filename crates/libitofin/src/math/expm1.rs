//! Complex `expm1` and `log1p`.
//!
//! Port of `ql/math/expm1.cpp` (Klaus Spanderen, 2023): cancellation-safe
//! complex versions of `e^z - 1` and `log(1 + z)`. Near the origin the naive
//! `z.exp() - 1` and `(1 + z).ln()` lose leading digits to subtraction of two
//! nearly-equal quantities; these forms defer to the real [`f64::exp_m1`] and
//! [`f64::ln_1p`] on the dominant term to keep full precision. They are the
//! primitives the Andersen-Lake Heston characteristic function
//! ([`crate::pricingengines::vanilla::analytichestonengine`]) relies on.

use crate::types::Complex;

/// Complex `e^z - 1` (`expm1.cpp:25`).
///
/// For `|z| < 1` uses the real [`f64::exp_m1`] on the real part and the
/// half-angle identity `cos(b) - 1 = -2 sin^2(b/2)` on the imaginary part to
/// avoid cancellation; otherwise falls back to `z.exp() - 1`.
pub fn expm1(z: Complex) -> Complex {
    if z.norm() < 1.0 {
        let a = z.re;
        let b = z.im;
        let exp_1 = a.exp_m1();
        let cos_1 = -2.0 * (0.5 * b).sin().powi(2);
        Complex::new(exp_1 * cos_1 + exp_1 + cos_1, b.sin() * a.exp())
    } else {
        z.exp() - Complex::new(1.0, 0.0)
    }
}

/// Complex `log(1 + z)` (`expm1.cpp:41`).
///
/// For `|Re z| < 0.5` and `|Im z| < 0.5` uses the real [`f64::ln_1p`] on the
/// squared modulus of `1 + z` to avoid cancellation in the real part;
/// otherwise falls back to `(1 + z).ln()`.
pub fn log1p(z: Complex) -> Complex {
    let a = z.re;
    let b = z.im;
    if a.abs() < 0.5 && b.abs() < 0.5 {
        let re = 0.5 * (a * a + 2.0 * a + b * b).ln_1p();
        Complex::new(re, (Complex::new(1.0, 0.0) + z).arg())
    } else {
        (Complex::new(1.0, 0.0) + z).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Real;

    const EPS: Real = f64::EPSILON;

    /// `expm1` at a tiny argument keeps full precision where the naive
    /// `z.exp() - 1` loses leading digits.
    ///
    /// Reference: `ql/math/expm1.cpp:25`. Analytic value at
    /// `z = 1e-10 + 1e-10 i`: `e^z - 1 = z + z^2/2 + ... `. With `a = b = 1e-10`
    /// the `z^2/2` term is `Re = (a^2 - b^2)/2 = 0`, `Im = a*b = 1e-20`, so
    /// `Re = 1e-10` and `Im = 1e-10 + 1e-20` to full double precision.
    #[test]
    fn expm1_tiny_beats_naive() {
        let z = Complex::new(1e-10, 1e-10);

        let got = expm1(z);
        let naive = z.exp() - Complex::new(1.0, 0.0);

        let re_ref = 1e-10;
        let im_ref = 1e-10 + 1e-20;

        assert!((got.re - re_ref).abs() <= 4.0 * EPS * re_ref.abs());
        assert!((got.im - im_ref).abs() <= 4.0 * EPS * im_ref.abs());

        assert!((naive.re - re_ref).abs() > 1e3 * (got.re - re_ref).abs());
    }

    /// `log1p` near the origin keeps full precision where the naive
    /// `(1 + z).ln()` loses leading digits.
    ///
    /// Reference: `ql/math/expm1.cpp:41`. Analytic value at `z = 1e-10`:
    /// `log(1 + z) = z - z^2/2 + ... = 1e-10 - 0.5e-20 + ...` (real).
    #[test]
    fn log1p_tiny_beats_naive() {
        let z = Complex::new(1e-10, 0.0);

        let got = log1p(z);
        let naive = (Complex::new(1.0, 0.0) + z).ln();

        let re_ref = 1e-10 - 0.5 * 1e-10 * 1e-10;

        assert!((got.re - re_ref).abs() <= 4.0 * EPS * re_ref.abs());
        assert!((naive.re - re_ref).abs() > 1e3 * (got.re - re_ref).abs());
    }

    /// Both forms agree with the naive fallback away from the origin (the
    /// large-argument branch is literally the naive form).
    #[test]
    fn expm1_log1p_match_naive_far_from_origin() {
        let z = Complex::new(1.5, 0.8);
        let e = expm1(z);
        let e_naive = z.exp() - Complex::new(1.0, 0.0);
        assert!((e - e_naive).norm() <= 8.0 * EPS * e.norm());

        let l = log1p(z);
        let l_naive = (Complex::new(1.0, 0.0) + z).ln();
        assert!((l - l_naive).norm() <= 8.0 * EPS * l.norm());
    }
}
