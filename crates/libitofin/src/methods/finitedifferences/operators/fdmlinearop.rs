//! The finite-difference operator contract.
//!
//! Port of `ql/methods/finitedifferences/operators/fdmlinearop.hpp:34`.

use crate::math::array::Array;

/// A linear operator over the values on a finite-difference grid.
///
/// The grid values are flattened row-major by
/// [`FdmLinearOpLayout`](super::FdmLinearOpLayout), so `r` and the result are
/// both indexed the way that layout indexes its points.
///
/// C++ also declares `toMatrix` pure-virtual (`fdmlinearop.hpp:40`). It is
/// deferred with the rest of the sparse-matrix work in #636: it returns a
/// `SparseMatrix`, which is not ported yet, and no operator on the current
/// path needs its own matrix form.
pub trait FdmLinearOp {
    /// Applies the operator to the grid values `r`.
    fn apply(&self, r: &Array) -> Array;
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::operators::FdmLinearOpLayout;
    use crate::types::Size;

    struct ShiftUp {
        layout: FdmLinearOpLayout,
        direction: Size,
    }

    impl FdmLinearOp for ShiftUp {
        fn apply(&self, r: &Array) -> Array {
            let mut result = Array::with_size(r.size());
            for position in self.layout.iter() {
                result[position.index()] =
                    r[self.layout.neighbourhood(&position, self.direction, 1)];
            }
            result
        }
    }

    #[test]
    fn apply_reads_grid_values_through_the_layout() {
        let layout = FdmLinearOpLayout::new(vec![3, 2]);
        let values = Array::incremental(layout.size(), 0.0, 1.0);

        let op = ShiftUp {
            layout,
            direction: 0,
        };
        let shifted = op.apply(&values);

        assert_eq!(shifted, Array::from([1.0, 2.0, 1.0, 4.0, 5.0, 4.0]));
    }
}
