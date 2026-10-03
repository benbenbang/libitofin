//! European plain-vanilla Monte Carlo pricing under the GJR-GARCH diffusion.
//!
//! Ports `ql/pricingengines/vanilla/mceuropeangjrgarchengine.hpp` using the
//! shared multivariate MC infrastructure and checked process evolution. The
//! supplied process, including its variance scheme, is sampled unchanged.
//! A model-derived process uses FullTruncation instead. This is a diffusion
//! approximation, not the discrete daily GJR-GARCH recursion.
//!
//! Unlike QuantLib's unbounded default, tolerance sampling defaults to at most
//! 50,000 samples. Requests are limited to 100,000 steps, 1,000,000 samples and
//! 100,000,000 factor-step evaluations, including antithetic partners. For the
//! pseudo-random policy, seed 0 retains the existing randomized-seed convention;
//! nonzero seeds reproduce the same draw stream. QMC accepts fixed samples and
//! provides no error estimate.

mod builder;
mod config;
#[cfg(test)]
mod domains;
#[cfg(test)]
mod matrix;
#[cfg(test)]
mod matrix_cases;
#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;

pub use builder::MakeMcEuropeanGjrGarchEngine;
use config::{Config, MAX_STEPS};

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{MultiPath, MultiVariate, PathPricer};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::payoff::Payoff;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::vanilla::McVanillaEngineBase;
use crate::processes::GjrGarchProcess;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess;
use crate::types::{DiscountFactor, Real, Size};
use crate::{fail, require};

/// Discounted terminal plain-vanilla payoff on asset zero, the spot leg.
///
/// The infallible [`PathPricer`] boundary returns NaN for an empty path or
/// overflowing payoff, so the shared checked statistics rejects that sample.
/// The engine propagates this as an error, never a nonfinite price result.
pub struct EuropeanGjrGarchPathPricer {
    payoff: PlainVanillaPayoff,
    discount: DiscountFactor,
}

impl EuropeanGjrGarchPathPricer {
    /// Constructs a terminal payoff pricer.
    ///
    /// # Errors
    /// Rejects a nonfinite/negative strike or nonpositive/nonfinite discount.
    pub fn new(option_type: OptionType, strike: Real, discount: DiscountFactor) -> QlResult<Self> {
        require!(
            strike.is_finite() && strike >= 0.0,
            "invalid GJR-GARCH MC strike"
        );
        require!(
            discount.is_finite() && discount > 0.0,
            "invalid GJR-GARCH MC discount"
        );
        Ok(Self {
            payoff: PlainVanillaPayoff::new(option_type, strike),
            discount,
        })
    }
}

impl PathPricer<MultiPath> for EuropeanGjrGarchPathPricer {
    fn price(&self, path: &MultiPath) -> Real {
        if path.asset_number() == 0 || path.path_size() == 0 {
            return Real::NAN;
        }
        let value = self.payoff.value(path[0].back()) * self.discount;
        if value.is_finite() { value } else { Real::NAN }
    }
}

/// Checked multivariate GJR-GARCH European MC engine, generic over RNG policy.
pub struct MCEuropeanGjrGarchEngine<RNG> {
    base: McVanillaEngineBase<RNG, MultiVariate>,
    process: Shared<GjrGarchProcess>,
    config: Config,
}

impl<RNG: McRngTraits> MCEuropeanGjrGarchEngine<RNG> {
    /// Constructs the engine. Prefer [`MakeMcEuropeanGjrGarchEngine`].
    ///
    /// # Errors
    /// Rejects conflicting, invalid or excessive simulation requests. Propagates
    /// the shared MC base construction failure.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: Shared<GjrGarchProcess>,
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        antithetic_variate: bool,
        required_samples: Option<Size>,
        required_tolerance: Option<Real>,
        max_samples: Option<Size>,
        seed: u32,
    ) -> QlResult<Self> {
        let mut config = Config {
            steps: time_steps,
            steps_per_year: time_steps_per_year,
            samples: required_samples,
            tolerance: required_tolerance,
            max_samples,
            antithetic: antithetic_variate,
        };
        config.validate::<RNG>()?;
        let base = McVanillaEngineBase::new(
            Shared::clone(&process) as Shared<dyn StochasticProcess>,
            time_steps,
            time_steps_per_year,
            false,
            antithetic_variate,
            false,
            required_samples,
            required_tolerance,
            config.max_samples,
            seed,
        )?;
        Ok(Self {
            base,
            process,
            config,
        })
    }

    /// The actual sampled process, preserving live inputs and selected scheme.
    pub fn process(&self) -> Shared<GjrGarchProcess> {
        Shared::clone(&self.process)
    }

    /// Builds the exercise grid after checking time and bounded work dimensions.
    ///
    /// # Errors
    /// Rejects missing/nonpositive exercise time, excessive work or invalid grids.
    pub fn time_grid(&self) -> QlResult<TimeGrid> {
        let Some(exercise) = &self.base.arguments().exercise else {
            fail!("no exercise given");
        };
        let time = self.process.time(&exercise.last_date())?;
        require!(
            time.is_finite() && time > 0.0,
            "invalid GJR-GARCH MC exercise time"
        );
        let steps = if let Some(steps) = self.config.steps {
            steps
        } else {
            let raw = self.config.steps_per_year.unwrap_or(0) as Real * time;
            require!(
                raw.is_finite() && raw <= MAX_STEPS as Real,
                "GJR-GARCH MC step limit exceeded"
            );
            (raw as Size).max(1)
        };
        self.config.validate_work(steps)?;
        let grid = TimeGrid::new(time, steps)?;
        require!(
            (0..steps).all(|i| grid.dt(i).is_finite() && grid.dt(i) > 0.0),
            "GJR-GARCH MC grid has an unrepresentable time step"
        );
        Ok(grid)
    }
}

impl<RNG: McRngTraits> AsObservable for MCEuropeanGjrGarchEngine<RNG> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<RNG: McRngTraits> PricingEngine for MCEuropeanGjrGarchEngine<RNG> {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }
    fn results(&self) -> &dyn Results {
        self.base.results()
    }
    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        self.base.reset();
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        require!(
            exercise.exercise_type() == ExerciseType::European,
            "not a European option"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff: &dyn StrikedTypePayoff = &**payoff;
        let Some(payoff) = (payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("non-plain payoff given");
        };
        let option_type = payoff.option_type();
        let strike = payoff.strike();
        let grid = self.time_grid()?;
        let Some(time) = grid.back() else {
            fail!("empty time grid");
        };
        let discount = self
            .process
            .risk_free_rate()
            .current_link()?
            .discount(time, false)?;
        let pricer = EuropeanGjrGarchPathPricer::new(option_type, strike, discount)?;
        let generator = self.base.path_generator_on_grid(grid, None)?;
        if let Err(error) = self.base.run_with(generator, pricer) {
            self.base.reset();
            return Err(error);
        }
        let results = &self.base.results_mut().instrument;
        if !results.value.is_some_and(|v| v.is_finite())
            || !results
                .error_estimate
                .is_none_or(|v| v.is_finite() && v >= 0.0)
        {
            self.base.reset();
            fail!("GJR-GARCH MC result is not finite");
        }
        Ok(())
    }
}
