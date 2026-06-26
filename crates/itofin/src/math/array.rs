//! One-dimensional numeric vector.
//!
//! Port of `ql/math/array.hpp`. QuantLib's `Array` is a fixed-purpose math
//! vector (element-wise arithmetic, dot product, transcendental maps), distinct
//! from a general container. Here it is a thin newtype over `Vec<Real>`: the
//! C++ lvalue/rvalue operator overloads collapse into element-wise `std::ops`
//! impls, and `Deref<Target = [Real]>` supplies iteration, slicing, and
//! `first`/`last` (QuantLib's `front`/`back`).

use std::ops::{Add, Deref, DerefMut, Div, Index, IndexMut, Mul, Neg, Sub};

use crate::types::{Real, Size};

/// A 1-D vector of [`Real`]s with element-wise arithmetic.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Array {
    data: Vec<Real>,
}

impl Array {
    /// An empty array.
    pub fn new() -> Self {
        Array { data: Vec::new() }
    }

    /// An array of `n` zeros.
    pub fn with_size(n: Size) -> Self {
        Array { data: vec![0.0; n] }
    }

    /// An array of `n` copies of `value`.
    pub fn filled(n: Size, value: Real) -> Self {
        Array {
            data: vec![value; n],
        }
    }

    /// An array of `n` values `value + i * increment` for `i` in `0..n`.
    pub fn incremental(n: Size, value: Real, increment: Real) -> Self {
        Array {
            data: (0..n).map(|i| value + i as Real * increment).collect(),
        }
    }

    /// The number of elements.
    pub fn size(&self) -> Size {
        self.data.len()
    }

    /// Whether the array has no elements.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Resizes to `n` elements, preserving the existing prefix and zero-filling
    /// any growth.
    pub fn resize(&mut self, n: Size) {
        self.data.resize(n, 0.0);
    }

    /// The inner product `∑ self[i] * other[i]`. Panics on a size mismatch.
    pub fn dot(&self, other: &Array) -> Real {
        assert_eq!(self.size(), other.size(), "array size mismatch");
        self.data.iter().zip(&other.data).map(|(a, b)| a * b).sum()
    }

    /// The Euclidean (L2) norm `√(self · self)`.
    pub fn norm2(&self) -> Real {
        self.dot(self).sqrt()
    }

    /// Element-wise absolute value.
    pub fn abs(&self) -> Array {
        self.map(Real::abs)
    }

    /// Element-wise square root.
    pub fn sqrt(&self) -> Array {
        self.map(Real::sqrt)
    }

    /// Element-wise natural logarithm.
    pub fn log(&self) -> Array {
        self.map(Real::ln)
    }

    /// Element-wise exponential.
    pub fn exp(&self) -> Array {
        self.map(Real::exp)
    }

    /// Element-wise power.
    pub fn pow(&self, exponent: Real) -> Array {
        self.map(|x| x.powf(exponent))
    }

    fn map(&self, f: impl Fn(Real) -> Real) -> Array {
        Array {
            data: self.data.iter().copied().map(f).collect(),
        }
    }
}

impl Deref for Array {
    type Target = [Real];
    fn deref(&self) -> &[Real] {
        &self.data
    }
}

impl DerefMut for Array {
    fn deref_mut(&mut self) -> &mut [Real] {
        &mut self.data
    }
}

impl Index<Size> for Array {
    type Output = Real;
    fn index(&self, i: Size) -> &Real {
        &self.data[i]
    }
}

impl IndexMut<Size> for Array {
    fn index_mut(&mut self, i: Size) -> &mut Real {
        &mut self.data[i]
    }
}

impl From<Vec<Real>> for Array {
    fn from(data: Vec<Real>) -> Self {
        Array { data }
    }
}

impl<const N: usize> From<[Real; N]> for Array {
    fn from(values: [Real; N]) -> Self {
        Array {
            data: values.to_vec(),
        }
    }
}

impl FromIterator<Real> for Array {
    fn from_iter<I: IntoIterator<Item = Real>>(iter: I) -> Self {
        Array {
            data: iter.into_iter().collect(),
        }
    }
}

impl Neg for &Array {
    type Output = Array;
    fn neg(self) -> Array {
        self.map(|x| -x)
    }
}

/// Implements an element-wise binary operator for `&Array ⊕ &Array`,
/// `&Array ⊕ Real`, and `Real ⊕ &Array`. Array/array forms panic on a size
/// mismatch.
macro_rules! impl_binop {
    ($trait:ident, $method:ident, $op:tt) => {
        impl $trait<&Array> for &Array {
            type Output = Array;
            fn $method(self, rhs: &Array) -> Array {
                assert_eq!(self.size(), rhs.size(), "array size mismatch");
                self.data.iter().zip(&rhs.data).map(|(a, b)| a $op b).collect()
            }
        }
        impl $trait<Real> for &Array {
            type Output = Array;
            fn $method(self, rhs: Real) -> Array {
                self.data.iter().map(|a| a $op rhs).collect()
            }
        }
        impl $trait<&Array> for Real {
            type Output = Array;
            fn $method(self, rhs: &Array) -> Array {
                rhs.data.iter().map(|b| self $op b).collect()
            }
        }
    };
}

impl_binop!(Add, add, +);
impl_binop!(Sub, sub, -);
impl_binop!(Mul, mul, *);
impl_binop!(Div, div, /);

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-12;

    fn assert_close(a: &Array, expected: &[Real]) {
        assert_eq!(a.size(), expected.len());
        for (x, e) in a.iter().zip(expected) {
            assert!((x - e).abs() < TOL, "got {x}, expected {e}");
        }
    }

    #[test]
    fn construction() {
        assert!(Array::new().is_empty());
        assert_eq!(Array::with_size(5).size(), 5);
        assert!(Array::with_size(5).iter().all(|&x| x == 0.0));

        let filled = Array::filled(5, 42.0);
        assert_eq!(filled.size(), 5);
        assert!(filled.iter().all(|&x| x == 42.0));

        let inc = Array::incremental(5, 42.0, 3.0);
        assert_close(&inc, &[42.0, 45.0, 48.0, 51.0, 54.0]);

        let lit = Array::from([1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_close(&lit, &[1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn indexing_and_first_last() {
        let mut a = Array::from([1.0, 2.0, 3.0]);
        a[1] = 9.0;
        assert_eq!(a[1], 9.0);
        assert_eq!(a.first(), Some(&1.0));
        assert_eq!(a.last(), Some(&3.0));
    }

    #[test]
    fn resize_preserves_prefix() {
        let mut a = Array::incremental(10, 1.0, 1.0);
        a.resize(5);
        assert_close(&a, &[1.0, 2.0, 3.0, 4.0, 5.0]);
        a.resize(7);
        assert_eq!(a.size(), 7);
        assert_close(&a, &[1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 0.0]);
    }

    #[test]
    fn functions_match_scalar_maps() {
        let a: Array = (0..5).map(|i| (i as Real).sin() + 1.1).collect();
        for (i, &x) in a.iter().enumerate() {
            assert!((a.abs()[i] - x.abs()).abs() < TOL);
            assert!((a.sqrt()[i] - x.sqrt()).abs() < TOL);
            assert!((a.log()[i] - x.ln()).abs() < TOL);
            assert!((a.exp()[i] - x.exp()).abs() < TOL);
            assert!((a.pow(-2.3)[i] - x.powf(-2.3)).abs() < TOL);
        }
    }

    #[test]
    fn dot_and_norm() {
        let a = Array::from([3.0, 4.0]);
        assert_eq!(a.dot(&a), 25.0);
        assert_eq!(a.norm2(), 5.0);
    }

    #[test]
    fn unary_operators() {
        let a = Array::from([1.1, 2.2, 3.3]);
        assert_close(&-&a, &[-1.1, -2.2, -3.3]);
    }

    #[test]
    fn array_array_operators() {
        let a = Array::from([1.1, 2.2, 3.3]);
        assert_close(&(&a + &a), &[2.2, 4.4, 6.6]);
        assert_close(&(&a - &a), &[0.0, 0.0, 0.0]);
        assert_close(&(&a * &a), &[1.1 * 1.1, 2.2 * 2.2, 3.3 * 3.3]);
        assert_close(&(&a / &a), &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn array_scalar_operators() {
        let a = Array::from([1.1, 2.2, 3.3]);
        assert_close(&(&a + 1.1), &[2.2, 3.3, 4.4]);
        assert_close(&(&a - 1.1), &[0.0, 1.1, 2.2]);
        assert_close(&(1.1 - &a), &[0.0, -1.1, -2.2]);
        assert_close(&(&a * 1.1), &[1.1 * 1.1, 2.2 * 1.1, 3.3 * 1.1]);
        assert_close(&(1.1 / &a), &[1.1 / 1.1, 1.1 / 2.2, 1.1 / 3.3]);
    }

    #[test]
    #[should_panic(expected = "array size mismatch")]
    fn size_mismatch_panics() {
        let _ = &Array::with_size(2) + &Array::with_size(3);
    }
}
