//! Histogram of a data set.
//!
//! Port of `ql/math/statistics/histogram.{hpp,cpp}`. The bin count can be
//! given directly, derived from the data by a [`HistogramAlgorithm`], or
//! implied by explicit break points; QuantLib's default-constructed empty
//! histogram and its `Algorithm::None` sentinel are not carried over, since
//! every constructor here produces a calculated histogram. Two further
//! deliberate deviations: a bin-width rule that degenerates to zero width
//! over a nonzero data range is reported as an error (QuantLib overflows the
//! bin count there), and the sample quantile clamps to the extreme samples at
//! the exact boundary probabilities (QuantLib reads one past the sorted
//! data).

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
    /// Returns an error if `data` is empty or `breaks` is `Size::MAX`.
    pub fn with_break_count(data: &[Real], breaks: Size) -> QlResult<Self> {
        require!(breaks < Size::MAX, "break count ({breaks}) is too large");
        Histogram::build(data, breaks + 1, None, Vec::new(), None)
    }

    /// A histogram whose bin count is chosen from the data by `algorithm`.
    ///
    /// All-equal data produces a single bin.
    ///
    /// # Errors
    ///
    /// Returns an error if `data` is empty or if the algorithm's bin width
    /// degenerates to zero over a nonzero data range.
    pub fn with_algorithm(data: &[Real], algorithm: HistogramAlgorithm) -> QlResult<Self> {
        let n = data.len() as Real;
        require!(!data.is_empty(), "no data given");
        let (min, max) = min_max(data);
        let bins = match algorithm {
            HistogramAlgorithm::Sturges => (n.log2() + 1.0).ceil() as Size,
            HistogramAlgorithm::FreedmanDiaconis => {
                let mut sorted = data.to_vec();
                sorted.sort_by(Real::total_cmp);
                let r1 = quantile(&sorted, 0.25)?;
                let r2 = quantile(&sorted, 0.75)?;
                let h = 2.0 * (r2 - r1) * n.powf(-1.0 / 3.0);
                width_bin_count(max - min, h)?
            }
            HistogramAlgorithm::Scott => {
                let mut summary = IncrementalStatistics::new();
                summary.add_sequence(data.iter().copied())?;
                let h = 3.5 * summary.standard_deviation()? * n.powf(-1.0 / 3.0);
                width_bin_count(max - min, h)?
            }
        };
        Histogram::build(
            data,
            bins.max(1),
            Some(algorithm),
            Vec::new(),
            Some((min, max)),
        )
    }

    /// A histogram over the given break points, which are sorted and
    /// deduplicated; the bins are `(-∞, b₁), [b₁, b₂), …, [bₙ, ∞)`.
    ///
    /// As in QuantLib, the bin count is fixed from the raw break-point list,
    /// so duplicate break points leave zero-count bins just before the last
    /// bin.
    ///
    /// # Errors
    ///
    /// Returns an error if `data` is empty.
    pub fn with_breaks(data: &[Real], mut breaks: Vec<Real>) -> QlResult<Self> {
        let bins = breaks.len() + 1;
        breaks.sort_by(Real::total_cmp);
        breaks.dedup_by(|a, b| close_enough(*a, *b));
        Histogram::build(data, bins, None, breaks, None)
    }

    /// Number of bins.
    pub fn bins(&self) -> Size {
        self.counts.len()
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
        extrema: Option<(Real, Real)>,
    ) -> QlResult<Self> {
        require!(!data.is_empty(), "no data given");
        if breaks.is_empty() {
            let (min, max) = extrema.unwrap_or_else(|| min_max(data));
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

/// Number of bins of width `width` needed to cover `range`, or a single bin
/// when the range itself is degenerate. Zero and subnormal widths over a
/// nonzero range are rejected rather than saturating the bin count.
fn width_bin_count(range: Real, width: Real) -> QlResult<Size> {
    if range < Real::MIN_POSITIVE {
        return Ok(1);
    }
    let width_is_positive = width >= Real::MIN_POSITIVE;
    require!(
        width_is_positive,
        "bin width ({width}) is not positive over the data range ({range})"
    );
    Ok((range / width).ceil() as Size)
}

/// Discontinuous sample quantile, method 8 of Hyndman and Fan (1996): the
/// estimates are approximately median-unbiased regardless of the sample
/// distribution. `sorted` must be in ascending order; probabilities at or
/// beyond the boundary cases clamp to the extreme samples.
fn quantile(sorted: &[Real], prob: Real) -> QlResult<Real> {
    let n = sorted.len();
    if !(0.0..=1.0).contains(&prob) {
        fail!("probability ({prob}) has to be in [0, 1]");
    }
    require!(n > 0, "the sample size has to be positive");

    if n == 1 {
        return Ok(sorted[0]);
    }

    let a = 1.0 / 3.0;
    let b = 2.0 * a / (n as Real + a);
    if prob <= b {
        return Ok(sorted[0]);
    }
    if prob >= 1.0 - b {
        return Ok(sorted[n - 1]);
    }

    let index = ((n as Real + a) * prob + a).floor() as Size;
    let weight = n as Real * prob + a - index as Real;
    Ok((1.0 - weight) * sorted[index - 1] + weight * sorted[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_data_into_even_bins() {
        let data = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0];
        let histogram = Histogram::with_break_count(&data, 2).unwrap();
        assert_eq!(histogram.bins(), 3);
        assert_eq!(histogram.breaks(), &[2.0, 3.0]);
        assert_eq!(histogram.counts(), &[1, 2, 7]);
        assert_eq!(histogram.frequencies(), &[0.1, 0.2, 0.7]);
        assert_eq!(histogram.algorithm(), None);
    }

    /// The bin count is fixed before deduplication as in QuantLib: four raw
    /// break points give five bins, the duplicate leaves the fourth bin
    /// empty, and points beyond the last break fall into the final bin.
    #[test]
    fn sorts_and_deduplicates_given_breaks() {
        let data = [0.5, 1.5, 2.5, 3.5];
        let histogram = Histogram::with_breaks(&data, vec![3.0, 1.0, 1.0, 2.0]).unwrap();
        assert_eq!(histogram.breaks(), &[1.0, 2.0, 3.0]);
        assert_eq!(histogram.bins(), 5);
        assert_eq!(histogram.counts(), &[1, 1, 1, 0, 1]);
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
    fn quantile_clamps_at_exact_boundary_probabilities() {
        let sorted = [1.0, 2.0];
        let a = 1.0 / 3.0;
        let b = 2.0 * a / (2.0 + a);
        assert_eq!(quantile(&sorted, b).unwrap(), 1.0);
        assert_eq!(quantile(&sorted, 1.0 - b).unwrap(), 2.0);
    }

    #[test]
    fn zero_iqr_over_nonzero_range_is_an_error() {
        let data = [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0];
        assert!(Histogram::with_algorithm(&data, HistogramAlgorithm::FreedmanDiaconis).is_err());
    }

    #[test]
    fn all_equal_data_yields_a_single_bin() {
        let data = [5.0; 8];
        let fd = Histogram::with_algorithm(&data, HistogramAlgorithm::FreedmanDiaconis).unwrap();
        assert_eq!(fd.bins(), 1);
        assert_eq!(fd.counts(), &[8]);
        let scott = Histogram::with_algorithm(&data, HistogramAlgorithm::Scott).unwrap();
        assert_eq!(scott.bins(), 1);
    }

    #[test]
    fn single_sample_builds_a_single_bin() {
        let sturges = Histogram::with_algorithm(&[7.0], HistogramAlgorithm::Sturges).unwrap();
        assert_eq!(sturges.bins(), 1);
        let fd = Histogram::with_algorithm(&[7.0], HistogramAlgorithm::FreedmanDiaconis).unwrap();
        assert_eq!(fd.bins(), 1);
        assert_eq!(fd.counts(), &[1]);
    }

    #[test]
    fn nan_samples_count_into_the_last_bin() {
        let data = [0.5, 1.5, Real::NAN];
        let histogram = Histogram::with_breaks(&data, vec![1.0, 2.0]).unwrap();
        assert_eq!(histogram.counts(), &[1, 1, 1]);
    }

    #[test]
    fn rejects_empty_data_and_maximal_break_count() {
        assert!(Histogram::with_break_count(&[], 3).is_err());
        assert!(Histogram::with_algorithm(&[], HistogramAlgorithm::Sturges).is_err());
        assert!(Histogram::with_breaks(&[], vec![1.0]).is_err());
        assert!(Histogram::with_break_count(&[1.0, 2.0], Size::MAX).is_err());
    }
}
