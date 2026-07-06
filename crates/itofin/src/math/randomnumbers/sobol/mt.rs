//! Minimal Mersenne Twister (MT19937) used to seed Sobol direction integers
//! for dimensions beyond the tabulated initializers, ported from
//! `ql/math/randomnumbers/mt19937uniformrng.{hpp,cpp}`.
//!
//! This copy is private to the Sobol module; it will be replaced by the
//! public `math::randomnumbers` generator once QL-1.11 lands.

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

pub(super) struct MersenneTwister {
    mt: [u32; N],
    mti: usize,
}

impl MersenneTwister {
    /// Seeds the state like QuantLib's `seedInitialization`, except that a
    /// zero seed is used literally instead of drawing a clock-based seed.
    pub(super) fn new(seed: u64) -> Self {
        let mut mt = [0u32; N];
        mt[0] = seed as u32;
        for i in 1..N {
            mt[i] = 1_812_433_253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, mti: N }
    }

    fn twist(&mut self) {
        for kk in 0..N {
            let y = (self.mt[kk] & UPPER_MASK) | (self.mt[(kk + 1) % N] & LOWER_MASK);
            let mag = if y & 1 == 1 { MATRIX_A } else { 0 };
            self.mt[kk] = self.mt[(kk + M) % N] ^ (y >> 1) ^ mag;
        }
        self.mti = 0;
    }

    pub(super) fn next_u32(&mut self) -> u32 {
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

    /// Uniform draw in (0, 1), matching QuantLib's `nextReal`.
    pub(super) fn next_real(&mut self) -> f64 {
        (f64::from(self.next_u32()) + 0.5) / 4_294_967_296.0
    }
}
