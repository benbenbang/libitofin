//! The simplex method of Nelder and Mead.
//!
//! The search keeps `n + 1` vertices ordered by objective value and replaces
//! the worst one each iteration by reflection, expansion, outside contraction
//! or inside contraction, falling back to a shrink towards the best vertex when
//! no candidate improves on it.
//!
//! A `+inf` value is a legal worse vertex, so a barrier objective converges
//! inside its feasible region. A `NaN` or `-inf` value ends the run instead,
//! reporting the best finite point reached.
//!
//! - Nelder, J. A. and Mead, R. (1965), "A simplex method for function
//!   minimization", The Computer Journal 7(4), 308-313: the four moves.
//! - Lagarias, J. C., Reeds, J. A., Wright, M. H. and Wright, P. E. (1998),
//!   "Convergence properties of the Nelder-Mead simplex method in low
//!   dimensions", SIAM Journal on Optimization 9(1), 112-147: the ordering and
//!   the tie rule, which a stable sort preserves.
//! - Gao, F. and Han, L. (2012), "Implementing the Nelder-Mead simplex
//!   algorithm with adaptive parameters", Computational Optimization and
//!   Applications 51(1), 259-277: the dimension-scaled coefficients selected by
//!   [`NelderMeadOptions::adaptive`].

use crate::counters::{Counters, Halt};
use crate::error::MinimizeError;
use crate::objective::Objective;
use crate::outcome::{Converged, Minimize, Termination};
use crate::problem::{Common, NelderMeadOptions, Problem};

/// The tolerance used when neither the option nor [`Common::tol`] supplies one.
const DEFAULT_TOLERANCE: f64 = 1e-4;

/// The relative perturbation that builds a vertex from a nonzero coordinate.
const COORDINATE_STEP: f64 = 0.05;

/// The absolute perturbation that builds a vertex from a zero coordinate, where
/// a relative one would leave the vertex on top of the starting point.
const ZERO_COORDINATE_STEP: f64 = 0.00025;

/// The budget both counters take when the caller sets neither of them, per
/// coordinate.
const BUDGET_PER_COORDINATE: usize = 200;

/// One simplex vertex and the value of the objective there, never `NaN`.
#[derive(Debug, Clone)]
struct Vertex {
    x: Vec<f64>,
    f: f64,
}

/// The four move lengths, along the line from the worst vertex through the
/// centroid of the others.
struct Coefficients {
    reflection: f64,
    expansion: f64,
    contraction: f64,
    shrink: f64,
}

/// The evaluation site: the one place the objective is called, so that the
/// counters, the nonfinite rule and the running best all see every value.
struct Search {
    counters: Counters,
    best: Option<(Vec<f64>, f64)>,
    first: Option<(Vec<f64>, f64)>,
}

impl Search {
    fn new(common: &Common) -> Self {
        Self {
            counters: Counters::new(common),
            best: None,
            first: None,
        }
    }

    /// Evaluates the objective at `x`, charging the call and recording it.
    ///
    /// `NaN` and `-inf` end the run with [`Termination::Nonfinite`], because
    /// neither gives the simplex a direction to follow: `NaN` orders against
    /// nothing, and `-inf` is an optimum the search can never improve on or
    /// move away from. `+inf` stays a legal worse value that simply never
    /// becomes the best, which is what makes a barrier objective work. The
    /// first value seen is kept whatever it is, so that a run whose every value
    /// is nonfinite still has a point to report.
    fn value<O: Objective>(&mut self, objective: &mut O, x: &[f64]) -> Result<f64, Halt<O::Error>> {
        let value = self.counters.value(objective, x)?;
        if self.first.is_none() {
            self.first = Some((x.to_vec(), value));
        }
        if value.is_nan() || value == f64::NEG_INFINITY {
            return Err(Halt::Terminated(Termination::Nonfinite));
        }
        if value.is_finite() && self.best.as_ref().is_none_or(|(_, best)| value < *best) {
            self.best = Some((x.to_vec(), value));
        }
        Ok(value)
    }
}

/// Resolves one tolerance: the explicit option, else the shared
/// [`Common::tol`], else [`DEFAULT_TOLERANCE`].
fn tolerance(option: Option<f64>, shared: Option<f64>) -> f64 {
    option.or(shared).unwrap_or(DEFAULT_TOLERANCE)
}

/// Fills in the default budgets.
///
/// Setting neither budget caps both at `200 n`; setting one leaves the other
/// unlimited, so that a caller who asks for a number of iterations is not cut
/// short by an evaluation cap they never chose.
fn budgets(common: &Common, n: usize) -> Common {
    if common.maxiter.is_some() || common.maxfev.is_some() {
        return common.clone();
    }
    let budget = Some(n * BUDGET_PER_COORDINATE);
    Common {
        maxiter: budget,
        maxfev: budget,
        tol: common.tol,
    }
}

/// The starting points: the ones the caller supplied, or `x0` with one
/// coordinate perturbed per further vertex.
fn starting_points(x0: &[f64], given: Option<&Vec<Vec<f64>>>) -> Vec<Vec<f64>> {
    if let Some(points) = given {
        return points.clone();
    }
    let mut points = Vec::with_capacity(x0.len() + 1);
    points.push(x0.to_vec());
    for (index, coordinate) in x0.iter().enumerate() {
        let mut point = x0.to_vec();
        point[index] = if *coordinate == 0.0 {
            ZERO_COORDINATE_STEP
        } else {
            coordinate * (1.0 + COORDINATE_STEP)
        };
        points.push(point);
    }
    points
}

/// The standard coefficients, or the ones Gao and Han scale with the dimension.
fn coefficients(adaptive: bool, n: usize) -> Coefficients {
    if !adaptive {
        return Coefficients {
            reflection: 1.0,
            expansion: 2.0,
            contraction: 0.5,
            shrink: 0.5,
        };
    }
    let dimension = n as f64;
    Coefficients {
        reflection: 1.0,
        expansion: 1.0 + 2.0 / dimension,
        contraction: 0.75 - 1.0 / (2.0 * dimension),
        shrink: 1.0 - 1.0 / dimension,
    }
}

/// Orders the vertices by value, keeping the previous order among ties.
fn order(vertices: &mut [Vertex]) {
    vertices.sort_by(|left, right| left.f.total_cmp(&right.f));
}

/// The centroid of every vertex but the worst.
fn centroid(vertices: &[Vertex], n: usize) -> Vec<f64> {
    let mut centre = vec![0.0; n];
    for vertex in &vertices[..n] {
        for (total, coordinate) in centre.iter_mut().zip(&vertex.x) {
            *total += coordinate;
        }
    }
    for total in &mut centre {
        *total /= n as f64;
    }
    centre
}

/// The point `step` of the way from the centroid along the direction leading
/// away from the worst vertex. A negative `step` contracts inside the simplex.
fn along(centre: &[f64], worst: &[f64], step: f64) -> Vec<f64> {
    centre
        .iter()
        .zip(worst)
        .map(|(middle, far)| middle + step * (middle - far))
        .collect()
}

/// Whether the simplex is small enough in both `x` and `f`.
///
/// The comparisons are written so that a `NaN` spread, which an all-infinite
/// simplex produces, fails the test rather than passing it.
fn converged(vertices: &[Vertex], xatol: f64, fatol: f64) -> bool {
    let best = &vertices[0];
    vertices[1..].iter().all(|vertex| {
        (vertex.f - best.f).abs() <= fatol
            && vertex
                .x
                .iter()
                .zip(&best.x)
                .all(|(coordinate, reference)| (coordinate - reference).abs() <= xatol)
    })
}

/// Pulls every vertex but the best towards the best one.
fn shrink<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    vertices: &mut [Vertex],
    factor: f64,
) -> Result<(), Halt<O::Error>> {
    let best = vertices[0].x.clone();
    for vertex in vertices.iter_mut().skip(1) {
        let x: Vec<f64> = best
            .iter()
            .zip(&vertex.x)
            .map(|(anchor, coordinate)| anchor + factor * (coordinate - anchor))
            .collect();
        let f = search.value(objective, &x)?;
        *vertex = Vertex { x, f };
    }
    Ok(())
}

/// Runs the simplex until a tolerance, a budget, a cancellation or a `NaN`
/// stops it.
fn run<O: Objective>(
    search: &mut Search,
    objective: &mut O,
    x0: &[f64],
    options: &NelderMeadOptions,
    xatol: f64,
    fatol: f64,
) -> Result<Termination, Halt<O::Error>> {
    let n = x0.len();
    let mut vertices = Vec::with_capacity(n + 1);
    for x in starting_points(x0, options.initial_simplex.as_ref()) {
        let f = search.value(objective, &x)?;
        vertices.push(Vertex { x, f });
    }
    order(&mut vertices);
    let step = coefficients(options.adaptive, n);
    loop {
        if converged(&vertices, xatol, fatol) {
            return Ok(Termination::Converged(Converged::XTol));
        }
        let centre = centroid(&vertices, n);
        let best = vertices[0].f;
        let next_worst = vertices[n - 1].f;
        let worst = vertices[n].f;
        let reflected = along(&centre, &vertices[n].x, step.reflection);
        let f_reflected = search.value(objective, &reflected)?;
        let replacement = if f_reflected < best {
            let expanded = along(&centre, &vertices[n].x, step.reflection * step.expansion);
            let f_expanded = search.value(objective, &expanded)?;
            if f_expanded < f_reflected {
                Some(Vertex {
                    x: expanded,
                    f: f_expanded,
                })
            } else {
                Some(Vertex {
                    x: reflected,
                    f: f_reflected,
                })
            }
        } else if f_reflected < next_worst {
            Some(Vertex {
                x: reflected,
                f: f_reflected,
            })
        } else if f_reflected < worst {
            let outside = along(&centre, &vertices[n].x, step.contraction * step.reflection);
            let f_outside = search.value(objective, &outside)?;
            (f_outside <= f_reflected).then_some(Vertex {
                x: outside,
                f: f_outside,
            })
        } else {
            let inside = along(&centre, &vertices[n].x, -step.contraction);
            let f_inside = search.value(objective, &inside)?;
            (f_inside < worst).then_some(Vertex {
                x: inside,
                f: f_inside,
            })
        };
        match replacement {
            Some(vertex) => vertices[n] = vertex,
            None => shrink(search, objective, &mut vertices, step.shrink)?,
        }
        order(&mut vertices);
        search
            .counters
            .end_iteration(objective, &vertices[0].x, vertices[0].f)?;
    }
}

/// Minimizes `objective` with the simplex method, assuming the inputs are
/// already validated.
pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &NelderMeadOptions,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let resolved = budgets(common, problem.x0.len());
    let xatol = tolerance(options.xatol, common.tol);
    let fatol = tolerance(options.fatol, common.tol);
    let mut search = Search::new(&resolved);
    let status = match run(&mut search, objective, &problem.x0, options, xatol, fatol) {
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
