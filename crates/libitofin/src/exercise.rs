//! Option exercise classes.
//!
//! Port of the European and American subset of `ql/exercise.{hpp,cpp}`: the
//! [`Exercise`] trait is the base exercise contract, [`EuropeanExercise`] its
//! single-date implementation and [`AmericanExercise`] the continuous
//! `[earliest, latest]` one.
//!
//! Divergence: C++ puts the `payoffAtExpiry` flag on an intermediate
//! `EarlyExercise` base (`exercise.hpp:57-65`). Rust has no class hierarchy to
//! inherit it from, so the flag is a defaulted
//! [`payoff_at_expiry`](Exercise::payoff_at_expiry) on the trait itself,
//! reporting `false` for every exercise that is not an early one - which is
//! what the C++ engines see when their `dynamic_pointer_cast<EarlyExercise>`
//! is not attempted.
//!
//! Deferred, omitted visibly rather than accepted and ignored:
//! - **`BermudanExercise`** (`exercise.cpp:52-59`): the sorted multi-date form,
//!   with the Bermudan simulation grid it needs
//!   (`mclongstaffschwartzengine.hpp:218-231`).
//! - **the latest-date-only `AmericanExercise` constructor**
//!   (`exercise.cpp:43-50`): it opens the window at `Date::minDate()`, a
//!   sentinel this stack has no date for.

use crate::errors::QlResult;
use crate::require;
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

    /// Whether an exercise pays off at expiry rather than on the exercise date
    /// (the C++ `EarlyExercise::payoffAtExpiry`, `exercise.hpp:62`). Only an
    /// early exercise can set it, so the default is `false`.
    fn payoff_at_expiry(&self) -> bool {
        false
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

/// American exercise: the option can be exercised on any date between two
/// given dates (`exercise.hpp:72-80`).
///
/// The window is carried as the two dates `[earliest, latest]`
/// (`exercise.cpp:38-41`); a pricing engine reads the continuum between them
/// from its own model, not from this list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AmericanExercise {
    dates: [Date; 2],
    payoff_at_expiry: bool,
}

impl AmericanExercise {
    /// Builds an American exercise over `[earliest, latest]`
    /// (`exercise.cpp:33-41`). `payoff_at_expiry` defers the payment of an
    /// early exercise to the expiry date.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `earliest` is after `latest` (`exercise.cpp:36-37`).
    pub fn new(earliest: Date, latest: Date, payoff_at_expiry: bool) -> QlResult<AmericanExercise> {
        require!(earliest <= latest, "earliest > latest exercise date");
        Ok(AmericanExercise {
            dates: [earliest, latest],
            payoff_at_expiry,
        })
    }

    /// The window over `[earliest, latest]` paying on exercise, the C++
    /// default (`exercise.hpp:76`).
    ///
    /// # Errors
    ///
    /// As [`new`](AmericanExercise::new).
    pub fn over(earliest: Date, latest: Date) -> QlResult<AmericanExercise> {
        AmericanExercise::new(earliest, latest, false)
    }
}

impl Exercise for AmericanExercise {
    fn exercise_type(&self) -> ExerciseType {
        ExerciseType::American
    }

    fn dates(&self) -> &[Date] {
        &self.dates
    }

    fn payoff_at_expiry(&self) -> bool {
        self.payoff_at_expiry
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

    /// `exercise.cpp:38-41`: the window is exactly the two bounding dates, in
    /// that order, and `lastDate()` is the closing one.
    #[test]
    fn american_exercise_holds_the_window() {
        let earliest = Date::new(17, Month::May, 1998);
        let latest = Date::new(17, Month::May, 1999);
        let exercise = AmericanExercise::over(earliest, latest).unwrap();

        assert_eq!(exercise.exercise_type(), ExerciseType::American);
        assert_eq!(exercise.dates(), &[earliest, latest]);
        assert_eq!(exercise.last_date(), latest);
        assert!(!exercise.payoff_at_expiry(), "the C++ default is false");
    }

    /// A one-day window is degenerate but legal (`exercise.cpp:36` compares
    /// with `<=`), while an inverted one is not.
    #[test]
    fn an_inverted_window_is_rejected() {
        let earlier = Date::new(17, Month::May, 1998);
        let later = Date::new(17, Month::May, 1999);

        assert!(AmericanExercise::over(earlier, earlier).is_ok());
        match AmericanExercise::over(later, earlier) {
            Err(e) => assert_eq!(e.message(), "earliest > latest exercise date"),
            Ok(_) => panic!("earliest after latest must be rejected"),
        }
    }

    /// The `payoffAtExpiry` flag of `exercise.hpp:60-62` round trips, and it is
    /// the only exercise that can report `true`.
    #[test]
    fn payoff_at_expiry_round_trips_and_defaults_to_false() {
        let earliest = Date::new(17, Month::May, 1998);
        let latest = Date::new(17, Month::May, 1999);

        assert!(
            AmericanExercise::new(earliest, latest, true)
                .unwrap()
                .payoff_at_expiry()
        );
        assert!(!EuropeanExercise::new(latest).payoff_at_expiry());
    }

    #[test]
    fn usable_as_trait_object() {
        let expiry = Date::new(31, Month::December, 2030);
        let exercise: &dyn Exercise = &EuropeanExercise::new(expiry);
        assert_eq!(exercise.last_date(), expiry);
        assert_eq!(exercise.dates().len(), 1);
    }
}
