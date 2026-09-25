//! Sequential least-squares quadratic programming (SLSQP).
//!
//! Each iteration solves a quadratic model of the Lagrangian subject to the
//! linearized constraints, in the least-squares form of Kraft (1988), searches
//! along the step on Powell's `l1` merit function, and updates the Hessian
//! approximation by Powell's damped BFGS formula. Box bounds are rows of the
//! subproblem and every trial point stays inside them.
//!
//! The run converges, [`Converged::FTol`], when the maximum constraint
//! violation is below `ftol` and either the objective moved by less than
//! `ftol` over an accepted step or the subproblem predicts a first-order change
//! `|g' d| + sum_j |lambda_j c_j|` below `ftol`. A relaxed subproblem that
//! reduces no violation at all ends it as [`Termination::Infeasible`]. Any
//! nonfinite objective, gradient, constraint or Jacobian value ends it as
//! [`Termination::Nonfinite`], reporting the last accepted iterate.
//!
//! Constraint and Jacobian evaluations are not charged to `nfev` or `njev`,
//! the SciPy convention; objective values and gradients are.
//!
//! - Kraft, D. (1988), "A software package for sequential quadratic
//!   programming", DFVLR-FB 88-28, DLR German Aerospace Center: the
//!   least-squares subproblem and its relaxation.
//! - Powell, M. J. D. (1978), "A fast algorithm for nonlinearly constrained
//!   optimization calculations", Lecture Notes in Mathematics 630: the damped
//!   update, the merit function and its penalty update.
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Sections 18.3 and 18.4, Algorithm 18.3: the line search SQP
//!   iteration on the `l1` merit function.

mod hessian;
mod subproblem;

use crate::counters::{Counters, Halt};
use crate::error::MinimizeError;
use crate::finite_difference::{FiniteDifference, gradient};
use crate::objective::{ConstraintKind, Objective};
use crate::outcome::{Converged, Minimize, Termination};
use crate::problem::{Common, Problem, SlsqpOptions};
use hessian::{Hessian, dot};
use subproblem::{Linearization, solve};

/// The tolerance used when neither the option nor [`Common::tol`] supplies one.
const DEFAULT_FTOL: f64 = 1e-6;

/// The iteration budget when [`Common::maxiter`] is unset.
const DEFAULT_MAXITER: usize = 100;

/// Trial steps per line search after the full one.
const MAX_BACKTRACKS: usize = 10;

/// The fraction of the predicted merit decrease a step must achieve.
const SUFFICIENT_DECREASE: f64 = 0.1;

/// The relaxation at or above which the subproblem is taken to have reduced no
/// violation: the linearized constraints are incompatible.
const FULL_RELAXATION: f64 = 1.0 - 1e-8;

/// The last accepted iterate, which every termination reports.
struct State {
    x: Vec<f64>,
    f: f64,
    c: Vec<f64>,
    multipliers: Option<Vec<f64>>,
}

/// The problem data fixed for the whole run.
struct Setup<'a> {
    kinds: &'a [ConstraintKind],
    lower: &'a [f64],
    upper: &'a [f64],
    ftol: f64,
}

fn failed<E: std::error::Error + 'static>(error: E) -> Halt<E> {
    Halt::Failed(MinimizeError::Objective(error))
}

fn finite<E: std::error::Error + 'static>(value: f64) -> Result<f64, Halt<E>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Halt::Terminated(Termination::Nonfinite))
    }
}

/// Evaluates every constraint at `x`.
fn constraints<O: Objective>(
    objective: &mut O,
    x: &[f64],
    m: usize,
) -> Result<Vec<f64>, Halt<O::Error>> {
    (0..m)
        .map(|j| finite(objective.constraint(j, x).map_err(failed)?))
        .collect()
}

/// The constraint Jacobian at `x`, where the constraints take the values `c`,
/// one row per constraint.
///
/// A row the objective does not supply is approximated by forward differences
/// with the step rule of the objective gradient, `sqrt(eps) max(1, |x_i|)`.
/// It calls the constraint directly rather than through [`Counters`], since
/// constraint evaluations are not charged, and so cannot share
/// [`gradient`], which charges every evaluation.
fn jacobian<O: Objective>(
    objective: &mut O,
    x: &[f64],
    c: &[f64],
) -> Result<Vec<f64>, Halt<O::Error>> {
    let n = x.len();
    let mut a = vec![0.0; c.len() * n];
    let mut point = x.to_vec();
    for (j, row) in a.chunks_exact_mut(n.max(1)).enumerate() {
        if objective.constraint_jacobian(j, x, row).map_err(failed)? {
            continue;
        }
        for (i, &xi) in x.iter().enumerate() {
            point[i] = xi + f64::EPSILON.sqrt() * xi.abs().max(1.0);
            let value = finite(objective.constraint(j, &point).map_err(failed)?)?;
            row[i] = (value - c[j]) / (point[i] - xi);
            point[i] = xi;
        }
    }
    a.iter().try_for_each(|&entry| finite(entry).map(drop))?;
    Ok(a)
}

/// The violation of each constraint: `|c_j|` for an equality, the negative
/// part of `c_j` for an inequality.
fn violations<'a>(kinds: &'a [ConstraintKind], c: &'a [f64]) -> impl Iterator<Item = f64> + 'a {
    kinds.iter().zip(c).map(|(kind, c)| match kind {
        ConstraintKind::Eq => c.abs(),
        ConstraintKind::Ineq => (-c).max(0.0),
    })
}

fn max_violation(kinds: &[ConstraintKind], c: &[f64]) -> f64 {
    violations(kinds, c).fold(0.0, f64::max)
}

/// The `l1` merit `f + sum_j penalty_j violation_j`.
fn merit(kinds: &[ConstraintKind], penalty: &[f64], f: f64, c: &[f64]) -> f64 {
    f + dot(penalty, &violations(kinds, c).collect::<Vec<_>>())
}

/// The gradient of the Lagrangian, `g - A' lambda`.
fn lagrangian(g: &[f64], a: &[f64], multipliers: &[f64]) -> Vec<f64> {
    let n = g.len();
    let mut out = g.to_vec();
    for (row, lambda) in a.chunks_exact(n.max(1)).zip(multipliers) {
        for (out, a) in out.iter_mut().zip(row) {
            *out -= lambda * a;
        }
    }
    out
}

/// A point on the search line, clipped into the bounds against rounding.
fn along(state: &State, d: &[f64], alpha: f64, setup: &Setup<'_>) -> Vec<f64> {
    (0..d.len())
        .map(|i| (state.x[i] + alpha * d[i]).clamp(setup.lower[i], setup.upper[i]))
        .collect()
}

/// Runs SQP iterations from `state` until a tolerance, a budget, a
/// cancellation, an infeasibility or a nonfinite value stops them.
fn run<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    state: &mut State,
    setup: &Setup<'_>,
) -> Result<Termination, Halt<O::Error>> {
    let (n, m, kinds) = (state.x.len(), setup.kinds.len(), setup.kinds);
    state.f = counters.value(objective, &state.x)?;
    finite(state.f)?;
    state.c = constraints(objective, &state.x, m)?;
    let mut g = vec![0.0; n];
    gradient(
        counters,
        objective,
        &state.x,
        state.f,
        FiniteDifference::Forward,
        &mut g,
    )?;
    let mut a = jacobian(objective, &state.x, &state.c)?;
    let mut hessian = Hessian::identity(n);
    let mut fresh = true;
    let mut penalty = vec![0.0; m];
    loop {
        let Some(l) = hessian.cholesky() else {
            hessian = Hessian::identity(n);
            fresh = true;
            continue;
        };
        let lower: Vec<f64> = setup
            .lower
            .iter()
            .zip(&state.x)
            .map(|(l, x)| l - x)
            .collect();
        let upper: Vec<f64> = setup
            .upper
            .iter()
            .zip(&state.x)
            .map(|(u, x)| u - x)
            .collect();
        let model = Linearization {
            gradient: &g,
            values: &state.c,
            jacobian: &a,
            kinds,
            lower: &lower,
            upper: &upper,
        };
        let Ok(step) = solve(&l, &model) else {
            return Ok(Termination::Infeasible);
        };
        if step.relaxation >= FULL_RELAXATION {
            return Ok(Termination::Infeasible);
        }
        let lambda = step.multipliers;
        state.multipliers = Some(lambda.clone());
        let violation = max_violation(kinds, &state.c);
        let complementarity: f64 = lambda
            .iter()
            .zip(&state.c)
            .map(|(l, c)| (l * c).abs())
            .sum();
        if dot(&g, &step.d).abs() + complementarity < setup.ftol && violation < setup.ftol {
            return Ok(Termination::Converged(Converged::FTol));
        }
        for (penalty, lambda) in penalty.iter_mut().zip(&lambda) {
            *penalty = lambda.abs().max((*penalty + lambda.abs()) / 2.0);
        }
        let phi0 = merit(kinds, &penalty, state.f, &state.c);
        let slope = dot(&g, &step.d)
            - (1.0 - step.relaxation)
                * dot(&penalty, &violations(kinds, &state.c).collect::<Vec<_>>());
        let mut alpha = 1.0;
        let mut accepted = false;
        let (mut x, mut f, mut c) = (Vec::new(), 0.0, Vec::new());
        for _ in 0..=MAX_BACKTRACKS {
            x = along(state, &step.d, alpha, setup);
            f = finite(counters.value(objective, &x)?)?;
            c = constraints(objective, &x, m)?;
            let phi = merit(kinds, &penalty, f, &c);
            if phi <= phi0 + SUFFICIENT_DECREASE * alpha * slope.min(0.0) {
                accepted = true;
                break;
            }
            let curvature = (phi - phi0 - slope * alpha) / (alpha * alpha);
            let minimizer = -slope / (2.0 * curvature);
            alpha = if minimizer.is_finite() {
                minimizer.clamp(0.1 * alpha, 0.5 * alpha)
            } else {
                0.5 * alpha
            };
        }
        if !accepted && !fresh {
            hessian = Hessian::identity(n);
            fresh = true;
            continue;
        }
        let mut g_new = vec![0.0; n];
        gradient(
            counters,
            objective,
            &x,
            f,
            FiniteDifference::Forward,
            &mut g_new,
        )?;
        let a_new = jacobian(objective, &x, &c)?;
        let s: Vec<f64> = x.iter().zip(&state.x).map(|(new, old)| new - old).collect();
        let y: Vec<f64> = lagrangian(&g_new, &a_new, &lambda)
            .iter()
            .zip(lagrangian(&g, &a, &lambda))
            .map(|(new, old)| new - old)
            .collect();
        hessian.update(&s, &y);
        fresh = false;
        let change = (f - state.f).abs();
        (state.x, state.f, state.c, g, a) = (x, f, c, g_new, a_new);
        counters.end_iteration(objective, &state.x, state.f)?;
        if change < setup.ftol && max_violation(kinds, &state.c) < setup.ftol {
            return Ok(Termination::Converged(Converged::FTol));
        }
    }
}

/// Minimizes `objective` subject to its constraints and the problem bounds by
/// SLSQP, assuming the inputs are already validated.
///
/// `x0` is first clipped into the bounds, as SciPy does, so that every
/// iterate is inside them.
pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &SlsqpOptions,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let n = problem.x0.len();
    let (lower, upper) = match &problem.bounds {
        Some(bounds) => (bounds.lower.clone(), bounds.upper.clone()),
        None => (vec![f64::NEG_INFINITY; n], vec![f64::INFINITY; n]),
    };
    let kinds: Vec<ConstraintKind> = (0..objective.constraint_count())
        .map(|j| objective.constraint_kind(j))
        .collect();
    let setup = Setup {
        kinds: &kinds,
        lower: &lower,
        upper: &upper,
        ftol: options.ftol.or(common.tol).unwrap_or(DEFAULT_FTOL),
    };
    let mut counters = Counters::new(&Common {
        maxiter: common.maxiter.or(Some(DEFAULT_MAXITER)),
        ..common.clone()
    });
    let mut state = State {
        x: (0..n)
            .map(|i| problem.x0[i].clamp(lower[i], upper[i]))
            .collect(),
        f: f64::NAN,
        c: Vec::new(),
        multipliers: None,
    };
    let status = match run(&mut counters, objective, &mut state, &setup) {
        Ok(status) | Err(Halt::Terminated(status)) => status,
        Err(Halt::Failed(error)) => return Err(error),
    };
    let mut result = Minimize::new(
        state.x,
        state.f,
        counters.nit(),
        counters.nfev(),
        counters.njev(),
        status,
    );
    if !kinds.is_empty() && state.c.len() == kinds.len() {
        result.message = format!(
            "{}; maximum constraint violation {:.3e}",
            result.message,
            max_violation(&kinds, &state.c)
        );
    }
    Ok(match state.multipliers {
        Some(multipliers) => result.with_multipliers(multipliers),
        None => result,
    })
}
