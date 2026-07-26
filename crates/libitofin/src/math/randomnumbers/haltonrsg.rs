//! Halton low-discrepancy sequence generator.
//!
//! Port of `ql/math/randomnumbers/haltonrsg.{hpp,cpp}`: successive draws are the
//! multi-dimensional Halton points, i.e. the van der Corput radical-inverse
//! sequence taken on a distinct prime base per dimension (base 2 for the first
//! dimension, 3 for the second, 5 for the third, and so on). See chapter 8,
//! paragraph 2 of Peter Jäckel, "Monte Carlo Methods in Finance".
//!
//! Divergences from `haltonrsg.cpp`:
//! - QuantLib's constructor takes `(dimensionality, seed, randomStart=true,
//!   randomShift=false)` and, when either flag is set, seeds a Mersenne twister
//!   to offset the sequence counter (`randomStart`, via
//!   `RandomSequenceGenerator::nextInt32Sequence`) and to add a uniform shift
//!   (`randomShift`). Both randomized modes are deferred here: `randomStart`
//!   depends on [`RandomSequenceGenerator::next_int32_sequence`], which is
//!   itself deferred on main, and #583 (the sole consumer) needs only a
//!   low-discrepancy restart sequence to escape local minima, which the
//!   deterministic sequence provides. [`HaltonRsg::new`] is therefore the
//!   `(randomStart=false, randomShift=false)` arm and does NOT reproduce
//!   QuantLib's default-constructed stream (whose default `randomStart=true`).
//! - with `randomShift` dropped, QuantLib's trailing `value -= long(value)` is a
//!   no-op (the radical inverse is already in `[0, 1)`) and is omitted.
//! - `PrimeNumbers::get` (a globally cached prime table) is replaced by
//!   [`first_primes`], which computes one prime base per dimension at
//!   construction.
//! - the `Sample<vector<Real>>` return type is reduced to `&[Real]`, following
//!   [`SobolRsg`](super::sobol::SobolRsg): the sample weight is always `1.0`,
//!   so the module drops the wrapper (see the module docs).

use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// Halton low-discrepancy sequence generator over `dimensionality` dimensions.
pub struct HaltonRsg {
    dimensionality: usize,
    sequence_counter: u64,
    bases: Vec<u64>,
    sequence: Vec<Real>,
}

impl HaltonRsg {
    /// A generator of `dimensionality`-wide deterministic Halton points.
    ///
    /// # Errors
    ///
    /// Returns an error if `dimensionality` is zero (`haltonrsg.cpp:44`).
    pub fn new(dimensionality: usize) -> QlResult<Self> {
        require!(dimensionality > 0, "dimensionality must be greater than 0");
        Ok(HaltonRsg {
            dimensionality,
            sequence_counter: 0,
            bases: first_primes(dimensionality),
            sequence: vec![0.0; dimensionality],
        })
    }

    /// The next Halton point.
    pub fn next_sequence(&mut self) -> &[Real] {
        self.sequence_counter += 1;
        for i in 0..self.dimensionality {
            let b = self.bases[i];
            let mut h = 0.0;
            let mut f = 1.0;
            let mut k = self.sequence_counter;
            while k != 0 {
                f /= b as Real;
                h += (k % b) as Real * f;
                k /= b;
            }
            self.sequence[i] = h;
        }
        &self.sequence
    }

    /// The most recently generated Halton point.
    pub fn last_sequence(&self) -> &[Real] {
        &self.sequence
    }

    /// The dimensionality of the generated points.
    pub fn dimension(&self) -> usize {
        self.dimensionality
    }
}

/// The first `n` prime numbers in ascending order (`2, 3, 5, 7, ...`).
fn first_primes(n: usize) -> Vec<u64> {
    let mut primes = Vec::with_capacity(n);
    let mut candidate = 1u64;
    while primes.len() < n {
        candidate += 1;
        if is_prime(candidate) {
            primes.push(candidate);
        }
    }
    primes
}

/// Whether `n` is prime, by trial division.
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2u64;
    while d * d <= n {
        if n.is_multiple_of(d) {
            return false;
        }
        d += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAN_DER_CORPUT_MOD_TWO: [Real; 32] = [
        0.50000, 0.25000, 0.75000, 0.12500, 0.62500, 0.37500, 0.87500, 0.06250, 0.56250, 0.31250,
        0.81250, 0.18750, 0.68750, 0.43750, 0.93750, 0.03125, 0.53125, 0.28125, 0.78125, 0.15625,
        0.65625, 0.40625, 0.90625, 0.09375, 0.59375, 0.34375, 0.84375, 0.21875, 0.71875, 0.46875,
        0.96875, 0.0,
    ];

    fn van_der_corput_mod_three() -> [Real; 26] {
        [
            1.0 / 3.0,
            2.0 / 3.0,
            1.0 / 9.0,
            4.0 / 9.0,
            7.0 / 9.0,
            2.0 / 9.0,
            5.0 / 9.0,
            8.0 / 9.0,
            1.0 / 27.0,
            10.0 / 27.0,
            19.0 / 27.0,
            4.0 / 27.0,
            13.0 / 27.0,
            22.0 / 27.0,
            7.0 / 27.0,
            16.0 / 27.0,
            25.0 / 27.0,
            2.0 / 27.0,
            11.0 / 27.0,
            20.0 / 27.0,
            5.0 / 27.0,
            14.0 / 27.0,
            23.0 / 27.0,
            8.0 / 27.0,
            17.0 / 27.0,
            26.0 / 27.0,
        ]
    }

    const TOLERANCE: Real = 1.0e-15;

    #[test]
    fn first_primes_are_the_van_der_corput_bases() {
        assert_eq!(first_primes(5), vec![2, 3, 5, 7, 11]);
    }

    #[test]
    fn rejects_zero_dimensionality() {
        assert!(HaltonRsg::new(0).is_err());
    }

    #[test]
    fn dimension_and_sequence_length_match() {
        let mut rsg = HaltonRsg::new(7).unwrap();
        assert_eq!(rsg.dimension(), 7);
        assert_eq!(rsg.next_sequence().len(), 7);
        assert_eq!(rsg.last_sequence().len(), 7);
    }

    #[test]
    fn first_dimension_is_van_der_corput_modulo_two() {
        let mut rsg = HaltonRsg::new(1).unwrap();
        for expected in VAN_DER_CORPUT_MOD_TWO.iter().take(31) {
            let drawn = rsg.next_sequence()[0];
            assert!(
                (drawn - expected).abs() <= TOLERANCE,
                "{drawn} vs {expected}"
            );
        }
    }

    #[test]
    fn second_dimension_is_van_der_corput_modulo_three() {
        let mut rsg = HaltonRsg::new(2).unwrap();
        let mod_three = van_der_corput_mod_three();
        for i in 0..26 {
            let point = rsg.next_sequence().to_vec();
            assert!(
                (point[0] - VAN_DER_CORPUT_MOD_TWO[i]).abs() <= TOLERANCE,
                "dim0 draw {}",
                i + 1
            );
            assert!(
                (point[1] - mod_three[i]).abs() <= TOLERANCE,
                "dim1 draw {}",
                i + 1
            );
        }
    }

    #[test]
    fn third_dimension_is_van_der_corput_modulo_five() {
        let mut rsg = HaltonRsg::new(3).unwrap();
        let expected = [1.0 / 5.0, 2.0 / 5.0, 3.0 / 5.0, 4.0 / 5.0, 1.0 / 25.0];
        for want in expected {
            let drawn = rsg.next_sequence()[2];
            assert!((drawn - want).abs() <= TOLERANCE, "{drawn} vs {want}");
        }
    }
}
