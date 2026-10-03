use std::marker::PhantomData;

use super::MCEuropeanGjrGarchEngine;
use crate::errors::QlResult;
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::processes::GjrGarchProcess;
use crate::shared::Shared;
use crate::types::{Real, Size};

/// Chainable factory for a checked, bounded GJR-GARCH European MC engine.
#[must_use = "a builder does nothing until build is called"]
pub struct MakeMcEuropeanGjrGarchEngine<RNG> {
    process: Shared<GjrGarchProcess>,
    steps: Option<Size>,
    steps_per_year: Option<Size>,
    samples: Option<Size>,
    max_samples: Option<Size>,
    tolerance: Option<Real>,
    antithetic: bool,
    seed: u32,
    _rng: PhantomData<RNG>,
}

impl<RNG: McRngTraits> MakeMcEuropeanGjrGarchEngine<RNG> {
    /// Starts a builder on the given GJR-GARCH process
    pub fn new(process: Shared<GjrGarchProcess>) -> MakeMcEuropeanGjrGarchEngine<RNG> {
        MakeMcEuropeanGjrGarchEngine {
            process,
            steps: None,
            steps_per_year: None,
            samples: None,
            max_samples: None,
            tolerance: None,
            antithetic: false,
            seed: 0,
            _rng: PhantomData,
        }
    }

    /// Sets the fixed number of time steps.
    pub fn with_steps(mut self, steps: Size) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Sets the number of time steps per year
    pub fn with_steps_per_year(mut self, steps: Size) -> Self {
        self.steps_per_year = Some(steps);
        self
    }

    /// Sets the required number of samples.
    pub fn with_samples(mut self, samples: Size) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Sets the required absolute tolerance.
    pub fn with_absolute_tolerance(mut self, tolerance: Real) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    /// Sets the maximum number of samples.
    pub fn with_max_samples(mut self, samples: Size) -> Self {
        self.max_samples = Some(samples);
        self
    }

    /// Sets the RNG seed.
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    /// Requests antithetic averaging with the negated Gaussian draw stream.
    pub fn with_antithetic_variate(mut self, antithetic: bool) -> Self {
        self.antithetic = antithetic;
        self
    }

    /// Builds the configured [`MCEuropeanGjrGarchEngine`].
    ///
    /// # Errors
    ///
    /// Rejects conflicting, nonfinite, zero or excessive simulation requests.
    /// Tolerance sampling requires an RNG policy with an error estimate.
    pub fn build(self) -> QlResult<MCEuropeanGjrGarchEngine<RNG>> {
        MCEuropeanGjrGarchEngine::new(
            self.process,
            self.steps,
            self.steps_per_year,
            self.antithetic,
            self.samples,
            self.tolerance,
            self.max_samples,
            self.seed,
        )
    }
}
