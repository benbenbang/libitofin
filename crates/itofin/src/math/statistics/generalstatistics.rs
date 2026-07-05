//! General (full-sample) statistics tool.
//!
//! Port of `ql/math/statistics/generalstatistics.{hpp,cpp}`. The accumulator
//! stores every `(value, weight)` pair and computes all statistics on demand
//! from the empirical distribution, with no gaussian assumption. It does not
//! suffer the numerical instability of the incremental accumulator; the
//! trade-off is that it keeps all samples in memory.

use crate::errors::QlResult;
use crate::types::{Real, Size};
use crate::{fail, require};

use super::{EmpiricalStatistics, MeanStdDev, Statistics};

/// Statistics tool accumulating the full sample set.
#[derive(Clone, Debug)]
pub struct GeneralStatistics {
    samples: Vec<(Real, Real)>,
    sorted: bool,
}

impl GeneralStatistics {
    /// An empty accumulator.
    pub fn new() -> Self {
        GeneralStatistics {
            samples: Vec::new(),
            sorted: true,
        }
    }

    /// The collected `(value, weight)` pairs.
    pub fn data(&self) -> &[(Real, Real)] {
        &self.samples
    }

    /// Informs the internal storage of a planned increase in size.
    pub fn reserve(&mut self, additional: Size) {
        self.samples.reserve(additional);
    }

    /// Sorts the data set in increasing order; a no-op when already sorted.
    pub fn sort(&mut self) {
        if !self.sorted {
            self.samples
                .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            self.sorted = true;
        }
    }

    /// Validates a percentile argument and returns the weight target it maps
    /// to, sorting the samples on the way.
    fn percentile_target(&mut self, percent: Real) -> QlResult<Real> {
        if percent <= 0.0 || percent > 1.0 {
            fail!("percentile ({percent}) must be in (0.0, 1.0]");
        }
        let sample_weight = self.weight_sum();
        if sample_weight <= 0.0 {
            fail!("empty sample set");
        }
        self.sort();
        Ok(percent * sample_weight)
    }
}

impl Default for GeneralStatistics {
    fn default() -> Self {
        GeneralStatistics::new()
    }
}

impl MeanStdDev for GeneralStatistics {
    fn mean(&self) -> QlResult<Real> {
        require!(self.samples() > 0, "empty sample set");
        let (value, _) = self
            .expectation_value(|x| x, |_| true)
            .expect("sample set is not empty");
        Ok(value)
    }

    fn standard_deviation(&self) -> QlResult<Real> {
        Ok(self.variance()?.sqrt())
    }
}

impl Statistics for GeneralStatistics {
    fn samples(&self) -> Size {
        self.samples.len()
    }

    fn weight_sum(&self) -> Real {
        self.samples.iter().map(|&(_, weight)| weight).sum()
    }

    fn variance(&self) -> QlResult<Real> {
        require!(self.samples() > 1, "sample number <= 1: insufficient");
        let n = self.samples() as Real;
        let mean = self.mean()?;
        let (s2, _) = self
            .expectation_value(
                |x| {
                    let d = x - mean;
                    d * d
                },
                |_| true,
            )
            .expect("sample set is not empty");
        Ok(s2 * n / (n - 1.0))
    }

    fn error_estimate(&self) -> QlResult<Real> {
        Ok((self.variance()? / self.samples() as Real).sqrt())
    }

    fn skewness(&self) -> QlResult<Real> {
        require!(self.samples() > 2, "sample number <= 2: insufficient");
        let n = self.samples() as Real;
        let mean = self.mean()?;
        let (third, _) = self
            .expectation_value(
                |x| {
                    let d = x - mean;
                    d * d * d
                },
                |_| true,
            )
            .expect("sample set is not empty");
        let sigma = self.standard_deviation()?;
        Ok((third / (sigma * sigma * sigma)) * (n / (n - 1.0)) * (n / (n - 2.0)))
    }

    fn kurtosis(&self) -> QlResult<Real> {
        require!(self.samples() > 3, "sample number <= 3: insufficient");
        let n = self.samples() as Real;
        let mean = self.mean()?;
        let (fourth, _) = self
            .expectation_value(
                |x| {
                    let d = x - mean;
                    let d2 = d * d;
                    d2 * d2
                },
                |_| true,
            )
            .expect("sample set is not empty");
        let sigma2 = self.variance()?;
        let c1 = (n / (n - 1.0)) * (n / (n - 2.0)) * ((n + 1.0) / (n - 3.0));
        let c2 = 3.0 * ((n - 1.0) / (n - 2.0)) * ((n - 1.0) / (n - 3.0));
        Ok(c1 * (fourth / (sigma2 * sigma2)) - c2)
    }

    fn min(&self) -> QlResult<Real> {
        require!(self.samples() > 0, "empty sample set");
        Ok(self
            .samples
            .iter()
            .map(|&(value, _)| value)
            .fold(Real::INFINITY, Real::min))
    }

    fn max(&self) -> QlResult<Real> {
        require!(self.samples() > 0, "empty sample set");
        Ok(self
            .samples
            .iter()
            .map(|&(value, _)| value)
            .fold(Real::NEG_INFINITY, Real::max))
    }

    fn add_weighted(&mut self, value: Real, weight: Real) -> QlResult<()> {
        if weight < 0.0 || weight.is_nan() {
            fail!("negative weight ({weight}) not allowed");
        }
        self.samples.push((value, weight));
        self.sorted = false;
        Ok(())
    }

    fn reset(&mut self) {
        self.samples = Vec::new();
        self.sorted = true;
    }
}

impl EmpiricalStatistics for GeneralStatistics {
    fn expectation_value<F, P>(&self, f: F, in_range: P) -> Option<(Real, Size)>
    where
        F: Fn(Real) -> Real,
        P: Fn(Real) -> bool,
    {
        let mut num = 0.0;
        let mut den = 0.0;
        let mut n: Size = 0;
        for &(x, w) in &self.samples {
            if in_range(x) {
                num += f(x) * w;
                den += w;
                n += 1;
            }
        }
        if n == 0 { None } else { Some((num / den, n)) }
    }

    fn percentile(&mut self, percent: Real) -> QlResult<Real> {
        let target = self.percentile_target(percent)?;
        Ok(cumulative_pick(self.samples.iter(), target))
    }

    fn top_percentile(&mut self, percent: Real) -> QlResult<Real> {
        let target = self.percentile_target(percent)?;
        Ok(cumulative_pick(self.samples.iter().rev(), target))
    }
}

/// The value at which the running weight total reaches `target`, or the last
/// value when rounding keeps the total short of it.
fn cumulative_pick<'a, I>(samples: I, target: Real) -> Real
where
    I: ExactSizeIterator<Item = &'a (Real, Real)>,
{
    let last = samples.len() - 1;
    let mut integral = 0.0;
    for (i, &(value, weight)) in samples.enumerate() {
        integral += weight;
        if integral >= target || i == last {
            return value;
        }
    }
    unreachable!("a positive weight sum implies at least one sample")
}

#[cfg(test)]
mod tests {
    use super::*;
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

                let mut s = GeneralStatistics::new();
                s.add_sequence_weighted(data.iter().map(|&x| (x, 1.0)))
                    .unwrap();

                assert_eq!(s.samples(), N_SAMPLES);
                check(
                    "weight sum",
                    average,
                    sigma,
                    s.weight_sum(),
                    N_SAMPLES as Real,
                    1e-10,
                );
                check("minimum", average, sigma, s.min().unwrap(), data_min, 1e-12);
                check("maximum", average, sigma, s.max().unwrap(), data_max, 1e-12);

                let tolerance = if average == 0.0 {
                    1e-13
                } else {
                    average.abs() * 1e-13
                };
                check(
                    "mean",
                    average,
                    sigma,
                    s.mean().unwrap(),
                    average,
                    tolerance,
                );

                let expected = sigma * sigma;
                check(
                    "variance",
                    average,
                    sigma,
                    s.variance().unwrap(),
                    expected,
                    expected * 1e-1,
                );
                check(
                    "standard deviation",
                    average,
                    sigma,
                    s.standard_deviation().unwrap(),
                    sigma,
                    sigma * 1e-1,
                );
                check("skewness", average, sigma, s.skewness().unwrap(), 0.0, 1e-4);
                check("kurtosis", average, sigma, s.kurtosis().unwrap(), 0.0, 1e-1);

                let tolerance = if average == 0.0 {
                    1e-3
                } else {
                    (average * 1e-3).abs()
                };
                check(
                    "percentile",
                    average,
                    sigma,
                    s.percentile(0.5).unwrap(),
                    average,
                    tolerance,
                );
            }
        }
    }

    #[test]
    fn expectation_value_filters_by_range() {
        let mut s = GeneralStatistics::new();
        s.add_sequence([1.0, 2.0, 3.0, 4.0]).unwrap();
        let (value, n) = s.expectation_value(|x| x, |x| x >= 3.0).unwrap();
        assert_eq!((value, n), (3.5, 2));
        assert!(s.expectation_value(|x| x, |x| x > 10.0).is_none());
    }

    #[test]
    fn percentiles_walk_the_weighted_distribution() {
        let mut s = GeneralStatistics::new();
        s.add_sequence([3.0, 1.0, 4.0, 2.0]).unwrap();
        assert_eq!(s.percentile(0.5).unwrap(), 2.0);
        assert_eq!(s.percentile(1.0).unwrap(), 4.0);
        assert_eq!(s.top_percentile(0.25).unwrap(), 4.0);
        assert_eq!(s.top_percentile(1.0).unwrap(), 1.0);
    }

    #[test]
    fn rejects_insufficient_samples_and_bad_arguments() {
        let mut s = GeneralStatistics::new();
        assert!(s.mean().is_err());
        assert!(s.min().is_err());
        assert!(s.percentile(0.5).is_err());
        assert!(s.add_weighted(1.0, -0.5).is_err());

        s.add(1.0).unwrap();
        assert!(s.variance().is_err());
        s.add(2.0).unwrap();
        assert!(s.skewness().is_err());
        s.add(3.0).unwrap();
        assert!(s.kurtosis().is_err());
        assert!(s.percentile(0.0).is_err());
        assert!(s.percentile(1.5).is_err());

        s.reset();
        assert_eq!(s.samples(), 0);
        assert_eq!(s.weight_sum(), 0.0);
    }
}
