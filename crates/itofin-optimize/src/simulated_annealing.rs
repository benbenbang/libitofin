//! Metropolis annealing from Kirkpatrick, Gelatt and Vecchi (1983),
//! <https://doi.org/10.1126/science.220.4598.671>, interleaved with coordinate
//! exploratory polls inspired by Hooke and Jeeves (1961),
//! <https://doi.org/10.1145/321062.321069>. Reflecting proposals, geometric
//! cooling, periodic reannealing and this local-search hybrid are explicit
//! finite-budget policies, not the full Hooke-Jeeves method or a global guarantee.

use crate::global::Random;
use crate::{
    Bounds, Common, Converged, Counters, Halt, HybridSimulatedAnnealingOptions, InvalidInput,
    Minimize, MinimizeError, Objective, Problem, Termination,
};

struct BoxSpace<'a> {
    bounds: &'a Bounds,
    free: Vec<usize>,
}

impl<'a> BoxSpace<'a> {
    fn new(bounds: &'a Bounds) -> Self {
        let free = bounds
            .lower
            .iter()
            .zip(&bounds.upper)
            .enumerate()
            .filter_map(|(index, (lower, upper))| (lower < upper).then_some(index))
            .collect();
        Self { bounds, free }
    }

    fn width(&self, index: usize) -> f64 {
        self.bounds.upper[index] - self.bounds.lower[index]
    }

    fn moved(&self, x: f64, index: usize, delta: f64, reflect: bool) -> f64 {
        if delta == 0.0 {
            return x;
        }
        let lower = self.bounds.lower[index];
        let upper = self.bounds.upper[index];
        let fraction = ((x - lower) / self.width(index)).clamp(0.0, 1.0) + delta;
        let fraction = if reflect && fraction < 0.0 {
            -fraction
        } else if reflect && fraction > 1.0 {
            2.0 - fraction
        } else {
            fraction
        }
        .clamp(0.0, 1.0);
        if fraction <= 0.5 {
            lower + fraction * self.width(index)
        } else {
            upper - (1.0 - fraction) * self.width(index)
        }
        .clamp(lower, upper)
    }

    fn proposal(&self, current: &[f64], step: f64, random: &mut Random) -> Vec<f64> {
        let mut point = current.to_vec();
        for &index in &self.free {
            point[index] = self.moved(
                current[index],
                index,
                (2.0 * random.unit() - 1.0) * step,
                true,
            );
        }
        point
    }
}

struct Search {
    counters: Counters,
    best: Option<(Vec<f64>, f64)>,
    first: Option<(Vec<f64>, f64)>,
}

impl Search {
    fn value<O: Objective>(
        &mut self,
        objective: &mut O,
        point: &[f64],
    ) -> Result<f64, Halt<O::Error>> {
        let value = self.counters.value(objective, point)?;
        if self.first.is_none() {
            self.first = Some((point.to_vec(), value));
        }
        if !value.is_finite() {
            return Err(Halt::Terminated(Termination::Nonfinite));
        }
        if self.best.as_ref().is_none_or(|(_, best)| value < *best) {
            self.best = Some((point.to_vec(), value));
        }
        Ok(value)
    }

    fn local_search<O: Objective>(
        &mut self,
        objective: &mut O,
        space: &BoxSpace<'_>,
        radius: &mut f64,
        options: &HybridSimulatedAnnealingOptions,
        common: &Common,
    ) -> Result<bool, Halt<O::Error>> {
        let xatol = options.xatol.or(common.tol).unwrap_or(1e-6);
        let fatol = options.fatol.or(common.tol).unwrap_or(1e-8);
        for _ in 0..options.local_search_steps {
            let Some((_, anchor)) = &self.best else {
                return Err(Halt::Terminated(Termination::Nonfinite));
            };
            let anchor = *anchor;
            let (mut minimum, mut maximum) = (anchor, anchor);
            for &index in &space.free {
                for direction in [1.0, -1.0] {
                    let Some((best, _)) = &self.best else {
                        return Err(Halt::Terminated(Termination::Nonfinite));
                    };
                    let mut trial = best.clone();
                    trial[index] = space.moved(best[index], index, direction * *radius, false);
                    if trial == *best {
                        continue;
                    }
                    let value = self.value(objective, &trial)?;
                    minimum = minimum.min(value);
                    maximum = maximum.max(value);
                }
            }
            let unsuccessful = self.best.as_ref().is_some_and(|(_, best)| *best == anchor);
            if unsuccessful {
                if *radius <= xatol && maximum - minimum <= fatol {
                    return Ok(true);
                }
                *radius *= 0.5;
            }
        }
        Ok(false)
    }
}

fn uphill_probability(current: f64, trial: f64, temperature: f64) -> f64 {
    let difference = trial - current;
    let scaled = if difference.is_finite() {
        difference / temperature
    } else {
        trial / temperature - current / temperature
    };
    (-scaled).exp()
}

fn run<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    problem: &Problem,
    space: &BoxSpace<'_>,
    options: &HybridSimulatedAnnealingOptions,
    common: &Common,
) -> Result<Termination, Halt<O::Error>> {
    let mut current = problem.x0.clone();
    let mut current_value = search.value(objective, &current)?;
    if space.free.is_empty() {
        return Ok(Termination::Converged(Converged::XTol));
    }
    let mut random = Random::new(options.seed);
    let mut temperature = options.initial_temperature;
    let mut radius = options.step_size;
    loop {
        let trial = space.proposal(&current, options.step_size, &mut random);
        if trial != current {
            let value = search.value(objective, &trial)?;
            if value <= current_value
                || random.unit() < uphill_probability(current_value, value, temperature)
            {
                current = trial;
                current_value = value;
            }
        }
        let cycle = search.counters.nit() + 1;
        let converged = if cycle.is_multiple_of(options.local_search_interval) {
            let converged = search.local_search(objective, space, &mut radius, options, common)?;
            if let Some((best, value)) = &search.best {
                current.clone_from(best);
                current_value = *value;
            }
            converged
        } else {
            false
        };
        if let Some((best, value)) = &search.best {
            search.counters.end_iteration(objective, best, *value)?;
        }
        if converged {
            return Ok(Termination::Converged(Converged::XTol));
        }
        temperature = (temperature * options.cooling_rate).max(f64::from_bits(1));
        if cycle.is_multiple_of(options.reanneal_interval) {
            temperature = options.initial_temperature;
            if let Some((best, value)) = &search.best {
                current.clone_from(best);
                current_value = *value;
            }
        }
    }
}

pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &HybridSimulatedAnnealingOptions,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let resolved = options.budgets(common)?;
    let bounds = problem
        .bounds
        .as_ref()
        .ok_or(InvalidInput::MissingGlobalBounds)?;
    let space = BoxSpace::new(bounds);
    let mut search = Search {
        counters: Counters::new(&resolved),
        best: None,
        first: None,
    };
    let status = match run(&mut search, objective, problem, &space, options, common) {
        Ok(status) | Err(Halt::Terminated(status)) => status,
        Err(Halt::Failed(error)) => return Err(error),
    };
    let (point, value) = search
        .best
        .or(search.first)
        .unwrap_or_else(|| (problem.x0.clone(), f64::NAN));
    Ok(Minimize::new(
        point,
        value,
        search.counters.nit(),
        search.counters.nfev(),
        search.counters.njev(),
        status,
    ))
}
