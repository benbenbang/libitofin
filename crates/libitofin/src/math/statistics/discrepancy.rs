//! Bounded unit-weight L2 discrepancy on the closed unit cube.

use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

use super::sequence_shape::{SequenceStatistic, validate_sequence_shape};

/// Maximum number of points retained for an L2 discrepancy query.
pub const MAX_DISCREPANCY_ROWS: usize = 4096;
/// Maximum `rows * rows * dimension` work for L2 discrepancy.
pub const MAX_DISCREPANCY_WORK: usize = 100_000_000;

/// Validate discrepancy dimensions and work, returning the input component count.
///
/// # Errors
/// Rejects dimensions outside `2..=256`, empty or oversized point sets,
/// shared sequence component limits, and overflowing or excessive pair work.
pub fn validate_discrepancy_shape(rows: usize, dimension: usize) -> QlResult<usize> {
    require!(dimension >= 2, "discrepancy dimension must be at least two");
    validate_sequence_shape(rows, dimension, SequenceStatistic::Mean)?;
    require!(
        rows <= MAX_DISCREPANCY_ROWS,
        "discrepancy row limit exceeded"
    );
    require!(
        rows.checked_mul(rows)
            .and_then(|pairs| pairs.checked_mul(dimension))
            .is_some_and(|work| work <= MAX_DISCREPANCY_WORK),
        "discrepancy pair work limit exceeded"
    );
    Ok(rows * dimension)
}

/// Evaluate QuantLib-normalized L2 discrepancy for flattened row-major points.
///
/// The result is `sqrt(A/N² - 2^(1-d) C/N + 3^(-d))`, where
/// `A = sum(i,j) product(k) (1 - max(x[i,k], x[j,k]))` and
/// `C = sum(i) product(k) (1 - x[i,k]²)`. This is neither centered discrepancy
/// nor a confidence interval. Omitted weights mean unit weights.
///
/// # Errors
/// Rejects invalid shapes, nonfinite coordinates, coordinates outside `[0,1]`,
/// mismatched weights, every weight other than exactly one, and invalid
/// numerical results.
pub fn evaluate_discrepancy_batch(
    values: &[Real],
    rows: usize,
    dimension: usize,
    weights: Option<&[Real]>,
) -> QlResult<Real> {
    let components = validate_discrepancy_shape(rows, dimension)?;
    require!(
        values.len() == components,
        "discrepancy sample shape mismatch"
    );
    if let Some(weights) = weights {
        require!(weights.len() == rows, "discrepancy weight length mismatch");
        require!(
            weights.iter().all(|&weight| weight == 1.0),
            "discrepancy requires unit weights"
        );
    }
    validate_coordinates(values)?;
    let mut statistics = DiscrepancyStatistics::new(dimension)?;
    for point in values.chunks_exact(dimension) {
        statistics.add(point)?;
    }
    statistics.discrepancy()
}

/// Bounded incremental L2 discrepancy for unit-weight points in `[0,1]^d`.
///
/// Storage is `O(Nd)`. Appending a point costs `O(Nd)` and querying costs
/// `O(1)`, using compensated pair and coordinate-product sums. Unlike native
/// QuantLib, nonunit weights and coordinates outside the unit cube are rejected.
#[derive(Clone, Debug)]
pub struct DiscrepancyStatistics {
    dimension: usize,
    values: Vec<Real>,
    pair_sum: CompensatedSum,
    coordinate_sum: CompensatedSum,
}

impl DiscrepancyStatistics {
    /// Construct an empty accumulator with an explicit dimension.
    ///
    /// # Errors
    /// Rejects dimensions outside `2..=256`.
    pub fn new(dimension: usize) -> QlResult<Self> {
        validate_discrepancy_shape(1, dimension)?;
        Ok(Self {
            dimension,
            values: Vec::new(),
            pair_sum: CompensatedSum::default(),
            coordinate_sum: CompensatedSum::default(),
        })
    }

    /// Number of coordinates per point.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Number of accepted points.
    pub fn samples(&self) -> usize {
        self.values.len() / self.dimension
    }

    /// Append one point atomically with unit weight.
    ///
    /// # Errors
    /// Rejects invalid coordinates, a mismatched dimension, or exceeded bounds.
    pub fn add(&mut self, sample: &[Real]) -> QlResult<()> {
        self.add_weighted(sample, 1.0)
    }

    /// Append one point atomically, requiring weight exactly one.
    ///
    /// # Errors
    /// Rejects every other weight, invalid coordinates, mismatched dimensions,
    /// and exceeded bounds without changing the accumulator.
    pub fn add_weighted(&mut self, sample: &[Real], weight: Real) -> QlResult<()> {
        require!(weight == 1.0, "discrepancy requires unit weights");
        require!(
            sample.len() == self.dimension,
            "discrepancy sample dimension mismatch"
        );
        validate_discrepancy_shape(self.samples() + 1, self.dimension)?;
        validate_coordinates(sample)?;
        let mut pair_sum = self.pair_sum;
        for point in self.values.chunks_exact(self.dimension) {
            let product = point
                .iter()
                .zip(sample)
                .map(|(&left, &right)| 1.0 - left.max(right))
                .product::<Real>();
            pair_sum.add(2.0 * product);
        }
        pair_sum.add(sample.iter().map(|&value| 1.0 - value).product());
        let coordinate_product = sample.iter().map(|&value| 1.0 - value * value).product();
        self.values.extend_from_slice(sample);
        self.pair_sum = pair_sum;
        self.coordinate_sum.add(coordinate_product);
        Ok(())
    }

    /// Clear all points, retaining the dimension when `dimension` is zero.
    ///
    /// # Errors
    /// Rejects invalid nonzero dimensions without changing the accumulator.
    pub fn reset(&mut self, dimension: usize) -> QlResult<()> {
        let dimension = if dimension == 0 {
            self.dimension
        } else {
            dimension
        };
        validate_discrepancy_shape(1, dimension)?;
        self.values.clear();
        self.pair_sum = CompensatedSum::default();
        self.coordinate_sum = CompensatedSum::default();
        self.dimension = dimension;
        Ok(())
    }

    /// QuantLib-normalized L2 discrepancy of the accepted points.
    ///
    /// A negative squared result is rounded to zero only within
    /// `8 * f64::EPSILON * (dimension + 2) * (|A/N²| + |2^(1-d) C/N| + |3^-d|)`.
    /// More negative or nonfinite results are errors. Product underflow and
    /// ordinary floating-point rounding remain possible in high dimensions.
    ///
    /// # Errors
    /// Rejects empty accumulators and invalid numerical results.
    pub fn discrepancy(&self) -> QlResult<Real> {
        require!(
            self.samples() > 0,
            "discrepancy requires at least one point"
        );
        let count = self.samples() as Real;
        let exponent = self.dimension as i32;
        let pair_term = self.pair_sum.total() / count / count;
        let coordinate_term = 2.0_f64.powi(1 - exponent) * self.coordinate_sum.total() / count;
        let constant_term = 3.0_f64.powi(-exponent);
        finish_discrepancy(pair_term, coordinate_term, constant_term, self.dimension)
    }
}

fn validate_coordinates(values: &[Real]) -> QlResult<()> {
    require!(
        values
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)),
        "discrepancy coordinates must be finite and in [0,1]"
    );
    Ok(())
}

fn finish_discrepancy(
    pair: Real,
    coordinate: Real,
    constant: Real,
    dimension: usize,
) -> QlResult<Real> {
    let squared = (pair - coordinate) + constant;
    let tolerance = 8.0
        * Real::EPSILON
        * (dimension + 2) as Real
        * (pair.abs() + coordinate.abs() + constant.abs());
    require!(
        squared.is_finite() && squared >= -tolerance,
        "invalid discrepancy squared result"
    );
    Ok(squared.max(0.0).sqrt())
}

#[derive(Clone, Copy, Debug, Default)]
struct CompensatedSum {
    sum: Real,
    correction: Real,
}

impl CompensatedSum {
    fn add(&mut self, value: Real) {
        let next = self.sum + value;
        self.correction += if self.sum.abs() >= value.abs() {
            (self.sum - next) + value
        } else {
            (value - next) + self.sum
        };
        self.sum = next;
    }

    fn total(self) -> Real {
        self.sum + self.correction
    }
}

#[cfg(test)]
#[path = "discrepancy_tests.rs"]
mod tests;
