//! Conditions applied to the grid values at every time step.
//!
//! Port of `ql/methods/finitedifferences/stepcondition.hpp:35,45`.

use crate::math::array::Array;
use crate::types::Time;

/// A condition the backward solver applies to the grid values after each step.
///
/// C++ templates this on `array_type` (`stepcondition.hpp:34`); the whole
/// finite-difference path instantiates it only with `Array`, so the Rust trait
/// takes the concrete [`Array`] (D10: concrete over generic at the boundary).
pub trait StepCondition {
    /// Applies the condition to the grid values `a` at time `t`.
    fn apply_to(&self, a: &mut Array, t: Time);
}

/// The step condition that does nothing.
///
/// Port of `NullCondition` (`stepcondition.hpp:45`).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullCondition;

impl StepCondition for NullCondition {
    fn apply_to(&self, _a: &mut Array, _t: Time) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_condition_leaves_the_array_unchanged() {
        let mut values = Array::from([1.0, -2.5, 0.0, 7.25]);
        let before = values.clone();

        NullCondition.apply_to(&mut values, 0.75);

        assert_eq!(values, before);
    }
}
