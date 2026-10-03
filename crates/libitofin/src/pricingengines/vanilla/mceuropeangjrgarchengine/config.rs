use crate::errors::QlResult;
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::methods::montecarlo::DEFAULT_MIN_SAMPLES;
use crate::require;
use crate::types::{Real, Size};

pub(super) const MAX_STEPS: Size = 100_000;
const MAX_SAMPLES: Size = 1_000_000;
const MAX_WORK: Size = 100_000_000;
pub(super) const DEFAULT_MAX_SAMPLES: Size = 50_000;

pub(super) struct Config {
    pub steps: Option<Size>,
    pub steps_per_year: Option<Size>,
    pub samples: Option<Size>,
    pub tolerance: Option<Real>,
    pub max_samples: Option<Size>,
    pub antithetic: bool,
}

impl Config {
    pub fn validate<RNG: McRngTraits>(&mut self) -> QlResult<()> {
        require!(
            self.steps.is_some() != self.steps_per_year.is_some(),
            "exactly one of steps and steps per year must be given"
        );
        for n in [self.steps, self.steps_per_year].into_iter().flatten() {
            require!(n > 0 && n <= MAX_STEPS, "GJR-GARCH MC step limit exceeded");
        }
        require!(
            self.samples.is_some() != self.tolerance.is_some(),
            "exactly one of samples and tolerance must be given"
        );
        if let Some(tolerance) = self.tolerance {
            require!(
                RNG::ALLOWS_ERROR_ESTIMATE,
                "chosen random generator policy does not allow an error estimate"
            );
            require!(
                tolerance.is_finite() && tolerance > 0.0,
                "GJR-GARCH MC tolerance must be positive and finite"
            );
            self.max_samples = Some(self.max_samples.unwrap_or(DEFAULT_MAX_SAMPLES));
            require!(
                self.max_samples.unwrap_or(0) >= DEFAULT_MIN_SAMPLES,
                "GJR-GARCH MC maximum samples is below the initial tolerance batch"
            );
        }
        for n in [self.samples, self.max_samples].into_iter().flatten() {
            let minimum = if RNG::ALLOWS_ERROR_ESTIMATE { 2 } else { 1 };
            require!(
                n >= minimum && n <= MAX_SAMPLES,
                "GJR-GARCH MC samples outside supported range"
            );
            if let Some(limit) = RNG::MAX_SAMPLES {
                require!(n <= limit, "random generator sequence period exceeded");
            }
        }
        if let (Some(samples), Some(maximum)) = (self.samples, self.max_samples) {
            require!(
                samples <= maximum,
                "required samples exceed maximum samples"
            );
        }
        if let Some(steps) = self.steps {
            self.validate_work(steps)?;
        }
        Ok(())
    }

    pub fn validate_work(&self, steps: Size) -> QlResult<()> {
        require!(
            steps > 0 && steps <= MAX_STEPS,
            "GJR-GARCH MC step limit exceeded"
        );
        let samples = self.samples.or(self.max_samples).unwrap_or(0);
        let work = steps
            .checked_mul(2)
            .and_then(|n| n.checked_mul(samples))
            .and_then(|n| n.checked_mul(if self.antithetic { 2 } else { 1 }));
        require!(
            work.is_some_and(|n| n <= MAX_WORK),
            "GJR-GARCH MC work limit exceeded"
        );
        Ok(())
    }
}
