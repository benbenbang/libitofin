//! Odometer over the points of an N-D finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/operators/fdmlinearopiterator.hpp:36`
//! (header-only). The C++ class is a hand-rolled forward iterator whose
//! `operator*` returns `*this`, so it is a position, not a cursor over
//! elements: it carries the flat `index` and the `coordinates` that index maps
//! to, and `operator++` (`:49-59`) advances both.
//!
//! Two members are C++ idiom artifacts and are not ported: `swap` (`:80`), and
//! the index-only `operator==`/`!=` (`:66-71`) that let `end()` be an iterator
//! carrying an index but no dimensions. Rust expresses the walk with
//! [`FdmLinearOpLayout::iter`](super::FdmLinearOpLayout::iter), which owns the
//! stopping condition, so no sentinel value is needed.

use crate::types::Size;

/// A position on an N-D grid: a flat row-major index plus its coordinates.
///
/// Dimension `0` varies fastest, matching the C++ carry order
/// (`fdmlinearopiterator.hpp:50-58`).
#[derive(Clone, Debug)]
pub struct FdmLinearOpIterator {
    index: Size,
    dim: Vec<Size>,
    coordinates: Vec<Size>,
}

impl FdmLinearOpIterator {
    /// The origin of a grid of extents `dim`: index `0`, all coordinates `0`.
    pub fn new(dim: Vec<Size>) -> Self {
        let coordinates = vec![0; dim.len()];
        FdmLinearOpIterator {
            index: 0,
            dim,
            coordinates,
        }
    }

    /// A position stated explicitly. The caller is responsible for `index`
    /// agreeing with `coordinates`; the C++ three-argument constructor
    /// (`fdmlinearopiterator.hpp:46`) does not check either.
    pub fn with_coordinates(dim: Vec<Size>, coordinates: Vec<Size>, index: Size) -> Self {
        FdmLinearOpIterator {
            index,
            dim,
            coordinates,
        }
    }

    /// The flat row-major index of this position.
    pub fn index(&self) -> Size {
        self.index
    }

    /// The per-dimension coordinates of this position.
    pub fn coordinates(&self) -> &[Size] {
        &self.coordinates
    }

    /// Steps to the next grid point (C++ `operator++`). The index increments
    /// unconditionally; the coordinates carry from dimension `0` upwards, and
    /// wrap to all-zero once the last point is passed.
    pub fn advance(&mut self) {
        self.index += 1;
        for i in 0..self.dim.len() {
            self.coordinates[i] += 1;
            if self.coordinates[i] == self.dim[i] {
                self.coordinates[i] = 0;
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_walks_row_major_with_dimension_zero_fastest() {
        let dim = vec![5, 7, 8];
        let mut iter = FdmLinearOpIterator::new(dim.clone());
        assert_eq!(iter.index(), 0);
        assert_eq!(iter.coordinates(), [0, 0, 0]);

        for m in 0..dim[2] {
            for l in 0..dim[1] {
                for k in 0..dim[0] {
                    assert_eq!(iter.coordinates(), [k, l, m]);
                    assert_eq!(iter.index(), k + l * dim[0] + m * dim[0] * dim[1]);
                    iter.advance();
                }
            }
        }

        assert_eq!(iter.index(), 5 * 7 * 8);
        assert_eq!(iter.coordinates(), [0, 0, 0]);
    }
}
