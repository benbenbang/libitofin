use super::tests::{flat, parameters, process, reference, result, set_arguments};
use super::*;
use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::instruments::{CashOrNothingPayoff, OptionArguments};
use crate::interestrate::Compounding;
use crate::math::randomnumbers::rngtraits::PseudoRandom;
use crate::processes::GjrGarchDiscretization;
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;

fn engine() -> MCEuropeanGjrGarchEngine<PseudoRandom> {
    let (process, _) = process(parameters(), GjrGarchDiscretization::FullTruncation);
    MakeMcEuropeanGjrGarchEngine::new(process)
        .with_steps(1)
        .with_samples(32)
        .with_seed(42)
        .build()
        .unwrap()
}

#[test]
fn missing_non_european_and_non_plain_arguments_are_rejected() {
    let mut engine = engine();
    assert!(engine.calculate().is_err());
    set_arguments(&mut engine);
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.payoff = None;
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("no payoff")
    );
    set_arguments(&mut engine);
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.exercise = Some(shared(
        AmericanExercise::new(reference(), reference() + 365, false).unwrap(),
    ) as Shared<dyn Exercise>);
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("European")
    );
    set_arguments(&mut engine);
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.payoff = Some(
        shared(CashOrNothingPayoff::new(OptionType::Call, 100.0, 10.0))
            as Shared<dyn StrikedTypePayoff>,
    );
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("non-plain")
    );
}

#[test]
fn invalid_strikes_discounts_and_nonpositive_exercise_time_fail() {
    for value in [-1.0, Real::NAN, Real::INFINITY] {
        assert!(EuropeanGjrGarchPathPricer::new(OptionType::Call, value, 1.0).is_err());
    }
    for value in [-1.0, 0.0, Real::NAN, Real::INFINITY] {
        assert!(EuropeanGjrGarchPathPricer::new(OptionType::Call, 100.0, value).is_err());
    }
    let mut engine = engine();
    set_arguments(&mut engine);
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.exercise = Some(shared(EuropeanExercise::new(reference())) as Shared<dyn Exercise>);
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("exercise time")
    );
}

#[test]
fn live_curve_updates_reprice_and_curve_failure_clears_previous_results() {
    let rate = shared(SimpleQuote::new(0.05));
    let curve = shared(FlatForward::new(
        reference(),
        Handle::new(rate.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>;
    let process = shared(
        GjrGarchProcess::new(
            Handle::new(curve),
            flat(0.02),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            parameters(),
            GjrGarchDiscretization::FullTruncation,
        )
        .unwrap(),
    );
    let mut engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps(2)
        .with_samples(1024)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    engine.calculate().unwrap();
    let original = result(&engine).instrument.value.unwrap();
    rate.set_value(0.10);
    engine.calculate().unwrap();
    assert!(result(&engine).instrument.value.unwrap() > original);
    rate.reset();
    assert!(engine.calculate().is_err());
    assert!(result(&engine).instrument.value.is_none());
    rate.set_value(0.05);
    engine.calculate().unwrap();
    assert_eq!(result(&engine).instrument.value.unwrap(), original);
}

#[test]
fn extreme_sample_moments_fail_without_publishing_nonfinite_results() {
    let (process, spot) = process(parameters(), GjrGarchDiscretization::FullTruncation);
    let mut engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps(1)
        .with_samples(32)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    spot.set_value(Real::MAX / 2.0);
    assert!(engine.calculate().is_err());
    assert!(result(&engine).instrument.value.is_none());
}

#[test]
fn path_pricer_uses_only_spot_and_rejects_nonfinite_payoff_at_accumulation_seam() {
    use crate::math::array::Array;
    use crate::methods::montecarlo::Path;
    let grid = TimeGrid::new(1.0, 1).unwrap();
    let path = MultiPath::from_paths(vec![
        Path::new(grid.clone(), Array::from([100.0, 120.0])).unwrap(),
        Path::new(grid.clone(), Array::from([Real::NAN, -1000.0])).unwrap(),
    ]);
    let pricer = EuropeanGjrGarchPathPricer::new(OptionType::Call, 100.0, 0.5).unwrap();
    assert_eq!(pricer.price(&path), 10.0);
    let extreme = MultiPath::from_paths(vec![
        Path::new(grid, Array::from([100.0, Real::MAX])).unwrap(),
    ]);
    let pricer = EuropeanGjrGarchPathPricer::new(OptionType::Call, 0.0, 2.0).unwrap();
    assert!(pricer.price(&extreme).is_nan());
    assert!(pricer.price(&MultiPath::from_paths(vec![])).is_nan());
}
