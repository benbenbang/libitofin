//! Bivariate cumulative normal distribution.
//!
//! Port of `BivariateCumulativeNormalDistributionDr78` from
//! `ql/math/distributions/bivariatenormaldistribution.{hpp,cpp}`: Drezner's
//! 1978 approximation of `P(X <= a, Y <= b)` for standard bivariate normals
//! with correlation `rho`, via 5-point Gauss-Legendre quadrature and a
//! reflection recurrence that reduces every sign/correlation case to the
//! `a <= 0, b <= 0, rho <= 0` quadrant. Accurate to ~1e-6 (the more accurate
//! West 2004 method is a separate type).
//!
//! Being a two-argument CDF, it is exposed as an inherent `value(a, b)` method
//! rather than the single-argument [`Cdf`](super::Cdf) trait.

use std::f64::consts::PI;

use super::normal::CumulativeNormalDistribution;
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

// Drezner's tabulated Gauss-Legendre weights (x_) and abscissae (y_).
const X: [Real; 5] = [
    0.24840615,
    0.39233107,
    0.21141819,
    0.03324666,
    0.00082485334,
];
const Y: [Real; 5] = [
    0.10024215,
    0.48281397,
    1.06094980,
    1.77972940,
    2.66976040000,
];

/// Drezner's 1978 bivariate cumulative normal distribution with correlation
/// `rho`.
#[derive(Clone, Copy, Debug)]
pub struct BivariateCumulativeNormalDistributionDr78 {
    rho: Real,
    rho2: Real,
}

impl BivariateCumulativeNormalDistributionDr78 {
    /// A bivariate normal CDF with correlation `rho`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `rho` lies in `[-1, 1]` (so `NaN` and the
    /// infinities are rejected).
    pub fn new(rho: Real) -> QlResult<Self> {
        require!(
            (-1.0..=1.0).contains(&rho),
            "correlation rho must be in [-1, 1], got {rho}"
        );
        Ok(BivariateCumulativeNormalDistributionDr78 {
            rho,
            rho2: rho * rho,
        })
    }

    /// `P(X <= a, Y <= b)` for the standard bivariate normal with correlation
    /// `rho`.
    ///
    /// A `NaN` argument yields `NaN`. Perfect correlation (`rho = +-1`) is
    /// degenerate - the quadrature would divide by `sqrt(2(1-rho^2)) = 0` - so it
    /// is evaluated through the exact closed form instead.
    pub fn value(&self, a: Real, b: Real) -> Real {
        // A NaN argument is invisible to f64::max/min below, so it would slip the
        // early returns and fall through to the unreachable branch; reject it here.
        if a.is_nan() || b.is_nan() {
            return Real::NAN;
        }
        let cum = CumulativeNormalDistribution::standard();
        // Degenerate perfect correlation: rho = 1 means Y = X, so
        // P(X<=a, Y<=b) = Phi(min(a,b)); rho = -1 means Y = -X, so
        // P = max(Phi(a) + Phi(b) - 1, 0). (rho can only reach +-1, not exceed
        // it, so the comparisons are exact rather than float-equality.)
        if self.rho >= 1.0 {
            return cum.value(a.min(b));
        }
        if self.rho <= -1.0 {
            return (cum.value(a) + cum.value(b) - 1.0).max(0.0);
        }
        let cum_a = cum.value(a);
        let cum_b = cum.value(b);
        let max_ab = cum_a.max(cum_b);
        let min_ab = cum_a.min(cum_b);

        if 1.0 - max_ab < 1e-15 {
            return min_ab;
        }
        if min_ab < 1e-15 {
            return min_ab;
        }

        let denom = (2.0 * (1.0 - self.rho2)).sqrt();
        let a1 = a / denom;
        let b1 = b / denom;

        if a <= 0.0 && b <= 0.0 && self.rho <= 0.0 {
            let mut sum = 0.0;
            for (&xi, &yi) in X.iter().zip(Y.iter()) {
                for (&xj, &yj) in X.iter().zip(Y.iter()) {
                    sum += xi
                        * xj
                        * (a1 * (2.0 * yi - a1)
                            + b1 * (2.0 * yj - b1)
                            + 2.0 * self.rho * (yi - a1) * (yj - b1))
                            .exp();
                }
            }
            (1.0 - self.rho2).sqrt() / PI * sum
        } else if a <= 0.0 && b >= 0.0 && self.rho >= 0.0 {
            cum_a - self.reflected(-self.rho).value(a, -b)
        } else if a >= 0.0 && b <= 0.0 && self.rho >= 0.0 {
            cum_b - self.reflected(-self.rho).value(-a, b)
        } else if a >= 0.0 && b >= 0.0 && self.rho <= 0.0 {
            cum_a + cum_b - 1.0 + self.value(-a, -b)
        } else if a * b * self.rho > 0.0 {
            let root = (a * a - 2.0 * self.rho * a * b + b * b).sqrt();
            let sign_a = if a > 0.0 { 1.0 } else { -1.0 };
            let sign_b = if b > 0.0 { 1.0 } else { -1.0 };
            let rho1 = (self.rho * a - b) * sign_a / root;
            let rho2 = (self.rho * b - a) * sign_b / root;
            let delta = (1.0 - sign_a * sign_b) / 4.0;
            self.reflected(rho1).value(a, 0.0) + self.reflected(rho2).value(b, 0.0) - delta
        } else {
            unreachable!(
                "Dr78 bivariate normal: unhandled case a={a}, b={b}, rho={}",
                self.rho
            )
        }
    }

    // A sibling distribution at a related correlation used by the reflection
    // recurrence. value() handles rho = +-1 (and NaN) up front, so this is only
    // reached with |rho| < 1, where the recurrence's correlations stay in [-1, 1].
    fn reflected(&self, rho: Real) -> Self {
        Self::new(rho).expect("reflection correlation stays within [-1, 1]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Dr78 = BivariateCumulativeNormalDistributionDr78;

    // Port of QuantLib checkBivariate: tabulated values from Haug, "Option
    // pricing formulas" (1998) p.193, plus known analytical/degenerate cases.
    #[test]
    fn matches_reference_table() {
        let third = 1.0 / 3.0;
        let cases: [(Real, Real, Real, Real); 43] = [
            (0.0, 0.0, 0.0, 0.250000),
            (0.0, 0.0, -0.5, 0.166667),
            (0.0, 0.0, 0.5, third),
            (0.0, -0.5, 0.0, 0.154269),
            (0.0, -0.5, -0.5, 0.081660),
            (0.0, -0.5, 0.5, 0.226878),
            (0.0, 0.5, 0.0, 0.345731),
            (0.0, 0.5, -0.5, 0.273122),
            (0.0, 0.5, 0.5, 0.418340),
            (-0.5, 0.0, 0.0, 0.154269),
            (-0.5, 0.0, -0.5, 0.081660),
            (-0.5, 0.0, 0.5, 0.226878),
            (-0.5, -0.5, 0.0, 0.095195),
            (-0.5, -0.5, -0.5, 0.036298),
            (-0.5, -0.5, 0.5, 0.163319),
            (-0.5, 0.5, 0.0, 0.213342),
            (-0.5, 0.5, -0.5, 0.145218),
            (-0.5, 0.5, 0.5, 0.272239),
            (0.5, 0.0, 0.0, 0.345731),
            (0.5, 0.0, -0.5, 0.273122),
            (0.5, 0.0, 0.5, 0.418340),
            (0.5, -0.5, 0.0, 0.213342),
            (0.5, -0.5, -0.5, 0.145218),
            (0.5, -0.5, 0.5, 0.272239),
            (0.5, 0.5, 0.0, 0.478120),
            (0.5, 0.5, -0.5, 0.419223),
            (0.5, 0.5, 0.5, 0.546244),
            (0.0, 0.0, (0.5_f64).sqrt(), 3.0 / 8.0),
            (0.0, 30.0, -1.0, 0.500000),
            (0.0, 30.0, 0.0, 0.500000),
            (0.0, 30.0, 1.0, 0.500000),
            (30.0, 30.0, -1.0, 1.000000),
            (30.0, 30.0, 0.0, 1.000000),
            (30.0, 30.0, 1.0, 1.000000),
            (-30.0, -1.0, -1.0, 0.000000),
            (-30.0, 0.0, -1.0, 0.000000),
            (-30.0, 1.0, -1.0, 0.000000),
            (-30.0, -1.0, 0.0, 0.000000),
            (-30.0, 0.0, 0.0, 0.000000),
            (-30.0, 1.0, 0.0, 0.000000),
            (-30.0, -1.0, 1.0, 0.000000),
            (-30.0, 0.0, 1.0, 0.000000),
            (-30.0, 1.0, 1.0, 0.000000),
        ];
        for (a, b, rho, expected) in cases {
            let got = Dr78::new(rho).unwrap().value(a, b);
            assert!(
                (got - expected).abs() < 1e-6,
                "BVN({a}, {b}; {rho}) = {got}, expected {expected}"
            );
        }
    }

    // Port of checkBivariateAtZero: BVN(0, 0, rho) = 1/4 + asin(rho)/(2 pi).
    #[test]
    fn at_zero_matches_arcsin_identity() {
        let rhos = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.99999];
        for r in rhos {
            for sgn in [-1.0, 1.0] {
                let rho = sgn * r;
                let got = Dr78::new(rho).unwrap().value(0.0, 0.0);
                let expected = 0.25 + rho.asin() / (2.0 * PI);
                assert!(
                    (got - expected).abs() < 1e-6,
                    "rho={rho}: {got} vs {expected}"
                );
            }
        }
    }

    // Port of checkBivariateTail: the CDF must not decrease as y rises in the
    // far tail (else partial-barrier greeks go to garbage).
    #[test]
    fn tail_is_monotonic() {
        let bvn = Dr78::new(-0.999).unwrap();
        let x = -6.9;
        let mut y = 6.9;
        let tol = 1e-5;
        for _ in 0..10 {
            let cdf0 = bvn.value(x, y);
            y += tol;
            let cdf1 = bvn.value(x, y);
            assert!(cdf0 <= cdf1, "cdf decreased in tail: {cdf0} -> {cdf1}");
        }
    }

    // Regression (QA B1/B2): rho = +-1 with small same-sign a, b used to panic
    // (root=0 -> rho1=NaN -> expect) or return Ok(NaN) (denom=0 -> a1=inf). Now
    // the exact degenerate closed form is used.
    #[test]
    fn perfect_correlation_uses_closed_form() {
        let cum = CumulativeNormalDistribution::standard();
        let pos = Dr78::new(1.0).unwrap();
        for (a, b) in [(0.5, 0.5), (2.0, 3.0), (-0.3, 0.7), (1.0, 1.0)] {
            let got = pos.value(a, b);
            assert!(
                (got - cum.value(a.min(b))).abs() < 1e-12,
                "rho=1 ({a}, {b})"
            );
        }
        let neg = Dr78::new(-1.0).unwrap();
        for (a, b) in [(0.5, 0.5), (0.5, -0.3), (2.0, 3.0), (-1.0, -1.0)] {
            let got = neg.value(a, b);
            let expected = (cum.value(a) + cum.value(b) - 1.0).max(0.0);
            assert!((got - expected).abs() < 1e-12, "rho=-1 ({a}, {b})");
        }
    }

    // Regression (QA B3): a NaN argument used to reach the unreachable!() branch
    // and panic, because f64::max/min ignore NaN. It must propagate to NaN.
    #[test]
    fn nan_argument_yields_nan_not_panic() {
        let bvn = Dr78::new(0.5).unwrap();
        assert!(bvn.value(Real::NAN, 0.5).is_nan());
        assert!(bvn.value(0.5, Real::NAN).is_nan());
        assert!(bvn.value(Real::NAN, Real::NAN).is_nan());
    }

    #[test]
    fn new_rejects_correlation_outside_unit_interval() {
        assert!(Dr78::new(-1.5).is_err());
        assert!(Dr78::new(1.5).is_err());
        assert!(Dr78::new(Real::NAN).is_err());
        assert!(Dr78::new(Real::INFINITY).is_err());
        // the closed interval endpoints are allowed
        assert!(Dr78::new(-1.0).is_ok());
        assert!(Dr78::new(1.0).is_ok());
    }
}
