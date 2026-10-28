use super::*;
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::math::distributions::poisson::InverseCumulativePoisson;
use crate::methods::montecarlo::Sample;

#[test]
fn quantlib_rngtraits_seed_oracles() {
    let mut gaussian = PseudoRandom::make_sequence_generator(100, 1234).unwrap();
    let mut scalar_normal = PseudoRandom::make_scalar_generator(1234);
    let values = gaussian.next_sequence().value.clone();
    for &value in &values {
        assert_eq!(scalar_normal.next_sample().unwrap().value, value);
    }
    let sum: f64 = values.iter().sum();
    assert!((sum - 4.09916).abs() <= 1e-5);
    for (lambda, expected) in [(1.0, 108.0), (4.0, 409.0)] {
        let mut sequence = PoissonPseudoRandom::with_lambda(100, 1234, lambda).unwrap();
        let mut scalar = PoissonPseudoRandom::make_scalar_generator(1234, lambda).unwrap();
        let sample = sequence.next_sequence().unwrap();
        assert_eq!(sample.value.iter().sum::<f64>(), expected);
        assert_eq!(sample.weight, 1.0);
        for &value in &sample.value {
            assert_eq!(scalar.next_sample().unwrap().value, value);
        }
        let mut copy = sequence.clone();
        assert_eq!(
            sequence.next_sequence().unwrap().value,
            copy.next_sequence().unwrap().value
        );
    }
    let mut default = PoissonPseudoRandom::make_sequence_generator(100, 1234).unwrap();
    assert_eq!(
        default.next_sequence().unwrap().value.iter().sum::<f64>(),
        108.0
    );
}

#[test]
fn sobol_policy_skips_zero_and_preserves_copied_state() {
    let mut sequence = LowDiscrepancy::make_sequence_generator(16, 42).unwrap();
    assert_eq!(sequence.next_sequence().value, vec![0.0; 16]);
    for _ in 0..4094 {
        assert!(sequence.next_sequence().value.iter().all(|x| x.is_finite()));
    }
    let mut copy = sequence.clone();
    assert_eq!(copy.next_sequence().value, sequence.next_sequence().value);
    for gray in [true, false] {
        let mut sobol =
            sobol::SobolRsg::with_gray_code(16, 42, sobol::DirectionIntegers::Jaeckel, gray);
        sobol.skip_to(4093);
        let mut a = SobolSequence::new(sobol.clone());
        assert_eq!(a.next_sequence().value, sobol.next_sequence());
        assert!(a.last_sequence().value.iter().all(|&x| x > 0.0 && x < 1.0));
    }
    assert!(LowDiscrepancy::make_sequence_generator(0, 0).is_err());
    assert!(LowDiscrepancy::make_sequence_generator(sobol::PPMT_MAX_DIM + 1, 0).is_err());
}

#[derive(Clone)]
struct Uniforms(Vec<f64>);
impl UniformRng for Uniforms {
    fn next_real(&mut self) -> f64 {
        self.0.remove(0)
    }
}

#[test]
fn scalar_transform_errors_recover_without_panics() {
    let mut normal = InverseCumulativeRng::new(
        Uniforms(vec![0.0, 1.0, f64::NAN, 0.5]),
        InverseCumulativeNormal::standard(),
    );
    for _ in 0..3 {
        assert!(normal.next_sample().is_err());
    }
    assert_eq!(normal.next_sample().unwrap().value, 0.0);
    let poisson = InverseCumulativePoisson::new(700.0).unwrap();
    assert!(poisson.try_evaluate(1.0).is_err());
    assert!(poisson.try_evaluate(-0.1).is_err());
    assert!(poisson.try_evaluate(f64::NAN).is_err());
    assert_eq!(poisson.try_evaluate(0.0).unwrap(), 0.0);
    assert!(PoissonPseudoRandom::with_lambda(1, 42, f64::INFINITY).is_err());
    assert!(PoissonPseudoRandom::with_lambda(0, 42, 1.0).is_err());
    assert!(PoissonPseudoRandom::with_lambda(1, 42, 750.0).is_err());
}

struct Weighted {
    sample: Sample<Vec<f64>>,
    next: Vec<f64>,
    next_weight: f64,
}
impl SequenceGenerator for Weighted {
    fn next_sequence(&mut self) -> &Sample<Vec<f64>> {
        std::mem::swap(&mut self.sample.value, &mut self.next);
        std::mem::swap(&mut self.sample.weight, &mut self.next_weight);
        &self.sample
    }
    fn last_sequence(&self) -> &Sample<Vec<f64>> {
        &self.sample
    }
    fn dimension(&self) -> usize {
        self.sample.value.len()
    }
}

#[test]
fn fallible_sequence_preserves_weights_and_last_success_on_error() {
    let source = Weighted {
        sample: Sample::new(vec![0.9, 1.0], 0.75),
        next: vec![0.25, 0.75],
        next_weight: 0.25,
    };
    let mut generator =
        FallibleInverseCumulativeRsg::new(source, InverseCumulativeNormal::standard());
    let first = generator.next_sequence().unwrap().clone();
    assert_eq!(first.weight, 0.25);
    assert!(first.value.iter().all(|x| x.is_finite() && *x != 0.0));
    assert!(generator.next_sequence().is_err());
    assert_eq!(generator.last_sequence().value, first.value);
    assert_eq!(generator.last_sequence().weight, first.weight);
    assert_eq!(generator.next_sequence().unwrap().value, first.value);
}

fn replays<S: Seeded, T: PartialEq + std::fmt::Debug>(
    seeded_with: impl Fn(u32) -> S,
    resolved: impl Fn(&S) -> u32,
    mut draw: impl FnMut(&mut S) -> T,
) {
    let mut random = seeded_with(0);
    let seed = resolved(&random);
    assert_ne!(seed, 0, "seed 0 resolves to a reported non-zero seed");
    let mut replay = seeded_with(seed);
    assert_eq!(resolved(&replay), seed);
    for _ in 0..1000 {
        assert_eq!(draw(&mut random), draw(&mut replay));
    }
}

#[test]
fn a_random_seed_is_reported_and_replays_every_generator_bit_for_bit() {
    let mt = |s: &MersenneTwisterUniformRng| s.seed().unwrap();
    replays(MersenneTwisterUniformRng::new, mt, |g| g.next_u32());
    replays(
        |s| KnuthUniformRng::new(i64::from(s)),
        |g| u32::try_from(g.seed()).unwrap(),
        |g| g.next_real().to_bits(),
    );
    replays(
        |s| Xoshiro256StarStarUniformRng::new(u64::from(s)),
        |g| u32::try_from(g.seed().unwrap()).unwrap(),
        |g| g.next_u64(),
    );
    replays(
        |s| BoxMullerGaussianRng::new(MersenneTwisterUniformRng::new(s)),
        |g| g.seed().unwrap(),
        |g| g.next_gaussian().to_bits(),
    );
    replays(
        |s| ZigguratGaussianRng::new(Xoshiro256StarStarUniformRng::new(u64::from(s))),
        |g| u32::try_from(g.seed().unwrap()).unwrap(),
        |g| g.next_gaussian().to_bits(),
    );
    replays(
        PseudoRandom::make_scalar_generator,
        |g| g.seed().unwrap(),
        |g| g.next_sample().unwrap().value.to_bits(),
    );
    replays(
        |s| PseudoRandom::make_sequence_generator(4, s).unwrap(),
        |g| g.seed().unwrap(),
        |g| g.next_sequence().value.clone(),
    );
    replays(
        |s| PoissonPseudoRandom::make_scalar_generator(s, 4.0).unwrap(),
        |g| g.seed().unwrap(),
        |g| g.next_sample().unwrap().value.to_bits(),
    );
    replays(
        |s| PoissonPseudoRandom::make_sequence_generator(4, s).unwrap(),
        |g| g.seed().unwrap(),
        |g| g.next_sequence().unwrap().value.clone(),
    );
}

#[test]
fn an_explicit_seed_is_reported_and_non_scalar_seeding_reports_none() {
    assert_eq!(MersenneTwisterUniformRng::new(42).seed(), Some(42));
    assert_eq!(KnuthUniformRng::new(-7).seed(), -7);
    assert_eq!(Xoshiro256StarStarUniformRng::new(42).seed(), Some(42));
    assert_eq!(
        RandomSequenceGenerator::with_seed(2, 42).unwrap().seed(),
        Some(42)
    );
    assert_eq!(MersenneTwisterUniformRng::from_seeds(&[1, 2]).seed(), None);
    assert_eq!(
        Xoshiro256StarStarUniformRng::from_state(1, 2, 3, 4).seed(),
        None
    );
}

#[test]
fn successive_random_seeds_differ() {
    assert_ne!(
        MersenneTwisterUniformRng::new(0).seed(),
        MersenneTwisterUniformRng::new(0).seed()
    );
}
