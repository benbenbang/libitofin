//! Synchronous global-best particle swarm from Kennedy and Eberhart (1995),
//! <https://doi.org/10.1109/ICNN.1995.488968>, with inertia from Shi and Eberhart
//! (1998), <https://doi.org/10.1109/ICEC.1998.699146>. Velocity clipping, zero
//! initial velocities and boundary absorption are explicit policies here.

use crate::global::Random;
use crate::{
    Bounds, Common, Converged, Counters, Halt, InvalidInput, Minimize, MinimizeError, Objective,
    ParticleSwarmOptions, Problem, Termination,
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

    fn fraction(&self, index: usize, x: f64) -> f64 {
        ((x - self.bounds.lower[index]) / self.width(index)).clamp(0.0, 1.0)
    }

    fn coordinate(&self, index: usize, fraction: f64) -> f64 {
        let lower = self.bounds.lower[index];
        let upper = self.bounds.upper[index];
        if fraction <= 0.5 {
            lower + fraction * self.width(index)
        } else {
            upper - (1.0 - fraction) * self.width(index)
        }
        .clamp(lower, upper)
    }

    fn sample(&self, random: &mut Random) -> Vec<f64> {
        let mut x = self.bounds.lower.clone();
        for &index in &self.free {
            x[index] = self.coordinate(index, random.unit());
        }
        x
    }
}

#[derive(Clone)]
struct Particle {
    x: Vec<f64>,
    f: f64,
    velocity: Vec<f64>,
    best: Vec<f64>,
    best_value: f64,
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

fn converged(population: &[Particle], space: &BoxSpace<'_>, xatol: f64, fatol: f64) -> bool {
    let fmin = population.iter().map(|p| p.f).fold(f64::INFINITY, f64::min);
    let fmax = population
        .iter()
        .map(|p| p.f)
        .fold(f64::NEG_INFINITY, f64::max);
    if fmax - fmin > fatol {
        return false;
    }
    space.free.iter().enumerate().all(|(slot, &index)| {
        let min = population
            .iter()
            .map(|p| p.x[index])
            .fold(f64::INFINITY, f64::min);
        let max = population
            .iter()
            .map(|p| p.x[index])
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = max - min;
        (if xatol == 0.0 {
            spread == 0.0
        } else {
            spread / space.width(index) <= xatol
        }) && population.iter().all(|p| p.velocity[slot].abs() <= xatol)
    })
}

fn moved(
    particle: &Particle,
    best: &[f64],
    space: &BoxSpace<'_>,
    options: &ParticleSwarmOptions,
    random: &mut Random,
) -> (Vec<f64>, Vec<f64>) {
    let mut x = particle.x.clone();
    let mut velocity = particle.velocity.clone();
    for (slot, &index) in space.free.iter().enumerate() {
        let r1 = random.unit();
        let r2 = random.unit();
        let v = (options.inertia * particle.velocity[slot]
            + options.cognitive
                * r1
                * ((particle.best[index] - particle.x[index]) / space.width(index))
            + options.social * r2 * ((best[index] - particle.x[index]) / space.width(index)))
        .clamp(-options.velocity_clamp, options.velocity_clamp);
        velocity[slot] = v;
        if v != 0.0 {
            let fraction = space.fraction(index, particle.x[index]) + v;
            x[index] = space.coordinate(index, fraction.clamp(0.0, 1.0));
            if (fraction <= 0.0 && v < 0.0) || (fraction >= 1.0 && v > 0.0) {
                velocity[slot] = 0.0;
            }
        }
    }
    (x, velocity)
}

fn run<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    problem: &Problem,
    space: &BoxSpace<'_>,
    options: &ParticleSwarmOptions,
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
        population.push(Particle {
            best: x.clone(),
            best_value: f,
            x,
            f,
            velocity: vec![0.0; space.free.len()],
        });
    }
    let xatol = global.xatol.or(common.tol).unwrap_or(1e-6);
    let fatol = global.fatol.or(common.tol).unwrap_or(1e-8);
    loop {
        if converged(&population, space, xatol, fatol) {
            return Ok(Termination::Converged(Converged::XTol));
        }
        let Some((best, _)) = &search.best else {
            return Ok(Termination::Nonfinite);
        };
        let best = best.clone();
        let mut next = population.clone();
        for (old, selected) in population.iter().zip(&mut next) {
            let (x, velocity) = moved(old, &best, space, options, &mut random);
            let f = search.value(objective, &x)?;
            if f < selected.best_value {
                selected.best.clone_from(&x);
                selected.best_value = f;
            }
            selected.x = x;
            selected.f = f;
            selected.velocity = velocity;
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
    options: &ParticleSwarmOptions,
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
