//! Empirical-distribution risk measures.
//!
//! Port of `ql/math/statistics/riskstatistics.hpp` (and the `Statistics`
//! typedef of `statistics.hpp`). QuantLib's `GenericRiskStatistics<S>` wraps
//! a statistics tool and derives risk measures from the data distribution it
//! reports; here [`RiskStatistics`] is an extension trait with default
//! methods, blanket implemented for every [`EmpiricalStatistics`]. QuantLib's
//! default `RiskStatistics`/`Statistics` tool is [`GeneralStatistics`] with
//! this trait and [`GaussianStatistics`](super::GaussianStatistics) in scope.
//!
//! [`GeneralStatistics`]: super::GeneralStatistics

use crate::errors::QlResult;
use crate::types::Real;
use crate::{ensure, fail, require};

use super::EmpiricalStatistics;

/// Risk measures based on the empirical distribution of the accumulated
/// samples.
pub trait RiskStatistics: EmpiricalStatistics {
    /// The variance of observations below the mean,
    /// `N/(N-1) E[(x - ⟨x⟩)² | x < ⟨x⟩]` (Markowitz 1959).
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples below the mean.
    fn semi_variance(&self) -> QlResult<Real> {
        self.regret(self.mean()?)
    }

    /// The semi deviation, the square root of the semi variance.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples below the mean.
    fn semi_deviation(&self) -> QlResult<Real> {
        Ok(self.semi_variance()?.sqrt())
    }

    /// The variance of observations below zero, `N/(N-1) E[x² | x < 0]`.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples below zero.
    fn downside_variance(&self) -> QlResult<Real> {
        self.regret(0.0)
    }

    /// The downside deviation, the square root of the downside variance.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples below zero.
    fn downside_deviation(&self) -> QlResult<Real> {
        Ok(self.downside_variance()?.sqrt())
    }

    /// The variance of observations below `target`,
    /// `N/(N-1) E[(x - t)² | x < t]` (Dembo and Freeman 2001).
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples below the target.
    fn regret(&self, target: Real) -> QlResult<Real> {
        let result = self.expectation_value(
            |x| {
                let d = x - target;
                d * d
            },
            |x| x < target,
        );
        let Some((value, n)) = result else {
            fail!("samples under target <= 1: insufficient");
        };
        require!(n > 1, "samples under target <= 1: insufficient");
        let n = n as Real;
        Ok(n / (n - 1.0) * value)
    }

    /// Potential upside (the gain-side counterpart of VAR) at a percentile
    /// in `[0.9, 1)`; floored at 0.
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)` or the
    /// sample set is empty.
    fn potential_upside(&mut self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        Ok(self.percentile(percentile)?.max(0.0))
    }

    /// Value-at-risk at a percentile in `[0.9, 1)`, reported as a positive
    /// loss (capped at 0 and negated).
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)` or the
    /// sample set is empty.
    fn value_at_risk(&mut self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        Ok(-self.percentile(1.0 - percentile)?.min(0.0))
    }

    /// Expected shortfall (conditional VAR) at a percentile in `[0.9, 1)`:
    /// the average of the observations below the VAR threshold, reported as
    /// a positive loss (Artzner et al. 1999).
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)`, the sample
    /// set is empty, or no data fall below the threshold.
    fn expected_shortfall(&mut self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        ensure!(self.samples() != 0, "empty sample set");
        let target = -self.value_at_risk(percentile)?;
        let result = self.expectation_value(|x| x, |x| x < target);
        let Some((value, _)) = result else {
            fail!("no data below the target");
        };
        Ok(-value.min(0.0))
    }

    /// The probability of missing `target`: the weight fraction of
    /// observations below it.
    ///
    /// # Errors
    ///
    /// Returns an error on an empty sample set.
    fn shortfall(&self, target: Real) -> QlResult<Real> {
        ensure!(self.samples() != 0, "empty sample set");
        let (value, _) = self
            .expectation_value(|x| if x < target { 1.0 } else { 0.0 }, |_| true)
            .expect("sample set is not empty");
        Ok(value)
    }

    /// The averaged shortfallness, `E[t - x | x < t]`.
    ///
    /// # Errors
    ///
    /// Returns an error when no data fall below the target.
    fn average_shortfall(&self, target: Real) -> QlResult<Real> {
        let result = self.expectation_value(|x| target - x, |x| x < target);
        let Some((value, _)) = result else {
            fail!("no data below the target");
        };
        Ok(value)
    }
}

impl<T: EmpiricalStatistics + ?Sized> RiskStatistics for T {}

/// Validates the percentile range `[0.9, 1)` shared by the VAR-style
/// measures.
fn require_var_percentile(percentile: Real) -> QlResult<()> {
    if !(0.9..1.0).contains(&percentile) {
        fail!("percentile ({percentile}) out of range [0.9, 1.0)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::{CumulativeNormalDistribution, NormalDistribution};
    use crate::math::statistics::testutil::{AVERAGES, SIGMAS, check, sobol_normal_samples};
    use crate::math::statistics::{GaussianStatistics, GeneralStatistics, Statistics};

    #[test]
    fn matches_quantlib_riskstats_oracle() {
        for average in AVERAGES {
            for sigma in SIGMAS {
                let normal = NormalDistribution::new(average, sigma).unwrap();
                let cumulative = CumulativeNormalDistribution::new(average, sigma).unwrap();

                let data = sobol_normal_samples(average, sigma);
                let mut s = GeneralStatistics::new();
                s.add_sequence(data).unwrap();

                let upper_tail = average + 2.0 * sigma;
                let lower_tail = average - 2.0 * sigma;
                let two_sigma = cumulative.value(upper_tail);

                let expected = upper_tail.max(0.0);
                let tolerance = if expected == 0.0 {
                    1e-3
                } else {
                    (expected * 1e-3).abs()
                };
                check(
                    "potential upside",
                    average,
                    sigma,
                    s.potential_upside(two_sigma).unwrap(),
                    expected,
                    tolerance,
                );

                let expected = -lower_tail.min(0.0);
                let tolerance = if expected == 0.0 {
                    1e-3
                } else {
                    (expected * 1e-3).abs()
                };
                check(
                    "value-at-risk",
                    average,
                    sigma,
                    s.value_at_risk(two_sigma).unwrap(),
                    expected,
                    tolerance,
                );

                if average > 0.0 && sigma < average {
                    continue;
                }

                let expected = -(average
                    - sigma * sigma * normal.value(lower_tail) / (1.0 - two_sigma))
                    .min(0.0);
                let tolerance = if expected == 0.0 {
                    1e-4
                } else {
                    (expected * 1e-2).abs()
                };
                check(
                    "expected shortfall",
                    average,
                    sigma,
                    s.expected_shortfall(two_sigma).unwrap(),
                    expected,
                    tolerance,
                );

                check(
                    "shortfall",
                    average,
                    sigma,
                    s.shortfall(average).unwrap(),
                    0.5,
                    0.5e-3,
                );

                let expected = sigma / (2.0 * std::f64::consts::PI).sqrt() * 2.0;
                check(
                    "average shortfall",
                    average,
                    sigma,
                    s.average_shortfall(average).unwrap(),
                    expected,
                    expected * 1e-3,
                );

                let expected = sigma * sigma;
                check(
                    "regret",
                    average,
                    sigma,
                    s.regret(average).unwrap(),
                    expected,
                    expected * 1e-1,
                );

                let expected = s.downside_variance().unwrap();
                let tolerance = if expected == 0.0 {
                    1e-3
                } else {
                    (expected * 1e-3).abs()
                };
                check(
                    "gaussian downside variance vs empirical",
                    average,
                    sigma,
                    s.gaussian_downside_variance().unwrap(),
                    expected,
                    tolerance,
                );

                if average == 0.0 {
                    let expected = sigma * sigma;
                    check(
                        "downside variance",
                        average,
                        sigma,
                        s.downside_variance().unwrap(),
                        expected,
                        expected * 1e-3,
                    );
                    check(
                        "semi variance",
                        average,
                        sigma,
                        s.semi_variance().unwrap(),
                        expected,
                        expected * 1e-3,
                    );
                }
            }
        }
    }

    #[test]
    fn rejects_out_of_range_and_insufficient_data() {
        let mut s = GeneralStatistics::new();
        s.add_sequence([0.1, 0.2, 0.3]).unwrap();
        assert!(s.potential_upside(0.5).is_err());
        assert!(s.value_at_risk(1.0).is_err());
        assert!(s.expected_shortfall(0.89).is_err());
        assert!(s.downside_variance().is_err());
        assert!(s.average_shortfall(0.05).is_err());

        let mut empty = GeneralStatistics::new();
        assert!(empty.shortfall(0.0).is_err());
        assert!(empty.expected_shortfall(0.95).is_err());
    }

    #[test]
    fn shortfall_measures_on_known_data() {
        let mut s = GeneralStatistics::new();
        s.add_sequence([-4.0, -2.0, 1.0, 5.0]).unwrap();
        assert_eq!(s.shortfall(0.0).unwrap(), 0.5);
        assert_eq!(s.average_shortfall(0.0).unwrap(), 3.0);
        assert_eq!(s.regret(0.0).unwrap(), 20.0);
        assert_eq!(s.downside_variance().unwrap(), s.regret(0.0).unwrap());
    }
}
