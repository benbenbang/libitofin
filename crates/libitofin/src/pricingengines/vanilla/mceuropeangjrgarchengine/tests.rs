use std::any::Any;

use super::*;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{OneAssetOptionResults, OptionArguments, VanillaOption};
use crate::interestrate::Compounding;
use crate::math::randomnumbers::rngtraits::{LowDiscrepancy, PseudoRandom, SequenceGenerator};
use crate::pricingengines::blackcalculator::BlackCalculator;
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{shared, shared_mut};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;

pub(super) fn reference() -> Date {
    Date::new(4, Month::January, 2021)
}

pub(super) fn parameters() -> GjrGarchParameters {
    GjrGarchParameters {
        v0: 0.04 / 252.0,
        omega: 0.0,
        alpha: 0.0,
        beta: 1.0,
        gamma: 0.0,
        lambda: 0.0,
        days_per_year: 252.0,
    }
}

pub(super) fn flat(rate: Real) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

pub(super) fn process(
    params: GjrGarchParameters,
    scheme: GjrGarchDiscretization,
) -> (Shared<GjrGarchProcess>, Shared<SimpleQuote>) {
    let spot = shared(SimpleQuote::new(100.0));
    let process = shared(
        GjrGarchProcess::new(
            flat(0.05),
            flat(0.02),
            Handle::new(spot.clone() as Shared<dyn Quote>),
            params,
            scheme,
        )
        .unwrap(),
    );
    (process, spot)
}

pub(super) fn set_arguments<RNG: McRngTraits>(engine: &mut MCEuropeanGjrGarchEngine<RNG>) {
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.payoff =
        Some(shared(PlainVanillaPayoff::new(OptionType::Call, 100.0))
            as Shared<dyn StrikedTypePayoff>);
    args.exercise = Some(shared(EuropeanExercise::new(reference() + 365)) as Shared<dyn Exercise>);
}

pub(super) fn result<RNG: McRngTraits>(
    engine: &MCEuropeanGjrGarchEngine<RNG>,
) -> &OneAssetOptionResults {
    (engine.results() as &dyn Any)
        .downcast_ref::<OneAssetOptionResults>()
        .unwrap()
}

fn maker() -> MakeMcEuropeanGjrGarchEngine<PseudoRandom> {
    MakeMcEuropeanGjrGarchEngine::new(
        process(parameters(), GjrGarchDiscretization::FullTruncation).0,
    )
}

#[test]
fn invalid_configurations_are_rejected_without_simulating() {
    for builder in [
        maker().with_samples(100),
        maker().with_steps(0).with_samples(100),
        maker()
            .with_steps(1)
            .with_steps_per_year(1)
            .with_samples(100),
        maker().with_steps(1),
        maker().with_steps(1).with_samples(1),
        maker()
            .with_steps(1)
            .with_samples(100)
            .with_absolute_tolerance(0.1),
        maker().with_steps(1).with_samples(100).with_max_samples(99),
        maker().with_steps(1).with_absolute_tolerance(0.0),
        maker().with_steps(1).with_absolute_tolerance(Real::NAN),
        maker()
            .with_steps(1)
            .with_absolute_tolerance(Real::INFINITY),
        maker()
            .with_steps(1)
            .with_absolute_tolerance(0.1)
            .with_max_samples(1022),
        maker().with_steps(usize::MAX).with_samples(100),
        maker().with_steps(1).with_samples(usize::MAX),
        maker().with_steps(100_000).with_samples(501),
    ] {
        assert!(builder.build().is_err());
    }
}

#[test]
fn seeded_price_and_error_match_independent_constant_variance_draws() {
    let mut engine = maker()
        .with_steps(1)
        .with_samples(4096)
        .with_seed(1234)
        .with_antithetic_variate(true)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    engine.calculate().unwrap();
    let mut draws = PseudoRandom::make_sequence_generator(2, 1234).unwrap();
    let values: Vec<Real> = (0..4096)
        .map(|_| {
            let z = draws.next_sequence().value[0];
            let terminal = |z: Real| 100.0 * (0.01 + 0.2 * z).exp();
            0.5 * (-0.05_f64).exp()
                * ((terminal(z) - 100.0).max(0.0) + (terminal(-z) - 100.0).max(0.0))
        })
        .collect();
    let mean = values.iter().sum::<Real>() / values.len() as Real;
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<Real>() / (values.len() - 1) as Real;
    let expected_error = (variance / values.len() as Real).sqrt();
    let actual = result(&engine).instrument.value.unwrap();
    let error = result(&engine).instrument.error_estimate.unwrap();
    assert!((actual - mean).abs() < 1e-12, "{actual} vs {mean}");
    assert!(
        (error - expected_error).abs() < 1e-12,
        "{error} vs {expected_error}"
    );
    engine.calculate().unwrap();
    assert_eq!(result(&engine).instrument.value.unwrap(), actual);
    assert_eq!(result(&engine).instrument.error_estimate.unwrap(), error);
}

#[test]
fn constant_variance_limit_agrees_with_black_with_sampling_only_band() {
    let mut engine = maker()
        .with_steps(4)
        .with_samples(30_000)
        .with_seed(42)
        .with_antithetic_variate(true)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    engine.calculate().unwrap();
    let analytic = BlackCalculator::new(
        OptionType::Call,
        100.0,
        100.0 * 0.03_f64.exp(),
        0.2,
        (-0.05_f64).exp(),
    )
    .unwrap()
    .value();
    let price = result(&engine).instrument.value.unwrap();
    let error = result(&engine).instrument.error_estimate.unwrap();
    assert!(
        (price - analytic).abs() < 4.0 * error,
        "MC {price}, Black {analytic}, SE {error}"
    );
}

#[test]
fn tolerance_converges_or_reports_exhaustion() {
    let mut engine = maker()
        .with_steps(1)
        .with_absolute_tolerance(1.0)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    engine.calculate().unwrap();
    assert!(result(&engine).instrument.error_estimate.unwrap() <= 1.0);
    let mut engine = maker()
        .with_steps(1)
        .with_absolute_tolerance(1e-12)
        .with_max_samples(1023)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("max number of samples")
    );
    assert!(result(&engine).instrument.value.is_none());
}

#[test]
fn low_discrepancy_is_fixed_sample_only_and_has_no_error_estimate() {
    let (process, _) = process(parameters(), GjrGarchDiscretization::FullTruncation);
    assert!(
        MakeMcEuropeanGjrGarchEngine::<LowDiscrepancy>::new(process.clone())
            .with_steps(1)
            .with_absolute_tolerance(0.1)
            .build()
            .is_err()
    );
    let mut engine = MakeMcEuropeanGjrGarchEngine::<LowDiscrepancy>::new(process)
        .with_steps(4)
        .with_samples(4095)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    engine.calculate().unwrap();
    let first = result(&engine).instrument.value.unwrap();
    assert!(result(&engine).instrument.error_estimate.is_none());
    engine.calculate().unwrap();
    assert_eq!(result(&engine).instrument.value.unwrap(), first);
}

#[test]
fn attached_option_reprices_live_spot_and_recovers_after_quote_failure() {
    let (process, spot) = process(parameters(), GjrGarchDiscretization::Reflection);
    let engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps(1)
        .with_samples(4096)
        .with_seed(42)
        .build()
        .unwrap();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let mut option = VanillaOption::new(
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)) as Shared<dyn StrikedTypePayoff>,
        shared(EuropeanExercise::new(reference() + 365)) as Shared<dyn Exercise>,
        settings,
    );
    option.base_mut().set_pricing_engine(shared_mut(engine));
    let original = option.npv().unwrap();
    spot.set_value(120.0);
    assert!(option.npv().unwrap() > original);
    spot.set_value(Real::NAN);
    assert!(option.npv().is_err());
    spot.set_value(100.0);
    assert_eq!(option.npv().unwrap(), original);
}

#[test]
fn process_evolution_failures_abort_and_clear_previous_results() {
    let params = GjrGarchParameters {
        v0: 1e100,
        ..parameters()
    };
    let (process, _) = process(params, GjrGarchDiscretization::PartialTruncation);
    let mut engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps(1)
        .with_samples(10)
        .with_seed(42)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    assert!(
        engine
            .calculate()
            .unwrap_err()
            .message()
            .contains("positive finite spot")
    );
    assert!(result(&engine).instrument.value.is_none());
}

#[test]
fn per_year_grid_work_limit_is_checked_before_allocation() {
    let mut engine = maker()
        .with_steps_per_year(100_000)
        .with_samples(501)
        .build()
        .unwrap();
    set_arguments(&mut engine);
    assert!(
        engine
            .time_grid()
            .unwrap_err()
            .message()
            .contains("work limit")
    );
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.exercise = Some(shared(EuropeanExercise::new(reference() + 730)) as Shared<dyn Exercise>);
    assert!(
        engine
            .time_grid()
            .unwrap_err()
            .message()
            .contains("step limit")
    );
}
