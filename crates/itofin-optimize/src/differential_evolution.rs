//! Independent DE/rand/1/bin from Storn and Price (1997), Journal of Global
//! Optimization 11, 341-359, <https://doi.org/10.1023/A:1008202821328>.
//! Parents are frozen for a generation, ties accept the trial, and out-of-box
//! mutant coordinates are independently resampled uniformly. Convergence is a
//! population-spread test, not a guarantee of global optimality.

use crate::global::Random;
use crate::{
    Bounds, Common, Converged, Counters, DifferentialEvolutionOptions, Halt, InvalidInput,
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

    fn normalized(&self, x: &[f64]) -> Vec<f64> {
        self.free
            .iter()
            .map(|&index| {
                ((x[index] - self.bounds.lower[index])
                    / (self.bounds.upper[index] - self.bounds.lower[index]))
                    .clamp(0.0, 1.0)
            })
            .collect()
    }

    fn coordinate(&self, index: usize, fraction: f64) -> f64 {
        let lower = self.bounds.lower[index];
        let upper = self.bounds.upper[index];
        let width = upper - lower;
        if fraction <= 0.5 {
            lower + fraction * width
        } else {
            upper - (1.0 - fraction) * width
        }
        .clamp(lower, upper)
    }

    fn physical(&self, normalized: &[f64]) -> Vec<f64> {
        let mut x = self.bounds.lower.clone();
        for (&index, &fraction) in self.free.iter().zip(normalized) {
            x[index] = self.coordinate(index, fraction);
        }
        x
    }
}

#[derive(Clone)]
struct Member {
    x: Vec<f64>,
    normalized: Vec<f64>,
    f: f64,
}

struct Search {
    counters: Counters,
    best: Option<(Vec<f64>, f64)>,
    first: Option<(Vec<f64>, f64)>,
}

impl Search {
    fn value<O: Objective>(&mut self, objective: &mut O, x: &[f64]) -> Result<f64, Halt<O::Error>> {
        let value = self.counters.value(objective, x)?;
        if self.first.is_none() {
            self.first = Some((x.to_vec(), value));
        }
        if !value.is_finite() {
            return Err(Halt::Terminated(Termination::Nonfinite));
        }
        if self.best.as_ref().is_none_or(|(_, best)| value < *best) {
            self.best = Some((x.to_vec(), value));
        }
        Ok(value)
    }
}

fn converged(population: &[Member], space: &BoxSpace<'_>, xatol: f64, fatol: f64) -> bool {
    let fmin = population
        .iter()
        .map(|member| member.f)
        .fold(f64::INFINITY, f64::min);
    let fmax = population
        .iter()
        .map(|member| member.f)
        .fold(f64::NEG_INFINITY, f64::max);
    if fmax - fmin > fatol {
        return false;
    }
    space.free.iter().all(|&index| {
        let min = population
            .iter()
            .map(|member| member.x[index])
            .fold(f64::INFINITY, f64::min);
        let max = population
            .iter()
            .map(|member| member.x[index])
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = max - min;
        if xatol == 0.0 {
            spread == 0.0
        } else {
            spread / (space.bounds.upper[index] - space.bounds.lower[index]) <= xatol
        }
    })
}

fn donors(random: &mut Random, target: usize, size: usize) -> [usize; 3] {
    let mut excluded = vec![target];
    let mut chosen = [0; 3];
    for slot in &mut chosen {
        let mut index = random.index(size - excluded.len());
        excluded.sort_unstable();
        for &skip in &excluded {
            if index >= skip {
                index += 1;
            }
        }
        *slot = index;
        excluded.push(index);
    }
    chosen
}

fn trial(
    population: &[Member],
    target: usize,
    space: &BoxSpace<'_>,
    options: &DifferentialEvolutionOptions,
    random: &mut Random,
) -> Vec<f64> {
    let [a, b, c] = donors(random, target, population.len());
    let mut point = population[target].x.clone();
    let forced = random.index(space.free.len());
    for (index, &physical_index) in space.free.iter().enumerate() {
        let crossover = random.unit();
        if index == forced || crossover < options.recombination {
            let candidate = population[a].normalized[index]
                + options.mutation
                    * (population[b].normalized[index] - population[c].normalized[index]);
            let normalized = if (0.0..=1.0).contains(&candidate) {
                candidate
            } else {
                random.unit()
            };
            point[physical_index] = space.coordinate(physical_index, normalized);
        }
    }
    point
}

fn run<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    problem: &Problem,
    space: &BoxSpace<'_>,
    options: &DifferentialEvolutionOptions,
    common: &Common,
) -> Result<Termination, Halt<O::Error>> {
    if space.free.is_empty() {
        search.value(objective, &problem.x0)?;
        return Ok(Termination::Converged(Converged::XTol));
    }
    let global = &options.global;
    let size = global.population_count(space.bounds);
    let mut random = Random::new(global.seed);
    let mut population = Vec::with_capacity(size);
    for row in 0..size {
        let x = match &global.initial_population {
            Some(points) => points[row].clone(),
            None if row == 0 => problem.x0.clone(),
            None => space.physical(
                &(0..space.free.len())
                    .map(|_| random.unit())
                    .collect::<Vec<_>>(),
            ),
        };
        let f = search.value(objective, &x)?;
        population.push(Member {
            normalized: space.normalized(&x),
            x,
            f,
        });
    }
    let xatol = global.xatol.or(common.tol).unwrap_or(1e-6);
    let fatol = global.fatol.or(common.tol).unwrap_or(1e-8);
    loop {
        if converged(&population, space, xatol, fatol) {
            return Ok(Termination::Converged(Converged::XTol));
        }
        let mut next = population.clone();
        for (target, selected) in next.iter_mut().enumerate() {
            let x = trial(&population, target, space, options, &mut random);
            let f = search.value(objective, &x)?;
            if f <= population[target].f {
                *selected = Member {
                    normalized: space.normalized(&x),
                    x,
                    f,
                };
            }
        }
        population = next;
        if let Some((x, fun)) = &search.best {
            search.counters.end_iteration(objective, x, *fun)?;
        }
    }
}

pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &DifferentialEvolutionOptions,
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
    let (x, fun) = search
        .best
        .or(search.first)
        .unwrap_or_else(|| (problem.x0.clone(), f64::NAN));
    Ok(Minimize::new(
        x,
        fun,
        search.counters.nit(),
        search.counters.nfev(),
        search.counters.njev(),
        status,
    ))
}
