//! Mersenne Twister uniform random number generator.
//!
//! Port of `ql/math/randomnumbers/mt19937uniformrng.{hpp,cpp}`, itself the
//! reference MT19937 implementation by Matsumoto and Nishimura (2002
//! initialization). The state words are `u32` with wrapping arithmetic where
//! the C code relies on `& 0xffffffff` masking; the sequences are identical.

use super::{UniformRng, seedgenerator};
use crate::types::Real;

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// Uniform random number generator with period `2^19937 - 1`.
#[derive(Clone)]
pub struct MersenneTwisterUniformRng {
    mt: [u32; N],
    mti: usize,
}

impl MersenneTwisterUniformRng {
    /// A generator initialized from the given seed; a seed of `0` draws a
    /// random seed from the [`seedgenerator`] singleton.
    pub fn new(seed: u32) -> Self {
        let s = if seed != 0 {
            seed
        } else {
            seedgenerator::get()
        };
        let mut mt = [0u32; N];
        mt[0] = s;
        for i in 1..N {
            mt[i] = 1_812_433_253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        MersenneTwisterUniformRng { mt, mti: N }
    }

    /// A generator initialized from an array of seeds, QuantLib's
    /// vector-of-seeds constructor (the reference `init_by_array`).
    ///
    /// # Panics
    ///
    /// Panics if `seeds` is empty (out-of-bounds access in QuantLib).
    pub fn from_seeds(seeds: &[u32]) -> Self {
        assert!(!seeds.is_empty(), "empty seed array");
        let mut rng = MersenneTwisterUniformRng::new(19_650_218);
        let mt = &mut rng.mt;
        let mut i = 1usize;
        let mut j = 0usize;
        for _ in 0..N.max(seeds.len()) {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(seeds[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            if j >= seeds.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
        }
        mt[0] = UPPER_MASK;
        rng.mti = N;
        rng
    }

    /// The next random integer, uniform over `[0, u32::MAX]`.
    pub fn next_u32(&mut self) -> u32 {
        if self.mti == N {
            self.twist();
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    fn twist(&mut self) {
        let mag01 = [0u32, MATRIX_A];
        for kk in 0..N - M {
            let y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
            self.mt[kk] = self.mt[kk + M] ^ (y >> 1) ^ mag01[(y & 0x1) as usize];
        }
        for kk in N - M..N - 1 {
            let y = (self.mt[kk] & UPPER_MASK) | (self.mt[kk + 1] & LOWER_MASK);
            self.mt[kk] = self.mt[kk + M - N] ^ (y >> 1) ^ mag01[(y & 0x1) as usize];
        }
        let y = (self.mt[N - 1] & UPPER_MASK) | (self.mt[0] & LOWER_MASK);
        self.mt[N - 1] = self.mt[M - 1] ^ (y >> 1) ^ mag01[(y & 0x1) as usize];
        self.mti = 0;
    }
}

impl UniformRng for MersenneTwisterUniformRng {
    fn next_real(&mut self) -> Real {
        (Real::from(self.next_u32()) + 0.5) / 4_294_967_296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_init_by_array_sequence() {
        let mut rng = MersenneTwisterUniformRng::from_seeds(&[0x123, 0x234, 0x345, 0x456]);
        let expected: [u32; 10] = [
            1067595299, 955945823, 477289528, 4107218783, 4228976476, 3344332714, 3355579695,
            227628506, 810200273, 2591290167,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(rng.next_u32(), e, "mismatch at index {i}");
        }
    }

    #[test]
    fn matches_reference_ten_thousandth_value_for_default_seed() {
        let mut rng = MersenneTwisterUniformRng::new(5489);
        for _ in 0..9999 {
            rng.next_u32();
        }
        assert_eq!(rng.next_u32(), 4123659995);
    }

    #[test]
    fn next_real_stays_in_the_open_unit_interval() {
        let mut rng = MersenneTwisterUniformRng::new(42);
        for _ in 0..1000 {
            let x = rng.next_real();
            assert!(x > 0.0 && x < 1.0);
        }
    }

    #[test]
    fn zero_seed_draws_distinct_random_seeds() {
        let mut a = MersenneTwisterUniformRng::new(0);
        let mut b = MersenneTwisterUniformRng::new(0);
        let same = (0..10).all(|_| a.next_u32() == b.next_u32());
        assert!(!same);
    }
}
