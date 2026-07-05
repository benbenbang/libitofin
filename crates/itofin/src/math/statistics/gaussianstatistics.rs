//! Gaussian-assumption risk measures.
//!
//! Port of `ql/math/statistics/gaussianstatistics.hpp`. QuantLib's
//! `GenericGaussianStatistics<Stat>` wraps any statistics tool and derives
//! risk measures from its mean and variance under a gaussian assumption; here
//! [`GaussianStatistics`] is an extension trait with default methods, blanket
//! implemented for every [`MeanStdDev`], so both accumulators and the
//! precomputed [`StatsHolder`] get the measures for free.

use crate::errors::QlResult;
use crate::fail;
use crate::math::distributions::normal::{
    CumulativeNormalDistribution, InverseCumulativeNormal, NormalDistribution,
};
use crate::types::Real;

use super::MeanStdDev;

/// Precomputed mean/standard-deviation pair, for taking gaussian risk
/// measures of an already-summarized distribution.
#[derive(Clone, Copy, Debug)]
pub struct StatsHolder {
    mean: Real,
    standard_deviation: Real,
}

impl StatsHolder {
    /// Wraps a precomputed mean and standard deviation.
    pub fn new(mean: Real, standard_deviation: Real) -> Self {
        StatsHolder {
            mean,
            standard_deviation,
        }
    }
}

impl MeanStdDev for StatsHolder {
    fn mean(&self) -> QlResult<Real> {
        Ok(self.mean)
    }

    fn standard_deviation(&self) -> QlResult<Real> {
        Ok(self.standard_deviation)
    }
}

/// Gaussian-assumption risk measures derived from a mean and a standard
/// deviation.
pub trait GaussianStatistics: MeanStdDev {
    /// Gaussian-assumption `percentile`-th percentile, for `percentile` in
    /// `(0, 1)`.
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `(0, 1)` or the
    /// underlying moments are unavailable.
    fn gaussian_percentile(&self, percentile: Real) -> QlResult<Real> {
        if percentile <= 0.0 {
            fail!("percentile ({percentile}) must be > 0.0");
        }
        if percentile >= 1.0 {
            fail!("percentile ({percentile}) must be < 1.0");
        }
        let inverse = InverseCumulativeNormal::new(self.mean()?, self.standard_deviation()?)?;
        inverse.value(percentile)
    }

    /// Gaussian-assumption top percentile, `gaussian_percentile(1 - p)`.
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `(0, 1)` or the
    /// underlying moments are unavailable.
    fn gaussian_top_percentile(&self, percentile: Real) -> QlResult<Real> {
        self.gaussian_percentile(1.0 - percentile)
    }

    /// Gaussian-assumption potential upside (the gain-side counterpart of
    /// VAR) at a percentile in `[0.9, 1)`; floored at 0.
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)` or the
    /// underlying moments are unavailable.
    fn gaussian_potential_upside(&self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        Ok(self.gaussian_percentile(percentile)?.max(0.0))
    }

    /// Gaussian-assumption value-at-risk at a percentile in `[0.9, 1)`,
    /// reported as a positive loss (capped at 0 and negated).
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)` or the
    /// underlying moments are unavailable.
    fn gaussian_value_at_risk(&self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        Ok(-self.gaussian_percentile(1.0 - percentile)?.min(0.0))
    }

    /// Gaussian-assumption expected shortfall (conditional VAR) at a
    /// percentile in `[0.9, 1)`: the expected loss given that the loss
    /// exceeds the VAR threshold, reported as a positive loss.
    ///
    /// # Errors
    ///
    /// Returns an error if `percentile` lies outside `[0.9, 1)` or the
    /// underlying moments are unavailable.
    fn gaussian_expected_shortfall(&self, percentile: Real) -> QlResult<Real> {
        require_var_percentile(percentile)?;
        let mean = self.mean()?;
        let std_dev = self.standard_deviation()?;
        let var = InverseCumulativeNormal::new(mean, std_dev)?.value(1.0 - percentile)?;
        let gaussian = NormalDistribution::new(mean, std_dev)?;
        let result = mean - std_dev * std_dev * gaussian.value(var) / (1.0 - percentile);
        Ok(-result.min(0.0))
    }

    /// Gaussian-assumption shortfall: the probability that a sample falls
    /// below `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying moments are unavailable.
    fn gaussian_shortfall(&self, target: Real) -> QlResult<Real> {
        let cumulative =
            CumulativeNormalDistribution::new(self.mean()?, self.standard_deviation()?)?;
        Ok(cumulative.value(target))
    }

    /// Gaussian-assumption average shortfall, `E[t - x | x < t]`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying moments are unavailable.
    fn gaussian_average_shortfall(&self, target: Real) -> QlResult<Real> {
        let mean = self.mean()?;
        let std_dev = self.standard_deviation()?;
        let cumulative = CumulativeNormalDistribution::new(mean, std_dev)?;
        let gaussian = NormalDistribution::new(mean, std_dev)?;
        Ok((target - mean) + std_dev * std_dev * gaussian.value(target) / cumulative.value(target))
    }

    /// Gaussian-assumption regret: the variance of observations below
    /// `target`, `E[(x - t)² | x < t]`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying moments are unavailable.
    fn gaussian_regret(&self, target: Real) -> QlResult<Real> {
        let mean = self.mean()?;
        let std_dev = self.standard_deviation()?;
        let variance = std_dev * std_dev;
        let cumulative = CumulativeNormalDistribution::new(mean, std_dev)?;
        let gaussian = NormalDistribution::new(mean, std_dev)?;
        let first_term = variance + mean * mean - 2.0 * target * mean + target * target;
        let alfa = cumulative.value(target);
        let second_term = mean - target;
        let beta = variance * gaussian.value(target);
        Ok((alfa * first_term - beta * second_term) / alfa)
    }

    /// Gaussian-assumption downside variance, `gaussian_regret(0)`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying moments are unavailable.
    fn gaussian_downside_variance(&self) -> QlResult<Real> {
        self.gaussian_regret(0.0)
    }

    /// Gaussian-assumption downside deviation, the square root of the
    /// downside variance.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying moments are unavailable.
    fn gaussian_downside_deviation(&self) -> QlResult<Real> {
        Ok(self.gaussian_downside_variance()?.sqrt())
    }
}

impl<T: MeanStdDev + ?Sized> GaussianStatistics for T {}

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
    use crate::math::comparison::close;
    use crate::math::statistics::testutil::{AVERAGES, SIGMAS, check, sobol_normal_samples};
    use crate::math::statistics::{GeneralStatistics, IncrementalStatistics, Statistics};

    #[test]
    fn matches_quantlib_riskstats_oracle() {
        for average in AVERAGES {
            for sigma in SIGMAS {
                let normal = NormalDistribution::new(average, sigma).unwrap();
                let cumulative = CumulativeNormalDistribution::new(average, sigma).unwrap();

                let data = sobol_normal_samples(average, sigma);
                let mut igs = IncrementalStatistics::new();
                let mut s = GeneralStatistics::new();
                igs.add_sequence(data.iter().copied()).unwrap();
                s.add_sequence(data.iter().copied()).unwrap();

                for stats in [&igs as &dyn GaussianStatistics, &s] {
                    let expected = average;
                    let tolerance = if expected == 0.0 {
                        1e-3
                    } else {
                        (expected * 1e-3).abs()
                    };
                    check(
                        "gaussian percentile",
                        average,
                        sigma,
                        stats.gaussian_percentile(0.5).unwrap(),
                        expected,
                        tolerance,
                    );

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
                        "gaussian potential upside",
                        average,
                        sigma,
                        stats.gaussian_potential_upside(two_sigma).unwrap(),
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
                        "gaussian value-at-risk",
                        average,
                        sigma,
                        stats.gaussian_value_at_risk(two_sigma).unwrap(),
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
                        "gaussian expected shortfall",
                        average,
                        sigma,
                        stats.gaussian_expected_shortfall(two_sigma).unwrap(),
                        expected,
                        tolerance,
                    );

                    check(
                        "gaussian shortfall",
                        average,
                        sigma,
                        stats.gaussian_shortfall(average).unwrap(),
                        0.5,
                        0.5e-3,
                    );

                    let expected = sigma / (2.0 * std::f64::consts::PI).sqrt() * 2.0;
                    check(
                        "gaussian average shortfall",
                        average,
                        sigma,
                        stats.gaussian_average_shortfall(average).unwrap(),
                        expected,
                        expected * 1e-3,
                    );

                    let expected = sigma * sigma;
                    check(
                        "gaussian regret",
                        average,
                        sigma,
                        stats.gaussian_regret(average).unwrap(),
                        expected,
                        expected * 1e-1,
                    );
                }
            }
        }
    }

    #[test]
    fn stats_holder_reproduces_accumulator_measures() {
        let data = sobol_normal_samples(-1.0, 2.0);
        let mut s = GeneralStatistics::new();
        s.add_sequence(data).unwrap();

        let holder = StatsHolder::new(s.mean().unwrap(), s.standard_deviation().unwrap());
        let two_sigma = CumulativeNormalDistribution::new(-1.0, 2.0)
            .unwrap()
            .value(-1.0 + 2.0 * 2.0);
        assert!(close(
            holder.gaussian_potential_upside(two_sigma).unwrap(),
            s.gaussian_potential_upside(two_sigma).unwrap(),
        ));
        assert!(close(
            holder.gaussian_downside_variance().unwrap(),
            s.gaussian_downside_variance().unwrap(),
        ));
    }

    #[test]
    fn rejects_out_of_range_percentiles() {
        let holder = StatsHolder::new(0.0, 1.0);
        assert!(holder.gaussian_percentile(0.0).is_err());
        assert!(holder.gaussian_percentile(1.0).is_err());
        assert!(holder.gaussian_potential_upside(0.5).is_err());
        assert!(holder.gaussian_value_at_risk(1.0).is_err());
        assert!(holder.gaussian_expected_shortfall(0.89).is_err());
    }
}
