use crate::{Bounds, Common, InvalidInput, Problem};

/// Shared controls for finite-box population solvers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlobalOptions {
    /// Reproducible SplitMix64 seed, including deterministic zero.
    pub seed: u64,
    /// Actual population count, not a dimension multiplier.
    pub population_size: Option<usize>,
    /// Explicit physical points, evaluated unchanged in their supplied order.
    pub initial_population: Option<Vec<Vec<f64>>>,
    /// Maximum free-coordinate population spread normalized by box width.
    /// Unset falls back to [`Common::tol`] and then `1e-6`.
    pub xatol: Option<f64>,
    /// Maximum absolute objective-value spread.
    /// Unset falls back to [`Common::tol`] and then `1e-8`.
    pub fatol: Option<f64>,
}

impl GlobalOptions {
    pub(crate) fn population_count(&self, bounds: &Bounds) -> usize {
        self.population_size.unwrap_or_else(|| {
            self.initial_population.as_ref().map_or_else(
                || {
                    let free = bounds
                        .lower
                        .iter()
                        .zip(&bounds.upper)
                        .filter(|(lower, upper)| lower < upper)
                        .count();
                    (15 * free).max(8)
                },
                Vec::len,
            )
        })
    }

    pub(crate) fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        problem.validate()?;
        range("dimension", problem.x0.len(), 1, 256)?;
        let bounds = problem
            .bounds
            .as_ref()
            .ok_or(InvalidInput::MissingGlobalBounds)?;
        for (index, (&lower, &upper)) in bounds.lower.iter().zip(&bounds.upper).enumerate() {
            if !lower.is_finite() || !upper.is_finite() || !(upper - lower).is_finite() {
                return Err(InvalidInput::NonfiniteGlobalBound { index });
            }
            if problem.x0[index] < lower || problem.x0[index] > upper {
                return Err(InvalidInput::OutsideGlobalBounds {
                    option: "x0",
                    point: 0,
                    index,
                });
            }
        }
        for (option, tolerance) in [("xatol", self.xatol), ("fatol", self.fatol)] {
            if tolerance.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(InvalidInput::NotFiniteNonnegative { option });
            }
        }
        let size = self.population_count(bounds);
        range("population_size", size, 4, 4096)?;
        if size
            .checked_mul(problem.x0.len())
            .is_none_or(|cells| cells > 1_000_000)
        {
            return Err(InvalidInput::PopulationCells { max: 1_000_000 });
        }
        if let Some(points) = &self.initial_population {
            if points.len() != size {
                return Err(InvalidInput::PopulationPointCount {
                    expected: size,
                    found: points.len(),
                });
            }
            for (point, x) in points.iter().enumerate() {
                if x.len() != problem.x0.len() {
                    return Err(InvalidInput::PopulationPointLength {
                        point,
                        expected: problem.x0.len(),
                        found: x.len(),
                    });
                }
                for (index, &value) in x.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(InvalidInput::NonfinitePopulation { point, index });
                    }
                    if value < bounds.lower[index] || value > bounds.upper[index] {
                        return Err(InvalidInput::OutsideGlobalBounds {
                            option: "initial_population",
                            point,
                            index,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// Serial deferred-generation DE/rand/1/bin options.
#[derive(Debug, Clone, PartialEq)]
pub struct DifferentialEvolutionOptions {
    /// Shared finite-box population controls.
    pub global: GlobalOptions,
    /// Differential weight in `(0, 2]`, default `0.8`.
    pub mutation: f64,
    /// Binomial crossover probability in `[0, 1]`, default `0.9`.
    pub recombination: f64,
}

impl Default for DifferentialEvolutionOptions {
    fn default() -> Self {
        Self {
            global: GlobalOptions::default(),
            mutation: 0.8,
            recombination: 0.9,
        }
    }
}

impl DifferentialEvolutionOptions {
    /// Validates the problem, finite-box population controls and coefficients.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for an invalid shape, bound, point or option.
    pub fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        self.global.validate(problem)?;
        for (option, value, valid, interval) in [
            (
                "mutation",
                self.mutation,
                self.mutation > 0.0 && self.mutation <= 2.0,
                "(0, 2]",
            ),
            (
                "recombination",
                self.recombination,
                (0.0..=1.0).contains(&self.recombination),
                "[0, 1]",
            ),
        ] {
            if !value.is_finite() || !valid {
                return Err(InvalidInput::DifferentialEvolutionCoefficient {
                    option,
                    range: interval,
                });
            }
        }
        Ok(())
    }

    /// Validates shared budgets and supplies bounded global-solver defaults.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for invalid shared options or work caps.
    pub fn budgets(&self, common: &Common) -> Result<Common, InvalidInput> {
        common.validate()?;
        let maxiter = common.maxiter.unwrap_or(1000);
        let maxfev = common.maxfev.unwrap_or(1_000_000);
        range("maxiter", maxiter, 1, 1_000_000)?;
        range("maxfev", maxfev, 1, 10_000_000)?;
        Ok(Common {
            maxiter: Some(maxiter),
            maxfev: Some(maxfev),
            tol: common.tol,
        })
    }
}

/// Serial synchronous global-best particle swarm with finite box bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSwarmOptions {
    /// Shared finite-box population controls.
    pub global: GlobalOptions,
    /// Previous-velocity weight in `[0, 1]`, default `0.7`.
    pub inertia: f64,
    /// Personal-best attraction in `[0, 4]`, default `1.4`.
    pub cognitive: f64,
    /// Global-best attraction in `[0, 4]`, default `1.4`.
    pub social: f64,
    /// Absolute normalized velocity limit in `(0, 1]`, default `0.2`.
    pub velocity_clamp: f64,
}

impl Default for ParticleSwarmOptions {
    fn default() -> Self {
        Self {
            global: GlobalOptions::default(),
            inertia: 0.7,
            cognitive: 1.4,
            social: 1.4,
            velocity_clamp: 0.2,
        }
    }
}

impl ParticleSwarmOptions {
    /// Validates the finite-box population and particle-swarm coefficients.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for an invalid shape, bound, point or option.
    pub fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        self.global.validate(problem)?;
        for (option, value, valid, interval) in [
            (
                "inertia",
                self.inertia,
                (0.0..=1.0).contains(&self.inertia),
                "[0, 1]",
            ),
            (
                "cognitive",
                self.cognitive,
                (0.0..=4.0).contains(&self.cognitive),
                "[0, 4]",
            ),
            (
                "social",
                self.social,
                (0.0..=4.0).contains(&self.social),
                "[0, 4]",
            ),
            (
                "velocity_clamp",
                self.velocity_clamp,
                self.velocity_clamp > 0.0 && self.velocity_clamp <= 1.0,
                "(0, 1]",
            ),
        ] {
            if !value.is_finite() || !valid {
                return Err(InvalidInput::ParticleSwarmCoefficient {
                    option,
                    range: interval,
                });
            }
        }
        Ok(())
    }

    /// Validates shared budgets and supplies the global-solver defaults.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for invalid shared options or work caps.
    pub fn budgets(&self, common: &Common) -> Result<Common, InvalidInput> {
        DifferentialEvolutionOptions::default().budgets(common)
    }
}

/// Hybrid annealing with reflecting random proposals and bounded coordinate polls.
///
/// This is a finite-budget heuristic, not a global-optimum guarantee. Local
/// polls use a persistent normalized radius, initially `step_size`, halved only
/// after unsuccessful complete sweeps. Reannealing preserves that radius.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridSimulatedAnnealingOptions {
    /// Reproducible SplitMix64 seed, including deterministic zero.
    pub seed: u64,
    /// Initial objective-unit temperature, finite and positive, default `1.0`.
    pub initial_temperature: f64,
    /// Geometric cooling factor in `(0, 1)`, default `0.95`.
    pub cooling_rate: f64,
    /// Uniform proposal half-width normalized by box width, `(0, 1]`, default `0.25`.
    pub step_size: f64,
    /// Completed cycles between local searches, `1..=1_000_000`, default `10`.
    pub local_search_interval: usize,
    /// Maximum complete coordinate sweeps per local search, `1..=256`, default `4`.
    pub local_search_steps: usize,
    /// Completed cycles between temperature resets, `1..=1_000_000`, default `100`.
    pub reanneal_interval: usize,
    /// Normalized unsuccessful local-poll radius tolerance.
    /// Unset falls back to [`Common::tol`] and then `1e-6`.
    pub xatol: Option<f64>,
    /// Absolute objective spread of an unsuccessful complete local poll.
    /// Unset falls back to [`Common::tol`] and then `1e-8`.
    pub fatol: Option<f64>,
}

impl Default for HybridSimulatedAnnealingOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            initial_temperature: 1.0,
            cooling_rate: 0.95,
            step_size: 0.25,
            local_search_interval: 10,
            local_search_steps: 4,
            reanneal_interval: 100,
            xatol: None,
            fatol: None,
        }
    }
}

impl HybridSimulatedAnnealingOptions {
    /// Validates the finite box, feasible starting point and annealing controls.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for an invalid shape, bound, point or option.
    pub fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        problem.validate()?;
        range("dimension", problem.x0.len(), 1, 256)?;
        let bounds = problem
            .bounds
            .as_ref()
            .ok_or(InvalidInput::MissingGlobalBounds)?;
        for (index, (&lower, &upper)) in bounds.lower.iter().zip(&bounds.upper).enumerate() {
            if !lower.is_finite() || !upper.is_finite() || !(upper - lower).is_finite() {
                return Err(InvalidInput::NonfiniteGlobalBound { index });
            }
            if problem.x0[index] < lower || problem.x0[index] > upper {
                return Err(InvalidInput::OutsideGlobalBounds {
                    option: "x0",
                    point: 0,
                    index,
                });
            }
        }
        for (option, value, valid, interval) in [
            (
                "initial_temperature",
                self.initial_temperature,
                self.initial_temperature > 0.0,
                "(0, infinity)",
            ),
            (
                "cooling_rate",
                self.cooling_rate,
                self.cooling_rate > 0.0 && self.cooling_rate < 1.0,
                "(0, 1)",
            ),
            (
                "step_size",
                self.step_size,
                self.step_size > 0.0 && self.step_size <= 1.0,
                "(0, 1]",
            ),
        ] {
            if !value.is_finite() || !valid {
                return Err(InvalidInput::HybridSimulatedAnnealingCoefficient {
                    option,
                    range: interval,
                });
            }
        }
        range(
            "local_search_interval",
            self.local_search_interval,
            1,
            1_000_000,
        )?;
        range("local_search_steps", self.local_search_steps, 1, 256)?;
        range("reanneal_interval", self.reanneal_interval, 1, 1_000_000)?;
        for (option, value) in [("xatol", self.xatol), ("fatol", self.fatol)] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(InvalidInput::NotFiniteNonnegative { option });
            }
        }
        Ok(())
    }

    /// Validates shared budgets and supplies the bounded global-solver defaults.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`] for invalid shared options or work caps.
    pub fn budgets(&self, common: &Common) -> Result<Common, InvalidInput> {
        DifferentialEvolutionOptions::default().budgets(common)
    }
}

fn range(option: &'static str, found: usize, min: usize, max: usize) -> Result<(), InvalidInput> {
    if found < min || found > max {
        return Err(InvalidInput::GlobalRange {
            option,
            min,
            max,
            found,
        });
    }
    Ok(())
}

pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }

    pub(crate) fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    pub(crate) fn index(&mut self, size: usize) -> usize {
        let bound = size as u64;
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next();
            if value >= threshold {
                return (value % bound) as usize;
            }
        }
    }
}
