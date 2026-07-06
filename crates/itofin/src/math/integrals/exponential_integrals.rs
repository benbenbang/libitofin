//! Sine and cosine integrals `Si` and `Ci`.
//!
//! Port of `ql/math/integrals/exponentialintegrals.*`. The real-valued sine
//! integral [`si`] and cosine integral [`ci`] use rational (minimax)
//! approximations for `|x| <= 4` and an asymptotic form built from the
//! auxiliary functions `f` and `g` beyond that. The complex-valued family
//! ([`ei`], [`e1`]) follows Pegoraro & Slusallek, "On the Evaluation of the
//! Complex-Valued Exponential Integral", combining a power series, an
//! asymptotic series and a continued fraction by region.

#![allow(clippy::excessive_precision)]

use crate::errors::QlResult;
use crate::types::{Complex, Real, Size};
use crate::{fail, require};

use std::f64::consts::PI;

/// The Euler-Mascheroni constant (QuantLib's `M_EULER_MASCHERONI`).
const EULER_MASCHERONI: Real = 0.5772156649015328606065120900824024;

/// Evaluates `coeffs[0] + coeffs[1] w + coeffs[2] w^2 + ...` by Horner's rule.
fn horner(w: Real, coeffs: &[Real]) -> Real {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * w + c)
}

// Auxiliary-function `f` numerator/denominator polynomials in w = 1/x^2.
const F_NUM: [Real; 11] = [
    1.0,
    7.44437068161936700618e2,
    1.96396372895146869801e5,
    2.37750310125431834034e7,
    1.43073403821274636888e9,
    4.33736238870432522765e10,
    6.40533830574022022911e11,
    4.20968180571076940208e12,
    1.00795182980368574617e13,
    4.94816688199951963482e12,
    -4.94701168645415959931e11,
];
const F_DEN: [Real; 10] = [
    1.0,
    7.46437068161927678031e2,
    1.97865247031583951450e5,
    2.41535670165126845144e7,
    1.47478952192985464958e9,
    4.58595115847765779830e10,
    7.08501308149515401563e11,
    5.06084464593475076774e12,
    1.43468549171581016479e13,
    1.11535493509914254097e13,
];

/// Auxiliary function `f` for the large-argument asymptotics of `Si`/`Ci`.
fn f_aux(x: Real) -> Real {
    let w = 1.0 / (x * x);
    horner(w, &F_NUM) / (x * horner(w, &F_DEN))
}

// Auxiliary-function `g` numerator (times w) / denominator polynomials.
const G_NUM: [Real; 11] = [
    1.0,
    8.1359520115168615e2,
    2.35239181626478200e5,
    3.12557570795778731e7,
    2.06297595146763354e9,
    6.83052205423625007e10,
    1.09049528450362786e12,
    7.57664583257834349e12,
    1.81004487464664575e13,
    6.43291613143049485e12,
    -1.36517137670871689e12,
];
const G_DEN: [Real; 10] = [
    1.0,
    8.19595201151451564e2,
    2.40036752835578777e5,
    3.26026661647090822e7,
    2.23355543278099360e9,
    7.87465017341829930e10,
    1.39866710696414565e12,
    1.17164723371736605e13,
    4.01839087307656620e13,
    3.99653257887490811e13,
];

/// Auxiliary function `g` for the large-argument asymptotics of `Si`/`Ci`.
fn g_aux(x: Real) -> Real {
    let w = 1.0 / (x * x);
    w * horner(w, &G_NUM) / horner(w, &G_DEN)
}

// Rational approximation of Si(x) for x <= 4, in w = x^2.
const SI_NUM: [Real; 8] = [
    1.0,
    -4.54393409816329991e-2,
    1.15457225751016682e-3,
    -1.41018536821330254e-5,
    9.43280809438713025e-8,
    -3.53201978997168357e-10,
    7.08240282274875911e-13,
    -6.05338212010422477e-16,
];
const SI_DEN: [Real; 7] = [
    1.0,
    1.01162145739225565e-2,
    4.99175116169755106e-5,
    1.55654986308745614e-7,
    3.28067571055789734e-10,
    4.5049097575386581e-13,
    3.21107051193712168e-16,
];

// Rational approximation of Ci(x) for x <= 4, in w = x^2. The numerator is
// multiplied by w (there is no constant term).
const CI_NUM: [Real; 7] = [
    -0.25,
    7.51851524438898291e-3,
    -1.27528342240267686e-4,
    1.05297363846239184e-6,
    -4.68889508144848019e-9,
    1.06480802891189243e-11,
    -9.93728488857585407e-15,
];
const CI_DEN: [Real; 8] = [
    1.0,
    1.1592605689110735e-2,
    6.72126800814254432e-5,
    2.55533277086129636e-7,
    6.97071295760958946e-10,
    1.38536352772778619e-12,
    1.89106054713059759e-15,
    1.39759616731376855e-18,
];

/// The sine integral `Si(x) = \int_0^x sin(t)/t dt`, defined for all real `x`
/// (odd: `Si(-x) = -Si(x)`).
pub fn si(x: Real) -> Real {
    if x < 0.0 {
        return -si(-x);
    }
    if x <= 4.0 {
        let w = x * x;
        return x * horner(w, &SI_NUM) / horner(w, &SI_DEN);
    }
    std::f64::consts::FRAC_PI_2 - f_aux(x) * x.cos() - g_aux(x) * x.sin()
}

/// The cosine integral `Ci(x) = \gamma + ln(x) + \int_0^x (cos(t)-1)/t dt`,
/// defined for `x > 0`.
///
/// For `x < 0` the cosine integral is complex (`Ci(x) = Ci(-x) + i pi`), so this
/// real-valued function returns `NaN` there, following the `f64` convention for
/// out-of-domain arguments; `Ci(0)` is `-inf`.
pub fn ci(x: Real) -> Real {
    if x < 0.0 {
        return Real::NAN;
    }
    if x <= 4.0 {
        let w = x * x;
        return EULER_MASCHERONI + x.ln() + w * horner(w, &CI_NUM) / horner(w, &CI_DEN);
    }
    f_aux(x) * x.sin() - g_aux(x) * x.cos()
}

/// Sign function matching `boost::math::sign`: `0` at zero (either signed
/// zero), otherwise `+1`/`-1`. Distinct from `f64::signum`, which maps `-0.0`
/// to `-1.0`.
fn sign(x: Real) -> Real {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `Ei(z) + acc`, where `acc` carries the branch adjustment used by [`e1`].
fn ei_with_acc(z: Complex, acc: Complex) -> QlResult<Complex> {
    if z.re == 0.0 && z.im == 0.0 {
        return Ok(Complex::new(Real::NEG_INFINITY, 0.0));
    }

    const DIST: Real = 4.5;
    const MAX_ERROR: Real = 5.0 * Real::EPSILON;

    let z_inf = (0.01 * Real::MAX).ln() + 100.0_f64.ln();
    let below_overflow_threshold = z.re < z_inf;
    require!(below_overflow_threshold, "argument error {z}");

    let z_asym = 2.0 - 1.035 * MAX_ERROR.ln();
    let abs_z = z.norm();

    let matches = |z1: Complex, z2: Complex| -> bool {
        let d = z1 - z2;
        d.re.abs() <= MAX_ERROR * z1.re.abs() && d.im.abs() <= MAX_ERROR * z1.im.abs()
    };

    if z.re > z_inf {
        return Ok(z.exp() / z + acc);
    }

    if abs_z > 1.1 * z_asym {
        let mut ei = acc + Complex::new(0.0, sign(z.im) * PI);
        let mut s = z.exp() / z;
        let last = abs_z.floor() as Size + 1;
        for i in 1..=last {
            if matches(ei + s, ei) {
                return Ok(ei + s);
            }
            ei += s;
            s *= i as Real / z;
        }
        fail!("series conversion issue for Ei({z})");
    }

    if abs_z > DIST && (z.re < 0.0 || z.im.abs() > DIST) {
        let mut cf = Complex::new(0.0, 0.0);
        for k in (1..=47u32).rev() {
            cf = -((k * k) as Real / (2.0 * k as Real + 1.0 - z + cf));
        }
        return Ok((acc + Complex::new(0.0, sign(z.im) * PI)) - z.exp() / (1.0 - z + cf));
    }

    let mut s = Complex::new(0.0, 0.0);
    let mut sn = z;
    let mut nn: Real = 1.0;
    let mut n: Size = 2;
    while n < 1000 && s + sn * nn != s {
        s += sn * nn;
        if (n & 1) != 0 {
            nn += 1.0 / (2.0 * (n / 2) as Real + 1.0);
        }
        sn *= -z / (2.0 * n as Real);
        n += 1;
    }
    require!(n < 1000, "series conversion issue for Ei({z})");

    let r = (EULER_MASCHERONI + acc) + z.ln() + (0.5 * z).exp() * s;
    if z.im != 0.0 {
        Ok(r)
    } else {
        Ok(Complex::new(r.re, acc.im))
    }
}

/// The complex exponential integral `Ei(z)` (principal branch).
///
/// `Ei(0)` is `-inf`; arguments with a real part beyond the overflow threshold
/// of `exp` are out of domain and return `Err`.
pub fn ei(z: Complex) -> QlResult<Complex> {
    ei_with_acc(z, Complex::new(0.0, 0.0))
}

/// The complex exponential integral `E1(z) = -Ei(-z)` on the principal branch.
pub fn e1(z: Complex) -> QlResult<Complex> {
    let acc = if z.im < 0.0 {
        Complex::new(0.0, -PI)
    } else if z.im > 0.0 || z.re < 0.0 {
        Complex::new(0.0, PI)
    } else {
        Complex::new(0.0, 0.0)
    };
    Ok(-(ei_with_acc(-z, acc)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // (x, Si(x), Ci(x)) reference values from QuantLib's testRealSiCiIntegrals
    // (computed with Mathematica / mpmath).
    const DATA: [(Real, Real, Real); 17] = [
        (1e-12, 1e-12, -27.0538054510270153677),
        (0.1, 0.09994446110827695570, -1.7278683866572965838),
        (1.0, 0.9460830703671830149, 0.3374039229009681347),
        (1.9999, 1.6053675097543679041, 0.4230016343635392),
        (3.9999, 1.758222058430840841, -0.140965355646150101),
        (4.0001, 1.758184218306157867, -0.140998037827177150),
        (5.0, 1.5499312449446741373, -0.19002974965664387862),
        (7.0, 1.4545966142480935906, 0.076695278482184518383),
        (10.0, 1.6583475942188740493, -0.045456433004455372635),
        (15.0, 1.6181944437083687391, 0.046278677674360439604),
        (20.0, 1.5482417010434398402, 0.04441982084535331654),
        (24.9, 1.532210740207620024, -0.010788215638781789846),
        (25.1, 1.5311526281483412938, -0.0028719014454227088097),
        (30.0, 1.566756540030351111, -0.033032417282071143779),
        (40.0, 1.5869851193547845068, 0.019020007896208766962),
        (400.0, 1.5721148692738117518, -0.00212398883084634893),
        (4000.0, 1.5709788562309441985, -0.00017083030544201591130),
    ];

    #[test]
    fn matches_reference_si_and_ci() {
        let tol = 1e-12;
        for (x, si_ref, ci_ref) in DATA {
            assert!(
                (si(x) - si_ref).abs() < tol,
                "Si({x}) = {} vs {si_ref}",
                si(x)
            );
            assert!(
                (ci(x) - ci_ref).abs() < tol,
                "Ci({x}) = {} vs {ci_ref}",
                ci(x)
            );
            // Si is odd: Si(-x) = -Si(x).
            assert!(
                (si(-x) + si_ref).abs() < tol,
                "Si(-{x}) = {} vs {}",
                si(-x),
                -si_ref
            );
        }
    }

    #[test]
    fn ci_is_nan_below_zero_and_neg_infinity_at_zero() {
        assert!(ci(-1.0).is_nan());
        assert_eq!(ci(0.0), Real::NEG_INFINITY);
    }

    #[test]
    fn si_at_zero_is_zero() {
        assert_eq!(si(0.0), 0.0);
    }

    // Port of testExponentialIntegralLimits: QL_CHECK_CLOSE(a, b, t) is
    // BOOST_CHECK_CLOSE, whose tolerance is a percentage, hence the /100.
    fn assert_close(label: &str, value: Real, reference: Real, rel_tol_percent: Real) {
        let rel = rel_tol_percent / 100.0;
        assert!(
            value == reference
                || ((value - reference).abs() <= rel * value.abs()
                    && (value - reference).abs() <= rel * reference.abs()),
            "{label}: {value} vs {reference} (rel tol {rel})"
        );
    }

    #[test]
    fn ei_limit_for_large_arguments() {
        let tol = 1000.0 * Real::EPSILON;
        let large_value = 0.75 * (0.1 * Real::MAX).ln();
        let expected_real = large_value.exp() / large_value;

        let pos_imag = ei(Complex::new(large_value, Real::MIN_POSITIVE)).unwrap();
        assert_close("Ei imag (+0 side)", pos_imag.im, PI, tol);
        assert_close(
            "Ei real (+0 side)",
            pos_imag.re,
            expected_real,
            1e3 / large_value,
        );

        let neg_imag = ei(Complex::new(large_value, -Real::MIN_POSITIVE)).unwrap();
        assert_close("Ei imag (-0 side)", neg_imag.im, -PI, tol);
        assert_close(
            "Ei real (-0 side)",
            neg_imag.re,
            expected_real,
            1e3 / large_value,
        );

        let zero_imag = ei(Complex::new(large_value, 0.0)).unwrap();
        assert_eq!(zero_imag.im, 0.0);
    }

    #[test]
    fn ei_at_zero_is_negative_infinity() {
        let value = ei(Complex::new(0.0, 0.0)).unwrap();
        assert_eq!(value, Complex::new(Real::NEG_INFINITY, 0.0));
    }

    #[test]
    fn ei_is_out_of_domain_beyond_exp_overflow() {
        assert!(ei(Complex::new(710.0, 0.0)).is_err());
    }

    #[test]
    fn ei_small_circle_limit_is_euler_plus_log() {
        let tol = 1000.0 * Real::EPSILON;
        let small_r = Real::EPSILON * Real::EPSILON;
        for x in -100..100 {
            let phi = x as Real / 100.0 * PI;
            let z = Complex::from_polar(small_r, phi);
            let value = ei(z).unwrap();
            let limit = EULER_MASCHERONI + z.ln();
            assert_close("Ei small-circle real", value.re, limit.re, tol);
            assert_close("Ei small-circle imag", value.im, limit.im, tol);
        }
    }

    #[test]
    fn ei_large_circle_limit_is_signed_pi() {
        let tol = 1000.0 * Real::EPSILON;
        let close_enough_tol = 42.0 * Real::EPSILON;
        let large_r = 0.75 * (0.1 * Real::MAX).ln();
        for x in -10..10 {
            let phi = x as Real / 10.0 * PI;
            if phi.abs() > 0.5 * PI {
                let z = Complex::from_polar(large_r, phi);
                let value = ei(z).unwrap();
                let limit_imag = sign(z.im) * PI;
                assert!(
                    value.re == 0.0 || value.re.abs() < close_enough_tol * close_enough_tol,
                    "Ei large-circle real: {} not close enough to 0",
                    value.re
                );
                assert_close("Ei large-circle imag", value.im, limit_imag, tol);
            }
        }
    }
}
