//! xoshiro256** uniform random number generator.
//!
//! Port of `ql/math/randomnumbers/xoshiro256starstaruniformrng.{hpp,cpp}`,
//! Blackman and Vigna's xoshiro256** 1.0 (https://prng.di.unimi.it/), with
//! the same SplitMix64 seed expansion.

use super::{Uint64Rng, UniformRng, seedgenerator};
use crate::types::Real;

struct SplitMix64 {
    x: u64,
}

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.x = self.x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.x;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Uniform random number generator with period `2^256 - 1`, the native
/// 64-bit source for the Ziggurat Gaussian transform.
#[derive(Clone)]
pub struct Xoshiro256StarStarUniformRng {
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
}

impl Xoshiro256StarStarUniformRng {
    /// A generator whose state is expanded from the given seed with
    /// SplitMix64; a seed of `0` draws a random seed from the
    /// [`seedgenerator`] singleton.
    pub fn new(seed: u64) -> Self {
        let seed = if seed != 0 {
            seed
        } else {
            u64::from(seedgenerator::get())
        };
        let mut split_mix = SplitMix64 { x: seed };
        Xoshiro256StarStarUniformRng {
            s0: split_mix.next(),
            s1: split_mix.next(),
            s2: split_mix.next(),
            s3: split_mix.next(),
        }
    }

    /// A generator with the given state, which must be chosen randomly and
    /// not everywhere zero; otherwise the first outputs are poorly
    /// distributed (all-zero state always returns `0`).
    pub fn from_state(s0: u64, s1: u64, s2: u64, s3: u64) -> Self {
        Xoshiro256StarStarUniformRng { s0, s1, s2, s3 }
    }
}

impl UniformRng for Xoshiro256StarStarUniformRng {
    fn next_real(&mut self) -> Real {
        ((self.next_u64() >> 11) as Real + 0.5) * (1.0 / (1u64 << 53) as Real)
    }
}

impl Uint64Rng for Xoshiro256StarStarUniformRng {
    fn next_u64(&mut self) -> u64 {
        let result = self.s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s1 << 17;
        self.s2 ^= self.s0;
        self.s3 ^= self.s1;
        self.s1 ^= self.s2;
        self.s0 ^= self.s3;
        self.s2 ^= t;
        self.s3 = self.s3.rotate_left(45);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 10108360646465513120;
    const S0: u64 = 18274946675476036270;
    const S1: u64 = 6043068446171522962;
    const S2: u64 = 96311065249897859;
    const S3: u64 = 16504445955133574805;

    #[test]
    fn seed_expansion_matches_quantlib() {
        let rng = Xoshiro256StarStarUniformRng::new(SEED);
        assert_eq!(
            (rng.s0, rng.s1, rng.s2, rng.s3),
            (S0, S1, S2, S3),
            "SplitMix64 state expansion diverged"
        );
    }

    #[test]
    fn matches_quantlib_sequence() {
        let mut from_seed = Xoshiro256StarStarUniformRng::new(SEED);
        let mut from_state = Xoshiro256StarStarUniformRng::from_state(S0, S1, S2, S3);
        let expected: [u64; 8] = [
            17514926931757914073,
            9264673906918214493,
            6472226376286417650,
            9410249674210661519,
            10338969804494286374,
            11659127091143561490,
            17821412726915611842,
            1118668267760426604,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(from_seed.next_u64(), e, "mismatch at index {i}");
        }
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(
                from_state.next_u64(),
                e,
                "constructor mismatch at index {i}"
            );
        }
        for i in expected.len()..1000 {
            assert_eq!(
                from_state.next_u64(),
                from_seed.next_u64(),
                "constructor mismatch at index {i}"
            );
        }
    }

    #[test]
    fn next_real_stays_in_the_open_unit_interval() {
        let mut rng = Xoshiro256StarStarUniformRng::new(1);
        for _ in 0..1000 {
            let x = rng.next_real();
            assert!(x > 0.0 && x < 1.0);
        }
    }
}
