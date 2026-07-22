//! Monte Carlo simulation driver.
//!
//! Port of `ql/pricingengines/mcsimulation.hpp`: the convergence machinery a
//! Monte Carlo pricing engine drives. It owns the [`MonteCarloModel`] and
//! offers [`value`](McSimulation::value) (add samples until the error falls
//! below a tolerance, `mcsimulation.hpp:104`),
//! [`value_with_samples`](McSimulation::value_with_samples) (a fixed count,
//! `mcsimulation.hpp:142`), and [`calculate`](McSimulation::calculate) (build
//! the model, then dispatch, `mcsimulation.hpp:158`).
//!
//! Divergences from `mcsimulation.hpp`, all deliberate:
//! - **abstract hooks become explicit arguments**: C++ declares `pathPricer()`,
//!   `pathGenerator()`, and `timeGrid()` as pure-virtual hooks the derived
//!   engine overrides (`mcsimulation.hpp:72-75`). Rust composition (the engine
//!   *holds* a `McSimulation` rather than inheriting it) makes those overrides
//!   impossible, so [`calculate`](McSimulation::calculate) takes the built
//!   generator and pricer as parameters: the engine supplies exactly what the
//!   C++ overrides would have.
//! - **`Null` sentinels become [`Option`]**: C++ threads `Null<Real>()` /
//!   `Null<Size>()` for an unset tolerance or sample count
//!   (`mcsimulation.hpp:162`); Rust uses `Option` (D10).
//! - **`QL_MAX_INTEGER` default becomes [`Size::MAX`]**: the unbounded
//!   `maxSamples` default (`mcsimulation.hpp:55`) is expressed as `Size::MAX`.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **control variate** (`mcsimulation.hpp:167-188`): [`new`] keeps the
//!   `control_variate` flag but [`calculate`](McSimulation::calculate) returns
//!   `Err` when it is `true`; the CV model-construction branch is omitted.
//! - **antithetic variate**: threaded to [`MonteCarloModel::new`], which averages
//!   the forward path with its antithetic partner when the generator supports it
//!   (the multi-factor path); the single-factor generator's antithetic draw is a
//!   fail-loud `Err`.
//! - **`maxError` over a sequence** (`mcsimulation.hpp:89-95`): the multi-variate
//!   `max_element` reduction is dropped; the single-variate `result_type = Real`
//!   is its own error.
//!
//! [`new`]: McSimulation::new

use crate::errors::QlResult;
use crate::math::statistics::{GeneralStatistics, Statistics};
use crate::methods::montecarlo::{MonteCarloModel, PathGen, PathPricer};
use crate::types::{Real, Size};
use crate::{fail, require};

/// The C++ default `minSamples` for the tolerance loop (`mcsimulation.hpp:56`).
pub const DEFAULT_MIN_SAMPLES: Size = 1023;

/// Owns the Monte Carlo model and runs the sampling loops.
///
/// `S` defaults to [`GeneralStatistics`], QuantLib's default `Statistics` tool.
pub struct McSimulation<PG, P, S = GeneralStatistics> {
    model: Option<MonteCarloModel<PG, P, S>>,
    antithetic_variate: bool,
    control_variate: bool,
}

impl<PG, P, S> McSimulation<PG, P, S>
where
    PG: PathGen,
    P: PathPricer<PG::PathType>,
    S: Statistics + Default,
{
    /// A simulation with no model yet built (`mcsimulation.hpp:68`); call
    /// [`calculate`](McSimulation::calculate) to populate it.
    pub fn new(antithetic_variate: bool, control_variate: bool) -> Self {
        McSimulation {
            model: None,
            antithetic_variate,
            control_variate,
        }
    }

    /// Adds samples until the mean's error estimate falls at or below
    /// `tolerance`, or `max_samples` is exhausted (`mcsimulation.hpp:104`).
    ///
    /// # Errors
    ///
    /// Errors if the model is not built, if `max_samples` is reached while the
    /// error is still above `tolerance`, or on an accumulation failure.
    pub fn value(
        &mut self,
        tolerance: Real,
        max_samples: Size,
        min_samples: Size,
    ) -> QlResult<Real> {
        let Some(model) = self.model.as_mut() else {
            fail!("Monte Carlo model not initialized");
        };

        let mut sample_number = model.sample_accumulator().samples();
        if sample_number < min_samples {
            model.add_samples(min_samples - sample_number)?;
            sample_number = model.sample_accumulator().samples();
        }

        let mut error = model.sample_accumulator().error_estimate()?;
        while error > tolerance {
            require!(
                sample_number < max_samples,
                "max number of samples ({max_samples}) reached, while error ({error}) is still above tolerance ({tolerance})"
            );

            let order = error * error / tolerance / tolerance;
            let ideal = sample_number as Real * order * 0.8 - sample_number as Real;
            let mut next_batch = ideal.max(min_samples as Real) as Size;
            next_batch = next_batch.min(max_samples - sample_number);

            sample_number += next_batch;
            model.add_samples(next_batch)?;
            error = model.sample_accumulator().error_estimate()?;
        }

        model.sample_accumulator().mean()
    }

    /// Simulates exactly `samples` paths and returns the mean
    /// (`mcsimulation.hpp:142`).
    ///
    /// # Errors
    ///
    /// Errors if the model is not built, if `samples` is fewer than already
    /// simulated, or on an accumulation failure.
    pub fn value_with_samples(&mut self, samples: Size) -> QlResult<Real> {
        let Some(model) = self.model.as_mut() else {
            fail!("Monte Carlo model not initialized");
        };

        let sample_number = model.sample_accumulator().samples();
        require!(
            samples >= sample_number,
            "number of already simulated samples ({sample_number}) greater than requested samples ({samples})"
        );

        model.add_samples(samples - sample_number)?;
        model.sample_accumulator().mean()
    }

    /// The error estimate on the samples simulated so far
    /// (`mcsimulation.hpp:210`).
    ///
    /// # Errors
    ///
    /// Errors if the model is not built or on fewer than two samples.
    pub fn error_estimate(&self) -> QlResult<Real> {
        let Some(model) = self.model.as_ref() else {
            fail!("Monte Carlo model not initialized");
        };
        model.sample_accumulator().error_estimate()
    }

    /// The sample accumulator, for the mean and richer statistics
    /// (`mcsimulation.hpp:62`).
    ///
    /// # Errors
    ///
    /// Errors if the model is not built.
    pub fn sample_accumulator(&self) -> QlResult<&S> {
        let Some(model) = self.model.as_ref() else {
            fail!("Monte Carlo model not initialized");
        };
        Ok(model.sample_accumulator())
    }

    /// Builds the model from the supplied generator and pricer, then adds
    /// samples per the tolerance or fixed-count request (`mcsimulation.hpp:158`).
    ///
    /// # Errors
    ///
    /// Errors if neither `required_tolerance` nor `required_samples` is set, if
    /// control variate is requested (deferred), or on an accumulation failure
    /// (including a single-factor antithetic draw, deferred).
    pub fn calculate(
        &mut self,
        path_generator: PG,
        path_pricer: P,
        required_tolerance: Option<Real>,
        required_samples: Option<Size>,
        max_samples: Option<Size>,
    ) -> QlResult<()> {
        require!(
            required_tolerance.is_some() || required_samples.is_some(),
            "neither tolerance nor number of samples set"
        );
        require!(!self.control_variate, "control variate not supported");

        self.model = Some(MonteCarloModel::new(
            path_generator,
            path_pricer,
            S::default(),
            self.antithetic_variate,
        )?);

        match required_tolerance {
            Some(tolerance) => {
                self.value(
                    tolerance,
                    max_samples.unwrap_or(Size::MAX),
                    DEFAULT_MIN_SAMPLES,
                )?;
            }
            None => {
                let samples = required_samples.expect("checked above");
                self.value_with_samples(samples)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::interestrate::Compounding;
    use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
    use crate::math::statistics::MeanStdDev;
    use crate::methods::montecarlo::{Path, PathGenerator};
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
    use crate::stochasticprocess::StochasticProcess1D;
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Volatility};

    const SPOT: Real = 100.0;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const VOL: Volatility = 0.20;
    const STEPS: Size = 4;

    type Rsg = <PseudoRandom as McRngTraits>::RsgType;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn gbs_process() -> Shared<dyn StochasticProcess1D> {
        let spot = make_quote_handle(SPOT);
        let vol = RelinkableHandle::new(shared(BlackConstantVol::new(
            reference(),
            Some(Target::new()),
            VOL,
            Actual360::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(BlackScholesMertonProcess::new(
            spot.handle(),
            flat_yield(Q),
            flat_yield(R),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>
    }

    fn path_generator(seed: u32) -> PathGenerator<Rsg> {
        let generator = PseudoRandom::make_sequence_generator(STEPS, seed).unwrap();
        PathGenerator::new(gbs_process(), 1.0, STEPS, generator, false).unwrap()
    }

    fn terminal(path: &Path) -> Real {
        path.back()
    }

    #[test]
    fn value_with_samples_equals_the_accumulator_mean() {
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, false);
        sim.calculate(path_generator(42), terminal, None, Some(5_000), None)
            .unwrap();

        assert_eq!(sim.sample_accumulator().unwrap().samples(), 5_000);
        let mean = sim.sample_accumulator().unwrap().mean().unwrap();
        assert_eq!(sim.value_with_samples(5_000).unwrap(), mean);
    }

    #[test]
    fn value_with_fewer_samples_than_simulated_is_rejected() {
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, false);
        sim.calculate(path_generator(42), terminal, None, Some(5_000), None)
            .unwrap();
        assert!(sim.value_with_samples(4_999).is_err());
    }

    #[test]
    fn tolerance_path_converges_below_the_tolerance() {
        const TOL: Real = 1.0;
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, false);
        sim.calculate(path_generator(42), terminal, Some(TOL), None, Some(100_000))
            .unwrap();
        assert!(sim.error_estimate().unwrap() <= TOL);
    }

    #[test]
    fn tolerance_path_errors_when_max_samples_is_exhausted() {
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, false);
        let result = sim.calculate(path_generator(42), terminal, Some(1e-4), None, Some(1_500));
        assert!(result.is_err());
    }

    #[test]
    fn calculate_without_tolerance_or_samples_is_rejected() {
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, false);
        let result = sim.calculate(path_generator(42), terminal, None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn control_variate_is_rejected_as_deferred() {
        let mut sim = McSimulation::<PathGenerator<Rsg>, fn(&Path) -> Real>::new(false, true);
        match sim.calculate(path_generator(42), terminal, None, Some(2_000), None) {
            Err(e) => assert!(e.message().contains("control variate")),
            Ok(()) => panic!("control_variate = true must be rejected as deferred"),
        }
    }
}
