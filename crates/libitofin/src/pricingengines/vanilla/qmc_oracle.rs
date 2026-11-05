use super::MakeMcEuropeanEngine;
use super::test_market::{market, today};
use crate::exercise::EuropeanExercise;
use crate::instrument::Instrument;
use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
use crate::math::randomnumbers::LowDiscrepancy;
use crate::option::OptionType;
use crate::pricingengine::PricingEngine;
use crate::shared::{Shared, SharedMut, shared, shared_mut};

#[test]
fn quantlib_qmc_european_all_108_cases() {
    let market = market();
    let mut cases = 0;
    for kind in [OptionType::Call, OptionType::Put] {
        for strike in [75.0, 100.0, 125.0] {
            let engine =
                MakeMcEuropeanEngine::<LowDiscrepancy>::new(Shared::clone(&market.process))
                    .with_steps(1)
                    .with_samples(4095)
                    .build()
                    .unwrap();
            let mut option = EuropeanOption::new(
                shared(PlainVanillaPayoff::new(kind, strike)),
                shared(EuropeanExercise::new(today() + 360)),
                Shared::clone(&market.settings),
            );
            option
                .base_mut()
                .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
            for q in [0.0, 0.05] {
                for r in [0.01, 0.05, 0.15] {
                    for vol in [0.11, 0.50, 1.20] {
                        market.set(100.0, q, r, vol);
                        let expected = market.option(kind, strike, today() + 360).npv().unwrap();
                        let actual = option.npv().unwrap();
                        assert!(actual.is_finite());
                        assert!(
                            (actual - expected).abs() / 100.0 <= 0.01,
                            "{kind:?} K={strike} q={q} r={r} vol={vol}: {actual} vs {expected}"
                        );
                        assert!(option.error_estimate().is_err());
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 108);
}

#[test]
fn qmc_rejects_missing_samples_tolerances_and_zero_steps() {
    let market = market();
    let make = || MakeMcEuropeanEngine::<LowDiscrepancy>::new(Shared::clone(&market.process));
    assert!(make().with_steps(1).build().is_err());
    if let Some(too_many) = (u32::MAX as usize).checked_add(1) {
        assert!(make().with_steps(1).with_samples(too_many).build().is_err());
    }
    assert!(make().with_steps(1).with_samples(0).build().is_err());
    assert!(make().with_steps(0).with_samples(10).build().is_err());
    assert!(
        make()
            .with_steps_per_year(0)
            .with_samples(10)
            .build()
            .is_err()
    );
    assert!(
        make()
            .with_steps(1)
            .with_absolute_tolerance(0.01)
            .build()
            .is_err()
    );
    assert!(
        make()
            .with_steps(1)
            .with_samples(10)
            .with_max_samples(10)
            .build()
            .is_err()
    );
}

/// The Sobol engine with the bridge (#1362). `LowDiscrepancy` has no error
/// estimate, so the band is `testQmcEngines`' (`europeanoption.cpp:1291`). The
/// structural pin is what proves the bridge runs: under exact Black-Scholes
/// evolution the terminal value depends only on the sum of the scaled draws,
/// which the bridge reduces to `sqrt(T) * z[0]` (`brownianbridge.hpp:114`).
/// Sobol's first coordinate does not depend on the dimension or the seed, so a
/// bridged 16-step run must reprice the 1-step run, which an unbridged one does
/// not.
#[test]
fn qmc_bridge_reprices_the_one_step_run_from_the_first_sobol_dimension() {
    let market = market();
    market.set(100.0, 0.02, 0.05, 0.20);
    let expiry = today() + 360;
    let price = |steps: usize, brownian_bridge: bool, seed: u32| {
        let engine = MakeMcEuropeanEngine::<LowDiscrepancy>::new(Shared::clone(&market.process))
            .with_steps(steps)
            .with_samples(4095)
            .with_brownian_bridge(brownian_bridge)
            .with_seed(seed)
            .build()
            .unwrap();
        let mut option = EuropeanOption::new(
            shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
            shared(EuropeanExercise::new(expiry)),
            Shared::clone(&market.settings),
        );
        option
            .base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        option.npv().unwrap()
    };

    let analytic = market
        .option(OptionType::Call, 100.0, expiry)
        .npv()
        .unwrap();
    let one_step = price(1, false, 0);
    let bridged = price(16, true, 0);
    let unbridged = price(16, false, 0);

    assert!(
        (bridged - analytic).abs() / 100.0 <= 0.01,
        "{bridged} vs {analytic}"
    );
    assert!(
        (bridged - one_step).abs() <= 1e-9,
        "bridged {bridged} vs one-step {one_step}"
    );
    assert!(
        (unbridged - one_step).abs() > 1e-5,
        "unbridged {unbridged} must differ from one-step {one_step}"
    );
    assert!(
        (price(16, true, 7) - bridged).abs() <= 1e-9,
        "only the seed-free first dimension may drive the bridged terminal"
    );
}

/// Sobol draws its direction integers above the 31 tabulated Jaeckel dimensions
/// from the seed (`sobol/mod.rs:194`), so seed 0 must stay a fixed seed for QMC:
/// the NPV is pinned to its value before seed reporting, and the reported seed
/// is the given one. The pin is a tolerance, not bits, because libm differs
/// across platforms; a wrongly randomised QMC seed moves the NPV far beyond it.
#[test]
fn qmc_seed_zero_above_the_tabulated_sobol_dimension_is_unchanged_and_reported() {
    let market = market();
    market.set(100.0, 0.02, 0.05, 0.20);
    let price = |seed: u32| {
        let engine = MakeMcEuropeanEngine::<LowDiscrepancy>::new(Shared::clone(&market.process))
            .with_steps(40)
            .with_samples(255)
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
        (option.npv().unwrap(), option.result::<f64>("seed").unwrap())
    };
    let (npv, seed) = price(0);
    let golden = 8.350388866949363;
    assert!((npv - golden).abs() <= 1e-12 * golden, "{npv}");
    assert_eq!(seed, 0.0);
    assert_eq!(price(0).0.to_bits(), npv.to_bits());
    let (other, seed) = price(7);
    assert_ne!(other.to_bits(), npv.to_bits());
    assert_eq!(seed, 7.0);
}
