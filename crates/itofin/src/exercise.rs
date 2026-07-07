//! Option exercise classes.
//!
//! Port of the European subset of `ql/exercise.{hpp,cpp}`: the [`Exercise`]
//! trait is the base exercise contract and [`EuropeanExercise`] its
//! single-date implementation. `EarlyExercise`, `AmericanExercise` and
//! `BermudanExercise` are follow-up work.

use crate::time::date::Date;

/// Exercise style of an option (QuantLib's `Exercise::Type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExerciseType {
    /// Exercisable at any time between two predefined dates.
    American,
    /// Exercisable only at a set of fixed dates.
    Bermudan,
    /// Exercisable only at one (expiry) date.
    European,
}

/// Base exercise contract.
///
/// Implementors guarantee at least one exercise date (their constructors
/// enforce it), so [`last_date`](Exercise::last_date) is infallible where
/// QuantLib's `lastDate()` throws on an empty date vector.
pub trait Exercise {
    /// The exercise style.
    fn exercise_type(&self) -> ExerciseType;

    /// All exercise dates, in ascending order.
    fn dates(&self) -> &[Date];

    /// The last exercise date.
    fn last_date(&self) -> Date {
        *self
            .dates()
            .last()
            .expect("no exercise date given (implementors guarantee at least one)")
    }
}

/// European exercise: the option can only be exercised at one (expiry) date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EuropeanExercise {
    dates: [Date; 1],
}

impl EuropeanExercise {
    /// Builds a European exercise at the given expiry date.
    pub fn new(date: Date) -> EuropeanExercise {
        EuropeanExercise { dates: [date] }
    }
}

impl Exercise for EuropeanExercise {
    fn exercise_type(&self) -> ExerciseType {
        ExerciseType::European
    }

    fn dates(&self) -> &[Date] {
        &self.dates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn european_exercise_holds_the_single_expiry() {
        let expiry = Date::new(17, Month::May, 2027);
        let exercise = EuropeanExercise::new(expiry);
        assert_eq!(exercise.exercise_type(), ExerciseType::European);
        assert_eq!(exercise.dates(), &[expiry]);
        assert_eq!(exercise.last_date(), expiry);
    }

    #[test]
    fn usable_as_trait_object() {
        let expiry = Date::new(31, Month::December, 2030);
        let exercise: &dyn Exercise = &EuropeanExercise::new(expiry);
        assert_eq!(exercise.last_date(), expiry);
        assert_eq!(exercise.dates().len(), 1);
    }
}
