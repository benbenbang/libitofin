use super::*;
use crate::ensure;

impl SequenceStatistics {
    /// Count-corrected component variances, including zero-weight rows in `N`.
    ///
    /// # Errors
    /// Requires two rows, positive total weight and finite results.
    pub fn variance(&self) -> QlResult<Vec<Real>> {
        self.require_moments()?;
        (0..self.dimension)
            .map(|column| {
                let centered = self.centered_column(column)?;
                finite(
                    compensated_sum(centered.iter().map(|value| value * value)) * self.correction(),
                )
            })
            .collect()
    }

    /// Square roots of the component variances.
    ///
    /// # Errors
    /// Has the same requirements as [`Self::variance`].
    pub fn standard_deviation(&self) -> QlResult<Vec<Real>> {
        Ok(self.variance()?.into_iter().map(Real::sqrt).collect())
    }

    /// Standard deviation divided by `sqrt(N)` for each component.
    ///
    /// # Errors
    /// Has the same requirements as [`Self::variance`].
    pub fn error_estimate(&self) -> QlResult<Vec<Real>> {
        let divisor = (self.samples() as Real).sqrt();
        Ok(self
            .standard_deviation()?
            .into_iter()
            .map(|value| value / divisor)
            .collect())
    }

    /// Count-corrected covariance in flattened row-major matrix order.
    ///
    /// # Errors
    /// Requires two rows, positive total weight, bounded matrix work and finite results.
    pub fn covariance(&self) -> QlResult<Vec<Real>> {
        self.require_moments()?;
        let size = validate_sequence_shape(
            self.samples(),
            self.dimension,
            SequenceStatistic::Covariance,
        )?;
        let columns: Vec<_> = (0..self.dimension)
            .map(|column| self.centered_column(column))
            .collect::<QlResult<_>>()?;
        let mut result = vec![0.0; size];
        for i in 0..self.dimension {
            for j in i..self.dimension {
                let value = finite(
                    compensated_sum(columns[i].iter().zip(&columns[j]).map(|(x, y)| x * y))
                        * self.correction(),
                )?;
                result[i * self.dimension + j] = value;
                result[j * self.dimension + i] = value;
            }
        }
        Ok(result)
    }

    /// Correlation in flattened row-major order with unit diagonals.
    ///
    /// Two zero-variance components correlate as one; exactly one correlates as zero.
    ///
    /// # Errors
    /// Has the same requirements as [`Self::covariance`].
    pub fn correlation(&self) -> QlResult<Vec<Real>> {
        let mut result = self.covariance()?;
        let deviations: Vec<_> = (0..self.dimension)
            .map(|i| result[i * self.dimension + i].sqrt())
            .collect();
        for i in 0..self.dimension {
            for j in i..self.dimension {
                let value = if i == j || (deviations[i] == 0.0 && deviations[j] == 0.0) {
                    1.0
                } else if deviations[i] == 0.0 || deviations[j] == 0.0 {
                    0.0
                } else {
                    finite(
                        result[i * self.dimension + j]
                            / deviations[i].max(deviations[j])
                            / deviations[i].min(deviations[j]),
                    )?
                };
                result[i * self.dimension + j] = value;
                result[j * self.dimension + i] = value;
            }
        }
        Ok(result)
    }

    fn require_moments(&self) -> QlResult<()> {
        self.require_weight()?;
        require!(
            self.samples() >= 2,
            "sequence moments require at least two rows"
        );
        Ok(())
    }

    fn correction(&self) -> Real {
        self.samples() as Real / (self.samples() - 1) as Real
    }

    pub(super) fn center(&self, column: usize) -> QlResult<Center> {
        let mut origin = self.values[self
            .weights
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .ok_or_else(|| {
                crate::errors::QlError::new(
                    "sequence total weight must be positive",
                    file!(),
                    line!(),
                )
            })?
            * self.dimension
            + column];
        let mut scale = 0.0_f64;
        for (row, &weight) in self.values.chunks_exact(self.dimension).zip(&self.weights) {
            if weight > 0.0 {
                scale = scale.max((row[column] - origin).abs());
            }
        }
        if !scale.is_finite() {
            origin = 0.0;
            scale = self
                .values
                .chunks_exact(self.dimension)
                .zip(&self.weights)
                .filter(|(_, weight)| **weight > 0.0)
                .map(|(row, _)| row[column].abs())
                .fold(0.0, Real::max);
        }
        if scale == 0.0 {
            return Ok(Center {
                origin,
                scale,
                normalized_mean: 0.0,
                mean: origin,
            });
        }
        let root_total = self.weight_sum.sqrt();
        let terms = || {
            self.values
                .chunks_exact(self.dimension)
                .zip(&self.weights)
                .filter(|(_, weight)| **weight > 0.0)
                .map(|(row, weight)| {
                    let root_weight = weight.sqrt() / root_total;
                    (((row[column] - origin) / scale) * root_weight, root_weight)
                })
        };
        let normalized_mean = compensated_sum(terms().map(|(value, weight)| value * weight));
        let offset = compensated_sum(terms().map(|(value, weight)| (value * scale) * weight));
        let shifted_mean = origin + offset;
        let mean = if shifted_mean.is_finite() && scale <= 0.5 * origin.abs() {
            shifted_mean
        } else {
            compensated_sum(
                self.values
                    .chunks_exact(self.dimension)
                    .zip(&self.weights)
                    .filter(|(_, weight)| **weight > 0.0)
                    .map(|(row, weight)| {
                        let ratio = weight / self.weight_sum;
                        if ratio > 0.0 {
                            row[column] * ratio
                        } else {
                            let root_weight = weight.sqrt() / root_total;
                            (row[column] * root_weight) * root_weight
                        }
                    }),
            )
        };
        Ok(Center {
            origin,
            scale,
            normalized_mean,
            mean: finite(mean)?,
        })
    }

    fn centered_column(&self, column: usize) -> QlResult<Vec<Real>> {
        let center = self.center(column)?;
        let root_total = self.weight_sum.sqrt();
        self.values
            .chunks_exact(self.dimension)
            .zip(&self.weights)
            .map(|(row, weight)| {
                if *weight == 0.0 || center.scale == 0.0 {
                    return Ok(0.0);
                }
                finite(
                    (((row[column] - center.origin) / center.scale - center.normalized_mean)
                        * (weight.sqrt() / root_total))
                        * center.scale,
                )
            })
            .collect()
    }
}

pub(super) struct Center {
    origin: Real,
    scale: Real,
    normalized_mean: Real,
    pub(super) mean: Real,
}

fn compensated_sum(values: impl Iterator<Item = Real>) -> Real {
    let mut sum = 0.0;
    let mut correction = 0.0;
    for value in values {
        let next = sum + value;
        correction += if sum.abs() >= value.abs() {
            (sum - next) + value
        } else {
            (value - next) + sum
        };
        sum = next;
    }
    sum + correction
}

fn finite(value: Real) -> QlResult<Real> {
    ensure!(value.is_finite(), "sequence statistic result is nonfinite");
    Ok(value)
}
