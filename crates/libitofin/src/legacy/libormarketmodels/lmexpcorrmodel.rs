//! Immutable exponential correlation: `rho(i, j) = exp(-rho * |i - j|)`.
//!
//! Matches `ql/legacy/libormarketmodels/lmexpcorrmodel.{hpp,cpp}` for finite
//! positive parameters. Queries additionally reject non-finite times and invalid
//! indices. The cached spectral factor has no guaranteed orientation.

use crate::errors::QlResult;
use crate::math::matrix::Matrix;
use crate::math::matrixutilities::{SalvagingAlgorithm, pseudo_sqrt};
use crate::require;
use crate::types::{Real, Size, Time};

/// Time-independent, full-factor exponential forward-row correlation.
#[derive(Clone, Debug)]
pub struct LmExponentialCorrelationModel {
    corr_matrix: Matrix,
    pseudo_sqrt: Matrix,
}

impl LmExponentialCorrelationModel {
    /// Builds a nonempty correlation matrix and its spectral pseudo square root.
    ///
    /// # Errors
    ///
    /// Rejects zero size, unrepresentable matrix capacity, non-finite or
    /// non-positive `rho`, and non-finite factorization output.
    ///
    /// # Panics
    ///
    /// The existing spectral backend panics if its eigensolver fails to converge.
    pub fn new(size: Size, rho: Real) -> QlResult<Self> {
        require!(size > 0, "correlation size must be positive");
        require!(
            rho.is_finite() && rho > 0.0,
            "rho must be finite and positive"
        );
        require!(
            size.checked_mul(size)
                .and_then(|n| n.checked_mul(size_of::<Real>()))
                .is_some_and(|bytes| bytes <= isize::MAX as usize),
            "correlation matrix capacity is unrepresentable"
        );
        let mut corr_matrix = Matrix::with_size(size, size);
        for i in 0..size {
            for j in i..size {
                let value = (-rho * (j - i) as Real).exp();
                corr_matrix[(i, j)] = value;
                corr_matrix[(j, i)] = value;
            }
        }
        let pseudo_sqrt = pseudo_sqrt(&corr_matrix, SalvagingAlgorithm::Spectral);
        require!(
            (0..size).all(|i| pseudo_sqrt.row(i).iter().all(|v| v.is_finite())),
            "correlation factorization is not finite"
        );
        Ok(Self {
            corr_matrix,
            pseudo_sqrt,
        })
    }

    /// Number of forward rows.
    pub fn size(&self) -> Size {
        self.corr_matrix.rows()
    }

    /// Number of factors; no factor reduction is applied.
    pub fn factors(&self) -> Size {
        self.size()
    }

    /// Borrows the time-independent correlation matrix.
    ///
    /// # Errors
    ///
    /// Rejects non-finite `t`, even though the matrix is time-independent.
    pub fn correlation(&self, t: Time) -> QlResult<&Matrix> {
        require!(t.is_finite(), "correlation time must be finite");
        Ok(&self.corr_matrix)
    }

    /// Returns the correlation between two forward rows.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range indices or non-finite `t`.
    pub fn correlation_ij(&self, i: Size, j: Size, t: Time) -> QlResult<Real> {
        require!(
            i < self.size() && j < self.size(),
            "correlation indices out of range"
        );
        Ok(self.correlation(t)?[(i, j)])
    }

    /// Borrows a factor whose product with its transpose reproduces correlation.
    ///
    /// # Errors
    ///
    /// Rejects non-finite `t`.
    pub fn pseudo_sqrt(&self, t: Time) -> QlResult<&Matrix> {
        require!(t.is_finite(), "correlation time must be finite");
        Ok(&self.pseudo_sqrt)
    }

    /// Correlations do not depend on calendar time.
    pub fn is_time_independent(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_correlation_and_spectral_product() {
        let model = LmExponentialCorrelationModel::new(5, 0.3).unwrap();
        assert_eq!(model.size(), 5);
        assert_eq!(model.factors(), 5);
        assert!(model.is_time_independent());
        let corr = model.correlation(0.0).unwrap();
        let factor = model.pseudo_sqrt(-2.0).unwrap();
        let reconstructed = factor * &factor.transpose();
        for i in 0_usize..5 {
            for j in 0..5 {
                let expected = (-0.3 * i.abs_diff(j) as Real).exp();
                assert!((corr[(i, j)] - expected).abs() < 1e-15);
                assert!((reconstructed[(i, j)] - expected).abs() < 2e-14);
                assert_eq!(model.correlation_ij(i, j, 9.0).unwrap(), corr[(i, j)]);
            }
        }
    }

    #[test]
    fn extreme_positive_rho_and_single_row_are_finite() {
        for rho in [Real::MIN_POSITIVE, 0.001, Real::MAX] {
            for size in [1, 4] {
                let model = LmExponentialCorrelationModel::new(size, rho).unwrap();
                let corr = model.correlation(0.0).unwrap();
                let factor = model.pseudo_sqrt(0.0).unwrap();
                let product = factor * &factor.transpose();
                for i in 0..size {
                    assert_eq!(corr[(i, i)], 1.0);
                    for j in 0..size {
                        assert!((product[(i, j)] - corr[(i, j)]).abs() < 2e-14);
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_invalid_parameters_and_capacity_without_panicking() {
        for rho in [0.0, -1.0, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(LmExponentialCorrelationModel::new(2, rho).is_err());
        }
        assert!(LmExponentialCorrelationModel::new(0, 0.2).is_err());
        assert!(LmExponentialCorrelationModel::new(usize::MAX, 0.2).is_err());
    }

    #[test]
    fn rejects_invalid_query_time_and_indices() {
        let model = LmExponentialCorrelationModel::new(2, 0.2).unwrap();
        for t in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(model.correlation(t).is_err());
            assert!(model.pseudo_sqrt(t).is_err());
            assert!(model.correlation_ij(0, 0, t).is_err());
        }
        assert!(model.correlation_ij(2, 0, 0.0).is_err());
        assert!(model.correlation_ij(0, usize::MAX, 0.0).is_err());
    }
}
