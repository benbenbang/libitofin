//! Normal distribution.
//!
//! Port of `ql/math/distributions/normaldistribution.{hpp,cpp}`:
//! [`NormalDistribution`] (density), [`CumulativeNormalDistribution`] (the CDF
//! via [`erf`], with an asymptotic tail for very negative arguments), and
//! [`InverseCumulativeNormal`] (Peter Acklam's rational approximation). The
//! Moro and Maddock variants are deferred.

// Math constants are transcribed verbatim from QuantLib's mathconstants.hpp;
// their precision exceeds f64 but rounds to the intended bit pattern.
#![allow(clippy::excessive_precision)]

use crate::errors::QlResult;
use crate::fail;
use crate::math::errorfunction::erf;
use crate::types::Real;

// QuantLib's M_SQRT_2 is 1/√2; the std constant is the same f64.
const M_SQRT_2: Real = std::f64::consts::FRAC_1_SQRT_2;
const M_1_SQRTPI: Real = 0.564189583547756286948;

/// Validates the `(average, sigma)` shared by the three normal distributions.
/// QuantLib validates neither; we reject a non-finite `average` (which would
/// otherwise make every `value` NaN) and a non-finite or non-positive `sigma`.
fn validate_params(average: Real, sigma: Real) -> QlResult<()> {
    if !average.is_finite() {
        fail!("average must be a finite number ({average} given)");
    }
    if !sigma.is_finite() || sigma <= 0.0 {
        fail!("sigma must be a finite positive number ({sigma} given)");
    }
    Ok(())
}

/// Gaussian probability density `N(average, sigma)`.
#[derive(Clone, Copy, Debug)]
pub struct NormalDistribution {
    average: Real,
    normalization_factor: Real,
    denominator: Real,
    der_normalization_factor: Real,
}

impl NormalDistribution {
    /// A normal density with the given mean and standard deviation.
    ///
    /// # Errors
    ///
    /// Returns an error unless `average` is finite and `sigma` is finite and
    /// `> 0` (a non-finite `average` yields NaN densities; a non-positive
    /// `sigma`, negative densities; a non-finite `sigma`, NaN outputs).
    pub fn new(average: Real, sigma: Real) -> QlResult<Self> {
        validate_params(average, sigma)?;
        let normalization_factor = M_SQRT_2 * M_1_SQRTPI / sigma;
        let der_normalization_factor = sigma * sigma;
        Ok(NormalDistribution {
            average,
            normalization_factor,
            denominator: 2.0 * der_normalization_factor,
            der_normalization_factor,
        })
    }

    /// The standard normal density `N(0, 1)`.
    pub fn standard() -> Self {
        NormalDistribution::new(0.0, 1.0).expect("standard normal (0, 1) is valid")
    }

    /// The density at `x`.
    pub fn value(&self, x: Real) -> Real {
        let deltax = x - self.average;
        let exponent = -(deltax * deltax) / self.denominator;
        // below this, exp(exponent) is ~1e-300 or less; QuantLib treats it as 0
        if exponent <= -690.0 {
            0.0
        } else {
            self.normalization_factor * exponent.exp()
        }
    }

    /// The derivative of the density at `x`.
    pub fn derivative(&self, x: Real) -> Real {
        (self.value(x) * (self.average - x)) / self.der_normalization_factor
    }
}

impl Default for NormalDistribution {
    fn default() -> Self {
        NormalDistribution::standard()
    }
}

/// Cumulative normal distribution function for `N(average, sigma)`.
#[derive(Clone, Copy, Debug)]
pub struct CumulativeNormalDistribution {
    average: Real,
    sigma: Real,
    gaussian: NormalDistribution,
}

impl CumulativeNormalDistribution {
    /// A cumulative normal for the given mean and standard deviation.
    ///
    /// # Errors
    ///
    /// Returns an error unless `average` is finite and `sigma` is finite and `> 0`.
    pub fn new(average: Real, sigma: Real) -> QlResult<Self> {
        validate_params(average, sigma)?;
        Ok(CumulativeNormalDistribution {
            average,
            sigma,
            gaussian: NormalDistribution::standard(),
        })
    }

    /// The standard cumulative normal `N(0, 1)`.
    pub fn standard() -> Self {
        CumulativeNormalDistribution {
            average: 0.0,
            sigma: 1.0,
            gaussian: NormalDistribution::standard(),
        }
    }

    /// The probability that a `N(average, sigma)` variate is at most `x`.
    pub fn value(&self, x: Real) -> Real {
        let z = (x - self.average) / self.sigma;
        let result = 0.5 * (1.0 + erf(z * M_SQRT_2));
        if result <= 1e-8 {
            // Asymptotic expansion for very negative z (Abramowitz & Stegun
            // 26.2.12), where the erf-based form loses all significance.
            let zsqr = z * z;
            let mut sum = 1.0;
            let mut i = 1.0;
            let mut g = 1.0;
            let mut a = Real::MAX;
            loop {
                let lasta = a;
                let x = (4.0 * i - 3.0) / zsqr;
                let y = x * ((4.0 * i - 1.0) / zsqr);
                a = g * (x - y);
                sum -= a;
                g *= y;
                i += 1.0;
                a = a.abs();
                if !(lasta > a && a >= (sum * Real::EPSILON).abs()) {
                    break;
                }
            }
            -self.gaussian.value(z) / z * sum
        } else {
            result
        }
    }

    /// The derivative of the CDF (i.e. the density) at `x`.
    pub fn derivative(&self, x: Real) -> Real {
        let xn = (x - self.average) / self.sigma;
        self.gaussian.value(xn) / self.sigma
    }
}

// Coefficients for Acklam's rational approximation.
const A1: Real = -3.969683028665376e+01;
const A2: Real = 2.209460984245205e+02;
const A3: Real = -2.759285104469687e+02;
const A4: Real = 1.383577518672690e+02;
const A5: Real = -3.066479806614716e+01;
const A6: Real = 2.506628277459239e+00;
const B1: Real = -5.447609879822406e+01;
const B2: Real = 1.615858368580409e+02;
const B3: Real = -1.556989798598866e+02;
const B4: Real = 6.680131188771972e+01;
const B5: Real = -1.328068155288572e+01;
const C1: Real = -7.784894002430293e-03;
const C2: Real = -3.223964580411365e-01;
const C3: Real = -2.400758277161838e+00;
const C4: Real = -2.549732539343734e+00;
const C5: Real = 4.374664141464968e+00;
const C6: Real = 2.938163982698783e+00;
const D1: Real = 7.784695709041462e-03;
const D2: Real = 3.224671290700398e-01;
const D3: Real = 2.445134137142996e+00;
const D4: Real = 3.754408661907416e+00;
const X_LOW: Real = 0.02425;
const X_HIGH: Real = 1.0 - X_LOW;

/// Inverse cumulative normal distribution via Peter Acklam's approximation.
#[derive(Clone, Copy, Debug)]
pub struct InverseCumulativeNormal {
    average: Real,
    sigma: Real,
}

impl InverseCumulativeNormal {
    /// An inverse cumulative normal for the given mean and standard deviation.
    ///
    /// # Errors
    ///
    /// Returns an error unless `average` is finite and `sigma` is finite and `> 0`.
    pub fn new(average: Real, sigma: Real) -> QlResult<Self> {
        validate_params(average, sigma)?;
        Ok(InverseCumulativeNormal { average, sigma })
    }

    /// The standard inverse cumulative normal `N(0, 1)`.
    pub fn standard() -> Self {
        InverseCumulativeNormal {
            average: 0.0,
            sigma: 1.0,
        }
    }

    /// The `y` such that `P(N(average, sigma) ≤ y) = x`, for `x` in `(0, 1)`.
    pub fn value(&self, x: Real) -> QlResult<Real> {
        Ok(self.average + self.sigma * Self::standard_value(x)?)
    }

    /// The inverse CDF for `N(0, 1)`.
    pub fn standard_value(x: Real) -> QlResult<Real> {
        if !(X_LOW..=X_HIGH).contains(&x) {
            Self::tail_value(x)
        } else {
            let z = x - 0.5;
            let r = z * z;
            Ok(
                (((((A1 * r + A2) * r + A3) * r + A4) * r + A5) * r + A6) * z
                    / (((((B1 * r + B2) * r + B3) * r + B4) * r + B5) * r + 1.0),
            )
        }
    }

    fn tail_value(x: Real) -> QlResult<Real> {
        if !x.is_finite() || x <= 0.0 || x >= 1.0 {
            // recover from numerical error at the boundaries; otherwise reject
            // (this also rejects NaN/infinite x, which the range check routes here)
            if (x - 1.0).abs() <= 42.0 * Real::EPSILON {
                return Ok(Real::MAX);
            } else if x.abs() < Real::EPSILON {
                return Ok(Real::MIN);
            }
            fail!("InverseCumulativeNormal({x}) undefined: must be 0 < x < 1");
        }
        Ok(if x < X_LOW {
            let z = (-2.0 * x.ln()).sqrt();
            (((((C1 * z + C2) * z + C3) * z + C4) * z + C5) * z + C6)
                / ((((D1 * z + D2) * z + D3) * z + D4) * z + 1.0)
        } else {
            let z = (-2.0 * (1.0 - x).ln()).sqrt();
            -(((((C1 * z + C2) * z + C3) * z + C4) * z + C5) * z + C6)
                / ((((D1 * z + D2) * z + D3) * z + D4) * z + 1.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-15;

    #[test]
    fn density_matches_analytic_gaussian() {
        let n = NormalDistribution::standard();
        assert!((n.value(0.0) - 0.398_942_280_401_432_7).abs() < TOL);
        assert!((n.value(1.0) - 0.241_970_724_519_143_37).abs() < TOL);
        // symmetric
        assert!((n.value(-2.3) - n.value(2.3)).abs() < TOL);
    }

    #[test]
    fn cdf_known_values() {
        let c = CumulativeNormalDistribution::standard();
        assert!((c.value(0.0) - 0.5).abs() < TOL);
        assert!((c.value(1.0) - 0.841_344_746_068_542_9).abs() < TOL);
        assert!((c.value(-1.0) - 0.158_655_253_931_457_07).abs() < TOL);
        assert!((c.value(1.96) - 0.975_002_104_851_779_5).abs() < TOL);
    }

    #[test]
    fn cdf_asymptotic_tail_for_very_negative() {
        // result <= 1e-8 triggers the asymptotic expansion (26.2.12), which
        // QuantLib uses here because the 0.5·(1+erf) form suffers catastrophic
        // cancellation for very negative z. The series is good to ~8 figures at
        // z = -6, so we check the true Φ(-6) only to a matching tolerance.
        let c = CumulativeNormalDistribution::standard();
        let phi_m6 = 9.865_876_450_376_946e-10;
        assert!(((c.value(-6.0) - phi_m6) / phi_m6).abs() < 1e-7);
    }

    #[test]
    fn cdf_derivative_is_density() {
        let c = CumulativeNormalDistribution::new(0.3, 1.7).unwrap();
        let n = NormalDistribution::new(0.3, 1.7).unwrap();
        for &x in &[-2.0, -0.5, 0.0, 1.1, 3.0] {
            assert!((c.derivative(x) - n.value(x)).abs() < TOL);
        }
    }

    #[test]
    fn inverse_of_one_half_is_zero() {
        let inv = InverseCumulativeNormal::standard();
        assert_eq!(inv.value(0.5).unwrap(), 0.0);
    }

    #[test]
    fn inverse_round_trips_with_cdf() {
        let cum = CumulativeNormalDistribution::new(0.1, 1.3).unwrap();
        let inv = InverseCumulativeNormal::new(0.1, 1.3).unwrap();
        let (lo, hi) = (0.1 - 6.0 * 1.3, 0.1 + 6.0 * 1.3);
        let n = 2000;
        let mut max_err: Real = 0.0;
        for i in 0..=n {
            let x = lo + (hi - lo) * (i as Real) / (n as Real);
            let round = inv.value(cum.value(x)).unwrap();
            max_err = max_err.max((round - x).abs());
        }
        assert!(max_err < 1e-7, "round-trip error {max_err} exceeds 1e-7");
    }

    #[test]
    fn invalid_sigma_is_rejected() {
        // every constructor must reject non-positive and non-finite sigma.
        for s in [0.0, -1.0, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(
                NormalDistribution::new(0.0, s).is_err(),
                "density sigma={s}"
            );
            assert!(
                CumulativeNormalDistribution::new(0.0, s).is_err(),
                "cumulative sigma={s}"
            );
            assert!(
                InverseCumulativeNormal::new(0.0, s).is_err(),
                "inverse sigma={s}"
            );
        }
    }

    #[test]
    fn non_finite_average_is_rejected() {
        // a non-finite average would otherwise make every density/CDF NaN, so
        // every constructor rejects it even though sigma is valid.
        for a in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(
                NormalDistribution::new(a, 1.0).is_err(),
                "density average={a}"
            );
            assert!(
                CumulativeNormalDistribution::new(a, 1.0).is_err(),
                "cumulative average={a}"
            );
            assert!(
                InverseCumulativeNormal::new(a, 1.0).is_err(),
                "inverse average={a}"
            );
        }
    }

    #[test]
    fn inverse_outside_unit_interval_errors() {
        assert!(InverseCumulativeNormal::standard_value(-0.1).is_err());
        assert!(InverseCumulativeNormal::standard_value(1.5).is_err());
        // NaN and infinities must error, not fall through to Ok(NaN).
        assert!(InverseCumulativeNormal::standard_value(Real::NAN).is_err());
        assert!(InverseCumulativeNormal::standard_value(Real::INFINITY).is_err());
    }
}
