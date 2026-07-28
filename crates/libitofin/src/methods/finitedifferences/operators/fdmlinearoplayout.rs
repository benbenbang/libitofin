//! Row-major index/coordinate mapping for an N-D finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/operators/fdmlinearoplayout.hpp:34`
//! and its `.cpp`. The layout owns the extents `dim` and the derived strides
//! `spacing` (`spacing[0] = 1`, then the running product of `dim`), which turn
//! coordinates into the flat index every operator addresses its
//! [`Array`](crate::math::array::Array) with.
//!
//! `end()` (`fdmlinearoplayout.hpp:47`) is not ported: it exists to terminate a
//! C++ range-for and is replaced by [`FdmLinearOpLayout::iter`].

use crate::types::{Integer, Size};

use super::fdmlinearopiterator::FdmLinearOpIterator;

/// The index space of an N-D grid.
#[derive(Clone, Debug)]
pub struct FdmLinearOpLayout {
    size: Size,
    dim: Vec<Size>,
    spacing: Vec<Size>,
}

impl FdmLinearOpLayout {
    /// A layout over a grid of extents `dim`. Panics if `dim` is empty, which
    /// the C++ constructor (`fdmlinearoplayout.hpp:36-43`) leaves undefined.
    pub fn new(dim: Vec<Size>) -> Self {
        assert!(!dim.is_empty(), "layout needs at least one dimension");
        let mut spacing = Vec::with_capacity(dim.len());
        let mut stride = 1;
        for extent in &dim {
            spacing.push(stride);
            stride *= extent;
        }
        FdmLinearOpLayout {
            size: stride,
            dim,
            spacing,
        }
    }

    /// The extents of the grid, one per dimension.
    pub fn dim(&self) -> &[Size] {
        &self.dim
    }

    /// The stride of each dimension in the flat index.
    pub fn spacing(&self) -> &[Size] {
        &self.spacing
    }

    /// The number of grid points, the product of the extents.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The flat index of `coordinates`. Panics on a length mismatch, where the
    /// C++ `inner_product` (`fdmlinearoplayout.hpp:63-68`) would truncate or
    /// read past `spacing_`.
    pub fn index(&self, coordinates: &[Size]) -> Size {
        assert_eq!(
            coordinates.len(),
            self.dim.len(),
            "coordinate count mismatch"
        );
        coordinates
            .iter()
            .zip(&self.spacing)
            .map(|(c, s)| c * s)
            .sum()
    }

    /// The origin of the grid.
    pub fn begin(&self) -> FdmLinearOpIterator {
        FdmLinearOpIterator::new(self.dim.clone())
    }

    /// Every grid point in row-major order. Each item clones its dimensions
    /// and coordinates;
    /// walking a single [`begin`](Self::begin) with
    /// [`advance`](FdmLinearOpIterator::advance) allocates nothing.
    pub fn iter(&self) -> impl Iterator<Item = FdmLinearOpIterator> + '_ {
        let mut current = self.begin();
        std::iter::from_fn(move || {
            if current.index() >= self.size {
                return None;
            }
            let position = current.clone();
            current.advance();
            Some(position)
        })
    }

    /// The index of the point `offset` steps from `iterator` along dimension
    /// `direction` (`fdmlinearoplayout.cpp:26-39`).
    ///
    /// A coordinate stepped past either end is reflected back into the grid,
    /// once: below `0` it negates, at or above `dim[direction]` it becomes
    /// `2 * (dim[direction] - 1) - coordinate`. The single reflection is not
    /// iterated and not range-checked, so an `offset` overshooting the far
    /// boundary after reflection is out of contract, as in C++.
    pub fn neighbourhood(
        &self,
        iterator: &FdmLinearOpIterator,
        direction: Size,
        offset: Integer,
    ) -> Size {
        let base = iterator.index() - iterator.coordinates()[direction] * self.spacing[direction];
        base + self.reflect(iterator, direction, offset) * self.spacing[direction]
    }

    /// The index of the point offset along two dimensions at once, each
    /// reflected independently (`fdmlinearoplayout.cpp:41-66`). The C++
    /// overload of [`neighbourhood`](Self::neighbourhood); Rust names it
    /// separately.
    pub fn neighbourhood2(
        &self,
        iterator: &FdmLinearOpIterator,
        direction1: Size,
        offset1: Integer,
        direction2: Size,
        offset2: Integer,
    ) -> Size {
        let base = iterator.index()
            - iterator.coordinates()[direction1] * self.spacing[direction1]
            - iterator.coordinates()[direction2] * self.spacing[direction2];
        base + self.reflect(iterator, direction1, offset1) * self.spacing[direction1]
            + self.reflect(iterator, direction2, offset2) * self.spacing[direction2]
    }

    /// The neighbour of [`neighbourhood`](Self::neighbourhood) as a full
    /// position rather than a bare index (`fdmlinearoplayout.cpp:69-84`).
    pub fn iter_neighbourhood(
        &self,
        iterator: &FdmLinearOpIterator,
        direction: Size,
        offset: Integer,
    ) -> FdmLinearOpIterator {
        let mut coordinates = iterator.coordinates().to_vec();
        coordinates[direction] = self.reflect(iterator, direction, offset);
        let index = self.index(&coordinates);
        FdmLinearOpIterator::with_coordinates(self.dim.clone(), coordinates, index)
    }

    fn reflect(&self, iterator: &FdmLinearOpIterator, direction: Size, offset: Integer) -> Size {
        let extent = self.dim[direction] as i64;
        let mut coordinate = iterator.coordinates()[direction] as i64 + offset as i64;
        if coordinate < 0 {
            coordinate = -coordinate;
        } else if coordinate >= extent {
            coordinate = 2 * (extent - 1) - coordinate;
        }
        coordinate as Size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIM: [Size; 3] = [5, 7, 8];

    fn layout() -> FdmLinearOpLayout {
        FdmLinearOpLayout::new(DIM.to_vec())
    }

    #[test]
    fn dimensions_size_and_spacing() {
        let layout = layout();
        assert_eq!(layout.dim().len(), DIM.len());
        assert_eq!(layout.size(), DIM.iter().product::<Size>());
        assert_eq!(layout.size(), 280);
        assert_eq!(layout.spacing(), [1, 5, 35]);
    }

    #[test]
    fn index_is_row_major() {
        let layout = layout();
        for k in 0..DIM[0] {
            for l in 0..DIM[1] {
                for m in 0..DIM[2] {
                    let expected = k + l * DIM[0] + m * DIM[0] * DIM[1];
                    assert_eq!(layout.index(&[k, l, m]), expected);
                }
            }
        }
    }

    #[test]
    fn neighbourhood_reflects_at_the_boundaries() {
        let layout = layout();
        let mut iter = layout.begin();

        for m in 0..DIM[2] {
            for l in 0..DIM[1] {
                for k in 0..DIM[0] {
                    assert_eq!(iter.coordinates(), [k, l, m]);

                    for n in 1..4 {
                        let reflected = if l < DIM[1] - n {
                            l + n
                        } else {
                            DIM[1] - 1 - (l + n - (DIM[1] - 1))
                        };
                        let expected = k + m * DIM[0] * DIM[1] + reflected * DIM[0];
                        assert_eq!(layout.neighbourhood(&iter, 1, n as Integer), expected);
                    }

                    for n in 1..7 {
                        let reflected = m.abs_diff(n);
                        let expected = k + l * DIM[0] + reflected * DIM[0] * DIM[1];
                        assert_eq!(layout.neighbourhood(&iter, 2, -(n as Integer)), expected);
                    }

                    iter.advance();
                }
            }
        }
    }

    #[test]
    fn iter_visits_every_point_in_order() {
        let layout = layout();
        let visited: Vec<_> = layout.iter().collect();
        assert_eq!(visited.len(), layout.size());
        for (expected_index, position) in visited.iter().enumerate() {
            assert_eq!(position.index(), expected_index);
            assert_eq!(layout.index(position.coordinates()), expected_index);
        }
    }

    #[test]
    fn iter_neighbourhood_agrees_with_neighbourhood() {
        let layout = layout();
        for position in layout.iter() {
            for direction in 0..DIM.len() {
                for offset in -3..=3 {
                    let neighbour = layout.iter_neighbourhood(&position, direction, offset);
                    assert_eq!(
                        neighbour.index(),
                        layout.neighbourhood(&position, direction, offset)
                    );
                }
            }
        }
    }

    #[test]
    fn neighbourhood2_offsets_both_directions() {
        let layout = layout();

        let origin = layout.begin();
        assert_eq!(layout.neighbourhood(&origin, 1, 3), 15);
        assert_eq!(layout.neighbourhood2(&origin, 0, 2, 2, 3), 2 + 3 * 35);
        assert_eq!(layout.neighbourhood2(&origin, 1, 8, 2, -1), 4 * 5 + 35);

        for position in layout.iter() {
            for (direction1, direction2) in [(0, 1), (1, 2), (2, 0)] {
                for offset1 in -3..=3 {
                    for offset2 in -3..=3 {
                        let stepped = layout.iter_neighbourhood(&position, direction1, offset1);
                        assert_eq!(
                            layout.neighbourhood2(
                                &position, direction1, offset1, direction2, offset2
                            ),
                            layout.neighbourhood(&stepped, direction2, offset2)
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "at least one dimension")]
    fn empty_dimensions_panic() {
        FdmLinearOpLayout::new(Vec::new());
    }

    #[test]
    #[should_panic(expected = "coordinate count mismatch")]
    fn short_coordinates_panic() {
        layout().index(&[1, 2]);
    }
}
