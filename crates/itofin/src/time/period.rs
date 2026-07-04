//! Length-plus-unit time spans.
//!
//! Partial port of `ql/time/period.hpp`, covering only what the calendar layer
//! needs: constructing a [`Period`] and reading back its length and unit. The
//! full QuantLib `Period` algebra (`Frequency` conversion, normalization, and
//! the arithmetic/comparison operators) belongs to the dedicated Date/Period
//! ticket and is intentionally deferred here.

use crate::time::timeunit::TimeUnit;
use crate::types::Integer;

/// A time span expressed as an integer `length` of a given [`TimeUnit`].
///
/// The length may be negative to express a span into the past.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Period {
    length: Integer,
    units: TimeUnit,
}

impl Period {
    /// Builds a period of `n` units.
    pub fn new(n: Integer, units: TimeUnit) -> Period {
        Period { length: n, units }
    }

    /// The (signed) number of units in the period.
    pub fn length(&self) -> Integer {
        self.length
    }

    /// The unit the period is measured in.
    pub fn units(&self) -> TimeUnit {
        self.units
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_length_and_units() {
        let p = Period::new(3, TimeUnit::Months);
        assert_eq!(p.length(), 3);
        assert_eq!(p.units(), TimeUnit::Months);
    }
}
