//! Error function.
//!
//! Port of `ql/math/errorfunction.{hpp,cpp}`, which is Sun's fdlibm `erf`:
//! piecewise rational approximations selected by `|x|` region. Used by the
//! cumulative normal distribution. QuantLib wraps it in a stateless
//! `ErrorFunction` functor; here it is a free function.

// The coefficients are transcribed verbatim from QuantLib's fdlibm-derived
// source. Their trailing digits exceed f64 precision but round to exactly the
// intended bit pattern, so we keep them as-is for traceability against the
// upstream constants rather than truncating.
#![allow(clippy::excessive_precision)]

use crate::types::Real;

const TINY: Real = Real::EPSILON;
const ONE: Real = 1.0;
// c = (float)0.84506291151
const ERX: Real = 8.45062911510467529297e-01;

// Coefficients for the approximation to erf on [0, 0.84375].
const EFX: Real = 1.28379167095512586316e-01;
const EFX8: Real = 1.02703333676410069053e+00;
const PP0: Real = 1.28379167095512558561e-01;
const PP1: Real = -3.25042107247001499370e-01;
const PP2: Real = -2.84817495755985104766e-02;
const PP3: Real = -5.77027029648944159157e-03;
const PP4: Real = -2.37630166566501626084e-05;
const QQ1: Real = 3.97917223959155352819e-01;
const QQ2: Real = 6.50222499887672944485e-02;
const QQ3: Real = 5.08130628187576562776e-03;
const QQ4: Real = 1.32494738004321644526e-04;
const QQ5: Real = -3.96022827877536812320e-06;

// Coefficients for the approximation to erf on [0.84375, 1.25].
const PA0: Real = -2.36211856075265944077e-03;
const PA1: Real = 4.14856118683748331666e-01;
const PA2: Real = -3.72207876035701323847e-01;
const PA3: Real = 3.18346619901161753674e-01;
const PA4: Real = -1.10894694282396677476e-01;
const PA5: Real = 3.54783043256182359371e-02;
const PA6: Real = -2.16637559486879084300e-03;
const QA1: Real = 1.06420880400844228286e-01;
const QA2: Real = 5.40397917702171048937e-01;
const QA3: Real = 7.18286544141962662868e-02;
const QA4: Real = 1.26171219808761642112e-01;
const QA5: Real = 1.36370839120290507362e-02;
const QA6: Real = 1.19844998467991074170e-02;

// Coefficients for the approximation to erfc on [1.25, 1/0.35].
const RA0: Real = -9.86494403484714822705e-03;
const RA1: Real = -6.93858572707181764372e-01;
const RA2: Real = -1.05586262253232909814e+01;
const RA3: Real = -6.23753324503260060396e+01;
const RA4: Real = -1.62396669462573470355e+02;
const RA5: Real = -1.84605092906711035994e+02;
const RA6: Real = -8.12874355063065934246e+01;
const RA7: Real = -9.81432934416914548592e+00;
const SA1: Real = 1.96512716674392571292e+01;
const SA2: Real = 1.37657754143519042600e+02;
const SA3: Real = 4.34565877475229228821e+02;
const SA4: Real = 6.45387271733267880336e+02;
const SA5: Real = 4.29008140027567833386e+02;
const SA6: Real = 1.08635005541779435134e+02;
const SA7: Real = 6.57024977031928170135e+00;
const SA8: Real = -6.04244152148580987438e-02;

// Coefficients for the approximation to erfc on [1/0.35, 28].
const RB0: Real = -9.86494292470009928597e-03;
const RB1: Real = -7.99283237680523006574e-01;
const RB2: Real = -1.77579549177547519889e+01;
const RB3: Real = -1.60636384855821916062e+02;
const RB4: Real = -6.37566443368389627722e+02;
const RB5: Real = -1.02509513161107724954e+03;
const RB6: Real = -4.83519191608651397019e+02;
const SB1: Real = 3.03380607434824582924e+01;
const SB2: Real = 3.25792512996573918826e+02;
const SB3: Real = 1.53672958608443695994e+03;
const SB4: Real = 3.19985821950859553908e+03;
const SB5: Real = 2.55305040643316442583e+03;
const SB6: Real = 4.74528541206955367215e+02;
const SB7: Real = -2.24409524465858183362e+01;

/// The error function `erf(x) = (2/√π) ∫₀ˣ e^(−t²) dt`.
pub fn erf(x: Real) -> Real {
    if !x.is_finite() {
        return if x.is_nan() {
            x
        } else if x > 0.0 {
            1.0
        } else {
            -1.0
        };
    }

    let ax = x.abs();

    if ax < 0.84375 {
        if ax < 3.7252902984e-09 {
            // |x| < 2^-28
            if ax < Real::MIN_POSITIVE * 16.0 {
                return 0.125 * (8.0 * x + EFX8 * x); // avoid underflow
            }
            return x + EFX * x;
        }
        let z = x * x;
        let r = PP0 + z * (PP1 + z * (PP2 + z * (PP3 + z * PP4)));
        let s = ONE + z * (QQ1 + z * (QQ2 + z * (QQ3 + z * (QQ4 + z * QQ5))));
        return x + x * (r / s);
    }

    if ax < 1.25 {
        let s = ax - ONE;
        let p = PA0 + s * (PA1 + s * (PA2 + s * (PA3 + s * (PA4 + s * (PA5 + s * PA6)))));
        let q = ONE + s * (QA1 + s * (QA2 + s * (QA3 + s * (QA4 + s * (QA5 + s * QA6)))));
        return if x >= 0.0 { ERX + p / q } else { -ERX - p / q };
    }

    if ax >= 6.0 {
        return if x >= 0.0 { ONE - TINY } else { TINY - ONE };
    }

    // Asymptotic region: erfc(x) = (1/x)·exp(-x²-0.5625+R/S).
    let s = ONE / (ax * ax);
    let (r, denom) = if ax < 2.85714285714285 {
        // |x| < 1/0.35
        let r =
            RA0 + s * (RA1 + s * (RA2 + s * (RA3 + s * (RA4 + s * (RA5 + s * (RA6 + s * RA7))))));
        let denom = ONE
            + s * (SA1
                + s * (SA2 + s * (SA3 + s * (SA4 + s * (SA5 + s * (SA6 + s * (SA7 + s * SA8)))))));
        (r, denom)
    } else {
        let r = RB0 + s * (RB1 + s * (RB2 + s * (RB3 + s * (RB4 + s * (RB5 + s * RB6)))));
        let denom =
            ONE + s * (SB1 + s * (SB2 + s * (SB3 + s * (SB4 + s * (SB5 + s * (SB6 + s * SB7))))));
        (r, denom)
    };
    let r = (-ax * ax - 0.5625 + r / denom).exp();
    if x >= 0.0 { ONE - r / ax } else { r / ax - ONE }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values from high-precision tables; fdlibm erf is accurate to
    // ~1 ulp, so 1e-15 is comfortably within reach.
    const TOL: Real = 1e-15;

    fn close(a: Real, b: Real) -> bool {
        (a - b).abs() <= TOL
    }

    #[test]
    fn known_values_across_regions() {
        assert_eq!(erf(0.0), 0.0);
        // [0, 0.84375)
        assert!(close(erf(0.1), 0.112_462_916_018_284_89));
        assert!(close(erf(0.5), 0.520_499_877_813_046_5));
        // [0.84375, 1.25)
        assert!(close(erf(0.9), 0.796_908_212_422_832_1));
        assert!(close(erf(1.0), 0.842_700_792_949_714_9));
        // [1.25, 1/0.35)
        assert!(close(erf(1.5), 0.966_105_146_475_310_7));
        assert!(close(erf(2.0), 0.995_322_265_018_952_7));
        // [1/0.35, 6)
        assert!(close(erf(4.0), 0.999_999_984_582_742_1));
    }

    #[test]
    fn tiny_argument_uses_linear_branch() {
        // erf(x) ≈ (2/√π)·x for very small x
        let x = 1e-10;
        assert!(close(erf(x), x * (1.0 + EFX)));
    }

    #[test]
    fn odd_symmetry() {
        for &x in &[0.3, 0.95, 1.7, 3.0, 5.0] {
            assert!(close(erf(-x), -erf(x)));
        }
    }

    #[test]
    fn saturates_and_handles_nonfinite() {
        assert!(close(erf(6.0), 1.0));
        assert!(close(erf(-6.0), -1.0));
        assert_eq!(erf(Real::INFINITY), 1.0);
        assert_eq!(erf(Real::NEG_INFINITY), -1.0);
        assert!(erf(Real::NAN).is_nan());
    }
}
