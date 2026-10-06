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
