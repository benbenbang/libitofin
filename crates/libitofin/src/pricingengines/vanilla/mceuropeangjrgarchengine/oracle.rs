//! Independent QuantLib 1.43 MC values from `sdk/testdata/gjrgarch-model-mc.json`.
//! The tolerance sample count is independently identified by native fixed-count replay.

use super::tests::{parameters, result};
use super::*;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::instruments::OptionArguments;
use crate::interestrate::Compounding;
use crate::math::randomnumbers::rngtraits::PseudoRandom;
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters};
use crate::quotes::make_quote_handle;
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(15, Month::January, 2026)
}

fn native_process(
    params: GjrGarchParameters,
    scheme: GjrGarchDiscretization,
) -> Shared<GjrGarchProcess> {
    let curve = |r| {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            r,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    shared(
        GjrGarchProcess::new(
            curve(0.03),
            curve(0.01),
            make_quote_handle(100.0).handle(),
            params,
            scheme,
        )
        .unwrap(),
    )
}

fn arguments(engine: &mut MCEuropeanGjrGarchEngine<PseudoRandom>, days: i32, kind: OptionType) {
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.exercise = Some(shared(EuropeanExercise::new(reference() + days)) as Shared<dyn Exercise>);
    args.payoff =
        Some(shared(PlainVanillaPayoff::new(kind, 100.0)) as Shared<dyn StrikedTypePayoff>);
}

#[test]
fn seeded_native_prices_and_errors_preserve_all_three_process_schemes() {
    let rows = [
        (
            GjrGarchDiscretization::PartialTruncation,
            OptionType::Call,
            false,
            3.572206680094478,
            0.1328629977339088,
        ),
        (
            GjrGarchDiscretization::PartialTruncation,
            OptionType::Put,
            false,
            3.735814655627582,
            0.13074308412142557,
        ),
        (
            GjrGarchDiscretization::PartialTruncation,
            OptionType::Call,
            true,
            3.7194782912203133,
            0.08293455900966888,
        ),
        (
            GjrGarchDiscretization::PartialTruncation,
            OptionType::Put,
            true,
            3.6167937775496353,
            0.07692502229097962,
        ),
        (
            GjrGarchDiscretization::FullTruncation,
            OptionType::Call,
            false,
            3.57220751431153,
            0.1328629927802038,
        ),
        (
            GjrGarchDiscretization::FullTruncation,
            OptionType::Put,
            false,
            3.735814655627582,
            0.13074308412142557,
        ),
        (
            GjrGarchDiscretization::FullTruncation,
            OptionType::Call,
            true,
            3.7194822860840895,
            0.08293459726774713,
        ),
        (
            GjrGarchDiscretization::FullTruncation,
            OptionType::Put,
            true,
            3.6167930613577863,
            0.07692503246404672,
        ),
        (
            GjrGarchDiscretization::Reflection,
            OptionType::Call,
            false,
            3.884892840856974,
            0.14498983462806309,
        ),
        (
            GjrGarchDiscretization::Reflection,
            OptionType::Put,
            false,
            4.073996361284169,
            0.13685334467116847,
        ),
        (
            GjrGarchDiscretization::Reflection,
            OptionType::Call,
            true,
            4.024609001698537,
            0.0841233780095511,
        ),
        (
            GjrGarchDiscretization::Reflection,
            OptionType::Put,
            true,
            3.9368383385172816,
            0.07427644681162955,
        ),
    ];
    let params = GjrGarchParameters {
        v0: 0.002,
        omega: 2e-6,
        alpha: 0.3,
        beta: 0.4,
        gamma: 0.9,
        lambda: -0.4,
        ..parameters()
    };
    for (scheme, kind, antithetic, price, error) in rows {
        let process = native_process(params, scheme);
        let mut engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process.clone())
            .with_steps(4)
            .with_samples(2048)
            .with_seed(42)
            .with_antithetic_variate(antithetic)
            .build()
            .unwrap();
        assert_eq!(engine.process().discretization(), scheme);
        assert_eq!(engine.process().parameters(), params);
        assert!(Shared::ptr_eq(&process, &engine.process()));
        arguments(&mut engine, 7, kind);
        engine.calculate().unwrap();
        let actual = result(&engine).instrument.value.unwrap();
        let actual_error = result(&engine).instrument.error_estimate.unwrap();
        assert!(
            (actual - price).abs() < 1e-10,
            "{scheme:?}/{kind:?}/anti={antithetic}: {actual} vs {price}"
        );
        assert!(
            (actual_error - error).abs() < 1e-12,
            "{scheme:?}/{kind:?}/anti={antithetic}: SE {actual_error} vs {error}"
        );
    }
}

#[test]
fn native_tolerance_price_and_error_match_independent_fixed_sample_replay() {
    let params = GjrGarchParameters {
        omega: 2e-6,
        alpha: 0.04,
        beta: 0.88,
        gamma: 0.08,
        lambda: -0.4,
        ..parameters()
    };
    let process = native_process(params, GjrGarchDiscretization::FullTruncation);
    let mut tolerance = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process.clone())
        .with_steps(8)
        .with_absolute_tolerance(0.04)
        .with_max_samples(100000)
        .with_seed(1234)
        .build()
        .unwrap();
    let mut fixed = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps(8)
        .with_samples(12095)
        .with_seed(1234)
        .build()
        .unwrap();
    for engine in [&mut tolerance, &mut fixed] {
        arguments(engine, 90, OptionType::Call);
        engine.calculate().unwrap();
        assert!((result(engine).instrument.value.unwrap() - 2.9423120714228577).abs() < 1e-10);
        assert!(
            (result(engine).instrument.error_estimate.unwrap() - 0.03921333473521664).abs() < 1e-12
        );
    }
    assert_eq!(
        result(&tolerance).instrument.value,
        result(&fixed).instrument.value
    );
    assert_eq!(
        result(&tolerance).instrument.error_estimate,
        result(&fixed).instrument.error_estimate
    );
}
