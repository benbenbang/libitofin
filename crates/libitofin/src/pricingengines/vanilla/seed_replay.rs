use super::MakeMcEuropeanEngine;
use super::test_market::{Market, market, today};
use crate::exercise::EuropeanExercise;
use crate::instrument::Instrument;
use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
use crate::math::randomnumbers::PseudoRandom;
use crate::option::OptionType;
use crate::pricingengine::PricingEngine;
use crate::shared::{Shared, SharedMut, shared, shared_mut};

fn price(market: &Market, seed: u32) -> (u64, u64, f64) {
    let engine = MakeMcEuropeanEngine::<PseudoRandom>::new(Shared::clone(&market.process))
        .with_steps(4)
        .with_samples(512)
        .with_seed(seed)
        .build()
        .unwrap();
    let mut option = EuropeanOption::new(
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
        shared(EuropeanExercise::new(today() + 360)),
        Shared::clone(&market.settings),
    );
    option
        .base_mut()
        .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
    (
        option.npv().unwrap().to_bits(),
        option.error_estimate().unwrap().to_bits(),
        option.result::<f64>("seed").unwrap(),
    )
}

#[test]
fn a_random_vanilla_mc_run_reports_its_seed_and_replays_bit_for_bit() {
    let market = market();
    market.set(100.0, 0.02, 0.05, 0.20);
    let (npv, error, seed) = price(&market, 0);
    assert!(seed > 0.0 && seed <= f64::from(u32::MAX) && seed.fract() == 0.0);
    assert_eq!(price(&market, seed as u32), (npv, error, seed));
    assert_ne!(price(&market, 0).2, seed);
    assert_eq!(price(&market, 42).2, 42.0);
}
