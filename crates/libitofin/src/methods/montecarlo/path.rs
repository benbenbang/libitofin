//! Single-factor random walk.
//!
//! Port of `ql/methods/montecarlo/path.hpp`: a [`TimeGrid`] paired with an
//! [`Array`] of asset values, one per grid point, with the initial value at
//! index 0 (`path.hpp:38`). Thin over the on-main [`Array`], exactly as the C++
//! is thin over its `Array`.
//!
//! Divergences from `path.hpp`, all deliberate:
//! - **`new` is fallible**: C++'s constructor throws `QL_REQUIRE` on a
//!   size mismatch (`path.hpp:86`); here it returns `Err` (D4). It only fails
//!   when non-empty `values` disagree with the grid; an empty `Array` still
//!   defaults to `grid.size()` zeros (`path.hpp:84`).
//! - **`operator[]`/`value` collapse into [`Index`]/[`IndexMut`]**: C++ exposes
//!   `operator[]` and the identically-defined `value(i)` as separate members
//!   (`path.hpp:98,114`). Matching the [`Array`] precedent, the single
//!   unchecked `[]` representation stands in for both.
//! - **`at` returns `Option`**: C++'s bounds-checked `at` throws
//!   (`path.hpp:102`); here `at`/`at_mut` return `Option`, mirroring
//!   [`TimeGrid::at`]. `front`/`back` stay unchecked, as in C++.

use std::ops::{Index, IndexMut};

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::timegrid::TimeGrid;
use crate::require;
use crate::types::{Real, Size, Time};

/// A single-factor random walk: asset values sampled on a [`TimeGrid`].
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Path {
    time_grid: TimeGrid,
    values: Array,
}

impl Path {
    /// A path over `time_grid` with the given `values`.
    ///
    /// An empty `values` defaults to `time_grid.size()` zeros
    /// (`path.hpp:84`); otherwise `values.size()` must equal the grid size.
    ///
    /// # Errors
    ///
    /// Returns `Err("different number of times and asset values")` when a
    /// non-empty `values` disagrees with the grid (`path.hpp:86`).
    pub fn new(time_grid: TimeGrid, values: Array) -> QlResult<Self> {
        let values = if values.is_empty() {
            Array::with_size(time_grid.size())
        } else {
            values
        };
        require!(
            values.size() == time_grid.size(),
            "different number of times and asset values"
        );
        Ok(Path { time_grid, values })
    }

    /// Whether the path has no points (`path.hpp:90`).
    pub fn empty(&self) -> bool {
        self.time_grid.empty()
    }

    /// The number of points (`path.hpp:94`).
    pub fn length(&self) -> Size {
        self.time_grid.size()
    }

    /// The bounds-checked asset value at index `i` (`path.hpp:102`), or `None`.
    pub fn at(&self, i: Size) -> Option<Real> {
        self.values.get(i).copied()
    }

    /// The bounds-checked mutable asset value at index `i` (`path.hpp:110`),
    /// or `None`.
    pub fn at_mut(&mut self, i: Size) -> Option<&mut Real> {
        self.values.get_mut(i)
    }

    /// The initial asset value (`path.hpp:122`). Unchecked, like C++.
    pub fn front(&self) -> Real {
        self.values[0]
    }

    /// The initial asset value, mutably (`path.hpp:126`). Unchecked, like C++.
    pub fn front_mut(&mut self) -> &mut Real {
        &mut self.values[0]
    }

    /// The final asset value (`path.hpp:130`). Unchecked, like C++.
    pub fn back(&self) -> Real {
        self.values[self.values.size() - 1]
    }

    /// The final asset value, mutably (`path.hpp:134`). Unchecked, like C++.
    pub fn back_mut(&mut self) -> &mut Real {
        let last = self.values.size() - 1;
        &mut self.values[last]
    }

    /// The time at the `i`-th point (`path.hpp:138`). Unchecked, like C++.
    pub fn time(&self, i: Size) -> Time {
        self.time_grid[i]
    }

    /// The underlying time grid (`path.hpp:142`).
    pub fn time_grid(&self) -> &TimeGrid {
        &self.time_grid
    }

    /// The asset values as a slice, covering C++'s `begin`/`end` iteration
    /// (`path.hpp:146`).
    pub fn values(&self) -> &[Real] {
        &self.values
    }
}

/// Unchecked asset-value access (`path.hpp:98`): panics if `i` is out of range.
impl Index<Size> for Path {
    type Output = Real;

    fn index(&self, i: Size) -> &Real {
        &self.values[i]
    }
}

/// Unchecked mutable asset-value access (`path.hpp:106`): panics if `i` is out
/// of range.
impl IndexMut<Size> for Path {
    fn index_mut(&mut self, i: Size) -> &mut Real {
        &mut self.values[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> TimeGrid {
        TimeGrid::new(1.0, 4).unwrap()
    }

    #[test]
    fn empty_values_default_to_grid_sized_zeros() {
        // path.hpp:84: an empty Array becomes grid.size() zeros.
        let path = Path::new(grid(), Array::new()).unwrap();
        assert_eq!(path.length(), 5);
        assert_eq!(path.values(), &[0.0; 5]);
        assert!(!path.empty());
    }

    #[test]
    fn provided_values_are_kept() {
        let values = Array::from([100.0, 101.0, 102.0, 103.0, 104.0]);
        let path = Path::new(grid(), values).unwrap();
        assert_eq!(path.front(), 100.0);
        assert_eq!(path.back(), 104.0);
        assert_eq!(path[2], 102.0);
        assert_eq!(path.at(4), Some(104.0));
        assert_eq!(path.at(5), None);
    }

    #[test]
    fn size_mismatch_is_rejected() {
        // path.hpp:86: QL_REQUIRE(values.size() == timeGrid.size()).
        let err = Path::new(grid(), Array::from([1.0, 2.0, 3.0])).unwrap_err();
        assert_eq!(err.message(), "different number of times and asset values");
    }

    #[test]
    fn mutable_accessors_write_through() {
        let mut path = Path::new(grid(), Array::new()).unwrap();
        *path.front_mut() = 100.0;
        *path.back_mut() = 200.0;
        path[2] = 150.0;
        *path.at_mut(1).unwrap() = 125.0;
        assert_eq!(path.values(), &[100.0, 125.0, 150.0, 0.0, 200.0]);
        assert!(path.at_mut(5).is_none());
    }

    #[test]
    fn time_reads_the_grid() {
        let path = Path::new(grid(), Array::new()).unwrap();
        for i in 0..path.length() {
            assert_eq!(path.time(i), path.time_grid()[i]);
        }
        assert_eq!(path.time(2), 0.5);
    }
}
