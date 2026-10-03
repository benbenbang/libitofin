//! QuantLib's original 36 cached MC cases, with separate native diffusion values.
//! `test-suite/gjrgarchmodel.cpp::testEngines` uses an absolute 0.15 cached-MC
//! band, not MC standard errors against the analytic approximation. The latter
//! can differ substantially for displaced innovations and is not an exact oracle.

use super::tests::result;
use super::*;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::instruments::OptionArguments;
use crate::interestrate::Compounding;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::math::randomnumbers::rngtraits::PseudoRandom;
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters};
use crate::quotes::make_quote_handle;
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounters::actualactual::{ActualActual, Convention};
use crate::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(15, Month::January, 2026)
}

#[test]
fn original_36_cached_mc_cases_keep_their_declared_absolute_band() {
    let curve = |rate| {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            ActualActual::with_convention(Convention::ISDA),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    for (lambda, days, strike, cached_mc, native_mc, native_error) in super::matrix_cases::CASES {
        let phi = (-lambda * lambda / 2.0_f64).exp() / (2.0 * std::f64::consts::PI).sqrt();
        let persistence = 0.93
            + (0.024 + 0.059 * CumulativeNormalDistribution::standard().value(lambda))
                * (1.0 + lambda * lambda)
            + 0.059 * lambda * phi;
        let params = GjrGarchParameters {
            v0: 2e-6 / (1.0 - persistence),
            omega: 2e-6,
            alpha: 0.024,
            beta: 0.93,
            gamma: 0.059,
            lambda,
            days_per_year: 365.0,
        };
        let process = shared(
            GjrGarchProcess::new(
                curve(0.05),
                curve(0.0),
                make_quote_handle(50.0).handle(),
                params,
                GjrGarchDiscretization::FullTruncation,
            )
            .unwrap(),
        );
        let mut engine = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
            .with_steps_per_year(20)
            .with_absolute_tolerance(0.02)
            .with_max_samples(1000000)
            .with_seed(1234)
            .build()
            .unwrap();
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.exercise =
            Some(shared(EuropeanExercise::new(reference() + days)) as Shared<dyn Exercise>);
        args.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Call, strike))
            as Shared<dyn StrikedTypePayoff>);
        engine.calculate().unwrap();
        let price = result(&engine).instrument.value.unwrap();
        let error = result(&engine).instrument.error_estimate.unwrap();
        assert!(
            (price - cached_mc).abs() <= 0.15,
            "lambda {lambda}, days {days}, K {strike}: {price} vs cached {cached_mc}"
        );
        assert!(
            (price - native_mc).abs() < 1e-9,
            "lambda {lambda}, days {days}, K {strike}: {price} vs native {native_mc}"
        );
        assert!(
            (error - native_error).abs() < 1e-11,
            "lambda {lambda}, days {days}, K {strike}: SE {error} vs native {native_error}"
        );
        assert!(error <= 0.02);
    }
}
