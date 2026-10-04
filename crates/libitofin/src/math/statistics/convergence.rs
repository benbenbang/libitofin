//! Bounded cumulative weighted means at doubling-plus-one checkpoints.

use super::SequenceStatistics;
use crate::errors::{QlError, QlResult};
use crate::require;

/// Maximum number of accepted convergence samples.
pub const MAX_CONVERGENCE_SAMPLES: usize = 100_000;

/// A cumulative weighted mean recorded at a completed checkpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvergencePoint {
    /// Number of samples, including zero-weight samples.
    pub samples: usize,
    /// Weighted mean of the prefix ending at this checkpoint.
    pub mean: f64,
}

/// Default checkpoints `1, 3, 7, 15, ...`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DoublingConvergenceSteps;

impl DoublingConvergenceSteps {
    /// Number of samples at the first checkpoint.
    pub const fn initial_samples() -> usize {
        1
    }

    /// Advance a checkpoint with checked `2 * current + 1` arithmetic.
    ///
    /// # Errors
    /// Rejects a checkpoint whose successor cannot fit in `usize`.
    pub fn next_samples(current: usize) -> QlResult<usize> {
        current
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| QlError::new("convergence checkpoint overflow", file!(), line!()))
    }
}

/// Validate bounded input length and return the number of completed checkpoints.
///
/// Empty input has no checkpoints. An incomplete final prefix is not recorded.
///
/// # Errors
/// Rejects lengths above [`MAX_CONVERGENCE_SAMPLES`] before sample allocation.
pub fn validate_convergence_length(len: usize) -> QlResult<usize> {
    require!(
        len <= MAX_CONVERGENCE_SAMPLES,
        "convergence sample limit exceeded"
    );
    let mut next = DoublingConvergenceSteps::initial_samples();
    let mut count = 0;
    while next <= len {
        count += 1;
        next = DoublingConvergenceSteps::next_samples(next)?;
    }
    Ok(count)
}

/// Evaluate cumulative weighted means using only the default checkpoint rule.
///
/// Weights apply once per sample. Omitted weights are unit weights. Zero-weight
/// samples count, but every recorded checkpoint must have positive total weight.
///
/// # Errors
/// Rejects oversized input, mismatched weights, nonfinite samples or weights,
/// negative weights, overflowing weight sums, or invalid checkpoint means.
pub fn evaluate_convergence_batch(
    values: &[f64],
    weights: Option<&[f64]>,
) -> QlResult<Vec<ConvergencePoint>> {
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(values, weights)?;
    Ok(statistics.table)
}

/// Stored scalar samples and cumulative weighted means at completed checkpoints.
///
/// Uses the stable one-dimensional [`SequenceStatistics`] mean. Checkpoint
/// recomputation has geometrically bounded prefix work, not quadratic append
/// work. A failed single addition or batch leaves all accepted state unchanged.
#[derive(Clone, Debug, Default)]
pub struct ConvergenceStatistics {
    values: Vec<f64>,
    weights: Vec<f64>,
    weight_sum: f64,
    table: Vec<ConvergencePoint>,
}

impl ConvergenceStatistics {
    /// Construct an empty accumulator with the default checkpoint rule.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of accepted samples, including zero-weight samples.
    pub fn samples(&self) -> usize {
        self.values.len()
    }

    /// Sum of accepted sample weights.
    pub fn weight_sum(&self) -> f64 {
        self.weight_sum
    }

    /// Borrow the recorded cumulative means without adding an incomplete prefix.
    pub fn convergence_table(&self) -> &[ConvergencePoint] {
        &self.table
    }

    /// Add a unit-weight sample atomically.
    ///
    /// # Errors
    /// Has the same rejection conditions as [`Self::add_batch`].
    pub fn add(&mut self, value: f64) -> QlResult<()> {
        self.add_weighted(value, 1.0)
    }

    /// Add one weighted sample atomically.
    ///
    /// # Errors
    /// Has the same rejection conditions as [`Self::add_batch`].
    pub fn add_weighted(&mut self, value: f64, weight: f64) -> QlResult<()> {
        self.add_batch(&[value], Some(&[weight]))
    }

    /// Add a complete batch atomically, preserving input order.
    ///
    /// # Errors
    /// Rejects oversized input, mismatched weights, nonfinite samples or weights,
    /// negative weights, overflowing totals, and invalid checkpoint means.
    /// A zero-total checkpoint rejects the whole batch, even if later weights
    /// would make its final total positive. Empty input is a no-op.
    pub fn add_batch(&mut self, values: &[f64], weights: Option<&[f64]>) -> QlResult<()> {
        let checkpoint_count = validate_addition(self.samples(), values.len())?;
        if let Some(weights) = weights {
            require!(
                weights.len() == values.len(),
                "convergence weight length mismatch"
            );
        }
        let mut total = self.weight_sum;
        for (index, &value) in values.iter().enumerate() {
            let weight = weights.map_or(1.0, |weights| weights[index]);
            require!(value.is_finite(), "nonfinite convergence sample");
            require!(
                weight.is_finite() && weight >= 0.0,
                "invalid convergence weight"
            );
            total += weight;
            require!(total.is_finite(), "convergence weight sum overflow");
        }
        let new_points = if checkpoint_count > self.table.len() {
            self.pending_points(values, weights, checkpoint_count - self.table.len())?
        } else {
            Vec::new()
        };
        self.values.extend_from_slice(values);
        self.weights
            .extend((0..values.len()).map(|index| weights.map_or(1.0, |weights| weights[index])));
        self.weight_sum = total;
        self.table.extend(new_points);
        Ok(())
    }

    /// Weighted mean of all accepted samples, including an incomplete prefix.
    ///
    /// # Errors
    /// Rejects an empty accumulator or a nonfinite computed mean.
    pub fn mean(&self) -> QlResult<f64> {
        let mut statistics = SequenceStatistics::new(1)?;
        for (&value, &weight) in self.values.iter().zip(&self.weights) {
            statistics.add_weighted(&[value], weight)?;
        }
        Ok(statistics.mean()?[0])
    }

    /// Clear samples, weights and checkpoints, retaining allocated storage.
    pub fn reset(&mut self) {
        self.values.clear();
        self.weights.clear();
        self.table.clear();
        self.weight_sum = 0.0;
    }

    fn pending_points(
        &self,
        values: &[f64],
        weights: Option<&[f64]>,
        count: usize,
    ) -> QlResult<Vec<ConvergencePoint>> {
        let mut next = self.table.last().map_or(Ok(1), |point| {
            DoublingConvergenceSteps::next_samples(point.samples)
        })?;
        let mut statistics = SequenceStatistics::new(1)?;
        let mut points = Vec::with_capacity(count);
        let existing = self
            .values
            .iter()
            .copied()
            .zip(self.weights.iter().copied());
        let incoming = values
            .iter()
            .copied()
            .enumerate()
            .map(|(index, value)| (value, weights.map_or(1.0, |weights| weights[index])));
        for (value, weight) in existing.chain(incoming) {
            statistics.add_weighted(&[value], weight)?;
            if statistics.samples() == next {
                points.push(ConvergencePoint {
                    samples: next,
                    mean: statistics.mean()?[0],
                });
                if points.len() == count {
                    break;
                }
                next = DoublingConvergenceSteps::next_samples(next)?;
            }
        }
        Ok(points)
    }
}

fn validate_addition(current: usize, additional: usize) -> QlResult<usize> {
    let final_count = current
        .checked_add(additional)
        .ok_or_else(|| QlError::new("convergence sample count overflow", file!(), line!()))?;
    validate_convergence_length(final_count)
}

#[cfg(test)]
#[path = "convergence_tests.rs"]
mod tests;
