use crate::errors::QlResult;
use crate::require;

/// Maximum supported sample dimension.
pub const MAX_SEQUENCE_DIMENSION: usize = 256;
/// Maximum number of sample rows.
pub const MAX_SEQUENCE_ROWS: usize = 100_000;
/// Maximum total number of sample components.
pub const MAX_SEQUENCE_COMPONENTS: usize = 1_000_000;
/// Maximum row/component-pair work for a matrix query.
pub const MAX_SEQUENCE_MATRIX_WORK: usize = 100_000_000;

/// Vector or matrix statistic computed from weighted sample rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequenceStatistic {
    /// Weighted arithmetic mean per component.
    Mean,
    /// Count-corrected variance per component.
    Variance,
    /// Square root of the component variances.
    StandardDeviation,
    /// Component standard deviations divided by the square root of row count.
    ErrorEstimate,
    /// Minimum component values, including zero-weight rows.
    Minimum,
    /// Maximum component values, including zero-weight rows.
    Maximum,
    /// Count-corrected covariance in flattened row-major matrix order.
    Covariance,
    /// Correlation in flattened row-major matrix order.
    Correlation,
}

/// Validate sample shape and bounded query work, returning the output length.
///
/// # Errors
///
/// Rejects empty or oversized shapes, overflowing products, and matrix work
/// above [`MAX_SEQUENCE_MATRIX_WORK`].
pub fn validate_sequence_shape(
    rows: usize,
    dimension: usize,
    measure: SequenceStatistic,
) -> QlResult<usize> {
    require!(
        dimension > 0 && dimension <= MAX_SEQUENCE_DIMENSION,
        "sequence dimension must be in 1..={MAX_SEQUENCE_DIMENSION}"
    );
    require!(
        rows > 0 && rows <= MAX_SEQUENCE_ROWS,
        "sequence rows must be in 1..={MAX_SEQUENCE_ROWS}"
    );
    require!(
        rows.checked_mul(dimension)
            .is_some_and(|size| size <= MAX_SEQUENCE_COMPONENTS),
        "sequence component limit exceeded"
    );
    if matches!(
        measure,
        SequenceStatistic::Covariance | SequenceStatistic::Correlation
    ) {
        let size = dimension * dimension;
        require!(
            rows.checked_mul(size)
                .is_some_and(|work| work <= MAX_SEQUENCE_MATRIX_WORK),
            "sequence matrix work limit exceeded"
        );
        Ok(size)
    } else {
        Ok(dimension)
    }
}
