//! The step condition that records the grid at one time.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/fdmsnapshotcondition.hpp:33`
//! and its `.cpp`.

use std::cell::RefCell;

use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::types::Time;

/// Captures the grid values as the solver rolls past one time.
///
/// C++ fills a `mutable Array values_` from a `const applyTo` (`hpp:43`). The
/// Rust trait takes `&self` and consumers hold the condition as a
/// [`Shared`](crate::shared::Shared), which yields no `&mut`, so the capture
/// lives behind a [`RefCell`] - the same shape as
/// [`FdmCellAveragingInnerValue`](crate::methods::finitedifferences::utilities::FdmCellAveragingInnerValue).
#[derive(Debug)]
pub struct FdmSnapshotCondition {
    t: Time,
    values: RefCell<Array>,
}

impl FdmSnapshotCondition {
    /// Builds a condition that captures at `t` (`cpp:26-28`).
    pub fn new(t: Time) -> Self {
        Self {
            t,
            values: RefCell::new(Array::new()),
        }
    }

    /// The time the capture fires at (`cpp:36-38`).
    pub fn time(&self) -> Time {
        self.t
    }

    /// The captured values, empty until the capture fires.
    ///
    /// C++ lends a `const Array&` (`cpp:41-43`); the capture sits behind a
    /// `RefCell`, so the Rust accessor clones rather than handing out a
    /// [`Ref`](std::cell::Ref) a caller could hold live across a later
    /// `apply_to` on a composite holding this condition (the
    /// [`TreeLattice`](crate::methods::lattices::TreeLattice) accessors do the
    /// same).
    pub fn values(&self) -> Array {
        self.values.borrow().clone()
    }
}

impl StepCondition for FdmSnapshotCondition {
    /// `cpp:31-34`: copies the grid if and only if `t` is exactly the capture
    /// time. The equality is exact in C++ and stays exact here - a tolerance
    /// would change which step captures.
    fn apply_to(&self, a: &mut Array, t: Time) {
        if t == self.t {
            *self.values.borrow_mut() = a.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_empty_before_the_capture_fires() {
        let condition = FdmSnapshotCondition::new(1.5);

        assert!(condition.values().is_empty());
        assert_eq!(condition.time(), 1.5);
    }

    #[test]
    fn a_time_other_than_the_capture_time_records_nothing() {
        let condition = FdmSnapshotCondition::new(1.5);
        let mut values = Array::from([1.0, -2.5, 4.0]);
        let before = values.clone();

        condition.apply_to(&mut values, 1.25);

        assert_eq!(values, before);
        assert!(condition.values().is_empty());
    }

    #[test]
    fn the_capture_time_records_the_whole_grid_unchanged() {
        let condition = FdmSnapshotCondition::new(1.5);
        let mut values = Array::from([1.0, -2.5, 4.0]);
        let before = values.clone();

        condition.apply_to(&mut values, 1.5);

        assert_eq!(values, before);
        assert_eq!(condition.values(), before);
    }

    #[test]
    fn a_later_time_does_not_overwrite_the_capture() {
        let condition = FdmSnapshotCondition::new(1.5);
        let mut captured = Array::from([1.0, -2.5, 4.0]);
        let mut other = Array::from([9.0, 9.0, 9.0]);

        condition.apply_to(&mut captured, 1.5);
        condition.apply_to(&mut other, 2.0);

        assert_eq!(condition.values(), Array::from([1.0, -2.5, 4.0]));
    }
}
