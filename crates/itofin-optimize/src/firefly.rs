//! Independent finite-box firefly search based on Yang (2008),
//! Nature-Inspired Metaheuristic Algorithms, and Yang (2009), "Firefly
//! Algorithms for Multimodal Optimization", <https://arxiv.org/abs/1003.1466>.
//!
//! Frozen brightness/targets, physical stable interpolation, uniform noise,
//! geometric noise decay and reflecting bounds are explicit local policies.
//! A completed generation uses supplied row order without sorting. Every
//! eligible attraction and no-brighter random walk consumes one uniform draw
//! per free coordinate; unchanged physical proposals cost no evaluation.

use crate::global::Random;
use crate::{
    Bounds, Common, Converged, Counters, FireflyOptions, Halt, InvalidInput, Minimize,
    MinimizeError, Objective, Problem, Termination,
};

struct BoxSpace<'a> {
    bounds: &'a Bounds,
    free: Vec<usize>,
}

impl<'a> BoxSpace<'a> {
    fn new(bounds: &'a Bounds) -> Self {
        Self {
            bounds,
            free: bounds
                .lower
                .iter()
                .zip(&bounds.upper)
                .enumerate()
                .filter_map(|(i, (lo, hi))| (lo < hi).then_some(i))
                .collect(),
        }
    }

    fn width(&self, i: usize) -> f64 {
        self.bounds.upper[i] - self.bounds.lower[i]
    }

    fn fraction(&self, i: usize, x: f64) -> f64 {
        ((x - self.bounds.lower[i]) / self.width(i)).clamp(0.0, 1.0)
    }

    fn coordinate(&self, i: usize, fraction: f64) -> f64 {
        let x = if fraction <= 0.5 {
            self.bounds.lower[i] + fraction * self.width(i)
        } else {
            self.bounds.upper[i] - (1.0 - fraction) * self.width(i)
        };
        x.clamp(self.bounds.lower[i], self.bounds.upper[i])
    }

    fn sample(&self, random: &mut Random) -> Vec<f64> {
        let mut x = self.bounds.lower.clone();
        for &i in &self.free {
            x[i] = self.coordinate(i, random.unit());
        }
        x
    }
}

#[derive(Clone)]
struct Firefly {
    x: Vec<f64>,
    f: f64,
}

struct Search {
    counters: Counters,
    best: Option<(Vec<f64>, f64)>,
    first: Option<(Vec<f64>, f64)>,
}

impl Search {
    fn value<O: Objective>(&mut self, objective: &mut O, x: &[f64]) -> Result<f64, Halt<O::Error>> {
        let f = self.counters.value(objective, x)?;
        if self.first.is_none() {
            self.first = Some((x.to_vec(), f));
        }
        if !f.is_finite() {
            return Err(Halt::Terminated(Termination::Nonfinite));
        }
        if self.best.as_ref().is_none_or(|(_, best)| f < *best) {
            self.best = Some((x.to_vec(), f));
        }
        Ok(f)
    }
}

fn attraction(distance_squared: f64, options: &FireflyOptions) -> f64 {
    options.beta0 * (-options.gamma * distance_squared).exp()
}

fn coordinate_move(
    x: f64,
    target: f64,
    beta: f64,
    noise: f64,
    space: &BoxSpace<'_>,
    i: usize,
) -> f64 {
    let difference = target - x;
    if beta * difference + noise * space.width(i) == 0.0 {
        return x;
    }
    let base = if beta <= 0.5 {
        x + beta * difference
    } else {
        target - (1.0 - beta) * difference
    }
    .clamp(space.bounds.lower[i], space.bounds.upper[i]);
    let physical = base + noise * space.width(i);
    if physical.is_finite()
        && physical >= space.bounds.lower[i]
        && physical <= space.bounds.upper[i]
    {
        physical
    } else {
        let wrapped = (space.fraction(i, base) + noise).rem_euclid(2.0);
        let reflected = if wrapped <= 1.0 {
            wrapped
        } else {
            2.0 - wrapped
        };
        space.coordinate(i, reflected)
    }
}

fn moved(
    x: &[f64],
    target: Option<&[f64]>,
    space: &BoxSpace<'_>,
    options: &FireflyOptions,
    alpha: f64,
    random: &mut Random,
) -> Vec<f64> {
    let distance = target.map_or(0.0, |target| {
        space
            .free
            .iter()
            .map(|&i| ((target[i] - x[i]) / space.width(i)).powi(2))
            .sum::<f64>()
    });
    let beta = target.map_or(0.0, |_| attraction(distance, options));
    let mut proposal = x.to_vec();
    for &i in &space.free {
        let noise = alpha * (random.unit() - 0.5);
        proposal[i] = coordinate_move(
            x[i],
            target.map_or(x[i], |target| target[i]),
            beta,
            noise,
            space,
            i,
        );
    }
    proposal
}

fn converged(
    population: &[Firefly],
    space: &BoxSpace<'_>,
    alpha: f64,
    xatol: f64,
    fatol: f64,
) -> bool {
    if alpha > xatol {
        return false;
    }
    let min = population.iter().map(|p| p.f).fold(f64::INFINITY, f64::min);
    let max = population
        .iter()
        .map(|p| p.f)
        .fold(f64::NEG_INFINITY, f64::max);
    if max - min > fatol {
        return false;
    }
    space.free.iter().all(|&i| {
        let min = population
            .iter()
            .map(|p| p.x[i])
            .fold(f64::INFINITY, f64::min);
        let max = population
            .iter()
            .map(|p| p.x[i])
            .fold(f64::NEG_INFINITY, f64::max);
        if xatol == 0.0 {
            max == min
        } else {
            (max - min) / space.width(i) <= xatol
        }
    })
}

fn generation<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    population: &[Firefly],
    space: &BoxSpace<'_>,
    options: &FireflyOptions,
    alpha: f64,
    random: &mut Random,
) -> Result<Vec<Firefly>, Halt<O::Error>> {
    let mut next = population.to_vec();
    for (old, selected) in population.iter().zip(&mut next) {
        let mut attracted = false;
        for target in population {
            if target.f < old.f {
                attracted = true;
                let x = moved(&selected.x, Some(&target.x), space, options, alpha, random);
                if x != selected.x {
                    selected.f = search.value(objective, &x)?;
                    selected.x = x;
                }
            }
        }
        if !attracted {
            let x = moved(&selected.x, None, space, options, alpha, random);
            if x != selected.x {
                selected.f = search.value(objective, &x)?;
                selected.x = x;
            }
        }
    }
    Ok(next)
}

fn run<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    problem: &Problem,
    space: &BoxSpace<'_>,
    options: &FireflyOptions,
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
            None => space.sample(&mut random),
        };
        let f = search.value(objective, &x)?;
        population.push(Firefly { x, f });
    }
    let xatol = global.xatol.or(common.tol).unwrap_or(1e-6);
    let fatol = global.fatol.or(common.tol).unwrap_or(1e-8);
    let mut alpha = options.alpha;
    loop {
        if converged(&population, space, alpha, xatol, fatol) {
            return Ok(Termination::Converged(Converged::XTol));
        }
        population = generation(
            search,
            objective,
            &population,
            space,
            options,
            alpha,
            &mut random,
        )?;
        alpha *= options.alpha_decay;
        if let Some((x, fun)) = &search.best {
            search.counters.end_iteration(objective, x, *fun)?;
        }
    }
}

pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &FireflyOptions,
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
