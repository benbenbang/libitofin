//! The early-exercise condition of a Bermudan option.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/fdmbermudanstepcondition.hpp:35`
//! and its `.cpp`.

use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::methods::finitedifferences::utilities::FdmInnerValueCalculator;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::Time;

/// Lifts the rolled-back grid to the intrinsic value, but only at the exercise
/// times.
///
/// This is what separates a Bermudan from an American: the American condition
/// fires on every step from its window opening on, this one only when the step
/// lands on an exercise date. The composite pushes
/// [`exercise_times`](FdmBermudanStepCondition::exercise_times) into the
/// solver's stopping times (`fdmstepconditioncomposite.cpp:139`) so that the
/// steps do land there.
pub struct FdmBermudanStepCondition {
    mesher: Shared<dyn FdmMesher>,
    calculator: Shared<dyn FdmInnerValueCalculator>,
    exercise_times: Vec<Time>,
}

impl FdmBermudanStepCondition {
    /// The condition over `mesher`, reading its intrinsic values off
    /// `calculator` and firing at each of `exercise_dates`, placed on the
    /// solver's clock by `day_counter` from `reference_date` (`cpp:27-39`).
    pub fn new(
        exercise_dates: &[Date],
        reference_date: Date,
        day_counter: &DayCounter,
        mesher: Shared<dyn FdmMesher>,
        calculator: Shared<dyn FdmInnerValueCalculator>,
    ) -> FdmBermudanStepCondition {
        let exercise_times = exercise_dates
            .iter()
            .map(|date| day_counter.year_fraction(reference_date, *date))
            .collect();

        FdmBermudanStepCondition {
            mesher,
            calculator,
            exercise_times,
        }
    }

    /// The exercise dates on the solver's clock (`cpp:41-43`).
    pub fn exercise_times(&self) -> &[Time] {
        &self.exercise_times
    }
}

impl StepCondition for FdmBermudanStepCondition {
    /// `cpp:45-65`: at an exercise time, the elementwise maximum of the
    /// rolled-back values and the intrinsic value; at any other time, nothing.
    ///
    /// The membership test is exact equality on `Time`, as in C++'s
    /// `std::find` (`cpp:46-47`), and that is faithful rather than fragile
    /// here: these very times reach the solver as stopping times, and the model
    /// applies the condition at the stopping time itself
    /// (`finitedifferencemodel.rs:110-119`), so the value compared is the bit
    /// pattern this constructor produced. A time that misses is a step that was
    /// never an exercise opportunity.
    ///
    /// The `locations` array of `cpp:52-58` is dropped: C++ fills it from the
    /// mesher on every grid point and then never reads it, so it is dead there
    /// and carrying it would only cost the port a per-node allocation.
    ///
    /// The shape check of `cpp:49-50` is an assertion rather than an `Err`
    /// because the trait returns unit, and a layout that disagrees with the
    /// array is a wiring bug in the solver rather than a market input.
    fn apply_to(&self, a: &mut Array, t: Time) {
        if !self.exercise_times.contains(&t) {
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
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
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

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn condition(values: Vec<Real>, exercise_dates: &[Date]) -> FdmBermudanStepCondition {
        let layout = shared(FdmLinearOpLayout::new(vec![values.len()]));
        let mesher = shared(UniformGridMesher::new(layout, &[(0.0, 1.0)]).unwrap());
        FdmBermudanStepCondition::new(
            exercise_dates,
            reference(),
            &Actual360::new(),
            mesher as Shared<dyn FdmMesher>,
            shared(TabulatedInnerValue { values }) as Shared<dyn FdmInnerValueCalculator>,
        )
    }

    #[test]
    fn the_dates_are_mapped_onto_the_day_counters_clock() {
        let condition = condition(vec![1.0], &[reference() + 90, reference() + 180]);

        assert_eq!(condition.exercise_times(), &[0.25, 0.5]);
    }

    #[test]
    fn values_below_the_intrinsic_value_are_lifted_at_an_exercise_time() {
        let condition = condition(vec![3.0, 1.0, 0.0], &[reference() + 180]);
        let mut values = Array::from([2.0, 1.5, 0.0]);

        condition.apply_to(&mut values, 0.5);

        assert_eq!(values, Array::from([3.0, 1.5, 0.0]));
    }

    /// The whole point of the condition: away from an exercise time it is inert,
    /// where the American one would lift here.
    #[test]
    fn nothing_is_lifted_between_the_exercise_times() {
        let condition = condition(vec![3.0, 1.0, 0.0], &[reference() + 180]);
        let mut values = Array::from([2.0, 1.5, 0.0]);
        let before = values.clone();

        condition.apply_to(&mut values, 0.4);

        assert_eq!(values, before);
    }

    #[test]
    fn every_exercise_time_fires() {
        let condition = condition(vec![3.0], &[reference() + 90, reference() + 180]);

        for time in [0.25, 0.5] {
            let mut values = Array::from([2.0]);
            condition.apply_to(&mut values, time);
            assert_eq!(values, Array::from([3.0]), "at {time}");
        }
    }
}
