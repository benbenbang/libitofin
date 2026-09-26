//! The bound-constrained L-BFGS-B iteration.
//!
//! Byrd, Lu, Nocedal and Zhu (1995), Sections 4 and 5.1 supply the
//! generalized Cauchy point and primal free-variable subproblem. The line
//! search uses strong Wolfe conditions in the box interior and accepts a
//! decreasing Armijo step on a limiting face, where Wolfe curvature may be
//! impossible.

use super::geometry::{generalized_cauchy, primal_minimize};
use super::gradient::{Difference, bounded_gradient};
use super::{Compact, dot};
use crate::counters::{Counters, Halt};
use crate::error::MinimizeError;
use crate::objective::Objective;
use crate::outcome::{Converged, Minimize, Termination};
use crate::problem::{Bounds, Common, LbfgsbOptions, Problem};

const DEFAULT_FTOL: f64 = 1e-9;
const DEFAULT_GTOL: f64 = 1e-5;
const DEFAULT_MAXCOR: usize = 10;
const DEFAULT_BUDGET: usize = 15_000;
const SEARCH_TRIALS: usize = 60;
const C1: f64 = 1e-4;
const C2: f64 = 0.9;

struct State {
    x: Vec<f64>,
    f: f64,
    g: Vec<f64>,
}

fn projected_norm(x: &[f64], g: &[f64], bounds: &Bounds) -> f64 {
    x.iter()
        .zip(g)
        .enumerate()
        .fold(0.0, |largest, (i, (&xi, &gi))| {
            let blocked =
                (xi <= bounds.lower[i] && gi > 0.0) || (xi >= bounds.upper[i] && gi < 0.0);
            largest.max(if blocked { 0.0 } else { gi.abs() })
        })
}

fn projected_start(problem: &Problem) -> (Vec<f64>, Bounds) {
    let bounds = problem.bounds.clone().unwrap_or_else(|| Bounds {
        lower: vec![f64::NEG_INFINITY; problem.x0.len()],
        upper: vec![f64::INFINITY; problem.x0.len()],
    });
    let x = problem
        .x0
        .iter()
        .enumerate()
        .map(|(i, &xi)| xi.clamp(bounds.lower[i], bounds.upper[i]))
        .collect();
    (x, bounds)
}

fn trial<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    state: &State,
    direction: &[f64],
    alpha: f64,
    bounds: &Bounds,
    difference: Difference,
) -> Result<State, Halt<O::Error>> {
    let x: Vec<f64> = state
        .x
        .iter()
        .zip(direction)
        .enumerate()
        .map(|(i, (&xi, &di))| (xi + alpha * di).clamp(bounds.lower[i], bounds.upper[i]))
        .collect();
    let f = counters.value(objective, &x)?;
    if !f.is_finite() {
        return Err(Halt::Terminated(Termination::Nonfinite));
    }
    let mut g = vec![0.0; x.len()];
    bounded_gradient(counters, objective, &x, f, bounds, difference, &mut g)?;
    Ok(State { x, f, g })
}

fn search<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    state: &State,
    direction: &[f64],
    bounds: &Bounds,
    difference: Difference,
) -> Result<Option<State>, Halt<O::Error>> {
    let slope0 = dot(&state.g, direction);
    if !slope0.is_finite() || slope0 >= 0.0 {
        return Ok(None);
    }
    let mut amax: f64 = 10.0;
    let mut limiting_face = false;
    for (i, &di) in direction.iter().enumerate() {
        let distance = if di > 0.0 {
            (bounds.upper[i] - state.x[i]) / di
        } else if di < 0.0 {
            (bounds.lower[i] - state.x[i]) / di
        } else {
            f64::INFINITY
        };
        if distance.is_finite() && distance <= amax {
            amax = distance;
            limiting_face = true;
        }
    }
    if !amax.is_finite() || amax <= 0.0 {
        return Ok(None);
    }
    let mut alpha = 1.0_f64.min(amax);
    let mut low = 0.0;
    let mut high = None;
    for _ in 0..SEARCH_TRIALS {
        let next = trial(
            counters, objective, state, direction, alpha, bounds, difference,
        )?;
        let decreases = next.f <= state.f + C1 * alpha * slope0;
        let slope = dot(&next.g, direction);
        if decreases
            && (slope.abs() <= C2 * slope0.abs()
                || (limiting_face && alpha == amax && next.f < state.f))
        {
            return Ok(Some(next));
        }
        if !decreases || slope >= 0.0 {
            high = Some(alpha);
        } else {
            low = alpha;
        }
        let following = match high {
            Some(upper) => (low + upper) * 0.5,
            None => (alpha * 2.0).min(amax),
        };
        if following == alpha || following == 0.0 {
            break;
        }
        alpha = following;
    }
    Ok(None)
}

fn run<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    state: &mut State,
    bounds: &Bounds,
    options: &LbfgsbOptions,
    common: &Common,
) -> Result<Termination, Halt<O::Error>> {
    state.f = counters.value(objective, &state.x)?;
    if !state.f.is_finite() {
        return Ok(Termination::Nonfinite);
    }
    let difference = Difference {
        scheme: options.finite_difference,
        eps: options.eps,
    };
    bounded_gradient(
        counters,
        objective,
        &state.x,
        state.f,
        bounds,
        difference,
        &mut state.g,
    )?;
    let mut compact = Compact::new(state.x.len(), options.maxcor.unwrap_or(DEFAULT_MAXCOR));
    let ftol = options.ftol.or(common.tol).unwrap_or(DEFAULT_FTOL);
    let gtol = options.gtol.or(common.tol).unwrap_or(DEFAULT_GTOL);
    loop {
        if projected_norm(&state.x, &state.g, bounds) <= gtol {
            return Ok(Termination::Converged(Converged::GTol));
        }
        let mut model_step =
            generalized_cauchy(&state.x, &state.g, bounds, &compact).and_then(|cauchy| {
                primal_minimize(&state.x, &state.g, &cauchy, bounds, &compact)
                    .map(|target| (cauchy, target))
            });
        if model_step.is_none() && !compact.pairs.is_empty() {
            compact = Compact::new(state.x.len(), options.maxcor.unwrap_or(DEFAULT_MAXCOR));
            model_step =
                generalized_cauchy(&state.x, &state.g, bounds, &compact).and_then(|cauchy| {
                    primal_minimize(&state.x, &state.g, &cauchy, bounds, &compact)
                        .map(|target| (cauchy, target))
                });
        }
        let Some((cauchy, target)) = model_step else {
            return Ok(Termination::LineSearchFailed);
        };
        let mut direction: Vec<f64> = target
            .iter()
            .zip(&state.x)
            .map(|(xi, old)| xi - old)
            .collect();
        if dot(&state.g, &direction) >= 0.0 {
            direction = cauchy
                .point
                .iter()
                .zip(&state.x)
                .map(|(xi, old)| xi - old)
                .collect();
        }
        let Some(next) = search(counters, objective, state, &direction, bounds, difference)? else {
            return Ok(Termination::LineSearchFailed);
        };
        let old_f = state.f;
        let s: Vec<f64> = next
            .x
            .iter()
            .zip(&state.x)
            .map(|(new, old)| new - old)
            .collect();
        let y: Vec<f64> = next
            .g
            .iter()
            .zip(&state.g)
            .map(|(new, old)| new - old)
            .collect();
        compact.update(s, y);
        *state = next;
        counters.end_iteration(objective, &state.x, state.f)?;
        if projected_norm(&state.x, &state.g, bounds) <= gtol {
            return Ok(Termination::Converged(Converged::GTol));
        }
        if old_f - state.f <= ftol * old_f.abs().max(state.f.abs()).max(1.0) {
            return Ok(Termination::Converged(Converged::FTol));
        }
    }
}

pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &LbfgsbOptions,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let (x, bounds) = projected_start(problem);
    let mut state = State {
        g: vec![0.0; x.len()],
        x,
        f: f64::NAN,
    };
    let budget = Common {
        maxiter: Some(common.maxiter.unwrap_or(DEFAULT_BUDGET)),
        maxfev: Some(common.maxfev.unwrap_or(DEFAULT_BUDGET)),
        tol: common.tol,
    };
    let mut counters = Counters::new(&budget);
    let status = match run(
        &mut counters,
        objective,
        &mut state,
        &bounds,
        options,
        common,
    ) {
        Ok(status) | Err(Halt::Terminated(status)) => status,
        Err(Halt::Failed(error)) => return Err(error),
    };
    Ok(Minimize::new(
        state.x,
        state.f,
        counters.nit(),
        counters.nfev(),
        counters.njev(),
        status,
    ))
}
