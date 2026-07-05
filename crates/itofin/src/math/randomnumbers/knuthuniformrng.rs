//! Knuth uniform random number generator.
//!
//! Port of `ql/math/randomnumbers/knuthuniformrng.{hpp,cpp}`, QuantLib's
//! wrapping of Knuth's lagged-Fibonacci `ranf_` routines (Seminumerical
//! Algorithms, 3rd edition, section 3.6). Note that QuantLib's copy predates
//! the 2002 revision of Knuth's code and omits its warm-up pass, so the
//! sequences match QuantLib, not Knuth's published self-test values.

#![allow(clippy::excessive_precision)]

use super::{UniformRng, seedgenerator};
use crate::types::Real;

const KK: usize = 100;
const LL: usize = 37;
const TT: usize = 70;
const QUALITY: usize = 1009;

/// Uniform random number generator by Knuth, producing deviates in `(0, 1)`.
#[derive(Clone)]
pub struct KnuthUniformRng {
    ranf_arr_buf: Vec<Real>,
    ranf_arr_ptr: usize,
    ranf_arr_sentinel: usize,
    ran_u: Vec<Real>,
}

fn mod_sum(x: Real, y: Real) -> Real {
    (x + y) - (x + y).trunc()
}

fn is_odd(s: i32) -> bool {
    (s & 1) != 0
}

impl KnuthUniformRng {
    /// A generator initialized from the given seed; a seed of `0` draws a
    /// random seed from the [`seedgenerator`] singleton.
    pub fn new(seed: i64) -> Self {
        let mut rng = KnuthUniformRng {
            ranf_arr_buf: vec![0.0; QUALITY],
            ranf_arr_ptr: QUALITY,
            ranf_arr_sentinel: QUALITY,
            ran_u: vec![0.0; QUALITY],
        };
        let seed = if seed != 0 {
            seed
        } else {
            i64::from(seedgenerator::get())
        };
        rng.ranf_start(seed);
        rng
    }

    fn ranf_start(&mut self, seed: i64) {
        let mut u = vec![0.0; KK + KK - 1];
        let mut ul = vec![0.0; KK + KK - 1];
        let ulp: Real = (1.0 / (1i64 << 30) as Real) / (1i64 << 22) as Real;
        let mut ss = 2.0 * ulp * (((seed & 0x3fff_ffff) + 2) as Real);

        for j in 0..KK {
            u[j] = ss;
            ul[j] = 0.0;
            ss += ss;
            if ss >= 1.0 {
                ss -= 1.0 - 2.0 * ulp;
            }
        }
        for j in KK..KK + KK - 1 {
            u[j] = 0.0;
            ul[j] = 0.0;
        }
        u[1] += ulp;
        ul[1] = ulp;
        let mut s = (seed & 0x3fff_ffff) as i32;
        let mut t = TT - 1;
        while t != 0 {
            for j in (1..KK).rev() {
                ul[j + j] = ul[j];
                u[j + j] = u[j];
            }
            let mut j = KK + KK - 2;
            while j > KK - LL {
                ul[KK + KK - 1 - j] = 0.0;
                u[KK + KK - 1 - j] = u[j] - ul[j];
                j -= 2;
            }
            for j in (KK..=KK + KK - 2).rev() {
                if ul[j] != 0.0 {
                    ul[j - (KK - LL)] = ulp - ul[j - (KK - LL)];
                    u[j - (KK - LL)] = mod_sum(u[j - (KK - LL)], u[j]);
                    ul[j - KK] = ulp - ul[j - KK];
                    u[j - KK] = mod_sum(u[j - KK], u[j]);
                }
            }
            if is_odd(s) {
                for j in (1..=KK).rev() {
                    ul[j] = ul[j - 1];
                    u[j] = u[j - 1];
                }
                ul[0] = ul[KK];
                u[0] = u[KK];
                if ul[KK] != 0.0 {
                    ul[LL] = ulp - ul[LL];
                    u[LL] = mod_sum(u[LL], u[KK]);
                }
            }
            if s != 0 {
                s >>= 1;
            } else {
                t -= 1;
            }
        }
        self.ran_u[KK - LL..KK].copy_from_slice(&u[..LL]);
        self.ran_u[..KK - LL].copy_from_slice(&u[LL..KK]);
    }

    fn ranf_array(&mut self, aa: &mut [Real], n: usize) {
        aa[..KK].copy_from_slice(&self.ran_u[..KK]);
        for j in KK..n {
            aa[j] = mod_sum(aa[j - KK], aa[j - LL]);
        }
        let mut j = n;
        for i in 0..LL {
            self.ran_u[i] = mod_sum(aa[j - KK], aa[j - LL]);
            j += 1;
        }
        for i in LL..KK {
            self.ran_u[i] = mod_sum(aa[j - KK], self.ran_u[i - LL]);
            j += 1;
        }
    }

    fn ranf_arr_cycle(&mut self) -> Real {
        let mut buf = std::mem::take(&mut self.ranf_arr_buf);
        self.ranf_array(&mut buf, QUALITY);
        self.ranf_arr_buf = buf;
        self.ranf_arr_ptr = 1;
        self.ranf_arr_sentinel = 100;
        self.ranf_arr_buf[0]
    }
}

impl UniformRng for KnuthUniformRng {
    fn next_real(&mut self) -> Real {
        if self.ranf_arr_ptr != self.ranf_arr_sentinel {
            let result = self.ranf_arr_buf[self.ranf_arr_ptr];
            self.ranf_arr_ptr += 1;
            result
        } else {
            self.ranf_arr_cycle()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_quantlib_ranf_array_state_evolution() {
        let mut rng = KnuthUniformRng::new(310952);
        let mut a = vec![0.0; 2009];
        for _ in 0..=2009 {
            rng.ranf_array(&mut a, 1009);
        }
        assert_eq!(rng.ran_u[0], 0.79844731758745512984);

        let mut rng = KnuthUniformRng::new(310952);
        let mut a = vec![0.0; 2009];
        for _ in 0..=1009 {
            rng.ranf_array(&mut a, 2009);
        }
        assert_eq!(rng.ran_u[0], 0.29783910367620269888);
    }

    #[test]
    fn matches_quantlib_sequence_for_seed_42() {
        let mut rng = KnuthUniformRng::new(42);
        let expected = [
            0.37353865951700893,
            0.5340107498451816,
            0.79241662590326234,
            0.68876261089152746,
            0.3916902075366393,
            0.34402138963306084,
            0.28510151224157809,
            0.61634074353985824,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(rng.next_real(), e, "mismatch at index {i}");
        }
    }

    #[test]
    fn deviates_stay_in_the_unit_interval() {
        let mut rng = KnuthUniformRng::new(1234);
        for _ in 0..2000 {
            let x = rng.next_real();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
