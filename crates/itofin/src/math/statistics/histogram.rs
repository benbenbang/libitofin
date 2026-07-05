//! Histogram of a data set.
//!
//! Port of `ql/math/statistics/histogram.{hpp,cpp}`. The bin count can be
//! given directly, derived from the data by a [`HistogramAlgorithm`], or
//! implied by explicit break points; QuantLib's default-constructed empty
//! histogram and its `Algorithm::None` sentinel are not carried over, since
//! every constructor here produces a calculated histogram.

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::math::statistics::{IncrementalStatistics, MeanStdDev, Statistics};
use crate::types::{Real, Size};
use crate::{fail, require};

/// Algorithm choosing the number of bins from the data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistogramAlgorithm {
    /// Sturges' rule, `⌈log₂ n + 1⌉`.
    Sturges,
    /// Freedman-Diaconis: bin width `2 IQR n^(-1/3)`.
    FreedmanDiaconis,
    /// Scott's rule: bin width `3.5 σ n^(-1/3)`.
    Scott,
}

/// Histogram of a given data set.
#[derive(Clone, Debug)]
pub struct Histogram {
    bins: Size,
    algorithm: Option<HistogramAlgorithm>,
    breaks: Vec<Real>,
    counts: Vec<Size>,
    frequencies: Vec<Real>,
}

impl Histogram {
    /// A histogram with `breaks` evenly spaced break points (`breaks + 1`
    /// bins) over the range of `data`.
    ///
    /// # Errors
    ///
    /// Returns an error if `data` is empty.
    pub fn with_bin_count(data: &[Real], breaks: Size) -> QlResult<Self> {
        Histogram::build(data, breaks + 1, None, Vec::new())
    }

    /// A histogram whose bin count is chosen from the data by `algorithm`.
    ///
    /// # Errors
    ///
    /// Returns an error if `data` is empty.
    pub fn with_algorithm(data: &[Real], algorithm: HistogramAlgorithm) -> QlResult<Self> {
        let n = data.len() as Real;
        require!(!data.is_empty(), "no data given");
        let (min, max) = min_max(data);
        let bins = match algorithm {
            HistogramAlgorithm::Sturges => (n.log2() + 1.0).ceil() as Size,
            HistogramAlgorithm::FreedmanDiaconis => {
                let r1 = quantile(data, 0.25)?;
                let r2 = quantile(data, 0.75)?;
                let h = 2.0 * (r2 - r1) * n.powf(-1.0 / 3.0);
                ((max - min) / h).ceil() as Size
            }
            HistogramAlgorithm::Scott => {
                let mut summary = IncrementalStatistics::new();
                summary.add_sequence(data.iter().copied())?;
                let h = 3.5 * summary.standard_deviation()? * n.powf(-1.0 / 3.0);
                ((max - min) / h).ceil() as Size
            }
        };
        Histogram::build(data, bins.max(1), Some(algorithm), Vec::new())
    }

    /// A histogram over the given break points, which are sorted and
    /// deduplicated; the bins are `(-∞, b₁), [b₁, b₂), …, [bₙ, ∞)`.
    ///
    /// # Errors
    ///
    /// Returns an error if `data` is empty.
    pub fn with_breaks(data: &[Real], mut breaks: Vec<Real>) -> QlResult<Self> {
        breaks.sort_by(Real::total_cmp);
        breaks.dedup_by(|a, b| close_enough(*a, *b));
        let bins = breaks.len() + 1;
        Histogram::build(data, bins, None, breaks)
    }

    /// Number of bins.
    pub fn bins(&self) -> Size {
        self.bins
    }

    /// The break points separating the bins.
    pub fn breaks(&self) -> &[Real] {
        &self.breaks
    }

    /// The algorithm that chose the bin count, if one was used.
    pub fn algorithm(&self) -> Option<HistogramAlgorithm> {
        self.algorithm
    }

    /// Number of data points in each bin.
    pub fn counts(&self) -> &[Size] {
        &self.counts
    }

    /// Fraction of the data points in each bin.
    pub fn frequencies(&self) -> &[Real] {
        &self.frequencies
    }

    fn build(
        data: &[Real],
        bins: Size,
        algorithm: Option<HistogramAlgorithm>,
        mut breaks: Vec<Real>,
    ) -> QlResult<Self> {
        require!(!data.is_empty(), "no data given");
        if breaks.is_empty() {
            let (min, max) = min_max(data);
            let h = (max - min) / bins as Real;
            breaks = (1..bins).map(|i| min + i as Real * h).collect();
        }

        let mut counts: Vec<Size> = vec![0; bins];
        for &point in data {
            let bin = breaks.iter().position(|&b| point < b).unwrap_or(bins - 1);
            counts[bin] += 1;
        }
        let total = data.len() as Real;
        let frequencies = counts.iter().map(|&c| c as Real / total).collect();

        Ok(Histogram {
            bins,
            algorithm,
            breaks,
            counts,
            frequencies,
        })
    }
}

fn min_max(data: &[Real]) -> (Real, Real) {
    data.iter()
        .fold((Real::INFINITY, Real::NEG_INFINITY), |(min, max), &x| {
            (min.min(x), max.max(x))
        })
}

/// Discontinuous sample quantile, method 8 of Hyndman and Fan (1996): the
/// estimates are approximately median-unbiased regardless of the sample
/// distribution.
fn quantile(samples: &[Real], prob: Real) -> QlResult<Real> {
    let n = samples.len();
    if !(0.0..=1.0).contains(&prob) {
        fail!("probability ({prob}) has to be in [0, 1]");
    }
    require!(n > 0, "the sample size has to be positive");

    if n == 1 {
        return Ok(samples[0]);
    }

    let a = 1.0 / 3.0;
    let b = 2.0 * a / (n as Real + a);
    let (min, max) = min_max(samples);
    if prob < b {
        return Ok(min);
    }
    if prob > 1.0 - b {
        return Ok(max);
    }

    let index = ((n as Real + a) * prob + a).floor() as Size;
    let mut sorted = samples.to_vec();
    sorted.sort_by(Real::total_cmp);

    let weight = n as Real * prob + a - index as Real;
    Ok((1.0 - weight) * sorted[index - 1] + weight * sorted[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_data_into_even_bins() {
        let data = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0];
        let histogram = Histogram::with_bin_count(&data, 2).unwrap();
        assert_eq!(histogram.bins(), 3);
        assert_eq!(histogram.breaks(), &[2.0, 3.0]);
        assert_eq!(histogram.counts(), &[1, 2, 7]);
        assert_eq!(histogram.frequencies(), &[0.1, 0.2, 0.7]);
        assert_eq!(histogram.algorithm(), None);
    }

    #[test]
    fn sorts_and_deduplicates_given_breaks() {
        let data = [0.5, 1.5, 2.5, 3.5];
        let histogram = Histogram::with_breaks(&data, vec![3.0, 1.0, 1.0, 2.0]).unwrap();
        assert_eq!(histogram.breaks(), &[1.0, 2.0, 3.0]);
        assert_eq!(histogram.bins(), 4);
        assert_eq!(histogram.counts(), &[1, 1, 1, 1]);
    }

    /// For 100 samples Sturges gives `⌈log₂ 100 + 1⌉ = 8` bins; for the data
    /// 1..=100 the type-8 quartiles are 25.3̅ and 75.3̅, so Freedman-Diaconis
    /// gives width `2 · 50 / ∛100` and `⌈99 / 21.544⌉ = 5` bins, and Scott
    /// gives width `3.5 · 29.0115 / ∛100` and `⌈99 / 21.876⌉ = 5` bins.
    #[test]
    fn bin_count_algorithms_match_hand_calculations() {
        let data: Vec<Real> = (1..=100).map(|i| i as Real).collect();

        let sturges = Histogram::with_algorithm(&data, HistogramAlgorithm::Sturges).unwrap();
        assert_eq!(sturges.bins(), 8);
        assert_eq!(sturges.algorithm(), Some(HistogramAlgorithm::Sturges));

        let fd = Histogram::with_algorithm(&data, HistogramAlgorithm::FreedmanDiaconis).unwrap();
        assert_eq!(fd.bins(), 5);

        let scott = Histogram::with_algorithm(&data, HistogramAlgorithm::Scott).unwrap();
        assert_eq!(scott.bins(), 5);

        let total: Size = sturges.counts().iter().sum();
        assert_eq!(total, data.len());
        let frequency_sum: Real = sturges.frequencies().iter().sum();
        assert!((frequency_sum - 1.0).abs() < 1e-15);
    }

    #[test]
    fn quantile_interpolates_and_clamps_at_boundaries() {
        let data: Vec<Real> = (1..=100).map(|i| i as Real).collect();
        assert!((quantile(&data, 0.25).unwrap() - (76.0 / 3.0)).abs() < 1e-12);
        assert!((quantile(&data, 0.75).unwrap() - (226.0 / 3.0)).abs() < 1e-12);
        assert_eq!(quantile(&data, 0.001).unwrap(), 1.0);
        assert_eq!(quantile(&data, 0.999).unwrap(), 100.0);
        assert_eq!(quantile(&[42.0], 0.5).unwrap(), 42.0);
        assert!(quantile(&data, -0.1).is_err());
        assert!(quantile(&[], 0.5).is_err());
    }

    #[test]
    fn rejects_empty_data() {
        assert!(Histogram::with_bin_count(&[], 3).is_err());
        assert!(Histogram::with_algorithm(&[], HistogramAlgorithm::Sturges).is_err());
        assert!(Histogram::with_breaks(&[], vec![1.0]).is_err());
    }
}
