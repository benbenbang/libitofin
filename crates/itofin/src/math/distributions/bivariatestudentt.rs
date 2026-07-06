//! Bivariate cumulative Student t-distribution.
//!
//! Port of `BivariateCumulativeStudentDistribution` from
//! `ql/math/distributions/bivariatestudenttdistribution.{hpp,cpp}`, following
//! Dunnett and Sobel (1954): `P(X <= x, Y <= y)` for a standard bivariate
//! Student t with `n` degrees of freedom and correlation `rho`, summed via the
//! even-order (eq. 10) and odd-order (eq. 11) series.
//!
//! Being a two-argument CDF, it is exposed as an inherent `value(x, y)` method.
//! The marginals are univariate `t_n`, used for the infinite-argument limits.

use std::f64::consts::{PI, TAU};

use super::Cdf;
use super::studentt::StudentT;
use crate::errors::QlResult;
use crate::math::comparison::sign;
use crate::require;
use crate::types::Real;

const EPSILON: Real = 1.0e-8;

/// The cumulative bivariate Student t-distribution with `n` degrees of freedom
/// and correlation `rho`.
#[derive(Clone, Copy, Debug)]
pub struct BivariateCumulativeStudentDistribution {
    n: u32,
    rho: Real,
}

impl BivariateCumulativeStudentDistribution {
    /// A bivariate Student t CDF with `n` degrees of freedom and correlation
    /// `rho`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `n >= 1` and `rho` lies in `[-1, 1]`.
    pub fn new(n: u32, rho: Real) -> QlResult<Self> {
        require!(n >= 1, "degrees of freedom must be >= 1, got {n}");
        require!(
            (-1.0..=1.0).contains(&rho),
            "correlation rho must be in [-1, 1], got {rho}"
        );
        Ok(BivariateCumulativeStudentDistribution { n, rho })
    }

    /// `P(X <= x, Y <= y)` for the standard bivariate Student t.
    ///
    /// A `NaN` argument yields `NaN`; infinite arguments give the exact CDF
    /// limits through the univariate `t_n` marginals.
    pub fn value(&self, x: Real, y: Real) -> Real {
        if x.is_nan() || y.is_nan() {
            return Real::NAN;
        }
        // -inf in either argument forces the joint probability to 0.
        if (x.is_infinite() && x < 0.0) || (y.is_infinite() && y < 0.0) {
            return 0.0;
        }
        // F(+inf, y) = T_n(y), F(x, +inf) = T_n(x); the numerical path would
        // otherwise form inf/inf = NaN inside f_x.
        if x.is_infinite() || y.is_infinite() {
            let t = StudentT::new(Real::from(self.n)).expect("n >= 1 is a valid Student-t df");
            return if x.is_infinite() { t.cdf(y) } else { t.cdf(x) };
        }
        p_n(x, y, self.n, self.rho)
    }
}

/// `atan2` mapped into `[0, 2*pi)` (QuantLib's `arctan`).
fn arctan(x: Real, y: Real) -> Real {
    let res = x.atan2(y);
    if res >= 0.0 { res } else { res + TAU }
}

/// The function `x(m, h, k)` from the top of p.155, guarded at the `rho = +-1`
/// limit where the denominator vanishes.
fn f_x(m: Real, h: Real, k: Real, rho: Real) -> Real {
    let un_cor = 1.0 - rho * rho;
    let sub = (h - rho * k).powi(2);
    let denom = sub + un_cor * (m + k * k);
    if denom < EPSILON {
        return 0.0;
    }
    sub / denom
}

/// The inner sum from the second/third lines of equation (10) (even `n`).
fn even_sum(x_term: Real, div_term: Real, sgn: Real, n: u32) -> Real {
    let dg_m = 2.0 * (1.0 - x_term);
    let gj_m = sgn * 2.0 / PI;
    let mut f_j = (PI / div_term).sqrt();
    let mut g_j = 1.0 + gj_m * arctan(x_term.sqrt(), (1.0 - x_term).sqrt());
    let mut sum = f_j * g_j;
    if n >= 4 {
        f_j *= 0.5 / div_term;
        let mut dgj = gj_m * (x_term * (1.0 - x_term)).sqrt();
        g_j += dgj;
        sum += f_j * g_j;
        for j in 3..=n / 2 {
            let jf = Real::from(j);
            f_j *= (jf - 1.5) / (jf - 1.0) / div_term;
            dgj *= (jf - 2.0) / (2.0 * jf - 3.0) * dg_m;
            g_j += dgj;
            sum += f_j * g_j;
        }
    }
    sum
}

/// The inner sum from the second/third lines of equation (11) (odd `n > 1`).
fn odd_sum(x_term: Real, div_term: Real, sgn: Real, n: u32) -> Real {
    let mult = (1.0 - x_term) / 2.0;
    let mut f_j = 2.0 / PI.sqrt() / div_term;
    let mut dgj = sgn * x_term.sqrt();
    let mut g_j = 1.0 + dgj;
    let mut sum = f_j * g_j;
    for j in 2..=(n - 1) / 2 {
        let jf = Real::from(j);
        f_j *= (jf - 1.0) / (jf - 0.5) / div_term;
        dgj *= (2.0 * jf - 3.0) / (jf - 1.0) * mult;
        g_j += dgj;
        sum += f_j * g_j;
    }
    sum
}

/// The bivariate Student t CDF `P_n(h, k)`.
fn p_n(h: Real, k: Real, n: u32, rho: Real) -> Real {
    let un_cor = 1.0 - rho * rho;
    let nf = Real::from(n);
    let div = 4.0 * (nf * PI).sqrt();
    let x_hk = f_x(nf, h, k, rho);
    let x_kh = f_x(nf, k, h, rho);
    let div_h = 1.0 + h * h / nf;
    let div_k = 1.0 + k * k / nf;
    let sgn_hk = sign(h - rho * k);
    let sgn_kh = sign(k - rho * h);

    if n.is_multiple_of(2) {
        // equation (10)
        let mut res = arctan(un_cor.sqrt(), -rho) / TAU;
        res += k / div * even_sum(x_hk, div_k, sgn_hk, n);
        res += h / div * even_sum(x_kh, div_h, sgn_kh, n);
        res
    } else {
        // equation (11)
        let hk = h * k;
        let hkcn = hk + rho * nf;
        let sqrt_expr = (h * h - 2.0 * rho * hk + k * k + nf * un_cor).sqrt();
        let mut res = arctan(
            nf.sqrt() * (-(h + k) * hkcn - (hk - nf) * sqrt_expr),
            (hk - nf) * hkcn - nf * (h + k) * sqrt_expr,
        ) / TAU;
        if n > 1 {
            res += k / div * odd_sum(x_hk, div_k, sgn_hk, n);
            res += h / div * odd_sum(x_kh, div_h, sgn_kh, n);
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Biv = BivariateCumulativeStudentDistribution;

    #[rustfmt::skip]
    const XS: [Real; 14] = [
        0.00, 0.50, 1.00, 1.50, 2.00, 2.50, 3.00,
        4.00, 5.00, 6.00, 7.00, 8.00, 9.00, 10.00,
    ];
    const NS: [u32; 20] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 20, 25, 30, 60, 90, 120, 150, 300, 600,
    ];
    #[rustfmt::skip]
    const EXPECTED_POS: [Real; 280] = [
        0.33333, 0.50000, 0.63497, 0.72338, 0.78063, 0.81943, 0.84704,
        0.88332, 0.90590, 0.92124, 0.93231, 0.94066, 0.94719, 0.95243,
        0.33333, 0.52017, 0.68114, 0.78925, 0.85607, 0.89754, 0.92417,
        0.95433, 0.96978, 0.97862, 0.98411, 0.98774, 0.99026, 0.99208,
        0.33333, 0.52818, 0.70018, 0.81702, 0.88720, 0.92812, 0.95238,
        0.97667, 0.98712, 0.99222, 0.99497, 0.99657, 0.99756, 0.99821,
        0.33333, 0.53245, 0.71052, 0.83231, 0.90402, 0.94394, 0.96612,
        0.98616, 0.99353, 0.99664, 0.99810, 0.99885, 0.99927, 0.99951,
        0.33333, 0.53510, 0.71701, 0.84196, 0.91449, 0.95344, 0.97397,
        0.99095, 0.99637, 0.99836, 0.99918, 0.99956, 0.99975, 0.99985,
        0.33333, 0.53689, 0.72146, 0.84862, 0.92163, 0.95972, 0.97893,
        0.99365, 0.99779, 0.99913, 0.99962, 0.99982, 0.99990, 0.99995,
        0.33333, 0.53819, 0.72470, 0.85348, 0.92679, 0.96415, 0.98230,
        0.99531, 0.99857, 0.99950, 0.99981, 0.99992, 0.99996, 0.99998,
        0.33333, 0.53917, 0.72716, 0.85719, 0.93070, 0.96743, 0.98470,
        0.99639, 0.99903, 0.99970, 0.99990, 0.99996, 0.99998, 0.99999,
        0.33333, 0.53994, 0.72909, 0.86011, 0.93375, 0.96995, 0.98650,
        0.99713, 0.99931, 0.99981, 0.99994, 0.99998, 0.99999, 1.00000,
        0.33333, 0.54056, 0.73065, 0.86247, 0.93621, 0.97194, 0.98788,
        0.99766, 0.99950, 0.99988, 0.99996, 0.99999, 1.00000, 1.00000,
        0.33333, 0.54243, 0.73540, 0.86968, 0.94362, 0.97774, 0.99168,
        0.99890, 0.99985, 0.99998, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54338, 0.73781, 0.87336, 0.94735, 0.98053, 0.99337,
        0.99932, 0.99993, 0.99999, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54395, 0.73927, 0.87560, 0.94959, 0.98216, 0.99430,
        0.99952, 0.99996, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54433, 0.74025, 0.87709, 0.95108, 0.98322, 0.99489,
        0.99963, 0.99998, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54528, 0.74271, 0.88087, 0.95482, 0.98580, 0.99623,
        0.99983, 0.99999, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54560, 0.74354, 0.88215, 0.95607, 0.98663, 0.99664,
        0.99987, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54576, 0.74396, 0.88279, 0.95669, 0.98704, 0.99683,
        0.99989, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54586, 0.74420, 0.88317, 0.95706, 0.98729, 0.99695,
        0.99990, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54605, 0.74470, 0.88394, 0.95781, 0.98777, 0.99717,
        0.99992, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.33333, 0.54615, 0.74495, 0.88432, 0.95818, 0.98801, 0.99728,
        0.99993, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
    ];
    #[rustfmt::skip]
    const EXPECTED_NEG: [Real; 280] = [
        0.16667, 0.36554, 0.54022, 0.65333, 0.72582, 0.77465, 0.80928,
        0.85466, 0.88284, 0.90196, 0.91575, 0.92616, 0.93429, 0.94081,
        0.16667, 0.38889, 0.59968, 0.73892, 0.82320, 0.87479, 0.90763,
        0.94458, 0.96339, 0.97412, 0.98078, 0.98518, 0.98823, 0.99044,
        0.16667, 0.39817, 0.62478, 0.77566, 0.86365, 0.91391, 0.94330,
        0.97241, 0.98483, 0.99086, 0.99410, 0.99598, 0.99714, 0.99790,
        0.16667, 0.40313, 0.63863, 0.79605, 0.88547, 0.93396, 0.96043,
        0.98400, 0.99256, 0.99614, 0.99782, 0.99868, 0.99916, 0.99944,
        0.16667, 0.40620, 0.64740, 0.80900, 0.89902, 0.94588, 0.97007,
        0.98972, 0.99591, 0.99816, 0.99909, 0.99951, 0.99972, 0.99983,
        0.16667, 0.40829, 0.65345, 0.81794, 0.90820, 0.95368, 0.97607,
        0.99290, 0.99755, 0.99904, 0.99958, 0.99980, 0.99989, 0.99994,
        0.16667, 0.40980, 0.65788, 0.82449, 0.91482, 0.95914, 0.98010,
        0.99482, 0.99844, 0.99946, 0.99979, 0.99991, 0.99996, 0.99998,
        0.16667, 0.41095, 0.66126, 0.82948, 0.91981, 0.96314, 0.98295,
        0.99605, 0.99895, 0.99968, 0.99989, 0.99996, 0.99998, 0.99999,
        0.16667, 0.41185, 0.66393, 0.83342, 0.92369, 0.96619, 0.98506,
        0.99689, 0.99926, 0.99980, 0.99994, 0.99998, 0.99999, 1.00000,
        0.16667, 0.41257, 0.66608, 0.83661, 0.92681, 0.96859, 0.98667,
        0.99748, 0.99946, 0.99987, 0.99996, 0.99999, 1.00000, 1.00000,
        0.16667, 0.41476, 0.67268, 0.84633, 0.93614, 0.97550, 0.99103,
        0.99884, 0.99984, 0.99998, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41586, 0.67605, 0.85129, 0.94078, 0.97877, 0.99292,
        0.99930, 0.99993, 0.99999, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41653, 0.67810, 0.85430, 0.94356, 0.98066, 0.99396,
        0.99950, 0.99996, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41698, 0.67947, 0.85632, 0.94540, 0.98189, 0.99461,
        0.99962, 0.99998, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41810, 0.68294, 0.86141, 0.94998, 0.98483, 0.99607,
        0.99982, 0.99999, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41847, 0.68411, 0.86312, 0.95149, 0.98577, 0.99651,
        0.99987, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41866, 0.68470, 0.86398, 0.95225, 0.98623, 0.99672,
        0.99989, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41877, 0.68505, 0.86449, 0.95270, 0.98650, 0.99684,
        0.99990, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41900, 0.68576, 0.86552, 0.95360, 0.98705, 0.99707,
        0.99992, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
        0.16667, 0.41911, 0.68612, 0.86604, 0.95405, 0.98731, 0.99719,
        0.99993, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
    ];
    #[rustfmt::skip]
    const CASES: [(u32, Real, Real, Real, Real); 66] = [
        (2, -1.0, 5.0, 8.0, 0.973491), (2, 1.0, -2.0, 8.0, 0.091752),
        (2, 1.0, 5.25, -9.5, 0.005450), (3, -0.5, -5.0, -5.0, 0.000220),
        (4, -1.0, -8.0, 7.5, 0.0), (4, 0.5, -5.5, 10.0, 0.002655),
        (4, 1.0, -5.0, 6.0, 0.003745), (4, 1.0, 6.0, 5.5, 0.997336),
        (5, -0.5, -7.0, -6.25, 0.000004), (5, -0.5, 3.75, -7.25, 0.000166),
        (5, -0.5, 7.75, -1.25, 0.133073), (6, 0.0, 7.5, 3.25, 0.991149),
        (7, -0.5, -1.0, -8.5, 0.000001), (7, -1.0, -4.25, -4.0, 0.0),
        (7, 0.0, 0.5, -2.25, 0.018819), (8, -1.0, 8.25, 1.75, 0.940866),
        (8, 0.0, 2.25, 4.75, 0.972105), (9, -0.5, -4.0, 8.25, 0.001550),
        (9, -1.0, -1.25, -8.75, 0.0), (9, -1.0, 5.75, -6.0, 0.0),
        (9, 0.5, -6.5, -9.5, 0.000001), (9, 1.0, -2.0, 9.25, 0.038276),
        (10, -1.0, -0.5, 6.0, 0.313881), (10, 0.5, 0.0, 9.25, 0.5),
        (10, 0.5, 6.75, -2.25, 0.024090), (10, 1.0, -1.75, -1.0, 0.055341),
        (15, 0.0, -1.25, -4.75, 0.000029), (15, 0.0, -2.0, -1.5, 0.003411),
        (15, 0.5, 3.0, -3.25, 0.002691), (20, -0.5, 2.0, -1.25, 0.098333),
        (20, -1.0, 3.0, 8.0, 0.996462), (20, 0.0, -7.5, 1.5, 0.0),
        (20, 0.5, 1.25, 9.75, 0.887136), (25, -1.0, -4.25, 5.0, 0.000111),
        (25, 0.5, 9.5, -1.5, 0.073069), (25, 1.0, -6.5, -3.25, 0.0),
        (30, -1.0, -7.75, 10.0, 0.0), (30, 1.0, 0.5, 9.5, 0.689638),
        (60, -1.0, -3.5, -8.25, 0.0), (60, -1.0, 4.25, 0.75, 0.771869),
        (60, -1.0, 5.75, 3.75, 0.9998), (60, 0.5, -4.5, 8.25, 0.000016),
        (60, 1.0, 6.5, -4.0, 0.000088), (90, -0.5, -3.75, -2.75, 0.0),
        (90, 0.5, 8.75, -7.0, 0.0), (120, 0.0, -3.5, -9.25, 0.0),
        (120, 0.0, -8.25, 5.0, 0.0), (120, 1.0, -0.75, 3.75, 0.227361),
        (120, 1.0, -3.5, -8.0, 0.0), (150, 0.0, 10.0, -1.75, 0.041082),
        (300, -0.5, -6.0, 3.75, 0.0), (300, -0.5, 3.5, -4.5, 0.000004),
        (300, 0.0, 6.5, -5.0, 0.0), (600, -0.5, 9.25, 1.5, 0.93293),
        (600, -1.0, -9.25, 1.5, 0.0), (600, 0.5, -5.0, 8.0, 0.0),
        (600, 1.0, -2.75, -9.0, 0.0), (1000, -0.5, -2.5, 0.25, 0.000589),
        (1000, -0.5, 3.0, 1.0, 0.839842), (2000, -1.0, 9.0, -4.75, 0.000001),
        (2000, 0.5, 9.75, 7.25, 1.0), (2000, 1.0, 0.75, -9.0, 0.0),
        (5000, -0.5, 9.75, 5.5, 1.0), (5000, -1.0, 6.0, 1.0, 0.841321),
        (5000, 1.0, 4.0, -7.75, 0.0), (10000, 0.5, 1.5, 6.0, 0.933177),
    ];

    // Port of testBivariateCumulativeStudent: the two Dunnett-Sobel reference
    // tables (rho = +0.5 and -0.5) over n in NS and x = y in XS, at 1e-5.
    #[test]
    fn matches_reference_tables() {
        for (i, &n) in NS.iter().enumerate() {
            let f1 = Biv::new(n, 0.5).unwrap();
            let f2 = Biv::new(n, -0.5).unwrap();
            for (j, &x) in XS.iter().enumerate() {
                let idx = i * XS.len() + j;
                let g1 = f1.value(x, x);
                let g2 = f2.value(x, x);
                assert!(
                    (g1 - EXPECTED_POS[idx]).abs() < 1e-5,
                    "n={n} x={x} +: {g1} vs {}",
                    EXPECTED_POS[idx]
                );
                assert!(
                    (g2 - EXPECTED_NEG[idx]).abs() < 1e-5,
                    "n={n} x={x} -: {g2} vs {}",
                    EXPECTED_NEG[idx]
                );
            }
        }
    }

    // The "more random cases" table from testBivariateCumulativeStudent, at 1e-6.
    #[test]
    fn matches_reference_cases() {
        for (n, rho, x, y, expected) in CASES {
            let got = Biv::new(n, rho).unwrap().value(x, y);
            assert!(
                (got - expected).abs() < 1e-6,
                "n={n} rho={rho} ({x},{y}): {got} vs {expected}"
            );
        }
    }

    #[test]
    fn boundary_and_construction() {
        let f = Biv::new(5, 0.5).unwrap();
        assert!(f.value(Real::NAN, 0.0).is_nan());
        // infinite limits via the t_n marginals
        let t = StudentT::new(5.0).unwrap();
        assert_eq!(f.value(Real::INFINITY, 1.0), t.cdf(1.0));
        assert_eq!(f.value(1.0, Real::INFINITY), t.cdf(1.0));
        assert_eq!(f.value(Real::NEG_INFINITY, 0.0), 0.0);
        assert_eq!(f.value(0.0, Real::NEG_INFINITY), 0.0);
        // construction validation
        assert!(Biv::new(0, 0.5).is_err());
        assert!(Biv::new(5, 1.5).is_err());
        assert!(Biv::new(5, Real::NAN).is_err());
    }
}
