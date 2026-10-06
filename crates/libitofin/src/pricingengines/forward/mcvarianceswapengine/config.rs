use crate::errors::QlResult;
use crate::methods::montecarlo::DEFAULT_MIN_SAMPLES;
use crate::require;

pub(super) const MAX_STEPS: usize = 100_000;
const MAX_SAMPLES: usize = 1_000_000;
const MAX_WORK: usize = 100_000_000;

pub(super) struct Config {
    pub steps: Option<usize>,
    pub steps_per_year: Option<usize>,
    pub samples: Option<usize>,
    pub tolerance: Option<f64>,
    pub max_samples: Option<usize>,
    pub seed: u32,
}

impl Config {
    pub fn validate(&mut self) -> QlResult<()> {
        require!(
            self.steps.is_some() != self.steps_per_year.is_some(),
            "exactly one of steps and steps per year must be given"
        );
        for n in [self.steps, self.steps_per_year].into_iter().flatten() {
            require!(n > 0 && n <= MAX_STEPS, "variance MC step limit exceeded");
        }
        require!(
            self.samples.is_some() != self.tolerance.is_some(),
            "exactly one of samples and tolerance must be given"
        );
        if let Some(tolerance) = self.tolerance {
            require!(
                tolerance.is_finite() && tolerance > 0.0,
                "variance MC tolerance must be finite and positive"
            );
            self.max_samples = Some(self.max_samples.unwrap_or(50_000));
            require!(
                self.max_samples.unwrap_or(0) >= DEFAULT_MIN_SAMPLES,
                "variance MC maximum samples below initial batch"
            );
        }
        for n in [self.samples, self.max_samples].into_iter().flatten() {
            require!(
                (2..=MAX_SAMPLES).contains(&n),
                "variance MC samples outside supported range"
            );
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

    pub fn time_steps(&self, time: f64) -> QlResult<usize> {
        let steps = if let Some(steps) = self.steps {
            steps
        } else {
            let count = self.steps_per_year.unwrap_or(0) as f64 * time;
            require!(
                count.is_finite() && count < (MAX_STEPS + 1) as f64,
                "variance MC step limit exceeded"
            );
            (count as usize).max(1)
        };
        self.validate_work(steps)?;
        Ok(steps)
    }

    fn validate_work(&self, steps: usize) -> QlResult<()> {
        require!(
            steps > 0 && steps <= MAX_STEPS,
            "variance MC step limit exceeded"
        );
        let samples = self.samples.or(self.max_samples).unwrap_or(0);
        let work = steps
            .checked_mul(2)
            .and_then(|n| n.checked_add(2))
            .and_then(|n| n.checked_mul(samples));
        require!(
            work.is_some_and(|n| n <= MAX_WORK),
            "variance MC work limit exceeded"
        );
        Ok(())
    }
}
