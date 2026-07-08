//! Gauss-Kronrod integration.
//!
//! Port of `ql/math/integrals/kronrodintegral.{hpp,cpp}`: the adaptive 15-point
//! integrator ([`GaussKronrodAdaptive`]) and the non-adaptive
//! ([`GaussKronrodNonAdaptive`]) variant that applies the 10-, 21-, 43- and
//! 87-point rules in succession, reusing earlier evaluations.

#![allow(clippy::excessive_precision)]

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, require_accuracy};
use crate::types::{Real, Size};

// 7-point Gauss-Legendre weights (4 unique values; the rule is symmetric).
const G7W: [Real; 4] = [
    0.417959183673469,
    0.381830050505119,
    0.279705391489277,
    0.129484966168870,
];
// 15-point Gauss-Kronrod weights (8 unique values).
const K15W: [Real; 8] = [
    0.209482141084728,
    0.204432940075298,
    0.190350578064785,
    0.169004726639267,
    0.140653259715525,
    0.104790010322250,
    0.063092092629979,
    0.022935322010529,
];
// 15-point Gauss-Kronrod abscissae (8 unique values, scaled to [-1, 1]).
const K15T: [Real; 8] = [
    0.000000000000000,
    0.207784955007898,
    0.405845151377397,
    0.586087235467691,
    0.741531185599394,
    0.864864423359769,
    0.949107912342758,
    0.991455371120813,
];

/// Adaptive Gauss-Kronrod integrator using the 15-point rule with recursive
/// bisection. Robust for less-smooth integrands, but it does not reuse points
/// between refinement levels.
pub struct GaussKronrodAdaptive {
    tolerance: Real,
    max_evaluations: Size,
}

impl GaussKronrodAdaptive {
    /// A new adaptive integrator. `tolerance` must be finite and above machine
    /// epsilon, and `max_evaluations` must be at least 15 (one 15-point rule).
    pub fn new(tolerance: Real, max_evaluations: Size) -> QlResult<Self> {
        require_accuracy(tolerance)?;
        if max_evaluations < 15 {
            fail!("required max evaluations ({max_evaluations}) must be >= 15");
        }
        Ok(GaussKronrodAdaptive {
            tolerance,
            max_evaluations,
        })
    }

    /// Integrates `f` over `[a, b]` with the 15-point rule; if the Gauss(7) and
    /// Kronrod(15) estimates disagree by more than `tolerance`, the interval is
    /// bisected and each half integrated at half the tolerance. `evaluations`
    /// accumulates the running count across the whole recursion.
    fn integrate_recursively<F>(
        &self,
        f: &mut F,
        a: Real,
        b: Real,
        tolerance: Real,
        evaluations: &mut Size,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        // Each level spends 15 evaluations before it can test for convergence, so
        // reserve them up front: refuse to descend unless the budget covers this
        // rule. This caps total evaluations at `max_evaluations` even when a node
        // converges (and so never reaches the recursion guard below).
        if *evaluations + 15 > self.max_evaluations {
            fail!("maximum number of function evaluations exceeded");
        }

        let halflength = (b - a) / 2.0;
        let center = (a + b) / 2.0;

        let fc = f(center);
        let mut g7 = fc * G7W[0];
        let mut k15 = fc * K15W[0];

        // The Gauss nodes are the even-indexed Kronrod nodes; accumulate g7 and
        // its share of k15 together (j2 = 2, 4, 6 alongside the Gauss weights).
        let mut j2 = 2;
        for &g7w in G7W.iter().skip(1) {
            let t = halflength * K15T[j2];
            let fsum = f(center - t) + f(center + t);
            g7 += fsum * g7w;
            k15 += fsum * K15W[j2];
            j2 += 2;
        }
        // The remaining odd-indexed Kronrod-only nodes.
        let mut j2 = 1;
        while j2 < 8 {
            let t = halflength * K15T[j2];
            let fsum = f(center - t) + f(center + t);
            k15 += fsum * K15W[j2];
            j2 += 2;
        }

        g7 *= halflength;
        k15 *= halflength;
        *evaluations += 15;

        // The error is bounded by |K15 - G7|; refine if it exceeds the tolerance.
        // Each half re-checks the budget at the top of its own call before
        // spending, so recursion can never overshoot `max_evaluations`.
        if (k15 - g7).abs() < tolerance {
            Ok(k15)
        } else {
            let left = self.integrate_recursively(f, a, center, tolerance / 2.0, evaluations)?;
            let right = self.integrate_recursively(f, center, b, tolerance / 2.0, evaluations)?;
            Ok(left + right)
        }
    }
}

impl Integrator for GaussKronrodAdaptive {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let mut evaluations = 0;
        self.integrate_recursively(f, a, b, self.tolerance, &mut evaluations)
    }
}

// ---------------------------------------------------------------------------
// Non-adaptive Gauss-Kronrod (10/21/43/87-point) tables and integrator.
// ---------------------------------------------------------------------------

// Abscissae common to the 10-, 21-, 43- and 87-point rules.
const X1: [Real; 5] = [
    0.973906528517171720077964012084452,
    0.865063366688984510732096688423493,
    0.679409568299024406234327365114874,
    0.433395394129247190799265943165784,
    0.148874338981631210884826001129720,
];
// Weights of the 10-point rule.
const W10: [Real; 5] = [
    0.066671344308688137593568809893332,
    0.149451349150580593145776339657697,
    0.219086362515982043995534934228163,
    0.269266719309996355091226921569469,
    0.295524224714752870173892994651338,
];
// Abscissae common to the 21-, 43- and 87-point rules.
const X2: [Real; 5] = [
    0.995657163025808080735527280689003,
    0.930157491355708226001207180059508,
    0.780817726586416897063717578345042,
    0.562757134668604683339000099272694,
    0.294392862701460198131126603103866,
];
// Weights of the 21-point rule for abscissae X1.
const W21A: [Real; 5] = [
    0.032558162307964727478818972459390,
    0.075039674810919952767043140916190,
    0.109387158802297641899210590325805,
    0.134709217311473325928054001771707,
    0.147739104901338491374841515972068,
];
// Weights of the 21-point rule for abscissae X2 (last entry is the centre).
const W21B: [Real; 6] = [
    0.011694638867371874278064396062192,
    0.054755896574351996031381300244580,
    0.093125454583697605535065465083366,
    0.123491976262065851077958109831074,
    0.142775938577060080797094273138717,
    0.149445554002916905664936468389821,
];
// Abscissae common to the 43- and 87-point rules.
const X3: [Real; 11] = [
    0.999333360901932081394099323919911,
    0.987433402908088869795961478381209,
    0.954807934814266299257919200290473,
    0.900148695748328293625099494069092,
    0.825198314983114150847066732588520,
    0.732148388989304982612354848755461,
    0.622847970537725238641159120344323,
    0.499479574071056499952214885499755,
    0.364901661346580768043989548502644,
    0.222254919776601296498260928066212,
    0.074650617461383322043914435796506,
];
// Weights of the 43-point rule for abscissae X1, X3.
const W43A: [Real; 10] = [
    0.016296734289666564924281974617663,
    0.037522876120869501461613795898115,
    0.054694902058255442147212685465005,
    0.067355414609478086075553166302174,
    0.073870199632393953432140695251367,
    0.005768556059769796184184327908655,
    0.027371890593248842081276069289151,
    0.046560826910428830743339154433824,
    0.061744995201442564496240336030883,
    0.071387267268693397768559114425516,
];
// Weights of the 43-point rule for abscissae X3 (last entry is the centre).
const W43B: [Real; 12] = [
    0.001844477640212414100389106552965,
    0.010798689585891651740465406741293,
    0.021895363867795428102523123075149,
    0.032597463975345689443882222526137,
    0.042163137935191811847627924327955,
    0.050741939600184577780189020092084,
    0.058379395542619248375475369330206,
    0.064746404951445885544689259517511,
    0.069566197912356484528633315038405,
    0.072824441471833208150939535192842,
    0.074507751014175118273571813842889,
    0.074722147517403005594425168280423,
];
// Abscissae of the 87-point rule.
const X4: [Real; 22] = [
    0.999902977262729234490529830591582,
    0.997989895986678745427496322365960,
    0.992175497860687222808523352251425,
    0.981358163572712773571916941623894,
    0.965057623858384619128284110607926,
    0.943167613133670596816416634507426,
    0.915806414685507209591826430720050,
    0.883221657771316501372117548744163,
    0.845710748462415666605902011504855,
    0.803557658035230982788739474980964,
    0.757005730685495558328942793432020,
    0.706273209787321819824094274740840,
    0.651589466501177922534422205016736,
    0.593223374057961088875273770349144,
    0.531493605970831932285268948562671,
    0.466763623042022844871966781659270,
    0.399424847859218804732101665817923,
    0.329874877106188288265053371824597,
    0.258503559202161551802280975429025,
    0.185695396568346652015917141167606,
    0.111842213179907468172398359241362,
    0.037352123394619870814998165437704,
];
// Weights of the 87-point rule for abscissae X1, X2, X3.
const W87A: [Real; 21] = [
    0.008148377384149172900002878448190,
    0.018761438201562822243935059003794,
    0.027347451050052286161582829741283,
    0.033677707311637930046581056957588,
    0.036935099820427907614589586742499,
    0.002884872430211530501334156248695,
    0.013685946022712701888950035273128,
    0.023280413502888311123409291030404,
    0.030872497611713358675466394126442,
    0.035693633639418770719351355457044,
    0.000915283345202241360843392549948,
    0.005399280219300471367738743391053,
    0.010947679601118931134327826856808,
    0.016298731696787335262665703223280,
    0.021081568889203835112433060188190,
    0.025370969769253827243467999831710,
    0.029189697756475752501446154084920,
    0.032373202467202789685788194889595,
    0.034783098950365142750781997949596,
    0.036412220731351787562801163687577,
    0.037253875503047708539592001191226,
];
// Weights of the 87-point rule for abscissae X4 (last entry is the centre).
const W87B: [Real; 23] = [
    0.000274145563762072350016527092881,
    0.001807124155057942948341311753254,
    0.004096869282759164864458070683480,
    0.006758290051847378699816577897424,
    0.009549957672201646536053581325377,
    0.012329447652244853694626639963780,
    0.015010447346388952376697286041943,
    0.017548967986243191099665352925900,
    0.019938037786440888202278192730714,
    0.022194935961012286796332102959499,
    0.024339147126000805470360647041454,
    0.026374505414839207241503786552615,
    0.028286910788771200659968002987960,
    0.030052581128092695322521110347341,
    0.031646751371439929404586051078883,
    0.033050413419978503290785944862689,
    0.034255099704226061787082821046821,
    0.035262412660156681033782717998428,
    0.036076989622888701185500318003895,
    0.036698604498456094498018047441094,
    0.037120549269832576114119958413599,
    0.037334228751935040321235449094698,
    0.037361073762679023410321241766599,
];

/// Rescales the raw error estimate (QuantLib's QUADPACK-derived `rescaleError`).
fn rescale_error(err: Real, result_abs: Real, result_asc: Real) -> Real {
    let mut err = err.abs();
    if result_asc != 0.0 && err != 0.0 {
        let scale = (200.0 * err / result_asc).powf(1.5);
        err = if scale < 1.0 {
            result_asc * scale
        } else {
            result_asc
        };
    }
    if result_abs > Real::MIN_POSITIVE / (50.0 * Real::EPSILON) {
        let min_err = 50.0 * Real::EPSILON * result_abs;
        if min_err > err {
            err = min_err;
        }
    }
    err
}

/// Non-adaptive Gauss-Kronrod integrator. Applies the 10-, 21-, 43- and
/// 87-point rules in succession, reusing earlier evaluations, until the error
/// estimate meets the absolute or relative accuracy. Fast for smooth integrands.
///
/// Unlike QuantLib this carries no `maxEvaluations`: the rule sequence is fixed
/// (at most 87 points), so that bound was only ever read by the post-hoc
/// `integrationSuccess()` inspector this port drops. If the 87-point rule still
/// misses the accuracy, `integrate` returns an error.
pub struct GaussKronrodNonAdaptive {
    absolute_accuracy: Real,
    relative_accuracy: Real,
}

impl GaussKronrodNonAdaptive {
    /// A new non-adaptive integrator. `absolute_accuracy` must be finite and
    /// above machine epsilon; `relative_accuracy` must be finite and `>= 0`.
    pub fn new(absolute_accuracy: Real, relative_accuracy: Real) -> QlResult<Self> {
        require_accuracy(absolute_accuracy)?;
        if !relative_accuracy.is_finite() || relative_accuracy < 0.0 {
            fail!("relative accuracy must be finite and non-negative, got {relative_accuracy}");
        }
        Ok(GaussKronrodNonAdaptive {
            absolute_accuracy,
            relative_accuracy,
        })
    }

    /// Whether the error estimate meets either the absolute or relative target.
    fn converged(&self, err: Real, result: Real) -> bool {
        err < self.absolute_accuracy || err < self.relative_accuracy * result.abs()
    }
}

impl Integrator for GaussKronrodNonAdaptive {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let half_length = 0.5 * (b - a);
        let center = 0.5 * (b + a);
        let f_center = f(center);

        let mut savfun = [0.0; 21];
        let mut fv1 = [0.0; 5];
        let mut fv2 = [0.0; 5];
        let mut fv3 = [0.0; 5];
        let mut fv4 = [0.0; 5];

        // 10- and 21-point rules, saving each pair sum for later reuse.
        let mut res10 = 0.0;
        let mut res21 = W21B[5] * f_center;
        let mut res_abs = W21B[5] * f_center.abs();
        for k in 0..5 {
            let abscissa = half_length * X1[k];
            let fval1 = f(center + abscissa);
            let fval2 = f(center - abscissa);
            let fval = fval1 + fval2;
            res10 += W10[k] * fval;
            res21 += W21A[k] * fval;
            res_abs += W21A[k] * (fval1.abs() + fval2.abs());
            savfun[k] = fval;
            fv1[k] = fval1;
            fv2[k] = fval2;
        }
        for k in 0..5 {
            let abscissa = half_length * X2[k];
            let fval1 = f(center + abscissa);
            let fval2 = f(center - abscissa);
            let fval = fval1 + fval2;
            res21 += W21B[k] * fval;
            res_abs += W21B[k] * (fval1.abs() + fval2.abs());
            savfun[k + 5] = fval;
            fv3[k] = fval1;
            fv4[k] = fval2;
        }

        let mut result = res21 * half_length;
        res_abs *= half_length;
        let mean = 0.5 * res21;
        let mut resasc = W21B[5] * (f_center - mean).abs();
        for k in 0..5 {
            resasc += W21A[k] * ((fv1[k] - mean).abs() + (fv2[k] - mean).abs())
                + W21B[k] * ((fv3[k] - mean).abs() + (fv4[k] - mean).abs());
        }
        let mut err = rescale_error((res21 - res10) * half_length, res_abs, resasc);
        resasc *= half_length;
        if self.converged(err, result) {
            return Ok(result);
        }

        // 43-point rule, reusing the 21 saved sums.
        let mut res43 = W43B[11] * f_center;
        for k in 0..10 {
            res43 += savfun[k] * W43A[k];
        }
        for k in 0..11 {
            let abscissa = half_length * X3[k];
            let fval = f(center + abscissa) + f(center - abscissa);
            res43 += fval * W43B[k];
            savfun[k + 10] = fval;
        }
        result = res43 * half_length;
        err = rescale_error((res43 - res21) * half_length, res_abs, resasc);
        if self.converged(err, result) {
            return Ok(result);
        }

        // 87-point rule, reusing the 43 saved sums.
        let mut res87 = W87B[22] * f_center;
        for k in 0..21 {
            res87 += savfun[k] * W87A[k];
        }
        for k in 0..22 {
            let abscissa = half_length * X4[k];
            res87 += W87B[k] * (f(center + abscissa) + f(center - abscissa));
        }
        result = res87 * half_length;
        err = rescale_error((res87 - res43) * half_length, res_abs, resasc);
        if self.converged(err, result) {
            Ok(result)
        } else {
            fail!("non-adaptive Gauss-Kronrod integration did not converge (error estimate {err})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    #[test]
    fn matches_known_integrals() {
        // QuantLib's testSeveral (Abcd case omitted, not yet ported).
        let gk = GaussKronrodAdaptive::new(TOL, 1000).unwrap();
        assert!((gk.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((gk.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((gk.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (gk.integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (gk.integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((gk.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
        // testDegeneratedDomain.
        assert_eq!(
            gk.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(),
            0.0
        );
    }

    #[test]
    fn too_small_budget_fails_to_converge() {
        // A tight tolerance on an oscillatory integrand exhausts a 15-evaluation
        // budget (one rule, no room to bisect).
        let gk = GaussKronrodAdaptive::new(1e-13, 15).unwrap();
        assert!(gk.integrate(|x| (50.0 * x).sin(), 0.0, 1.0).is_err());
    }

    #[test]
    fn never_exceeds_the_evaluation_budget() {
        // Regression: a converging node used to spend its 15 evaluations without
        // ever testing the budget, so an oscillatory integrand under a tight
        // tolerance could finish (Ok) or recurse well past `max_evaluations`. The
        // count must never exceed the cap, whether the run succeeds or fails.
        for max in [15, 30, 45, 120, 300] {
            let gk = GaussKronrodAdaptive::new(1e-13, max).unwrap();
            let mut calls = 0usize;
            let _ = gk.integrate(
                |x| {
                    calls += 1;
                    (50.0 * x).sin()
                },
                0.0,
                1.0,
            );
            assert!(calls <= max, "max={max} spent {calls} evaluations");
        }
    }

    #[test]
    fn invalid_configuration_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                GaussKronrodAdaptive::new(acc, 1000).is_err(),
                "accuracy={acc}"
            );
        }
        // Fewer than 15 evaluations cannot fit a single rule.
        assert!(GaussKronrodAdaptive::new(TOL, 14).is_err());
    }

    #[test]
    fn non_adaptive_matches_known_integrals() {
        // QuantLib's testSeveral / testDegeneratedDomain for the non-adaptive
        // integrator (Abcd case omitted, not yet ported).
        let gk = GaussKronrodNonAdaptive::new(TOL, TOL).unwrap();
        assert!((gk.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((gk.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((gk.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (gk.integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (gk.integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((gk.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
        assert_eq!(
            gk.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(),
            0.0
        );
    }

    #[test]
    fn non_adaptive_reversed_limits_negate() {
        let gk = GaussKronrodNonAdaptive::new(TOL, TOL).unwrap();
        assert!((gk.integrate(|x| x, 1.0, 0.0).unwrap() - (-0.5)).abs() < TOL);
    }

    #[test]
    fn non_adaptive_invalid_configuration_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                GaussKronrodNonAdaptive::new(acc, TOL).is_err(),
                "abs accuracy={acc}"
            );
        }
        for rel in [-1.0, Real::NAN, Real::INFINITY] {
            assert!(
                GaussKronrodNonAdaptive::new(TOL, rel).is_err(),
                "rel accuracy={rel}"
            );
        }
    }
}
