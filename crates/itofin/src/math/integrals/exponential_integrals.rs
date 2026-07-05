//! Sine and cosine integrals `Si` and `Ci`.
//!
//! Port of the real-valued part of `ql/math/integrals/exponentialintegrals.*`:
//! the sine integral [`si`] and cosine integral [`ci`]. Both use rational
//! (minimax) approximations for `|x| <= 4` and an asymptotic form built from the
//! auxiliary functions `f` and `g` beyond that. The complex `Ei`/`E1`/`Si`/`Ci`
//! family is a separate, later addition.

#![allow(clippy::excessive_precision)]

use crate::types::Real;

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
}
