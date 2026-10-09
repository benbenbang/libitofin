//! Immutable linear-exponential instantaneous forward volatility.
//!
//! Matches `ql/legacy/libormarketmodels/lmlinexpvolmodel.{hpp,cpp}`:
//! `sigma_i(t) = (a * (T_i - t) + d) * exp(-b * (T_i - t)) + c` before
//! fixing, and exactly zero at or after fixing. Integrated variance is absent.
//! Checked queries reject non-finite inputs and unrepresentable results. A
//! log-domain fallback avoids spurious intermediate overflow or underflow.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::require;
use crate::types::{Real, Size, Time};

/// Immutable positive-coefficient volatility model preserving fixing-row order.
#[derive(Clone, Debug)]
pub struct LmLinearExponentialVolatilityModel {
    fixing_times: Vec<Time>,
    a: Real,
    b: Real,
    c: Real,
    d: Real,
}

impl LmLinearExponentialVolatilityModel {
    /// Builds the model with finite positive coefficients, as in QuantLib.
    ///
    /// Fixing times can be unsorted, repeated or negative; row order is retained.
    ///
    /// # Errors
    ///
    /// Rejects empty or non-finite fixing times and non-finite or non-positive
    /// coefficients. Finite coefficients can still overflow at query time.
    pub fn new(fixing_times: Vec<Time>, a: Real, b: Real, c: Real, d: Real) -> QlResult<Self> {
        require!(!fixing_times.is_empty(), "fixing times must not be empty");
        require!(
            fixing_times.iter().all(|t| t.is_finite()),
            "fixing times must be finite"
        );
        require!(
            [a, b, c, d].iter().all(|v| v.is_finite() && *v > 0.0),
            "volatility coefficients must be finite and positive"
        );
        Ok(Self {
            fixing_times,
            a,
            b,
            c,
            d,
        })
    }

    /// Number of forward rows.
    pub fn size(&self) -> Size {
        self.fixing_times.len()
    }

    /// Borrows fixing times in their original row order.
    pub fn fixing_times(&self) -> &[Time] {
        &self.fixing_times
    }

    /// Returns instantaneous volatilities in fixing-row order.
    ///
    /// # Errors
    ///
    /// Rejects non-finite time, unrepresentable elapsed time or volatility.
    pub fn volatility(&self, t: Time) -> QlResult<Array> {
        require!(t.is_finite(), "volatility time must be finite");
        (0..self.size()).map(|i| self.volatility_i(i, t)).collect()
    }

    /// Returns one row's volatility, exactly zero at or after its fixing.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range row, non-finite time, unrepresentable elapsed time
    /// or volatility. Exponential underflow is a valid asymptotic limit.
    pub fn volatility_i(&self, i: Size, t: Time) -> QlResult<Real> {
        require!(i < self.size(), "volatility index out of range");
        require!(t.is_finite(), "volatility time must be finite");
        let fixing = self.fixing_times[i];
        if fixing <= t {
            return Ok(0.0);
        }
        let elapsed = fixing - t;
        require!(elapsed.is_finite(), "elapsed time is unrepresentable");
        let exponent = self.b * elapsed;
        let affine = self.a * elapsed + self.d;
        let decay = (-exponent).exp();
        let term = if affine.is_finite() && decay >= Real::MIN_POSITIVE {
            affine * decay
        } else {
            let log_linear = self.a.ln() + elapsed.ln();
            let log_constant = self.d.ln();
            let log_affine =
                log_linear.max(log_constant) + (-(log_linear - log_constant).abs()).exp().ln_1p();
            (log_affine - exponent).exp()
        };
        let result = term + self.c;
        require!(result.is_finite(), "volatility is unrepresentable");
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_preserves_row_order_and_zeroes_at_fixing() {
        let model =
            LmLinearExponentialVolatilityModel::new(vec![3.0, 1.0, -1.0, 1.0], 0.2, 0.4, 0.1, 0.3)
                .unwrap();
        assert_eq!(model.fixing_times(), &[3.0, 1.0, -1.0, 1.0]);
        assert_eq!(model.size(), 4);
        for t in [-2.0, 0.0, 1.0, 3.0, 4.0] {
            let vols = model.volatility(t).unwrap();
            for (i, &fixing) in model.fixing_times().iter().enumerate() {
                let expected = if fixing > t {
                    (0.2 * (fixing - t) + 0.3) * (-0.4 * (fixing - t)).exp() + 0.1
                } else {
                    0.0
                };
                assert!((vols[i] - expected).abs() < 1e-15);
                assert_eq!(vols[i], model.volatility_i(i, t).unwrap());
            }
        }
    }

    #[test]
    fn long_elapsed_time_has_finite_asymptotic_volatility() {
        let model =
            LmLinearExponentialVolatilityModel::new(vec![1e6, Real::MAX], 0.2, 0.4, 0.1, 0.3)
                .unwrap();
        assert_eq!(&*model.volatility(0.0).unwrap(), &[0.1, 0.1]);
    }

    #[test]
    fn log_fallback_avoids_intermediate_overflow_and_decay_underflow() {
        let model = LmLinearExponentialVolatilityModel::new(
            vec![2.0, 750.0, 744.0],
            Real::MAX,
            1.0,
            Real::MIN_POSITIVE,
            1.0,
        )
        .unwrap();
        let first = model.volatility_i(0, 0.0).unwrap();
        assert!(first.is_finite());
        let expected = Real::MAX * (2.0 * (-2.0_f64).exp());
        assert!((first / expected - 1.0).abs() < 2e-13);
        let second = model.volatility_i(1, 0.0).unwrap();
        let expected = (Real::MAX.ln() + 750.0_f64.ln() - 750.0).exp();
        assert!((second / expected - 1.0).abs() < 2e-13);
        assert!(second > 1e-16);
        let subnormal_decay = model.volatility_i(2, 0.0).unwrap();
        let expected = (Real::MAX.ln() + 744.0_f64.ln() - 744.0).exp();
        assert!((subnormal_decay / expected - 1.0).abs() < 2e-13);
    }

    #[test]
    fn rejects_empty_times_and_each_invalid_coefficient() {
        assert!(LmLinearExponentialVolatilityModel::new(vec![], 1.0, 1.0, 1.0, 1.0).is_err());
        for bad in [0.0, -1.0, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            for index in 0..4 {
                let mut p = [1.0; 4];
                p[index] = bad;
                assert!(
                    LmLinearExponentialVolatilityModel::new(vec![1.0], p[0], p[1], p[2], p[3])
                        .is_err()
                );
            }
        }
        for bad in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(
                LmLinearExponentialVolatilityModel::new(vec![bad], 1.0, 1.0, 1.0, 1.0).is_err()
            );
        }
    }

    #[test]
    fn rejects_invalid_queries_and_true_output_overflow() {
        let model = LmLinearExponentialVolatilityModel::new(
            vec![Real::MAX],
            Real::MAX,
            Real::MIN_POSITIVE,
            Real::MAX,
            Real::MAX,
        )
        .unwrap();
        for t in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(model.volatility(t).is_err());
            assert!(model.volatility_i(0, t).is_err());
        }
        assert!(model.volatility_i(1, 0.0).is_err());
        assert!(model.volatility_i(0, -Real::MAX).is_err());
        assert!(model.volatility(0.0).is_err());
        assert_eq!(model.volatility_i(0, Real::MAX).unwrap(), 0.0);
    }
}
