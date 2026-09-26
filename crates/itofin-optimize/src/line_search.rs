//! A line search for a step satisfying the strong Wolfe conditions.
//!
//! Along the direction `p` from `x`, with `phi(alpha) = f(x + alpha p)`, a step
//! is accepted when it gives sufficient decrease,
//! `phi(alpha) <= phi(0) + c1 alpha phi'(0)`, and the curvature condition
//! `|phi'(alpha)| <= c2 |phi'(0)|`. The search expands the step until it
//! brackets such a point, then zooms into the bracket by interpolation.
//!
//! Every value and gradient goes through [`Counters`], so the search is
//! charged and budget-limited like the rest of the solver. Following the
//! line-search rule of D-OPT 5, any nonfinite value or gradient ends the run
//! with [`Termination::Nonfinite`].
//!
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Section 3.5: Algorithm 3.5 (the bracketing phase) and
//!   Algorithm 3.6 (zoom); equation 3.59 for the cubic interpolant, and the
//!   quadratic interpolant of Section 3.5 when only one endpoint slope is known.

use crate::counters::{Counters, Halt};
use crate::finite_difference::{self, FiniteDifference};
use crate::objective::Objective;
use crate::outcome::Termination;

/// The fraction of the bracket width an interpolated trial must keep from
/// either endpoint; closer trials are replaced by the midpoint.
const SAFEGUARD: f64 = 0.1;

/// The line search parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Wolfe {
    /// The sufficient decrease constant, in `(0, c2)`.
    pub c1: f64,
    /// The curvature constant, in `(c1, 1)`.
    pub c2: f64,
    /// The first trial step.
    pub alpha0: f64,
    /// The largest step tried.
    pub amax: f64,
    /// The largest number of trial steps, bracketing and zoom together.
    pub maxiter: usize,
    pub eps: Option<f64>,
}

impl Wolfe {
    /// Parameters with the quasi-Newton constants `c1 = 1e-4` and `c2 = 0.9`.
    pub(crate) fn new(alpha0: f64, amax: f64, maxiter: usize) -> Self {
        Self {
            c1: 1e-4,
            c2: 0.9,
            alpha0,
            amax,
            maxiter,
            eps: None,
        }
    }
}

/// The point the search starts from: `x`, `f(x)` and the gradient there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Start<'a> {
    pub x: &'a [f64],
    pub f: f64,
    pub g: &'a [f64],
}

/// An accepted step and the objective state it reaches.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Step {
    pub alpha: f64,
    pub x: Vec<f64>,
    pub f: f64,
    pub g: Vec<f64>,
}

/// Why the search found no step, as opposed to why the run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LineSearchFailure {
    /// The direction does not descend: `g'p >= 0`.
    #[error("the search direction is not a descent direction")]
    NotDescent,
    /// No step within `amax` and `maxiter` satisfies the strong Wolfe
    /// conditions.
    #[error("no step satisfies the strong Wolfe conditions")]
    NoStrongWolfeStep,
}

/// Everything the search can end with other than a step.
#[derive(Debug)]
pub(crate) enum LineSearchError<E: std::error::Error + 'static> {
    /// The run is over or failed: budget, nonfinite value or objective error.
    Halt(Halt<E>),
    /// The search itself found no acceptable step.
    Failure(LineSearchFailure),
}

impl<E: std::error::Error + 'static> From<Halt<E>> for LineSearchError<E> {
    fn from(halt: Halt<E>) -> Self {
        LineSearchError::Halt(halt)
    }
}

impl<E: std::error::Error + 'static> From<LineSearchFailure> for LineSearchError<E> {
    fn from(failure: LineSearchFailure) -> Self {
        LineSearchError::Failure(failure)
    }
}

/// Searches along `p` from `start` for a step satisfying the strong Wolfe
/// conditions, approximating gradients by `scheme` when the objective has
/// none.
///
/// # Errors
///
/// [`LineSearchFailure::NotDescent`] before any evaluation when `p` does not
/// descend, [`LineSearchFailure::NoStrongWolfeStep`] when the step or trial
/// budget runs out, and [`LineSearchError::Halt`] when the evaluation budget
/// runs out, a value is nonfinite or the objective fails.
pub(crate) fn strong_wolfe<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    scheme: FiniteDifference,
    start: Start<'_>,
    p: &[f64],
    params: &Wolfe,
) -> Result<Step, LineSearchError<O::Error>> {
    let dphi0 = dot(start.g, p);
    if dphi0.is_nan() || dphi0 >= 0.0 {
        return Err(LineSearchFailure::NotDescent.into());
    }
    let mut search = Search {
        counters,
        objective,
        scheme,
        start,
        p,
        params,
        dphi0,
        trials: 0,
    };
    let mut previous = Known {
        alpha: 0.0,
        f: start.f,
        dphi: dphi0,
    };
    let mut alpha = params.alpha0.min(params.amax);
    loop {
        let (x, f) = search.value(alpha)?;
        if !search.decreases(alpha, f) || (previous.alpha > 0.0 && f >= previous.f) {
            return search.zoom(
                previous,
                Point {
                    alpha,
                    f,
                    dphi: None,
                },
            );
        }
        let (dphi, g) = search.slope(&x, f)?;
        if search.flat(dphi) {
            return Ok(Step { alpha, x, f, g });
        }
        let current = Known { alpha, f, dphi };
        if dphi >= 0.0 {
            return search.zoom(current, previous.into());
        }
        if alpha >= params.amax {
            return Err(LineSearchFailure::NoStrongWolfeStep.into());
        }
        previous = current;
        alpha = (2.0 * alpha).min(params.amax);
    }
}

/// A trial whose slope was evaluated.
#[derive(Debug, Clone, Copy)]
struct Known {
    alpha: f64,
    f: f64,
    dphi: f64,
}

/// A trial whose slope may not have been evaluated.
#[derive(Debug, Clone, Copy)]
struct Point {
    alpha: f64,
    f: f64,
    dphi: Option<f64>,
}

impl From<Known> for Point {
    fn from(known: Known) -> Self {
        Self {
            alpha: known.alpha,
            f: known.f,
            dphi: Some(known.dphi),
        }
    }
}

struct Search<'s, 'a, O> {
    counters: &'s mut Counters,
    objective: &'s mut O,
    scheme: FiniteDifference,
    start: Start<'a>,
    p: &'s [f64],
    params: &'s Wolfe,
    dphi0: f64,
    trials: usize,
}

impl<O: Objective> Search<'_, '_, O> {
    fn value(&mut self, alpha: f64) -> Result<(Vec<f64>, f64), LineSearchError<O::Error>> {
        if self.trials >= self.params.maxiter {
            return Err(LineSearchFailure::NoStrongWolfeStep.into());
        }
        self.trials += 1;
        let pairs = self.start.x.iter().zip(self.p);
        let x: Vec<f64> = pairs.map(|(xi, pi)| xi + alpha * pi).collect();
        let f = self.counters.value(self.objective, &x)?;
        if !f.is_finite() {
            return Err(Halt::Terminated(Termination::Nonfinite).into());
        }
        Ok((x, f))
    }

    fn slope(&mut self, x: &[f64], f: f64) -> Result<(f64, Vec<f64>), LineSearchError<O::Error>> {
        let mut g = vec![0.0; x.len()];
        finite_difference::gradient_with_step(
            self.counters,
            self.objective,
            x,
            f,
            self.scheme,
            self.params.eps,
            &mut g,
        )?;
        Ok((dot(&g, self.p), g))
    }

    fn decreases(&self, alpha: f64, f: f64) -> bool {
        f <= self.start.f + self.params.c1 * alpha * self.dphi0
    }

    fn flat(&self, dphi: f64) -> bool {
        dphi.abs() <= -self.params.c2 * self.dphi0
    }

    fn zoom(&mut self, mut lo: Known, mut hi: Point) -> Result<Step, LineSearchError<O::Error>> {
        loop {
            let width = hi.alpha - lo.alpha;
            if width.abs() <= f64::EPSILON * lo.alpha.abs().max(hi.alpha.abs()) {
                return Err(LineSearchFailure::NoStrongWolfeStep.into());
            }
            let alpha = interpolate(lo, hi);
            let (x, f) = self.value(alpha)?;
            if !self.decreases(alpha, f) || f >= lo.f {
                hi = Point {
                    alpha,
                    f,
                    dphi: None,
                };
                continue;
            }
            let (dphi, g) = self.slope(&x, f)?;
            if self.flat(dphi) {
                return Ok(Step { alpha, x, f, g });
            }
            if dphi * width >= 0.0 {
                hi = lo.into();
            }
            lo = Known { alpha, f, dphi };
        }
    }
}

/// The minimizer of the cubic through both endpoints, or of the quadratic when
/// `hi` has no slope, kept inside the bracket by [`SAFEGUARD`] else bisected.
fn interpolate(lo: Known, hi: Point) -> f64 {
    let (a, b) = (lo.alpha, hi.alpha);
    let candidate = match hi.dphi {
        Some(db) => {
            let d1 = lo.dphi + db - 3.0 * (lo.f - hi.f) / (a - b);
            let d2 = (b - a).signum() * (d1 * d1 - lo.dphi * db).sqrt();
            b - (b - a) * (db + d2 - d1) / (db - lo.dphi + 2.0 * d2)
        }
        None => {
            let span = b - a;
            a - lo.dphi * span * span / (2.0 * (hi.f - lo.f - lo.dphi * span))
        }
    };
    let margin = SAFEGUARD * (b - a).abs();
    let (low, high) = (a.min(b) + margin, a.max(b) - margin);
    if candidate.is_finite() && (low..=high).contains(&candidate) {
        candidate
    } else {
        0.5 * (a + b)
    }
}

fn dot(u: &[f64], v: &[f64]) -> f64 {
    u.iter().zip(v).map(|(ui, vi)| ui * vi).sum()
}

#[cfg(test)]
mod tests {
    use super::{LineSearchError, LineSearchFailure, Start, Step, Wolfe, dot, strong_wolfe};
    use crate::finite_difference::{FiniteDifference, gradient};
    use crate::{Common, Counters, Halt, Objective, Termination};
    use std::convert::Infallible;

    const C1: f64 = 1e-4;
    const C2: f64 = 0.9;

    struct Analytic(fn(&[f64]) -> (f64, Vec<f64>));

    impl Objective for Analytic {
        type Error = Infallible;

        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            Ok((self.0)(x).0)
        }

        fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
            out.copy_from_slice(&(self.0)(x).1);
            Ok(true)
        }
    }

    fn quadratic(x: &[f64]) -> (f64, Vec<f64>) {
        (
            0.5 * (3.0 * x[0] * x[0] + x[1] * x[1]),
            vec![3.0 * x[0], x[1]],
        )
    }

    fn rosenbrock(x: &[f64]) -> (f64, Vec<f64>) {
        let r = x[1] - x[0] * x[0];
        let f = 100.0 * r * r + (1.0 - x[0]).powi(2);
        (f, vec![-400.0 * x[0] * r - 2.0 * (1.0 - x[0]), 200.0 * r])
    }

    type Run<E> = (Result<Step, LineSearchError<E>>, Counters);

    fn run<O: Objective>(o: &mut O, x: &[f64], p: &[f64], w: Wolfe, c: &Common) -> Run<O::Error> {
        let scheme = FiniteDifference::Forward;
        let f = o.value(x).ok().unwrap();
        let mut g = vec![0.0; x.len()];
        let mut free = Counters::new(&Common::default());
        gradient(&mut free, o, x, f, scheme, &mut g).ok().unwrap();
        let mut counters = Counters::new(c);
        let start = Start { x, f, g: &g };
        (
            strong_wolfe(&mut counters, o, scheme, start, p, &w),
            counters,
        )
    }

    fn plain<O: Objective>(o: &mut O, x: &[f64], p: &[f64]) -> Run<O::Error> {
        run(o, x, p, Wolfe::new(1.0, 10.0, 20), &Common::default())
    }

    #[test]
    fn the_newton_step_on_a_quadratic_is_accepted_at_once() {
        let (step, counters) = plain(&mut Analytic(quadratic), &[1.0, -2.0], &[-1.0, 2.0]);
        let step = step.ok().unwrap();
        assert_eq!((step.alpha, step.f), (1.0, 0.0));
        assert_eq!((step.x, step.g), (vec![0.0, 0.0], vec![0.0, 0.0]));
        assert_eq!((counters.nfev(), counters.njev()), (1, 1));
    }

    #[test]
    fn zoom_lands_on_the_exact_steepest_descent_step_of_a_quadratic() {
        let (step, _) = plain(&mut Analytic(quadratic), &[1.0, 1.0], &[-3.0, -1.0]);
        assert!((step.ok().unwrap().alpha - 10.0 / 28.0).abs() < 1e-12);
    }

    #[test]
    fn every_step_of_steepest_descent_on_rosenbrock_is_strong_wolfe() {
        let (mut x, mut counters) = (vec![-1.2, 1.0], Counters::new(&Common::default()));
        let (mut f, mut g) = rosenbrock(&x);
        for _ in 0..25 {
            let p: Vec<f64> = g.iter().map(|gi| -gi).collect();
            let dphi0 = dot(&g, &p);
            let start = Start { x: &x, f, g: &g };
            let w = Wolfe::new(1.0, 100.0, 60);
            let scheme = FiniteDifference::Forward;
            let objective = &mut Analytic(rosenbrock);
            let step = strong_wolfe(&mut counters, objective, scheme, start, &p, &w);
            let step = step.ok().unwrap();
            assert!(step.f <= f + C1 * step.alpha * dphi0, "sufficient decrease");
            assert!(dot(&step.g, &p).abs() <= C2 * dphi0.abs(), "curvature");
            (x, f, g) = (step.x, step.f, step.g);
        }
        assert!(f < 24.2);
    }

    #[test]
    fn an_ascent_direction_is_not_descent_and_costs_nothing() {
        let (step, counters) = plain(&mut Analytic(quadratic), &[1.0, 1.0], &[3.0, 1.0]);
        let failure = LineSearchFailure::NotDescent;
        assert!(matches!(step, Err(LineSearchError::Failure(f)) if f == failure));
        assert_eq!((counters.nfev(), counters.njev()), (0, 0));
    }

    #[test]
    fn finite_difference_gradients_serve_the_search() {
        let mut objective = |x: &[f64]| -> Result<f64, Infallible> { Ok(quadratic(x).0) };
        let (step, counters) = plain(&mut objective, &[1.0, -2.0], &[-1.0, 2.0]);
        assert_eq!(step.ok().unwrap().alpha, 1.0);
        assert_eq!((counters.nfev(), counters.njev()), (3, 1));
    }

    #[test]
    fn an_unbounded_line_finds_no_strong_wolfe_step() {
        let mut objective = |x: &[f64]| -> Result<f64, Infallible> { Ok(-x[0]) };
        for (amax, maxiter, trials) in [(4.0, 20, 3), (1e6, 2, 2)] {
            let w = Wolfe::new(1.0, amax, maxiter);
            let (step, counters) = run(&mut objective, &[0.0], &[1.0], w, &Common::default());
            let failure = LineSearchFailure::NoStrongWolfeStep;
            assert!(matches!(step, Err(LineSearchError::Failure(f)) if f == failure));
            assert_eq!((counters.nfev(), counters.njev()), (2 * trials, trials));
        }
    }

    #[test]
    fn an_exhausted_budget_halts_the_search() {
        let common = Common {
            maxfev: Some(1),
            ..Common::default()
        };
        let w = Wolfe::new(1.0, 10.0, 20);
        let (step, counters) = run(
            &mut Analytic(rosenbrock),
            &[-1.2, 1.0],
            &[215.6, 88.0],
            w,
            &common,
        );
        let budget = Termination::MaxEvaluations;
        assert!(matches!(step, Err(LineSearchError::Halt(Halt::Terminated(t))) if t == budget));
        assert_eq!((counters.nfev(), counters.njev()), (1, 0));
    }

    #[test]
    fn a_nonfinite_trial_value_halts_the_search() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut objective =
                |x: &[f64]| -> Result<f64, Infallible> { Ok(if x[0] > 0.5 { bad } else { -x[0] }) };
            let (step, _) = plain(&mut objective, &[0.0], &[1.0]);
            let nonfinite = Termination::Nonfinite;
            assert!(
                matches!(step, Err(LineSearchError::Halt(Halt::Terminated(t))) if t == nonfinite)
            );
        }
    }
}
