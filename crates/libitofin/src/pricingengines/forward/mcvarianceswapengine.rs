//! Bounded pseudo-random Monte Carlo of annualized integrated local variance.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::instruments::{VarianceSwapArguments, VarianceSwapEngine, VarianceSwapResults};
use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
use crate::math::statistics::{MeanStdDev, Statistics};
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{McSimulation, PathGenerator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::position::Position;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::{fail, require};

mod config;
mod pricer;
use config::Config;
use pricer::VariancePathPricer;

/// Spot-start variance pricing using native integrated-diffusion path statistics.
/// Sampling error excludes discretization bias. Seed zero is randomized; other
/// seeds restart reproducibly on each calculation. No bridge or variance reduction.
pub struct MCVarianceSwapEngine {
    base: VarianceSwapEngine,
    process: Shared<GeneralizedBlackScholesProcess>,
    config: Config,
}

impl MCVarianceSwapEngine {
    /// Exactly one grid mode and one samples/annualized-variance-error mode.
    ///
    /// # Errors
    /// Rejects invalid or excessive budgets and seeds outside the 32-bit MT range.
    /// Caps are 100,000 steps, 1,000,000 samples and 100,000,000 total path work.
    /// Tolerance mode starts at 1023 samples and defaults to a 50,000 sample cap.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        steps: Option<usize>,
        steps_per_year: Option<usize>,
        samples: Option<usize>,
        tolerance: Option<f64>,
        max_samples: Option<usize>,
        seed: u64,
    ) -> QlResult<Self> {
        let Ok(seed) = u32::try_from(seed) else {
            fail!("variance MC seed exceeds 32-bit range");
        };
        let mut config = Config {
            steps,
            steps_per_year,
            samples,
            tolerance,
            max_samples,
            seed,
        };
        config.validate()?;
        let base = VarianceSwapEngine::new(
            VarianceSwapArguments::default(),
            VarianceSwapResults::default(),
        );
        base.register_with(process.observable());
        Ok(Self {
            base,
            process,
            config,
        })
    }

    fn market(&self) -> QlResult<(f64, f64)> {
        let arguments = self.base.arguments();
        arguments.validate()?;
        let Some((start, maturity)) = arguments.start_date.zip(arguments.maturity_date) else {
            fail!("variance swap dates missing");
        };
        let Some(settings) = &arguments.settings else {
            fail!("variance swap settings missing");
        };
        require!(
            settings.evaluation_date() == Some(start),
            "variance swap requires current spot start; forward/seasoned starts unsupported"
        );
        let risk_free = self.process.risk_free_rate().current_link()?;
        let dividend = self.process.dividend_yield().current_link()?;
        let volatility = self.process.black_volatility().current_link()?;
        require!(
            risk_free.reference_date()? == start
                && dividend.reference_date()? == start
                && volatility.reference_date()? == start,
            "variance swap start must match all market reference dates"
        );
        let time = self.process.time(&maturity)?;
        require!(
            time.is_finite() && time > 0.0,
            "variance MC time must be finite and positive"
        );
        for market_time in [
            volatility.time_from_reference(maturity)?,
            dividend.time_from_reference(maturity)?,
        ] {
            require!(
                market_time.is_finite() && market_time > 0.0,
                "invalid variance MC market time"
            );
        }
        let spot = self.process.x0()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "variance MC spot must be finite and positive"
        );
        let local = self.process.local_volatility()?.current_link()?;
        require!(
            local.reference_date()? == start,
            "variance MC local-vol reference date mismatch"
        );
        for discount in [
            risk_free.discount(time, false)?,
            dividend.discount_date(maturity, false)?,
        ] {
            require!(
                discount.is_finite() && discount > 0.0,
                "invalid variance MC discount"
            );
        }
        let discount = risk_free.discount_date(maturity, false)?;
        require!(
            discount.is_finite() && discount > 0.0,
            "invalid variance MC settlement discount"
        );
        Ok((time, discount))
    }
}

impl AsObservable for MCVarianceSwapEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for MCVarianceSwapEngine {
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
        self.base.results_mut().reset();
        let (time, discount) = self.market()?;
        let steps = self.config.time_steps(time)?;
        let grid = TimeGrid::new(time, steps)?;
        require!(
            grid.dt(0).is_finite()
                && grid.dt(0) > 0.0
                && grid.back().is_some_and(|v| v.is_finite() && v > 0.0),
            "invalid variance MC grid"
        );
        let generator = PseudoRandom::make_sequence_generator(steps, self.config.seed)?;
        let path_generator =
            PathGenerator::from_time_grid(self.process.clone(), grid, generator, false)?;
        let pricer = shared(VariancePathPricer {
            process: self.process.clone(),
            error: RefCell::new(None),
        });
        let mut simulation =
            McSimulation::<_, _, crate::math::statistics::GeneralStatistics>::new(false, false);
        let calculation = simulation.calculate(
            path_generator,
            pricer.clone(),
            self.config.tolerance,
            self.config.samples,
            self.config.max_samples,
        );
        if let Some(error) = pricer.error.borrow_mut().take() {
            return Err(error);
        }
        calculation?;
        let accumulator = simulation.sample_accumulator()?;
        let variance = accumulator.mean()?;
        let variance_error = simulation.error_estimate()?;
        let samples = accumulator.samples();
        require!(
            variance.is_finite()
                && variance >= 0.0
                && variance_error.is_finite()
                && variance_error >= 0.0,
            "invalid variance MC sampling results"
        );
        let arguments = self.base.arguments();
        let Some((position, (notional, strike))) = arguments
            .position
            .zip(arguments.notional.zip(arguments.strike))
        else {
            fail!("variance swap terms missing");
        };
        let sign = match position {
            Position::Long => 1.0,
            Position::Short => -1.0,
        };
        let multiplier = sign * discount * notional;
        let difference = variance - strike;
        let value = multiplier * difference;
        let error = multiplier * variance_error;
        require!(
            [multiplier, difference, value, error]
                .iter()
                .all(|v| v.is_finite()),
            "nonfinite variance MC NPV or error estimate"
        );
        let results = self.base.results_mut();
        results.variance = Some(variance);
        results.variance_error = Some(variance_error);
        results.samples = Some(samples);
        results.instrument.value = Some(value);
        results.instrument.error_estimate = Some(error);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod stochastic_tests;

#[cfg(test)]
mod native_cases;
#[cfg(test)]
mod native_tests;
