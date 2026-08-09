//! The early-exercise condition of an American option.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/fdmamericanstepcondition.hpp:37`
//! and its `.cpp`.

use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::methods::finitedifferences::utilities::FdmInnerValueCalculator;
use crate::shared::Shared;
use crate::types::Time;

/// Lifts the rolled-back grid to the intrinsic value wherever holding is worth
/// less than exercising.
///
/// The condition is inert before `exercise_start`, which is what makes an
/// American window opening after the reference date price below one opening at
/// it (`cpp:37-38`).
pub struct FdmAmericanStepCondition {
    mesher: Shared<dyn FdmMesher>,
    calculator: Shared<dyn FdmInnerValueCalculator>,
    exercise_start: Time,
}

impl FdmAmericanStepCondition {
    /// The condition over `mesher`, reading its intrinsic values off
    /// `calculator` and applying from `exercise_start` on (`cpp:28-34`).
    pub fn new(
        mesher: Shared<dyn FdmMesher>,
        calculator: Shared<dyn FdmInnerValueCalculator>,
        exercise_start: Time,
    ) -> FdmAmericanStepCondition {
        FdmAmericanStepCondition {
            mesher,
            calculator,
            exercise_start,
        }
    }
}

impl StepCondition for FdmAmericanStepCondition {
    /// `cpp:36-49`: the elementwise maximum of the rolled-back values and the
    /// intrinsic value, taken at every grid point.
    ///
    /// The intrinsic value comes from
    /// [`inner_value`](FdmInnerValueCalculator::inner_value) rather than
    /// [`avg_inner_value`](FdmInnerValueCalculator::avg_inner_value)
    /// (`cpp:44`): the cell average seeds the terminal grid, the point value
    /// bounds it from below at every step.
    ///
    /// The shape check of `cpp:40-41` is an assertion rather than an `Err`
    /// because the trait returns unit, and a layout that disagrees with the
    /// array is a wiring bug in the solver rather than a market input.
    fn apply_to(&self, a: &mut Array, t: Time) {
        if t < self.exercise_start {
            return;
        }

        let layout = self.mesher.layout();
        assert_eq!(layout.size(), a.size(), "inconsistent array dimensions");

        for iter in layout.iter() {
            let inner_value = self.calculator.inner_value(&iter, t);
            if inner_value > a[iter.index()] {
                a[iter.index()] = inner_value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::{FdmLinearOpIterator, FdmLinearOpLayout};
    use crate::shared::shared;
    use crate::types::Real;

    /// An intrinsic value read straight out of a table indexed the way the
    /// layout indexes its points, so a test can state the payoff it wants
    /// without building one.
    struct TabulatedInnerValue {
        values: Vec<Real>,
    }

    impl FdmInnerValueCalculator for TabulatedInnerValue {
        fn inner_value(&self, iter: &FdmLinearOpIterator, _t: Time) -> Real {
            self.values[iter.index()]
        }

        fn avg_inner_value(&self, iter: &FdmLinearOpIterator, t: Time) -> Real {
            self.inner_value(iter, t)
        }
    }

    fn condition(values: Vec<Real>, exercise_start: Time) -> FdmAmericanStepCondition {
        let layout = shared(FdmLinearOpLayout::new(vec![values.len()]));
        let mesher = shared(UniformGridMesher::new(layout, &[(0.0, 1.0)]).unwrap());
        FdmAmericanStepCondition::new(
            mesher as Shared<dyn FdmMesher>,
            shared(TabulatedInnerValue { values }) as Shared<dyn FdmInnerValueCalculator>,
            exercise_start,
        )
    }

    #[test]
    fn values_below_the_intrinsic_value_are_lifted_and_the_rest_are_left_alone() {
        let condition = condition(vec![3.0, 1.0, 0.0], 0.0);
        let mut values = Array::from([2.0, 1.5, 0.0]);

        condition.apply_to(&mut values, 0.5);

        assert_eq!(values, Array::from([3.0, 1.5, 0.0]));
    }

    #[test]
    fn nothing_is_lifted_before_the_exercise_window_opens() {
        let condition = condition(vec![3.0, 1.0, 0.0], 0.25);
        let mut values = Array::from([2.0, 1.5, 0.0]);
        let before = values.clone();

        condition.apply_to(&mut values, 0.2);

        assert_eq!(values, before);
    }

    #[test]
    fn the_window_is_open_at_its_own_start() {
        let condition = condition(vec![3.0, 1.0, 0.0], 0.25);
        let mut values = Array::from([2.0, 1.5, 0.0]);

        condition.apply_to(&mut values, 0.25);

        assert_eq!(values, Array::from([3.0, 1.5, 0.0]));
    }
}
