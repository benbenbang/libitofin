//! "Luxury" random number generator.
//!
//! Port of `ql/math/randomnumbers/ranluxuniformrng.{hpp}`, M. Luescher's
//! ranlux generator. QuantLib proxies the C++ standard library engines
//! (`std::subtract_with_carry_engine` over 48-bit words wrapped in a
//! `std::discard_block_engine`); Rust has no standard equivalent, so both
//! engines are implemented here following the C++ standard's specification,
//! including its linear-congruential seeding scheme.

#![allow(clippy::excessive_precision)]

use super::UniformRng;
use crate::types::Real;

const WORD_BITS: u32 = 48;
const SHORT_LAG: usize = 10;
const LONG_LAG: usize = 24;
const WORD_MASK: u64 = (1u64 << WORD_BITS) - 1;

/// `std::subtract_with_carry_engine<std::uint_fast64_t, 48, 10, 24>`, the
/// `ranlux64_base_01` engine underneath both luxury levels.
#[derive(Clone)]
struct SubtractWithCarry48 {
    x: [u64; LONG_LAG],
    carry: u64,
    k: usize,
}

impl SubtractWithCarry48 {
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 { 19_780_503 } else { seed };
        let mut lcg = (seed % 2_147_483_563).max(1);
        let mut next_lcg = || {
            lcg = (40014 * lcg) % 2_147_483_563;
            lcg
        };
        let mut x = [0u64; LONG_LAG];
        for word in &mut x {
            let lo = next_lcg();
            let hi = next_lcg();
            *word = (lo + (hi << 32)) & WORD_MASK;
        }
        let carry = u64::from(x[LONG_LAG - 1] == 0);
        SubtractWithCarry48 { x, carry, k: 0 }
    }

    fn next(&mut self) -> u64 {
        let short_index = (self.k + LONG_LAG - SHORT_LAG) % LONG_LAG;
        let mut y = self.x[short_index] as i64 - self.x[self.k] as i64 - self.carry as i64;
        if y < 0 {
            y += 1i64 << WORD_BITS;
            self.carry = 1;
        } else {
            self.carry = 0;
        }
        let y = y as u64;
        self.x[self.k] = y;
        self.k = (self.k + 1) % LONG_LAG;
        y
    }
}

/// Uniform random number generator based on M. Luescher's luxury levels: a
/// `std::discard_block_engine<ranlux64_base_01, P, R>` returning deviates in
/// `[0, 1)`.
///
/// Use the [`Ranlux3UniformRng`] and [`Ranlux4UniformRng`] aliases; `P` is
/// the block size, `R` the number of values used per block.
#[derive(Clone)]
pub struct Ranlux64UniformRng<const P: usize, const R: usize> {
    base: SubtractWithCarry48,
    used: usize,
}

/// Ranlux level 3: any theoretically possible correlations have a very small
/// chance of being observed.
pub type Ranlux3UniformRng = Ranlux64UniformRng<223, 24>;

/// Ranlux level 4: the highest possible luxury.
pub type Ranlux4UniformRng = Ranlux64UniformRng<389, 24>;

impl<const P: usize, const R: usize> Ranlux64UniformRng<P, R> {
    /// A generator initialized from the given seed. Seed `0` selects the
    /// standard engine's default seed `19780503` (also QuantLib's default),
    /// per the C++ standard's seeding scheme.
    pub fn new(seed: u64) -> Self {
        Ranlux64UniformRng {
            base: SubtractWithCarry48::new(seed),
            used: 0,
        }
    }

    fn next_u48(&mut self) -> u64 {
        if self.used >= R {
            for _ in 0..P - R {
                self.base.next();
            }
            self.used = 0;
        }
        self.used += 1;
        self.base.next()
    }
}

impl<const P: usize, const R: usize> Default for Ranlux64UniformRng<P, R> {
    fn default() -> Self {
        Ranlux64UniformRng::new(19_780_503)
    }
}

impl<const P: usize, const R: usize> UniformRng for Ranlux64UniformRng<P, R> {
    fn next_real(&mut self) -> Real {
        self.next_u48() as Real * (1.0 / (1u64 << WORD_BITS) as Real)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::comparison::close_enough;

    #[test]
    fn matches_quantlib_ranlux_sequences() {
        let mut ranlux3 = Ranlux3UniformRng::new(2_938_723);
        let mut ranlux4 = Ranlux4UniformRng::new(4_390_109);

        let ranlux3_expected = [
            0.307448851544538826,
            0.666313657894363587,
            0.698528013702823358,
            0.0217381272445322793,
            0.862964516238161394,
            0.909193419106014034,
            0.674484308686746914,
            0.849607570377191479,
            0.054626078713596371,
            0.416474163715683687,
        ];
        let ranlux4_expected = [
            0.222209169374078641,
            0.420181950405986271,
            0.0302156663005135329,
            0.0836259809475237148,
            0.480549766594993599,
            0.723472021829124401,
            0.905819507194266293,
            0.54072519936540786,
            0.445908421479817463,
            0.651084788437518824,
        ];

        for _ in 0..10010 {
            ranlux3.next_real();
            ranlux4.next_real();
        }
        for i in 0..10 {
            assert!(
                close_enough(ranlux3.next_real(), ranlux3_expected[i]),
                "ranlux3 mismatch at index {i}"
            );
            assert!(
                close_enough(ranlux4.next_real(), ranlux4_expected[i]),
                "ranlux4 mismatch at index {i}"
            );
        }
    }

    #[test]
    fn seed_zero_selects_default_seed() {
        let mut from_zero = Ranlux3UniformRng::new(0);
        let mut from_default = Ranlux3UniformRng::new(19_780_503);
        for i in 0..100 {
            assert_eq!(
                from_zero.next_u48(),
                from_default.next_u48(),
                "seed-0 sequence diverged from default seed at index {i}"
            );
        }
    }
}
