//! Statistics accumulators ported from `ql/math/statistics/`.
//!
//! QuantLib layers its statistics tools through class templates
//! (`GenericGaussianStatistics<S>`, `GenericRiskStatistics<S>`); we express
//! the same genericity over the sample accumulator with traits:
//! [`MeanStdDev`] is the minimal interface (what a precomputed-distribution
//! holder provides), [`Statistics`] the full accumulator interface, and
//! [`EmpiricalStatistics`] the extra interface of accumulators that keep the
//! whole sample set, such as [`GeneralStatistics`].

use crate::errors::QlResult;
use crate::types::{Real, Size};

mod generalstatistics;

pub use generalstatistics::GeneralStatistics;

/// Mean and standard deviation of a distribution, the minimal interface
/// required by the gaussian-assumption risk measures.
pub trait MeanStdDev {
    /// The mean, `Σ wᵢ xᵢ / Σ wᵢ`.
    ///
    /// # Errors
    ///
    /// Returns an error on an empty sample set.
    fn mean(&self) -> QlResult<Real>;

    /// The standard deviation, the square root of [`Statistics::variance`].
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples.
    fn standard_deviation(&self) -> QlResult<Real>;
}

/// Interface of the sample accumulators (QuantLib's *statistics tool*
/// concept): weighted samples go in, moment estimates come out.
pub trait Statistics: MeanStdDev {
    /// Number of samples collected.
    fn samples(&self) -> Size;

    /// Sum of the sample weights.
    fn weight_sum(&self) -> Real;

    /// The unbiased sample variance, `N/(N-1) ⟨(x - ⟨x⟩)²⟩`.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples.
    fn variance(&self) -> QlResult<Real>;

    /// The error estimate on the mean, `σ/√N`.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than two samples.
    fn error_estimate(&self) -> QlResult<Real>;

    /// The bias-corrected sample skewness; 0 for a gaussian distribution.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than three samples.
    fn skewness(&self) -> QlResult<Real>;

    /// The bias-corrected excess kurtosis; 0 for a gaussian distribution.
    ///
    /// # Errors
    ///
    /// Returns an error on fewer than four samples.
    fn kurtosis(&self) -> QlResult<Real>;

    /// The minimum sample value.
    ///
    /// # Errors
    ///
    /// Returns an error on an empty sample set.
    fn min(&self) -> QlResult<Real>;

    /// The maximum sample value.
    ///
    /// # Errors
    ///
    /// Returns an error on an empty sample set.
    fn max(&self) -> QlResult<Real>;

    /// Adds a datum with the given weight.
    ///
    /// # Errors
    ///
    /// Returns an error if `weight` is negative.
    fn add_weighted(&mut self, value: Real, weight: Real) -> QlResult<()>;

    /// Resets the accumulator to an empty sample set.
    fn reset(&mut self);

    /// Adds a datum with unit weight.
    ///
    /// # Errors
    ///
    /// Never fails; kept fallible for uniformity with [`Self::add_weighted`].
    fn add(&mut self, value: Real) -> QlResult<()> {
        self.add_weighted(value, 1.0)
    }

    /// Adds a sequence of data, each with unit weight.
    ///
    /// # Errors
    ///
    /// Never fails; kept fallible for uniformity with [`Self::add_weighted`].
    fn add_sequence<I>(&mut self, values: I) -> QlResult<()>
    where
        I: IntoIterator<Item = Real>,
    {
        for value in values {
            self.add(value)?;
        }
        Ok(())
    }

    /// Adds a sequence of `(value, weight)` pairs.
    ///
    /// # Errors
    ///
    /// Returns an error if any weight is negative.
    fn add_sequence_weighted<I>(&mut self, values: I) -> QlResult<()>
    where
        I: IntoIterator<Item = (Real, Real)>,
    {
        for (value, weight) in values {
            self.add_weighted(value, weight)?;
        }
        Ok(())
    }
}

/// Extra interface of accumulators that store the full sample set and can
/// thus answer questions about the empirical distribution.
pub trait EmpiricalStatistics: Statistics {
    /// Expectation value of `f` over the samples for which `in_range` holds,
    /// together with the number of such samples; `None` if no sample is in
    /// range.
    fn expectation_value<F, P>(&self, f: F, in_range: P) -> Option<(Real, Size)>
    where
        F: Fn(Real) -> Real,
        P: Fn(Real) -> bool;

    /// The `percent`-th percentile: the value `x̄` such that `percent` of the
    /// total sample weight lies at or below it.
    ///
    /// Takes `&mut self` because the samples are sorted lazily (QuantLib
    /// sorts through a `mutable` member instead).
    ///
    /// # Errors
    ///
    /// Returns an error unless `percent` lies in `(0, 1]` and the sample
    /// weights have a positive sum.
    fn percentile(&mut self, percent: Real) -> QlResult<Real>;

    /// The `percent`-th top percentile: the value `x̄` such that `percent` of
    /// the total sample weight lies at or above it.
    ///
    /// # Errors
    ///
    /// Returns an error unless `percent` lies in `(0, 1]` and the sample
    /// weights have a positive sum.
    fn top_percentile(&mut self, percent: Real) -> QlResult<Real>;
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Oracle data mirroring `test-suite/riskstats.cpp`, which draws
    //! `2^16 - 1` inverse-cumulative-normal variates from a dimension-1
    //! `SobolRsg`. That sequence is the gray-code van der Corput sequence in
    //! base 2, reproduced here without porting the generator.

    use crate::math::distributions::normal::InverseCumulativeNormal;
    use crate::types::Real;

    pub const AVERAGES: [Real; 5] = [-100.0, -1.0, 0.0, 1.0, 100.0];
    pub const SIGMAS: [Real; 3] = [0.1, 1.0, 100.0];
    pub const N_SAMPLES: usize = (1 << 16) - 1;

    pub fn sobol_normal_samples(average: Real, sigma: Real) -> Vec<Real> {
        let inverse =
            InverseCumulativeNormal::new(average, sigma).expect("valid normal parameters");
        (1..=N_SAMPLES as u32)
            .map(|i| {
                let gray = i ^ (i >> 1);
                let u = Real::from(gray.reverse_bits()) / (1u64 << 32) as Real;
                inverse.value(u).expect("u lies in (0, 1)")
            })
            .collect()
    }

    pub fn check(
        label: &str,
        average: Real,
        sigma: Real,
        calculated: Real,
        expected: Real,
        tolerance: Real,
    ) {
        assert!(
            (calculated - expected).abs() <= tolerance,
            "wrong {label} for N({average}, {sigma}): calculated {calculated}, expected {expected}, tolerance {tolerance}"
        );
    }
}
