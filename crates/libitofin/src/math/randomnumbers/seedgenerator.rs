//! Random seed generator.
//!
//! Port of `ql/math/randomnumbers/seedgenerator.{hpp,cpp}`: a process-wide
//! Mersenne Twister seeded from the wall clock, used by the generators when
//! their seed argument is `0`. QuantLib exposes it as the `SeedGenerator`
//! singleton with a single `get()` method; we expose the free function
//! [`get`] over a lazily initialized, mutex-guarded instance instead.

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use super::mt19937uniformrng::MersenneTwisterUniformRng;

static INSTANCE: OnceLock<Mutex<MersenneTwisterUniformRng>> = OnceLock::new();

fn initialize() -> MersenneTwisterUniformRng {
    let first_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is set before the Unix epoch")
        .as_secs() as u32;
    let mut first = MersenneTwisterUniformRng::new(first_seed);

    let second_seed = first.next_u32();
    let mut second = MersenneTwisterUniformRng::new(second_seed);

    let skip = second.next_u32() % 1000;
    let init = [
        second.next_u32(),
        second.next_u32(),
        second.next_u32(),
        second.next_u32(),
    ];
    let mut rng = MersenneTwisterUniformRng::from_seeds(&init);
    for _ in 0..skip {
        rng.next_u32();
    }
    rng
}

/// The next seed from the process-wide seed generator.
pub fn get() -> u32 {
    INSTANCE
        .get_or_init(|| Mutex::new(initialize()))
        .lock()
        .expect("seed generator mutex poisoned")
        .next_u32()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successive_seeds_differ() {
        let seeds: Vec<u32> = (0..5).map(|_| get()).collect();
        assert!(seeds.windows(2).any(|w| w[0] != w[1]));
    }
}
