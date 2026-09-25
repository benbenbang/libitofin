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

#[cfg(test)]
mod tests {
    use super::{InverseHessian, dot};
    use crate::{
        BfgsOptions, Bounds, Common, Converged, FiniteDifference, Flow, InvalidInput,
        IterationState, Method, Minimize, MinimizeError, Norm, Objective, Problem, Termination,
        minimize,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
    #[error("boom at evaluation {0}")]
    struct Boom(usize);

    fn rosenbrock(x: &[f64]) -> (f64, Vec<f64>) {
        let r = x[1] - x[0] * x[0];
        let f = 100.0 * r * r + (1.0 - x[0]).powi(2);
        (f, vec![-400.0 * x[0] * r - 2.0 * (1.0 - x[0]), 200.0 * r])
    }

    fn inf_norm(g: &[f64]) -> f64 {
        g.iter().fold(0.0, |largest, gi| gi.abs().max(largest))
    }

    /// Rosenbrock with an optional analytic gradient and switchable faults.
    #[derive(Default)]
    struct Model {
        analytic: bool,
        offset: f64,
        stop_at: Option<usize>,
        fail_at: Option<usize>,
        nan_at: Option<usize>,
        calls: usize,
    }

    impl Objective for Model {
        type Error = Boom;

        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            self.calls += 1;
            if self.fail_at == Some(self.calls) {
                return Err(Boom(self.calls));
            }
            if self.nan_at == Some(self.calls) {
                return Ok(f64::NAN);
            }
            Ok(rosenbrock(x).0)
        }

        fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
            if !self.analytic {
                return Ok(false);
            }
            out.copy_from_slice(&rosenbrock(x).1);
            out[0] += self.offset;
            Ok(true)
        }

        fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, Self::Error> {
            Ok(if self.stop_at == Some(state.nit) {
                Flow::Stop
            } else {
                Flow::Continue
            })
        }
    }

    fn analytic() -> Model {
        Model {
            analytic: true,
            ..Model::default()
        }
    }

    fn run<O: Objective>(
        objective: &mut O,
        x0: Vec<f64>,
        options: BfgsOptions,
        common: &Common,
    ) -> Result<Minimize, MinimizeError<O::Error>> {
        let problem = Problem { x0, bounds: None };
        minimize(objective, &problem, &Method::Bfgs(options), common)
    }

    fn rosen<O: Objective>(objective: &mut O, common: &Common) -> Minimize {
        run(objective, vec![-1.2, 1.0], BfgsOptions::default(), common)
            .ok()
            .unwrap()
    }

    #[test]
    fn rosenbrock_converges_with_the_same_njev_analytic_or_finite_difference() {
        let exact = rosen(&mut analytic(), &Common::default());
        let approximate = rosen(&mut Model::default(), &Common::default());
        for result in [&exact, &approximate] {
            assert_eq!(result.status, Termination::Converged(Converged::GTol));
            assert!(result.success && result.fun < 1e-10, "{result:?}");
        }
        assert_eq!(exact.njev, approximate.njev);
        assert!(approximate.nfev > exact.nfev);
    }

    #[test]
    fn an_ill_conditioned_quadratic_converges_within_15_n_iterations() {
        let n = 10;
        let d: Vec<f64> = (0..n).map(|i| 10f64.powf(6.0 * i as f64 / 9.0)).collect();
        let mut quadratic = Analytic(|x: &[f64]| {
            let g: Vec<f64> = x.iter().zip(&d).map(|(xi, di)| di * xi).collect();
            (0.5 * dot(x, &g), g)
        });
        let result = run(
            &mut quadratic,
            vec![1.0; n],
            BfgsOptions::default(),
            &Common::default(),
        )
        .ok()
        .unwrap();
        assert_eq!(result.status, Termination::Converged(Converged::GTol));
        let g: Vec<f64> = result.x.iter().zip(&d).map(|(xi, di)| di * xi).collect();
        assert!(inf_norm(&g) <= 1e-5);
        assert!(result.nit <= 15 * n, "nit {}", result.nit);
    }

    struct Analytic<F>(F);

    impl<F: FnMut(&[f64]) -> (f64, Vec<f64>)> Objective for Analytic<F> {
        type Error = Boom;

        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            Ok((self.0)(x).0)
        }

        fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
            out.copy_from_slice(&(self.0)(x).1);
            Ok(true)
        }
    }

    #[test]
    fn a_wrong_gradient_ends_in_a_line_search_failure_at_a_finite_better_point() {
        let mut wrong = Model {
            offset: 1.0,
            ..analytic()
        };
        let result = rosen(&mut wrong, &Common::default());
        assert_eq!(result.status, Termination::LineSearchFailed);
        assert!(!result.success);
        assert!(result.x.iter().all(|xi| xi.is_finite()));
        assert!(result.fun < rosenbrock(&[-1.2, 1.0]).0);
    }

    #[test]
    fn an_update_without_positive_curvature_is_skipped_and_leaves_h_unchanged() {
        for y in [[-1.0, 0.0], [0.0, 1.0], [f64::NAN, 1.0]] {
            let mut inverse = InverseHessian::identity(2);
            assert!(!inverse.update(&[1.0, 0.0], &y));
            assert_eq!(inverse.h, vec![1.0, 0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn an_accepted_update_satisfies_the_secant_condition() {
        let mut inverse = InverseHessian::identity(2);
        let (s, y) = ([1.0, 0.5], [2.0, 3.0]);
        assert!(inverse.update(&s, &y));
        let hy = inverse.times(&y);
        assert!((hy[0] - s[0]).abs() < 1e-12 && (hy[1] - s[1]).abs() < 1e-12);
        assert_eq!(inverse.h[1], inverse.h[2]);
    }

    #[test]
    fn a_stationary_start_converges_in_zero_iterations() {
        let result = run(
            &mut analytic(),
            vec![1.0, 1.0],
            BfgsOptions::default(),
            &Common::default(),
        )
        .ok()
        .unwrap();
        assert_eq!(result.status, Termination::Converged(Converged::GTol));
        assert_eq!((result.nit, result.nfev, result.njev), (0, 1, 1));
    }

    #[test]
    fn budgets_and_the_callback_stop_the_run() {
        let iterations = Common {
            maxiter: Some(3),
            ..Common::default()
        };
        let result = rosen(&mut analytic(), &iterations);
        assert_eq!((result.status, result.nit), (Termination::MaxIterations, 3));
        let evaluations = Common {
            maxfev: Some(10),
            ..Common::default()
        };
        let result = rosen(&mut Model::default(), &evaluations);
        assert_eq!(result.status, Termination::MaxEvaluations);
        assert_eq!(result.nfev, 10);
        let mut stopping = Model {
            stop_at: Some(2),
            ..analytic()
        };
        let result = rosen(&mut stopping, &Common::default());
        assert_eq!((result.status, result.nit), (Termination::Cancelled, 2));
        assert!(result.fun < rosenbrock(&[-1.2, 1.0]).0);
    }

    #[test]
    fn an_objective_error_is_returned_by_value() {
        let mut failing = Model {
            fail_at: Some(5),
            ..analytic()
        };
        let result = run(
            &mut failing,
            vec![-1.2, 1.0],
            BfgsOptions::default(),
            &Common::default(),
        );
        assert!(matches!(result, Err(MinimizeError::Objective(Boom(5)))));
    }

    #[test]
    fn a_nan_ends_the_run_nonfinite_at_the_last_finite_iterate() {
        let mut at_start = Model {
            nan_at: Some(1),
            ..analytic()
        };
        let result = rosen(&mut at_start, &Common::default());
        assert_eq!(result.status, Termination::Nonfinite);
        assert_eq!((result.x, result.nit), (vec![-1.2, 1.0], 0));
        assert!(result.fun.is_nan());
        let mut midway = Model {
            nan_at: Some(6),
            ..Model::default()
        };
        let result = rosen(&mut midway, &Common::default());
        assert_eq!(result.status, Termination::Nonfinite);
        assert!(result.fun.is_finite() && result.x.iter().all(|xi| xi.is_finite()));
    }

    #[test]
    fn bounds_and_a_bad_gtol_are_rejected() {
        let problem = Problem {
            x0: vec![0.0],
            bounds: Some(Bounds {
                lower: vec![-1.0],
                upper: vec![1.0],
            }),
        };
        let method = Method::Bfgs(BfgsOptions::default());
        let result = minimize(&mut analytic(), &problem, &method, &Common::default());
        let unsupported = InvalidInput::Unsupported {
            method: "BFGS",
            option: "bounds",
        };
        assert!(matches!(result, Err(MinimizeError::InvalidInput(e)) if e == unsupported));
        for gtol in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let options = BfgsOptions {
                gtol: Some(gtol),
                ..BfgsOptions::default()
            };
            let result = run(&mut analytic(), vec![0.0, 0.0], options, &Common::default());
            let invalid = InvalidInput::NotFinitePositive { option: "gtol" };
            assert!(matches!(result, Err(MinimizeError::InvalidInput(e)) if e == invalid));
        }
    }

    #[test]
    fn gtol_takes_the_option_then_common_tol_then_the_default() {
        let loose = Common {
            tol: Some(1e-1),
            ..Common::default()
        };
        let shared = rosen(&mut analytic(), &loose);
        let g = rosenbrock(&shared.x).1;
        assert!(inf_norm(&g) <= 1e-1 && inf_norm(&g) > 1e-5);
        let options = BfgsOptions {
            gtol: Some(1e-8),
            norm: Norm::Two,
            finite_difference: FiniteDifference::Central,
        };
        let explicit = run(&mut analytic(), vec![-1.2, 1.0], options, &loose)
            .ok()
            .unwrap();
        let g = rosenbrock(&explicit.x).1;
        assert_eq!(explicit.status, Termination::Converged(Converged::GTol));
        assert!(dot(&g, &g).sqrt() <= 1e-8);
    }
}
