//! Weighted Monte Carlo sample.
//!
//! Port of `ql/methods/montecarlo/sample.hpp`: a value paired with the weight of
//! the path that produced it. C++'s `value_type` typedef is dropped (the payload
//! type is simply the generic `T`).

use crate::types::Real;

/// A value of type `T` carrying the weight of the sampled path
/// (`sample.hpp:35`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample<T> {
    /// The sampled value.
    pub value: T,
    /// The weight of the path.
    pub weight: Real,
}

impl<T> Sample<T> {
    /// Builds a weighted sample, mirroring the C++ constructor
    /// (`sample.hpp:38`).
    pub fn new(value: T, weight: Real) -> Self {
        Sample { value, weight }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_value_and_weight() {
        let s = Sample::new(3.0, 0.5);
        assert_eq!(s.value, 3.0);
        assert_eq!(s.weight, 0.5);
    }

    #[test]
    fn carries_arbitrary_payloads() {
        let s = Sample::new(vec![1.0, 2.0], 1.0);
        assert_eq!(s.value, vec![1.0, 2.0]);
        assert_eq!(s.weight, 1.0);
    }
}
