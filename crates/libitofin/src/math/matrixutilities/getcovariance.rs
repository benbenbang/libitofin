//! Covariance conversion from standard deviations and correlation.
//!
//! Port of `ql/math/matrixutilities/getcovariance.hpp`. The conversion checks
//! dimensions, near symmetry and unit diagonal, but does not impose positive
//! semidefiniteness or silently salvage the input matrix.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::require;
use crate::types::Real;

/// Converts standard deviations and correlation into a symmetric covariance.
///
/// Off-diagonal entries use the average of the two correlation entries, as in
/// QuantLib. Zero standard deviations and empty inputs are supported.
///
/// # Errors
///
/// Rejects incompatible dimensions, nonfinite or negative standard deviations
/// or tolerance, nonfinite correlations, non-unit diagonals or asymmetry beyond
/// `tolerance`, and covariance entries that overflow.
pub fn get_covariance(std_dev: &Array, corr: &Matrix, tolerance: Real) -> QlResult<Matrix> {
    require!(
        tolerance.is_finite() && tolerance >= 0.0,
        "covariance tolerance must be finite and nonnegative"
    );
    let size = std_dev.size();
    require!(
        corr.rows() == size,
        "dimension mismatch between volatilities ({size}) and correlation rows ({})",
        corr.rows()
    );
    require!(
        corr.columns() == size,
        "correlation matrix is not square: {size} rows and {} columns",
        corr.columns()
    );
    for (i, value) in std_dev.iter().enumerate() {
        require!(
            value.is_finite() && *value >= 0.0,
            "standard deviation {i} must be finite and nonnegative"
        );
    }
    let mut covariance = Matrix::with_size(size, size);
    for i in 0..size {
        require!(
            corr[(i, i)].is_finite() && (corr[(i, i)] - 1.0).abs() <= tolerance,
            "invalid correlation diagonal at row {i}"
        );
        let variance = std_dev[i] * std_dev[i];
        require!(variance.is_finite(), "variance at row {i} overflowed");
        covariance[(i, i)] = variance;
        for j in 0..i {
            let lower = corr[(i, j)];
            let upper = corr[(j, i)];
            require!(
                lower.is_finite() && upper.is_finite(),
                "correlation entries ({i},{j}) must be finite"
            );
            let difference = (lower - upper).abs();
            require!(
                difference.is_finite() && difference <= tolerance,
                "correlation matrix not symmetric at ({i},{j})"
            );
            let entry = std_dev[i] * std_dev[j] * (0.5 * lower + 0.5 * upper);
            require!(entry.is_finite(), "covariance entry ({i},{j}) overflowed");
            covariance[(i, j)] = entry;
            covariance[(j, i)] = entry;
        }
    }
    Ok(covariance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_quantlib_formula_and_averages_near_symmetry() {
        let corr = Matrix::from([
            [1.0, 0.5, -0.25],
            [0.5 + 2e-13, 1.0, 0.75],
            [-0.25, 0.75, 1.0],
        ]);
        let got = get_covariance(&Array::from([0.2, 0.3, 0.4]), &corr, 1e-12).unwrap();
        let expected = Matrix::from([
            [
                0.04000000000000001,
                0.030000000000005994,
                -0.020000000000000004,
            ],
            [0.030000000000005994, 0.09, 0.09],
            [-0.020000000000000004, 0.09, 0.16000000000000003],
        ]);
        for i in 0..3 {
            for j in 0..3 {
                assert!((got[(i, j)] - expected[(i, j)]).abs() < 1e-15);
                assert_eq!(got[(i, j)], got[(j, i)]);
            }
        }
    }

    #[test]
    fn handles_empty_and_zero_standard_deviations() {
        let empty = get_covariance(&Array::with_size(0), &Matrix::with_size(0, 0), 0.0).unwrap();
        assert_eq!((empty.rows(), empty.columns()), (0, 0));
        let corr = Matrix::from([[1.0, -1.0], [-1.0, 1.0]]);
        let got = get_covariance(&Array::from([0.0, 0.3]), &corr, 0.0).unwrap();
        assert_eq!(got[(0, 0)], 0.0);
        assert_eq!(got[(0, 1)], 0.0);
        assert!((got[(1, 1)] - 0.09).abs() < 1e-15);
    }

    #[test]
    fn accepts_inclusive_tolerance_without_salvaging() {
        let corr = Matrix::from([[1.125, 0.0], [0.125, 1.0]]);
        let covariance = get_covariance(&Array::from([2.0, 3.0]), &corr, 0.125).unwrap();
        assert_eq!(covariance[(0, 0)], 4.0);
        assert_eq!(covariance[(0, 1)], 0.375);
        assert!(get_covariance(&Array::from([2.0, 3.0]), &corr, 0.124).is_err());
        let not_psd = Matrix::from([
            [1.0, -0.75, -0.75],
            [-0.75, 1.0, -0.75],
            [-0.75, -0.75, 1.0],
        ]);
        let covariance = get_covariance(&Array::from([1.0, 1.0, 1.0]), &not_psd, 0.0).unwrap();
        assert_eq!(covariance, not_psd);
    }

    #[test]
    fn rejects_invalid_inputs_and_overflow() {
        let vols = Array::from([0.2, 0.3]);
        let corr = Matrix::from([[1.0, 0.5], [0.5, 1.0]]);
        for invalid in [-1.0, Real::NAN, Real::INFINITY] {
            assert!(get_covariance(&vols, &corr, invalid).is_err());
            assert!(get_covariance(&Array::from([invalid, 0.3]), &corr, 1e-12).is_err());
        }
        for invalid in [Real::NAN, Real::INFINITY, 0.5] {
            let mut changed = corr.clone();
            changed[(0, 0)] = invalid;
            assert!(get_covariance(&vols, &changed, 1e-12).is_err());
        }
        let mut asymmetric = corr.clone();
        asymmetric[(0, 1)] += 2e-12;
        assert!(get_covariance(&vols, &asymmetric, 1e-12).is_err());
        for invalid in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            asymmetric[(0, 1)] = invalid;
            asymmetric[(1, 0)] = invalid;
            assert!(get_covariance(&vols, &asymmetric, 1e-12).is_err());
        }
        assert!(get_covariance(&vols, &Matrix::with_size(3, 3), 1e-12).is_err());
        assert!(get_covariance(&vols, &Matrix::with_size(2, 3), 1e-12).is_err());
        assert!(get_covariance(&Array::from([Real::MAX, 0.3]), &corr, 1e-12).is_err());
    }
}
