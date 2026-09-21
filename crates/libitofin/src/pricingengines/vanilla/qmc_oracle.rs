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
