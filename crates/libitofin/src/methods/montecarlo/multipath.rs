//! Correlated multiple-asset paths.
//!
//! Port of `ql/methods/montecarlo/multipath.hpp`: a thin `Vec<Path>` where
//! `multipath[j]` is the [`Path`] followed by the `j`-th asset (`multipath.hpp:34`).
//! Thin over the on-main [`Path`], exactly as the C++ is thin over its
//! `std::vector<Path>`.
//!
//! Divergences from `multipath.hpp`, all deliberate:
//! - **`new` is fallible**: C++'s constructor throws `QL_REQUIRE(nAsset > 0)`
//!   (`multipath.hpp:66`); here it returns `Err` (D4). It also propagates the
//!   fallible [`Path::new`] (`path.rs:46`), though the empty-values path it
//!   drives never actually errors.
//! - **`operator[]`/`at` split into [`Index`]/[`IndexMut`] plus `at`/`at_mut`**:
//!   C++ overloads `operator[]` and `at` for const and non-const
//!   (`multipath.hpp:52-55`). Matching the [`Path`] house style, unchecked `[]`
//!   stands in for `operator[]` and `at`/`at_mut` return `Option`.

use std::ops::{Index, IndexMut};

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::Path;
use crate::require;
use crate::types::Size;

/// The list of single-asset paths making up a correlated multi-asset scenario.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MultiPath {
    paths: Vec<Path>,
}

impl MultiPath {
    /// `n_asset` paths over `time_grid`, each a fresh [`Path`] of grid-sized
    /// zeros (`multipath.hpp:64-67`).
    ///
    /// # Errors
    ///
    /// Returns `Err("number of asset must be positive")` when `n_asset` is zero
    /// (`multipath.hpp:66`); also propagates any [`Path::new`] error.
    pub fn new(n_asset: Size, time_grid: &TimeGrid) -> QlResult<Self> {
        require!(n_asset > 0, "number of asset must be positive");
        let paths = (0..n_asset)
            .map(|_| Path::new(time_grid.clone(), Array::new()))
            .collect::<QlResult<Vec<Path>>>()?;
        Ok(MultiPath { paths })
    }

    /// A multi-path from already-built component paths (`multipath.hpp:44,69`).
    pub fn from_paths(paths: Vec<Path>) -> Self {
        MultiPath { paths }
    }

    /// The number of assets (`multipath.hpp:47`).
    pub fn asset_number(&self) -> Size {
        self.paths.len()
    }

    /// The number of points on each asset's path (`multipath.hpp:48`).
    /// Unchecked on the first component, like C++.
    pub fn path_size(&self) -> Size {
        self.paths[0].length()
    }

    /// The bounds-checked `j`-th asset path (`multipath.hpp:53`), or `None`.
    pub fn at(&self, j: Size) -> Option<&Path> {
        self.paths.get(j)
    }

    /// The bounds-checked `j`-th asset path, mutably (`multipath.hpp:55`), or
    /// `None`.
    pub fn at_mut(&mut self, j: Size) -> Option<&mut Path> {
        self.paths.get_mut(j)
    }
}

/// Unchecked asset-path access (`multipath.hpp:52`): panics if `j` is out of
/// range.
impl Index<Size> for MultiPath {
    type Output = Path;

    fn index(&self, j: Size) -> &Path {
        &self.paths[j]
    }
}

/// Unchecked mutable asset-path access (`multipath.hpp:54`): panics if `j` is
/// out of range.
impl IndexMut<Size> for MultiPath {
    fn index_mut(&mut self, j: Size) -> &mut Path {
        &mut self.paths[j]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> TimeGrid {
        TimeGrid::new(1.0, 4).unwrap()
    }

    #[test]
    fn new_builds_n_asset_grid_sized_paths() {
        // multipath.hpp:64-67: nAsset copies of Path(timeGrid).
        let mp = MultiPath::new(3, &grid()).unwrap();
        assert_eq!(mp.asset_number(), 3);
        assert_eq!(mp.path_size(), 5);
        for j in 0..mp.asset_number() {
            assert_eq!(mp[j].values(), &[0.0; 5]);
        }
    }

    #[test]
    fn zero_assets_is_rejected() {
        // multipath.hpp:66: QL_REQUIRE(nAsset > 0).
        let err = MultiPath::new(0, &grid()).unwrap_err();
        assert_eq!(err.message(), "number of asset must be positive");
    }

    #[test]
    fn from_paths_round_trips() {
        let paths = vec![
            Path::new(grid(), Array::from([1.0, 2.0, 3.0, 4.0, 5.0])).unwrap(),
            Path::new(grid(), Array::from([6.0, 7.0, 8.0, 9.0, 10.0])).unwrap(),
        ];
        let mp = MultiPath::from_paths(paths);
        assert_eq!(mp.asset_number(), 2);
        assert_eq!(mp[0].front(), 1.0);
        assert_eq!(mp[1].back(), 10.0);
    }

    #[test]
    fn index_and_at_accessors() {
        let mut mp = MultiPath::new(2, &grid()).unwrap();
        *mp[0].front_mut() = 100.0;
        assert_eq!(mp[0].front(), 100.0);
        assert!(mp.at(1).is_some());
        assert!(mp.at(2).is_none());

        *mp.at_mut(1).unwrap().back_mut() = 200.0;
        assert_eq!(mp[1].back(), 200.0);
        assert!(mp.at_mut(2).is_none());
    }
}
