//! Dense row-major matrix.
//!
//! Port of the basic algebra in `ql/math/matrix.hpp`: construction, element and
//! row access, element-wise `+`/`-`, scalar `*`/`/`, matrix and matrix-vector
//! products, and transpose. General decompositions live in `matrixutilities`; the fixed-size
//! [`det3`] and [`inverse_3x3`] helpers use checked row-scaled pivoting. The C++ lvalue/rvalue operator
//! overloads collapse into `&Matrix`-based `std::ops` impls.

use std::ops::{Add, Div, Index, IndexMut, Mul, Neg, Sub};

use crate::math::array::Array;
use crate::types::{Real, Size};

/// A dense matrix of [`Real`]s stored row-major.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Matrix {
    data: Vec<Real>,
    rows: Size,
    columns: Size,
}

impl Matrix {
    /// An empty 0×0 matrix.
    pub fn new() -> Self {
        Matrix {
            data: Vec::new(),
            rows: 0,
            columns: 0,
        }
    }

    /// A `rows`×`columns` matrix of zeros.
    pub fn with_size(rows: Size, columns: Size) -> Self {
        let len = rows
            .checked_mul(columns)
            .expect("matrix size overflows usize");
        Matrix {
            data: vec![0.0; len],
            rows,
            columns,
        }
    }

    /// A `rows`×`columns` matrix filled with `value`.
    pub fn filled(rows: Size, columns: Size, value: Real) -> Self {
        let len = rows
            .checked_mul(columns)
            .expect("matrix size overflows usize");
        Matrix {
            data: vec![value; len],
            rows,
            columns,
        }
    }

    /// The number of rows.
    pub fn rows(&self) -> Size {
        self.rows
    }

    /// The number of columns.
    pub fn columns(&self) -> Size {
        self.columns
    }

    /// Whether the matrix has no elements.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Borrows row `i` as a slice.
    pub fn row(&self, i: Size) -> &[Real] {
        assert!(
            i < self.rows,
            "row index {i} out of bounds for {} rows",
            self.rows
        );
        &self.data[i * self.columns..(i + 1) * self.columns]
    }

    /// Mutably borrows row `i` as a slice.
    pub fn row_mut(&mut self, i: Size) -> &mut [Real] {
        assert!(
            i < self.rows,
            "row index {i} out of bounds for {} rows",
            self.rows
        );
        &mut self.data[i * self.columns..(i + 1) * self.columns]
    }

    /// The main diagonal as an [`Array`].
    pub fn diagonal(&self) -> Array {
        let n = self.rows.min(self.columns);
        (0..n).map(|i| self[(i, i)]).collect()
    }

    /// The transpose (a `columns`×`rows` matrix).
    pub fn transpose(&self) -> Matrix {
        let mut t = Matrix::with_size(self.columns, self.rows);
        for i in 0..self.rows {
            for j in 0..self.columns {
                t[(j, i)] = self[(i, j)];
            }
        }
        t
    }
}

/// Builds a matrix from nested row arrays, e.g. `Matrix::from([[1.0, 2.0], [3.0, 4.0]])`.
impl<const R: usize, const C: usize> From<[[Real; C]; R]> for Matrix {
    fn from(rows: [[Real; C]; R]) -> Self {
        Matrix {
            data: rows.iter().flatten().copied().collect(),
            rows: R,
            columns: C,
        }
    }
}

impl Index<Size> for Matrix {
    type Output = [Real];
    fn index(&self, i: Size) -> &[Real] {
        self.row(i)
    }
}

impl IndexMut<Size> for Matrix {
    fn index_mut(&mut self, i: Size) -> &mut [Real] {
        self.row_mut(i)
    }
}

impl Matrix {
    /// The flat data index for element `(i, j)`, bounds-checked on both axes.
    /// Without the column check `i * columns + j` would alias the next row for
    /// `j >= columns` instead of panicking like a slice access.
    fn element_index(&self, i: Size, j: Size) -> Size {
        assert!(
            i < self.rows && j < self.columns,
            "matrix index ({i}, {j}) out of bounds for a {}x{} matrix",
            self.rows,
            self.columns
        );
        i * self.columns + j
    }
}

impl Index<(Size, Size)> for Matrix {
    type Output = Real;
    fn index(&self, (i, j): (Size, Size)) -> &Real {
        &self.data[self.element_index(i, j)]
    }
}

impl IndexMut<(Size, Size)> for Matrix {
    fn index_mut(&mut self, (i, j): (Size, Size)) -> &mut Real {
        let k = self.element_index(i, j);
        &mut self.data[k]
    }
}

impl Neg for &Matrix {
    type Output = Matrix;
    fn neg(self) -> Matrix {
        Matrix {
            data: self.data.iter().map(|x| -x).collect(),
            rows: self.rows,
            columns: self.columns,
        }
    }
}

/// Element-wise `+`/`-` for `&Matrix ⊕ &Matrix`; panics on a shape mismatch.
macro_rules! impl_elementwise {
    ($trait:ident, $method:ident, $op:tt) => {
        impl $trait<&Matrix> for &Matrix {
            type Output = Matrix;
            fn $method(self, rhs: &Matrix) -> Matrix {
                assert_eq!(
                    (self.rows, self.columns),
                    (rhs.rows, rhs.columns),
                    "matrix shape mismatch"
                );
                Matrix {
                    data: self.data.iter().zip(&rhs.data).map(|(a, b)| a $op b).collect(),
                    rows: self.rows,
                    columns: self.columns,
                }
            }
        }
    };
}

impl_elementwise!(Add, add, +);
impl_elementwise!(Sub, sub, -);

/// Scalar `*`/`/` for `&Matrix ⊕ Real`.
macro_rules! impl_scalar {
    ($trait:ident, $method:ident, $op:tt) => {
        impl $trait<Real> for &Matrix {
            type Output = Matrix;
            fn $method(self, rhs: Real) -> Matrix {
                Matrix {
                    data: self.data.iter().map(|a| a $op rhs).collect(),
                    rows: self.rows,
                    columns: self.columns,
                }
            }
        }
    };
}

impl_scalar!(Mul, mul, *);
impl_scalar!(Div, div, /);

impl Mul<&Matrix> for Real {
    type Output = Matrix;
    fn mul(self, rhs: &Matrix) -> Matrix {
        rhs * self
    }
}

impl Mul<&Matrix> for &Matrix {
    type Output = Matrix;
    fn mul(self, rhs: &Matrix) -> Matrix {
        assert_eq!(self.columns, rhs.rows, "matrix product shape mismatch");
        let mut result = Matrix::with_size(self.rows, rhs.columns);
        for i in 0..self.rows {
            for k in 0..self.columns {
                let a = self[(i, k)];
                for j in 0..rhs.columns {
                    result[(i, j)] += a * rhs[(k, j)];
                }
            }
        }
        result
    }
}

impl Mul<&Array> for &Matrix {
    type Output = Array;
    fn mul(self, rhs: &Array) -> Array {
        assert_eq!(self.columns, rhs.len(), "matrix-vector shape mismatch");
        (0..self.rows)
            .map(|i| self.row(i).iter().zip(rhs.iter()).map(|(a, b)| a * b).sum())
            .collect()
    }
}

impl Mul<&Matrix> for &Array {
    type Output = Array;
    fn mul(self, rhs: &Matrix) -> Array {
        assert_eq!(self.len(), rhs.rows, "vector-matrix shape mismatch");
        (0..rhs.columns)
            .map(|j| (0..rhs.rows).map(|i| self[i] * rhs[(i, j)]).sum())
            .collect()
    }
}

/// Determinant of an exactly 3 by 3 matrix, with row scaling and pivoting.
///
/// This fixed-size helper does not replace a general decomposition. No absolute
/// pivot tolerance is imposed; accuracy still depends on conditioning.
///
/// # Errors
/// Rejects wrong shapes, nonfinite inputs, unresolvable row scaling and a
/// nonzero determinant that cannot be represented as a finite nonzero `Real`.
pub fn det3(matrix: &Matrix) -> crate::errors::QlResult<Real> {
    let (mut rows, scales) = scaled_3x3(matrix)?;
    let mut factors = [scales[0], scales[1], scales[2], 1.0, 1.0, 1.0];
    let mut sign = 1.0;
    for column in 0..3 {
        let pivot = (column..3)
            .max_by(|&i, &j| rows[i][column].abs().total_cmp(&rows[j][column].abs()))
            .expect("a three-dimensional pivot range is nonempty");
        if rows[pivot][column] == 0.0 {
            return Ok(0.0);
        }
        if pivot != column {
            rows.swap(pivot, column);
            sign = -sign;
        }
        factors[column + 3] = rows[column][column];
        let pivot_row = rows[column];
        for row in rows.iter_mut().skip(column + 1) {
            let multiplier = row[column] / pivot_row[column];
            for (value, &pivot_value) in row.iter_mut().zip(&pivot_row).skip(column + 1) {
                *value = (-multiplier).mul_add(pivot_value, *value);
            }
        }
    }
    Ok(sign * scaled_product(&factors)?)
}

/// Inverse of an exactly 3 by 3 matrix, using row-scaled pivoted elimination.
///
/// The inverse does not depend on a representable determinant. Uniformly tiny
/// or large matrices can therefore have a valid finite inverse. No conditioning
/// estimate or absolute determinant tolerance is supplied. Both identity-product
/// residuals must be at most `256 * Real::EPSILON`; unresolved results error.
///
/// # Errors
/// Rejects malformed/nonfinite input, singular or numerically unresolved
/// systems, and unrepresentable intermediate or final inverse entries.
pub fn inverse_3x3(matrix: &Matrix) -> crate::errors::QlResult<Matrix> {
    let (rows, scales) = scaled_3x3(matrix)?;
    let mut augmented = [[0.0; 6]; 3];
    for i in 0..3 {
        augmented[i][..3].copy_from_slice(&rows[i]);
        augmented[i][i + 3] = 1.0 / scales[i];
        crate::require!(
            augmented[i][i + 3].is_finite(),
            "inverse scale is not representable"
        );
    }
    for column in 0..3 {
        let pivot = (column..3)
            .max_by(|&i, &j| {
                augmented[i][column]
                    .abs()
                    .total_cmp(&augmented[j][column].abs())
            })
            .expect("a three-dimensional pivot range is nonempty");
        augmented.swap(pivot, column);
        let divisor = augmented[column][column];
        crate::require!(divisor != 0.0, "singular or numerically unresolved matrix");
        let pivot_row = augmented[column];
        for (index, row) in augmented.iter_mut().enumerate() {
            if index != column {
                let multiplier = row[column] / divisor;
                for (value, &pivot_value) in row.iter_mut().zip(&pivot_row) {
                    *value = (-multiplier).mul_add(pivot_value, *value);
                    crate::require!(
                        value.is_finite(),
                        "inverse elimination is not representable"
                    );
                }
                row[column] = 0.0;
            }
        }
        for value in &mut augmented[column] {
            let previous = *value;
            *value /= divisor;
            crate::require!(
                value.is_finite() && (previous == 0.0 || *value != 0.0),
                "inverse elimination is not representable"
            );
        }
    }
    let inverse = Matrix::from(std::array::from_fn::<_, 3, _>(|i| {
        std::array::from_fn::<_, 3, _>(|j| augmented[i][j + 3])
    }));
    for i in 0..3 {
        for j in 0..3 {
            let left: Real = (0..3).map(|k| matrix[(i, k)] * inverse[(k, j)]).sum();
            let right: Real = (0..3).map(|k| inverse[(i, k)] * matrix[(k, j)]).sum();
            let expected = Real::from(i == j);
            crate::require!(
                left.is_finite()
                    && right.is_finite()
                    && (left - expected).abs() <= 256.0 * Real::EPSILON
                    && (right - expected).abs() <= 256.0 * Real::EPSILON,
                "inverse identity residual is numerically unresolved"
            );
        }
    }
    Ok(inverse)
}

fn scaled_3x3(matrix: &Matrix) -> crate::errors::QlResult<([[Real; 3]; 3], [Real; 3])> {
    crate::require!(
        matrix.rows() == 3 && matrix.columns() == 3,
        "matrix must be exactly 3 by 3"
    );
    let mut rows = [[0.0; 3]; 3];
    let mut scales = [1.0; 3];
    for i in 0..3 {
        let mut scale: Real = 0.0;
        for j in 0..3 {
            let value = matrix[(i, j)];
            crate::require!(value.is_finite(), "matrix entries must be finite");
            scale = scale.max(value.abs());
        }
        if scale != 0.0 {
            scales[i] = binary_power(binary_parts(scale).1);
        }
        for j in 0..3 {
            rows[i][j] = matrix[(i, j)] / scales[i];
            crate::require!(
                matrix[(i, j)] == 0.0 || rows[i][j] != 0.0,
                "matrix row scaling is numerically unresolved"
            );
        }
    }
    Ok((rows, scales))
}

fn scaled_product(factors: &[Real]) -> crate::errors::QlResult<Real> {
    let mut mantissa: Real = 1.0;
    let mut exponent = 0;
    for &factor in factors {
        let (fraction, power) = binary_parts(factor);
        mantissa *= fraction;
        exponent += power;
    }
    let (fraction, power) = binary_parts(mantissa);
    exponent += power;
    crate::require!(
        (-1075..=1023).contains(&exponent),
        "determinant is not representable"
    );
    let first_power = exponent.clamp(-1022, 1023);
    let value = (fraction * binary_power(first_power)) * binary_power(exponent - first_power);
    crate::require!(
        value.is_finite() && value != 0.0,
        "determinant is not representable"
    );
    Ok(value)
}

fn binary_parts(value: Real) -> (Real, i32) {
    let bits = value.abs().to_bits();
    let encoded = ((bits >> 52) & 0x7ff) as i32;
    let exponent = if encoded == 0 {
        63 - bits.leading_zeros() as i32 - 1074
    } else {
        encoded - 1023
    };
    (value / binary_power(exponent), exponent)
}

fn binary_power(exponent: i32) -> Real {
    if exponent >= -1022 {
        Real::from_bits(((exponent + 1023) as u64) << 52)
    } else {
        Real::from_bits(1 << (exponent + 1074))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-12;

    fn assert_close(m: &Matrix, expected: &Matrix) {
        assert_eq!(
            (m.rows(), m.columns()),
            (expected.rows(), expected.columns())
        );
        for (a, e) in m.data.iter().zip(&expected.data) {
            assert!((a - e).abs() < TOL, "got {a}, expected {e}");
        }
    }

    #[test]
    fn construction_and_access() {
        assert!(Matrix::new().is_empty());
        let z = Matrix::with_size(2, 3);
        assert_eq!((z.rows(), z.columns()), (2, 3));
        assert!(z.row(0).iter().all(|&x| x == 0.0));

        let m = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        assert_eq!((m.rows(), m.columns()), (2, 3));
        assert_eq!(m[(0, 0)], 1.0);
        assert_eq!(m[(1, 2)], 6.0);
        assert_eq!(m[1][0], 4.0); // row-then-column indexing
    }

    #[test]
    fn mutation_through_indexing() {
        let mut m = Matrix::with_size(2, 2);
        m[(0, 1)] = 7.0;
        m[1][0] = 9.0;
        assert_eq!(m[(0, 1)], 7.0);
        assert_eq!(m[(1, 0)], 9.0);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn tuple_index_column_out_of_range_panics() {
        // Before the fix m[(0, 3)] aliased m[(1, 0)] instead of panicking.
        let m = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let _ = m[(0, 3)];
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn tuple_index_row_out_of_range_panics() {
        let m = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let _ = m[(2, 0)];
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn tuple_index_mut_out_of_range_panics() {
        let mut m = Matrix::with_size(2, 2);
        m[(0, 2)] = 1.0;
    }

    #[test]
    fn elementwise_and_scalar_operators() {
        let m = Matrix::filled(2, 3, 4.0);
        assert_close(&-&m, &Matrix::filled(2, 3, -4.0));
        assert_close(&(&m + &m), &Matrix::filled(2, 3, 8.0));
        assert_close(&(&m - &m), &Matrix::filled(2, 3, 0.0));
        assert_close(&(&m * 1.5), &Matrix::filled(2, 3, 6.0));
        assert_close(&(1.5 * &m), &Matrix::filled(2, 3, 6.0));
        assert_close(&(&m / 2.0), &Matrix::filled(2, 3, 2.0));
    }

    #[test]
    fn matrix_product() {
        let a = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]); // 2x3
        let b = Matrix::from([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]); // 3x2
        // [[1*1+2*3+3*5, 1*2+2*4+3*6],[4*1+5*3+6*5, 4*2+5*4+6*6]]
        assert_close(&(&a * &b), &Matrix::from([[22.0, 28.0], [49.0, 64.0]]));
    }

    #[test]
    fn matrix_vector_products() {
        let a = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]); // 2x3
        let x = Array::from([1.0, 1.0, 1.0]);
        // A·x = row sums
        assert_eq!((&a * &x).to_vec(), vec![6.0, 15.0]);
        // y·A, y length 2
        let y = Array::from([1.0, 1.0]);
        assert_eq!((&y * &a).to_vec(), vec![5.0, 7.0, 9.0]);
    }

    #[test]
    fn transpose_and_diagonal() {
        let m = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let t = m.transpose();
        assert_eq!((t.rows(), t.columns()), (3, 2));
        assert_eq!(t[(0, 1)], 4.0);
        assert_eq!(t[(2, 0)], 3.0);

        let sq = Matrix::from([[1.0, 2.0], [3.0, 4.0]]);
        assert_eq!(sq.diagonal().to_vec(), vec![1.0, 4.0]);
    }

    #[test]
    #[should_panic(expected = "matrix product shape mismatch")]
    fn product_shape_mismatch_panics() {
        let _ = &Matrix::with_size(2, 3) * &Matrix::with_size(2, 2);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn row_out_of_bounds_panics_even_with_zero_columns() {
        // with columns == 0 the slice range is 0..0 for any i; the explicit
        // bounds check is what makes an out-of-range row index still panic
        let m = Matrix::with_size(3, 0);
        let _ = m.row(5);
    }
}
