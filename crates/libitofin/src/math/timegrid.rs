//! Discrete time grid.
//!
//! Port of `ql/timegrid.{hpp,cpp}`, restricted to the regularly spaced grid used
//! by the Monte Carlo path generators. [`TimeGrid::new`] mirrors
//! `TimeGrid(Time end, Size steps)` (`timegrid.cpp:26`).
//!
//! Divergences from QuantLib, all deliberate:
//! - **`steps == 0`**: C++ does not guard this; `dt = end/0` is `+inf` and the
//!   grid collapses to a single `NaN` point. Per D10 (usability at boundaries)
//!   we return `Err("at least one step required")` instead of a poisoned grid.
//! - **`front`/`back`/`at`**: return `Option<Time>` rather than a bare `Time`,
//!   matching the `first`/`last` slice mapping already used by
//!   [`Array`](crate::math::array::Array); a [`Default`] (empty) grid is
//!   representable, so bare accessors would panic. `Index`/[`dt`](TimeGrid::dt)
//!   stay unchecked, mirroring C++'s unchecked `operator[]`/`dt`.
//! - **storage**: `Vec<Time>` rather than `Array`; the grid needs no
//!   element-wise math, only sequence access.
//!
//! Deferred (not needed by the MC path): the two mandatory-times template ctors
//! (`timegrid.hpp:54,85`) and the initializer-list ctors (`:141,143`), plus
//! `index`/`closest_index`/`closest_time` (`timegrid.hpp:149-153`).

use std::ops::Index;

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

/// A discrete, regularly spaced grid of times starting at zero.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TimeGrid {
    times: Vec<Time>,
    dt: Vec<Time>,
    mandatory_times: Vec<Time>,
}

impl TimeGrid {
    /// Regularly spaced grid: `steps + 1` points `0, dt, 2*dt, ..., end` with
    /// `dt = end / steps`.
    ///
    /// Mirrors `TimeGrid(Time end, Size steps)` (`timegrid.cpp:26`). Points are
    /// built by multiplication (`dt * i`) rather than running accumulation, as
    /// in C++, to preserve exactness at the endpoints.
    ///
    /// # Errors
    /// Returns `Err` if `end <= 0.0` ("negative times not allowed", the C++
    /// message) or if `steps == 0` (see the module divergence note).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(end: Time, steps: Size) -> QlResult<Self> {
        require!(end > 0.0, "negative times not allowed");
        require!(steps > 0, "at least one step required");

        let dt = end / steps as Real;
        let times = (0..=steps).map(|i| dt * i as Real).collect();
        Ok(TimeGrid {
            times,
            dt: vec![dt; steps],
            mandatory_times: vec![end],
        })
    }

    /// The spacing `dt_[i]` at step `i` (`timegrid.hpp:159`). Unchecked, like
    /// C++'s `dt`: panics if `i` is out of range.
    pub fn dt(&self, i: Size) -> Time {
        self.dt[i]
    }

    /// The mandatory time points guaranteed to lie on the grid
    /// (`timegrid.hpp:156`). For a regular grid this is `[end]`.
    pub fn mandatory_times(&self) -> &[Time] {
        &self.mandatory_times
    }

    /// The grid points as a slice, covering C++'s `begin`/`end` iteration
    /// (`timegrid.hpp:171`): iterate with `grid.times().iter()`.
    pub fn times(&self) -> &[Time] {
        &self.times
    }

    /// The number of grid points (`timegrid.hpp:169`).
    pub fn size(&self) -> Size {
        self.times.len()
    }

    /// Whether the grid has no points (`timegrid.hpp:170`).
    pub fn empty(&self) -> bool {
        self.times.is_empty()
    }

    /// The bounds-checked point at index `i`, mirroring C++'s `at`
    /// (`timegrid.hpp:168`), but returning `None` rather than throwing.
    pub fn at(&self, i: Size) -> Option<Time> {
        self.times.get(i).copied()
    }

    /// The index `i` such that `grid[i]` is closest to `t` (`timegrid.cpp:80`).
    ///
    /// Mirrors C++'s `std::lower_bound` walk: find the first node `>= t`, then
    /// return whichever of it and its predecessor is nearer, clamping at the
    /// ends. On an empty grid this returns `0`, matching C++'s
    /// `begin == end` short-circuit (there `size()-1` underflows, so callers of
    /// [`index`](TimeGrid::index) never reach it on an empty grid).
    pub fn closest_index(&self, t: Time) -> Size {
        let result = self.times.partition_point(|&x| x < t);
        if result == 0 {
            0
        } else if result == self.times.len() {
            self.times.len() - 1
        } else {
            let dt1 = self.times[result] - t;
            let dt2 = t - self.times[result - 1];
            if dt1 < dt2 { result } else { result - 1 }
        }
    }

    /// The index `i` such that `grid[i] == t` (`timegrid.cpp:43`).
    ///
    /// Finds the [`closest_index`](TimeGrid::closest_index) and requires it to
    /// coincide with `t` under [`close_enough`]; otherwise the grid cannot
    /// resolve `t` to a node and this returns `Err` (C++ `QL_FAIL`, split into
    /// three messages there, folded into one here per D4).
    ///
    /// # Errors
    /// Returns `Err` when no grid node is [`close_enough`] to `t`.
    pub fn index(&self, t: Time) -> QlResult<Size> {
        let i = self.closest_index(t);
        if close_enough(t, self.times[i]) {
            Ok(i)
        } else {
            fail!(
                "using inadequate time grid: no node is close enough to the \
                 required time t = {t} (closest node is t1 = {})",
                self.times[i]
            );
        }
    }

    /// The first grid point (`timegrid.hpp:175`), or `None` if empty.
    pub fn front(&self) -> Option<Time> {
        self.times.first().copied()
    }

    /// The last grid point (`timegrid.hpp:176`), or `None` if empty.
    pub fn back(&self) -> Option<Time> {
        self.times.last().copied()
    }
}

/// Unchecked point access, mirroring C++'s `operator[]` (`timegrid.hpp:167`):
/// panics if `i` is out of range.
impl Index<Size> for TimeGrid {
    type Output = Time;

    fn index(&self, i: Size) -> &Time {
        &self.times[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_grid_matches_reference() {
        // Oracle: TimeGrid(1.0, 4) -> 5 points, uniform 0.25 spacing.
        let grid = TimeGrid::new(1.0, 4).unwrap();
        assert_eq!(grid.size(), 5);
        assert_eq!(grid.times(), &[0.0, 0.25, 0.5, 0.75, 1.0]);
        for i in 0..4 {
            assert_eq!(grid.dt(i), 0.25);
        }
        assert_eq!(grid.front(), Some(0.0));
        assert_eq!(grid.back(), Some(1.0));
        assert_eq!(grid.mandatory_times(), &[1.0]);
    }

    #[test]
    fn indexing_and_bounds_checks() {
        let grid = TimeGrid::new(1.0, 4).unwrap();
        assert_eq!(grid[2], 0.5);
        assert_eq!(grid.at(4), Some(1.0));
        assert_eq!(grid.at(5), None);
        assert!(!grid.empty());
    }

    #[test]
    fn default_grid_is_empty() {
        let grid = TimeGrid::default();
        assert!(grid.empty());
        assert_eq!(grid.size(), 0);
        assert_eq!(grid.front(), None);
        assert_eq!(grid.back(), None);
    }

    #[test]
    fn non_positive_end_is_rejected() {
        // C++ QL_REQUIRE(end > 0.0, "negative times not allowed").
        let err = TimeGrid::new(0.0, 4).unwrap_err();
        assert_eq!(err.message(), "negative times not allowed");
        assert!(TimeGrid::new(-1.0, 4).is_err());
    }

    #[test]
    fn zero_steps_is_rejected() {
        // Divergence from C++: guard the inf/NaN grid at the boundary.
        let err = TimeGrid::new(1.0, 0).unwrap_err();
        assert_eq!(err.message(), "at least one step required");
    }

    #[test]
    fn index_resolves_grid_aligned_times() {
        // timegrid.cpp:43: index(t) returns i with grid[i] == t.
        let grid = TimeGrid::new(1.0, 4).unwrap();
        assert_eq!(grid.index(0.0).unwrap(), 0);
        assert_eq!(grid.index(0.25).unwrap(), 1);
        assert_eq!(grid.index(0.5).unwrap(), 2);
        assert_eq!(grid.index(1.0).unwrap(), 4);
    }

    #[test]
    fn index_rejects_off_grid_times() {
        // timegrid.cpp:47: a t between nodes resolves to no index.
        let grid = TimeGrid::new(1.0, 4).unwrap();
        assert!(grid.index(0.3).is_err());
        assert!(grid.index(1.5).is_err());
        assert!(grid.index(-0.1).is_err());
    }

    #[test]
    fn closest_index_snaps_to_nearest_node() {
        // timegrid.cpp:80: nearest node, ends clamped, ties to the lower node.
        let grid = TimeGrid::new(1.0, 4).unwrap();
        assert_eq!(grid.closest_index(0.3), 1);
        assert_eq!(grid.closest_index(0.4), 2);
        assert_eq!(grid.closest_index(2.0), 4);
        assert_eq!(grid.closest_index(-1.0), 0);
        // A midpoint tie: dt1 (0.25 - 0.125) == dt2, so the lower node wins.
        assert_eq!(grid.closest_index(0.125), 0);
    }
}
