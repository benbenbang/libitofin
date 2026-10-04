//! Bounded weighted vector statistics with centered covariance arithmetic.

use crate::errors::QlResult;
use crate::require;

use super::sequence_shape::*;
use crate::types::Real;

#[path = "sequence_moments.rs"]
mod moments;

/// Evaluate weighted vector rows without retaining an accumulator.
///
/// `values` is a flattened row-major `rows` by `dimension` sample array.
/// Omitted weights mean unit weights. Zero-weight rows count towards the
/// `N/(N-1)` correction and minima/maxima. Covariance and correlation return
/// flattened row-major `dimension` by `dimension` matrices.
///
/// # Errors
///
/// Rejects invalid shapes, nonfinite samples, invalid weights, nonpositive
/// total weight, insufficient rows for moments, or nonfinite results.
pub fn evaluate_sequence_batch(
    values: &[Real],
    rows: usize,
    dimension: usize,
    weights: Option<&[Real]>,
    measure: SequenceStatistic,
) -> QlResult<Vec<Real>> {
    validate_sequence_shape(rows, dimension, measure)?;
    require!(
        values.len() == rows * dimension,
        "sequence sample shape mismatch"
    );
    if let Some(weights) = weights {
        require!(weights.len() == rows, "sequence weight length mismatch");
    }
    let mut statistics = SequenceStatistics::new(dimension)?;
    for (index, sample) in values.chunks_exact(dimension).enumerate() {
        statistics.add_weighted(sample, weights.map_or(1.0, |weights| weights[index]))?;
    }
    match measure {
        SequenceStatistic::Mean => statistics.mean(),
        SequenceStatistic::Variance => statistics.variance(),
        SequenceStatistic::StandardDeviation => statistics.standard_deviation(),
        SequenceStatistic::ErrorEstimate => statistics.error_estimate(),
        SequenceStatistic::Minimum => statistics.min(),
        SequenceStatistic::Maximum => statistics.max(),
        SequenceStatistic::Covariance => statistics.covariance(),
        SequenceStatistic::Correlation => statistics.correlation(),
    }
}

/// Bounded accumulator for weighted vector samples.
///
/// Unlike native raw second moments, centered arithmetic preserves variance
/// near large offsets. Weights are normalized before moment multiplication.
/// A row's zero weight does not bypass sample validation or sample counting.
#[derive(Clone, Debug)]
pub struct SequenceStatistics {
    dimension: usize,
    values: Vec<Real>,
    weights: Vec<Real>,
    weight_sum: Real,
}

impl SequenceStatistics {
    /// Construct an empty accumulator with an explicit positive dimension.
    ///
    /// # Errors
    /// Rejects dimensions outside `1..=256`.
    pub fn new(dimension: usize) -> QlResult<Self> {
        validate_sequence_shape(1, dimension, SequenceStatistic::Mean)?;
        Ok(Self {
            dimension,
            values: Vec::new(),
            weights: Vec::new(),
            weight_sum: 0.0,
        })
    }

    /// Number of components per row.
    pub fn dimension(&self) -> usize {
        self.dimension
    }
    /// Number of rows, including zero-weight rows.
    pub fn samples(&self) -> usize {
        self.weights.len()
    }
    /// Sum of accepted row weights.
    pub fn weight_sum(&self) -> Real {
        self.weight_sum
    }

    /// Add one unit-weight sample atomically.
    ///
    /// # Errors
    /// Rejects an invalid row or an exceeded storage limit without changing samples.
    pub fn add(&mut self, sample: &[Real]) -> QlResult<()> {
        self.add_weighted(sample, 1.0)
    }

    /// Add one weighted sample atomically.
    ///
    /// # Errors
    /// Rejects mismatched or nonfinite samples, negative/nonfinite weights,
    /// overflowing weight sums, and exceeded storage limits.
    pub fn add_weighted(&mut self, sample: &[Real], weight: Real) -> QlResult<()> {
        require!(
            sample.len() == self.dimension,
            "sequence sample dimension mismatch"
        );
        require!(
            sample.iter().all(|value| value.is_finite()),
            "nonfinite sequence sample"
        );
        require!(
            weight.is_finite() && weight >= 0.0,
            "invalid sequence weight"
        );
        let total = self.weight_sum + weight;
        require!(total.is_finite(), "sequence weight sum overflow");
        validate_sequence_shape(self.samples() + 1, self.dimension, SequenceStatistic::Mean)?;
        self.values.extend_from_slice(sample);
        self.weights.push(weight);
        self.weight_sum = total;
        Ok(())
    }

    /// Clear the samples and set a new explicit dimension atomically.
    ///
    /// # Errors
    /// Rejects invalid dimensions without changing the current accumulator.
    pub fn reset(&mut self, dimension: usize) -> QlResult<()> {
        validate_sequence_shape(1, dimension, SequenceStatistic::Mean)?;
        self.values.clear();
        self.weights.clear();
        self.weight_sum = 0.0;
        self.dimension = dimension;
        Ok(())
    }

    /// Weighted mean vector.
    ///
    /// # Errors
    /// Requires positive total weight and finite results.
    pub fn mean(&self) -> QlResult<Vec<Real>> {
        self.require_weight()?;
        (0..self.dimension)
            .map(|column| self.center(column).map(|center| center.mean))
            .collect()
    }

    /// Component minima, including zero-weight rows.
    ///
    /// # Errors
    /// Requires positive total weight.
    pub fn min(&self) -> QlResult<Vec<Real>> {
        self.extrema(Real::min)
    }

    /// Component maxima, including zero-weight rows.
    ///
    /// # Errors
    /// Requires positive total weight.
    pub fn max(&self) -> QlResult<Vec<Real>> {
        self.extrema(Real::max)
    }

    fn require_weight(&self) -> QlResult<()> {
        require!(
            self.weight_sum.is_finite() && self.weight_sum > 0.0,
            "sequence total weight must be positive"
        );
        Ok(())
    }

    fn extrema(&self, operation: fn(Real, Real) -> Real) -> QlResult<Vec<Real>> {
        self.require_weight()?;
        let mut result = self.values[..self.dimension].to_vec();
        for row in self.values.chunks_exact(self.dimension).skip(1) {
            for (value, &sample) in result.iter_mut().zip(row) {
                *value = operation(*value, sample);
            }
        }
        Ok(result)
    }
}
