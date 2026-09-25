//! The quasi-Newton method of Broyden, Fletcher, Goldfarb and Shanno.
//!
//! Each iteration searches along `p = -H g`, where `H` approximates the
//! inverse Hessian, for a step satisfying the strong Wolfe conditions, then
//! updates `H` from the step `s` and the gradient change `y`. `H` starts as the
//! identity and is rescaled to `(y's / y'y) I` once the first step is known.
//! An update whose curvature `y's` is not positive would lose positive
//! definiteness, so it is skipped and counted instead.
//!
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Section 6.1: Algorithm 6.1, the inverse update 6.17 and the
//!   initial scaling 6.20.

use crate::counters::{Counters, Halt};
use crate::error::MinimizeError;
use crate::finite_difference;
use crate::line_search::{self, LineSearchError, LineSearchFailure, Start, Wolfe};
use crate::objective::Objective;
use crate::outcome::{Converged, Minimize, Termination};
use crate::problem::{BfgsOptions, Common, Norm, Problem};

/// The gradient tolerance used when neither the option nor [`Common::tol`]
/// supplies one.
const DEFAULT_GTOL: f64 = 1e-5;

/// The budget both counters take when the caller sets neither of them, per
/// coordinate.
const BUDGET_PER_COORDINATE: usize = 200;

/// The largest step the line search tries, far beyond the unit quasi-Newton
/// step it starts from.
const LINE_SEARCH_AMAX: f64 = 1e10;

/// The largest number of trial steps per line search: enough to double from
/// the unit step up to [`LINE_SEARCH_AMAX`] and still zoom.
const LINE_SEARCH_MAXITER: usize = 60;

/// Fills in the default budgets: `200 n` for both when neither is set.
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

fn norm(kind: Norm, g: &[f64]) -> f64 {
    match kind {
        Norm::Inf => g.iter().fold(0.0, |largest, gi| gi.abs().max(largest)),
        Norm::Two => dot(g, g).sqrt(),
    }
}

fn dot(u: &[f64], v: &[f64]) -> f64 {
    u.iter().zip(v).map(|(ui, vi)| ui * vi).sum()
}

/// The inverse Hessian approximation, a symmetric `n x n` matrix stored by
/// rows.
struct InverseHessian {
    n: usize,
    h: Vec<f64>,
    scaled: bool,
}

impl InverseHessian {
    fn identity(n: usize) -> Self {
        let mut h = vec![0.0; n * n];
        for i in 0..n {
            h[i * n + i] = 1.0;
        }
        Self {
            n,
            h,
            scaled: false,
        }
    }

    fn times(&self, v: &[f64]) -> Vec<f64> {
        self.h.chunks(self.n).map(|row| dot(row, v)).collect()
    }

    /// Applies the update 6.17 for the step `s` and gradient change `y`, first
    /// rescaling the identity by 6.20. Returns `false`, leaving `H` untouched,
    /// when the curvature `y's` is not positive.
    fn update(&mut self, s: &[f64], y: &[f64]) -> bool {
        let sy = dot(s, y);
        if !(sy.is_finite() && sy > 0.0) {
            return false;
        }
        if !self.scaled {
            let gamma = sy / dot(y, y);
            self.h.iter_mut().for_each(|entry| *entry *= gamma);
            self.scaled = true;
        }
        let rho = 1.0 / sy;
        let hy = self.times(y);
        let shift = rho * rho * dot(y, &hy) + rho;
        for i in 0..self.n {
            for j in 0..self.n {
                self.h[i * self.n + j] += shift * s[i] * s[j] - rho * (s[i] * hy[j] + hy[i] * s[j]);
            }
        }
        true
    }
}

/// The current iterate, which is always the best one: every accepted step
/// decreases the objective.
struct Iterate {
    x: Vec<f64>,
    f: f64,
    skipped: usize,
    failure: Option<LineSearchFailure>,
}

fn run<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    options: &BfgsOptions,
    gtol: f64,
    at: &mut Iterate,
) -> Result<Termination, Halt<O::Error>> {
    let n = at.x.len();
    at.f = counters.value(objective, &at.x)?;
    if !at.f.is_finite() {
        return Ok(Termination::Nonfinite);
    }
    let scheme = options.finite_difference;
    let mut g = vec![0.0; n];
    finite_difference::gradient(counters, objective, &at.x, at.f, scheme, &mut g)?;
    let wolfe = Wolfe::new(1.0, LINE_SEARCH_AMAX, LINE_SEARCH_MAXITER);
    let mut inverse = InverseHessian::identity(n);
    loop {
        if norm(options.norm, &g) <= gtol {
            return Ok(Termination::Converged(Converged::GTol));
        }
        let p: Vec<f64> = inverse.times(&g).iter().map(|v| -v).collect();
        let start = Start {
            x: &at.x,
            f: at.f,
            g: &g,
        };
        let step = match line_search::strong_wolfe(counters, objective, scheme, start, &p, &wolfe) {
            Ok(step) => step,
            Err(LineSearchError::Failure(failure)) => {
                at.failure = Some(failure);
                return Ok(Termination::LineSearchFailed);
            }
            Err(LineSearchError::Halt(halt)) => return Err(halt),
        };
        let s: Vec<f64> = p.iter().map(|pi| step.alpha * pi).collect();
        let y: Vec<f64> = step.g.iter().zip(&g).map(|(new, old)| new - old).collect();
        if !inverse.update(&s, &y) {
            at.skipped += 1;
        }
        (at.x, at.f, g) = (step.x, step.f, step.g);
        counters.end_iteration(objective, &at.x, at.f)?;
    }
}

/// Minimizes `objective` with BFGS, assuming the inputs are already
/// validated.
pub(crate) fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    options: &BfgsOptions,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let gtol = options.gtol.or(common.tol).unwrap_or(DEFAULT_GTOL);
    let mut counters = Counters::new(&budgets(common, problem.x0.len()));
    let mut at = Iterate {
        x: problem.x0.clone(),
        f: f64::NAN,
        skipped: 0,
        failure: None,
    };
    let status = match run(&mut counters, objective, options, gtol, &mut at) {
        Ok(status) | Err(Halt::Terminated(status)) => status,
        Err(Halt::Failed(error)) => return Err(error),
    };
    let mut result = Minimize::new(
        at.x,
        at.f,
        counters.nit(),
        counters.nfev(),
        counters.njev(),
        status,
    );
    if at.failure == Some(LineSearchFailure::NotDescent) {
        result.message = format!("{} ({})", result.message, LineSearchFailure::NotDescent);
    }
    if at.skipped > 0 {
        result.message = format!(
            "{}; {} update(s) skipped on nonpositive curvature",
            result.message, at.skipped
        );
    }
    Ok(result)
}
