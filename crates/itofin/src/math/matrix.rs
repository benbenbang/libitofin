//! Dense row-major matrix.
//!
//! Port of the basic algebra in `ql/math/matrix.hpp`: construction, element and
//! row access, element-wise `+`/`-`, scalar `*`/`/`, matrix and matrix-vector
//! products, and transpose. Decompositions (SVD, QR, Cholesky, inverse,
//! determinant) are a separate ticket (QL-1.14). The C++ lvalue/rvalue operator
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

impl Index<(Size, Size)> for Matrix {
    type Output = Real;
    fn index(&self, (i, j): (Size, Size)) -> &Real {
        &self.data[i * self.columns + j]
    }
}

impl IndexMut<(Size, Size)> for Matrix {
    fn index_mut(&mut self, (i, j): (Size, Size)) -> &mut Real {
        &mut self.data[i * self.columns + j]
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
