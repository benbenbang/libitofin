//! Statistics tool based on incremental accumulation.
//!
//! Port of `ql/math/statistics/incrementalstatistics.{hpp,cpp}`, which wraps
//! the boost accumulator library. Instead of boost's raw-moment forms we
//! update the weighted central moments online (Pébay's single-pass update
//! formulas): mathematically equal, but stabler when the mean is large
//! relative to the spread. A second accumulator tracks the negative samples
//! for the downside measures, mirroring QuantLib's `downsideAcc_`.

use crate::errors::QlResult;
use crate::types::{Real, Size};
use crate::{fail, require};

use super::{MeanStdDev, Statistics};

/// Single-pass statistics accumulator; it keeps running moments instead of
/// the samples themselves.
#[derive(Clone, Debug, Default)]
pub struct IncrementalStatistics {
    count: Size,
    weight_sum: Real,
    mean: Real,
    m2: Real,
    m3: Real,
    m4: Real,
    min: Real,
    max: Real,
    downside_count: Size,
    downside_weight_sum: Real,
    downside_second_moment_sum: Real,
}

impl IncrementalStatistics {
    /// An empty accumulator.
    pub fn new() -> Self {
        IncrementalStatistics::default()
    }

    /// Number of negative samples collected.
    pub fn downside_samples(&self) -> Size {
        self.downside_count
    }

    /// Sum of the data weights for the negative samples.
    pub fn downside_weight_sum(&self) -> Real {
        self.downside_weight_sum
    }

    /// The downside variance, `N/(N-1) Σ wᵢ xᵢ² / Σ wᵢ` over the negative
    /// samples.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two negative samples or a zero
    /// downside weight sum.
    pub fn downside_variance(&self) -> QlResult<Real> {
        if self.downside_weight_sum <= 0.0 {
            fail!("sample weight is 0: insufficient");
        }
        require!(self.downside_count > 1, "sample number <= 1: insufficient");
        let n = self.downside_count as Real;
        Ok(n / (n - 1.0) * self.downside_second_moment_sum / self.downside_weight_sum)
    }

    /// The downside deviation, the square root of the downside variance.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two negative samples.
    pub fn downside_deviation(&self) -> QlResult<Real> {
        Ok(self.downside_variance()?.sqrt())
    }

    /// The biased (population) central moment of order `k`, `Mₖ / Σ wᵢ`.
    fn central_moment(&self, m: Real) -> Real {
        m / self.weight_sum
    }
}

impl MeanStdDev for IncrementalStatistics {
    fn mean(&self) -> QlResult<Real> {
        if self.weight_sum <= 0.0 {
            fail!("sample weight is 0: insufficient");
        }
        Ok(self.mean)
    }

    fn standard_deviation(&self) -> QlResult<Real> {
        Ok(self.variance()?.sqrt())
    }
}

impl Statistics for IncrementalStatistics {
    fn samples(&self) -> Size {
        self.count
    }

    fn weight_sum(&self) -> Real {
        self.weight_sum
    }

    fn variance(&self) -> QlResult<Real> {
        if self.weight_sum <= 0.0 {
            fail!("sample weight is 0: insufficient");
        }
        require!(self.count > 1, "sample number <= 1: insufficient");
        let n = self.count as Real;
        Ok(n / (n - 1.0) * self.central_moment(self.m2))
    }

    fn error_estimate(&self) -> QlResult<Real> {
        Ok((self.variance()? / self.count as Real).sqrt())
    }

    fn skewness(&self) -> QlResult<Real> {
        require!(self.count > 2, "sample number <= 2: insufficient");
        let n = self.count as Real;
        let g1 = self.central_moment(self.m3) / self.central_moment(self.m2).powf(1.5);
        let r1 = n / (n - 2.0);
        let r2 = (n - 1.0) / (n - 2.0);
        Ok((r1 * r2).sqrt() * g1)
    }

    fn kurtosis(&self) -> QlResult<Real> {
        require!(self.count > 3, "sample number <= 3: insufficient");
        let n = self.count as Real;
        let m2 = self.central_moment(self.m2);
        let g2 = self.central_moment(self.m4) / (m2 * m2) - 3.0;
        let r1 = (n - 1.0) / (n - 2.0);
        let r2 = (n + 1.0) / (n - 3.0);
        let r3 = (n - 1.0) / (n - 3.0);
        Ok(((3.0 + g2) * r2 - 3.0 * r3) * r1)
    }

    fn min(&self) -> QlResult<Real> {
        require!(self.count > 0, "empty sample set");
        Ok(self.min)
    }

    fn max(&self) -> QlResult<Real> {
        require!(self.count > 0, "empty sample set");
        Ok(self.max)
    }

    fn add_weighted(&mut self, value: Real, weight: Real) -> QlResult<()> {
        if weight < 0.0 || weight.is_nan() {
            fail!("negative weight ({weight}) not allowed");
        }
        self.min = if self.count == 0 {
            value
        } else {
            self.min.min(value)
        };
        self.max = if self.count == 0 {
            value
        } else {
            self.max.max(value)
        };
        self.count += 1;
        if weight > 0.0 {
            let old_weight_sum = self.weight_sum;
            self.weight_sum += weight;
            let delta = value - self.mean;
            let shift = delta * weight / self.weight_sum;
            self.m4 += weight
                * delta.powi(4)
                * old_weight_sum
                * (old_weight_sum * old_weight_sum - old_weight_sum * weight + weight * weight)
                / self.weight_sum.powi(3)
                + 6.0 * shift * shift * self.m2
                - 4.0 * shift * self.m3;
            self.m3 += weight * delta.powi(3) * old_weight_sum * (old_weight_sum - weight)
                / (self.weight_sum * self.weight_sum)
                - 3.0 * shift * self.m2;
            self.m2 += weight * delta * (delta - shift);
            self.mean += shift;
        }
        if value < 0.0 {
            self.downside_count += 1;
            self.downside_weight_sum += weight;
            self.downside_second_moment_sum += weight * value * value;
        }
        Ok(())
    }

    fn reset(&mut self) {
        *self = IncrementalStatistics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::statistics::GeneralStatistics;
    use crate::math::statistics::testutil::{
        AVERAGES, N_SAMPLES, SIGMAS, check, sobol_normal_samples,
    };

    #[test]
    fn matches_quantlib_riskstats_oracle() {
        for average in AVERAGES {
            for sigma in SIGMAS {
                let data = sobol_normal_samples(average, sigma);
                let data_min = data.iter().copied().fold(Real::INFINITY, Real::min);
                let data_max = data.iter().copied().fold(Real::NEG_INFINITY, Real::max);

                let mut igs = IncrementalStatistics::new();
                igs.add_sequence_weighted(data.iter().map(|&x| (x, 1.0)))
                    .unwrap();

                assert_eq!(igs.samples(), N_SAMPLES);
                check(
                    "weight sum",
                    average,
                    sigma,
                    igs.weight_sum(),
                    N_SAMPLES as Real,
                    1e-10,
                );
                check(
                    "minimum",
                    average,
                    sigma,
                    igs.min().unwrap(),
                    data_min,
                    1e-12,
                );
                check(
                    "maximum",
                    average,
                    sigma,
                    igs.max().unwrap(),
                    data_max,
                    1e-12,
                );

                let tolerance = if average == 0.0 {
                    1e-13
                } else {
                    average.abs() * 1e-13
                };
                check(
                    "mean",
                    average,
                    sigma,
                    igs.mean().unwrap(),
                    average,
                    tolerance,
                );

                let expected = sigma * sigma;
                check(
                    "variance",
                    average,
                    sigma,
                    igs.variance().unwrap(),
                    expected,
                    expected * 1e-1,
                );
                check(
                    "standard deviation",
                    average,
                    sigma,
                    igs.standard_deviation().unwrap(),
                    sigma,
                    sigma * 1e-1,
                );
                check(
                    "skewness",
                    average,
                    sigma,
                    igs.skewness().unwrap(),
                    0.0,
                    1e-4,
                );
                check(
                    "kurtosis",
                    average,
                    sigma,
                    igs.kurtosis().unwrap(),
                    0.0,
                    1e-1,
                );

                if average == 0.0 {
                    let expected = sigma * sigma;
                    check(
                        "downside variance",
                        average,
                        sigma,
                        igs.downside_variance().unwrap(),
                        expected,
                        expected * 1e-3,
                    );
                }
            }
        }
    }

    #[test]
    fn agrees_with_general_statistics_on_weighted_data() {
        let values = [2.0, -3.5, 1.25, 8.0, -0.5, 4.75, -6.0, 3.0];
        let weights = [1.0, 2.0, 0.5, 1.5, 3.0, 1.0, 2.5, 0.75];

        let mut incremental = IncrementalStatistics::new();
        let mut general = GeneralStatistics::new();
        for (&value, &weight) in values.iter().zip(&weights) {
            incremental.add_weighted(value, weight).unwrap();
            general.add_weighted(value, weight).unwrap();
        }

        assert_eq!(incremental.samples(), general.samples());
        for (label, a, b) in [
            ("weight sum", incremental.weight_sum(), general.weight_sum()),
            ("mean", incremental.mean().unwrap(), general.mean().unwrap()),
            (
                "variance",
                incremental.variance().unwrap(),
                general.variance().unwrap(),
            ),
            (
                "skewness",
                incremental.skewness().unwrap(),
                general.skewness().unwrap(),
            ),
            (
                "kurtosis",
                incremental.kurtosis().unwrap(),
                general.kurtosis().unwrap(),
            ),
            (
                "minimum",
                incremental.min().unwrap(),
                general.min().unwrap(),
            ),
            (
                "maximum",
                incremental.max().unwrap(),
                general.max().unwrap(),
            ),
        ] {
            assert!(
                (a - b).abs() <= 1e-12,
                "{label}: incremental {a}, general {b}"
            );
        }

        let downside: Vec<(Real, Real)> = values
            .iter()
            .zip(&weights)
            .filter(|&(&value, _)| value < 0.0)
            .map(|(&value, &weight)| (value, weight))
            .collect();
        let n = downside.len() as Real;
        let weight_sum: Real = downside.iter().map(|&(_, w)| w).sum();
        let second: Real = downside.iter().map(|&(x, w)| w * x * x).sum();
        let expected = n / (n - 1.0) * second / weight_sum;
        assert!((incremental.downside_variance().unwrap() - expected).abs() <= 1e-12);
        assert_eq!(incremental.downside_samples(), downside.len());
    }

    #[test]
    fn zero_weight_samples_count_but_carry_no_mass() {
        let mut s = IncrementalStatistics::new();
        s.add_weighted(-5.0, 0.0).unwrap();
        assert_eq!(s.samples(), 1);
        assert_eq!(s.weight_sum(), 0.0);
        assert_eq!(s.min().unwrap(), -5.0);
        assert!(s.mean().is_err());

        s.add(1.0).unwrap();
        s.add(3.0).unwrap();
        assert_eq!(s.mean().unwrap(), 2.0);
        assert_eq!(s.min().unwrap(), -5.0);
    }

    #[test]
    fn rejects_insufficient_samples_and_bad_arguments() {
        let mut s = IncrementalStatistics::new();
        assert!(s.mean().is_err());
        assert!(s.min().is_err());
        assert!(s.downside_variance().is_err());
        assert!(s.add_weighted(1.0, -1.0).is_err());

        s.add(1.0).unwrap();
        assert!(s.variance().is_err());
        s.add(2.0).unwrap();
        assert!(s.skewness().is_err());
        s.add(3.0).unwrap();
        assert!(s.kurtosis().is_err());

        s.reset();
        assert_eq!(s.samples(), 0);
        assert_eq!(s.weight_sum(), 0.0);
    }
}
