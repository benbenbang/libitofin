use crate::{
    Bounds, Common, ConstraintKind, Converged, Flow, IterationState, Method, Minimize, Objective,
    Problem, SlsqpOptions, Termination, minimize,
};
use std::convert::Infallible;

type Function = fn(&[f64]) -> f64;

/// An objective with plain-function constraints, cancelling after `stop_after`
/// iterations when set, and counting the calls it receives.
struct Constrained {
    f: Function,
    constraints: Vec<(ConstraintKind, Function)>,
    stop_after: Option<usize>,
    value_calls: usize,
    constraint_calls: usize,
}

impl Constrained {
    fn new(f: Function, constraints: Vec<(ConstraintKind, Function)>) -> Self {
        Self {
            f,
            constraints,
            stop_after: None,
            value_calls: 0,
            constraint_calls: 0,
        }
    }
}

impl Objective for Constrained {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.value_calls += 1;
        Ok((self.f)(x))
    }

    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        Ok(match self.stop_after {
            Some(nit) if state.nit >= nit => Flow::Stop,
            _ => Flow::Continue,
        })
    }

    fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    fn constraint_kind(&self, i: usize) -> ConstraintKind {
        self.constraints[i].0
    }

    fn constraint(&mut self, i: usize, x: &[f64]) -> Result<f64, Self::Error> {
        self.constraint_calls += 1;
        Ok((self.constraints[i].1)(x))
    }
}

fn run(objective: &mut Constrained, x0: &[f64], bounds: Option<Bounds>, ftol: f64) -> Minimize {
    let problem = Problem {
        x0: x0.to_vec(),
        bounds,
    };
    let method = Method::Slsqp(SlsqpOptions { ftol: Some(ftol) });
    minimize(objective, &problem, &method, &Common::default()).expect("infallible")
}

fn box_bounds(lower: f64, upper: f64, n: usize) -> Option<Bounds> {
    Some(Bounds {
        lower: vec![lower; n],
        upper: vec![upper; n],
    })
}

fn max_violation(objective: &Constrained, x: &[f64]) -> f64 {
    objective
        .constraints
        .iter()
        .map(|(kind, c)| match kind {
            ConstraintKind::Eq => c(x).abs(),
            ConstraintKind::Ineq => (-c(x)).max(0.0),
        })
        .fold(0.0, f64::max)
}

fn assert_solved(result: &Minimize, x: &[f64], fun: f64, tol: f64) {
    assert_eq!(
        result.status,
        Termination::Converged(Converged::FTol),
        "{result:?}"
    );
    assert!(result.success);
    assert!((result.fun - fun).abs() <= tol, "{result:?}");
    for (ours, expected) in result.x.iter().zip(x) {
        assert!((ours - expected).abs() <= tol.sqrt(), "{result:?}");
    }
}

/// `min x0^2 + 2 x1^2 + 3 x2^2` on `x0 + x1 + x2 = 1`: stationarity gives
/// `x_i = lambda / (2 w_i)`, so `lambda = 12 / 11`, `x = (6, 3, 2) / 11` and
/// `f = 6 / 11`.
#[test]
fn an_equality_constrained_quadratic_reaches_its_closed_form() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0] + 2.0 * x[1] * x[1] + 3.0 * x[2] * x[2],
        vec![(ConstraintKind::Eq, |x| x[0] + x[1] + x[2] - 1.0)],
    );
    let result = run(&mut objective, &[1.0, 1.0, 1.0], None, 1e-10);
    assert_solved(
        &result,
        &[6.0 / 11.0, 3.0 / 11.0, 2.0 / 11.0],
        6.0 / 11.0,
        1e-9,
    );
    let multipliers = result.multipliers.expect("SLSQP reports multipliers");
    assert!(
        (multipliers[0] - 12.0 / 11.0).abs() <= 1e-5,
        "{multipliers:?}"
    );
    assert!(result.message.contains("maximum constraint violation"));
}

/// Hock and Schittkowski problem 1: Rosenbrock with `x1 >= -1.5`, `f* = 0` at
/// `(1, 1)`.
#[test]
fn hock_schittkowski_1_uses_the_bounds_as_inequalities() {
    let mut objective = Constrained::new(
        |x| 100.0 * (x[1] - x[0] * x[0]).powi(2) + (1.0 - x[0]).powi(2),
        Vec::new(),
    );
    let bounds = Bounds {
        lower: vec![f64::NEG_INFINITY, -1.5],
        upper: vec![f64::INFINITY; 2],
    };
    let result = run(&mut objective, &[-2.0, 1.0], Some(bounds), 1e-12);
    assert_solved(&result, &[1.0, 1.0], 0.0, 1e-8);
    assert_eq!(result.multipliers, Some(Vec::new()));
}

/// Hock and Schittkowski problem 35: `f* = 1 / 9` at `(4 / 3, 7 / 9, 4 / 9)`.
#[test]
fn hock_schittkowski_35_reaches_the_published_optimum() {
    let mut objective = Constrained::new(
        |x| {
            9.0 - 8.0 * x[0] - 6.0 * x[1] - 4.0 * x[2]
                + 2.0 * x[0] * x[0]
                + 2.0 * x[1] * x[1]
                + x[2] * x[2]
                + 2.0 * x[0] * x[1]
                + 2.0 * x[0] * x[2]
        },
        vec![(ConstraintKind::Ineq, |x| 3.0 - x[0] - x[1] - 2.0 * x[2])],
    );
    let bounds = box_bounds(0.0, f64::INFINITY, 3);
    let result = run(&mut objective, &[0.5, 0.5, 0.5], bounds, 1e-10);
    assert_solved(&result, &[4.0 / 3.0, 7.0 / 9.0, 4.0 / 9.0], 1.0 / 9.0, 1e-8);
    let multipliers = result.multipliers.expect("SLSQP reports multipliers");
    assert!(
        (multipliers[0] - 2.0 / 9.0).abs() <= 1e-4,
        "{multipliers:?}"
    );
}

/// Hock and Schittkowski problem 71: `f* = 17.0140173` at
/// `(1, 4.7429994, 3.8211503, 1.3794082)`.
#[test]
fn hock_schittkowski_71_reaches_the_published_optimum() {
    let mut objective = Constrained::new(
        |x| x[0] * x[3] * (x[0] + x[1] + x[2]) + x[2],
        vec![
            (ConstraintKind::Ineq, |x| x[0] * x[1] * x[2] * x[3] - 25.0),
            (ConstraintKind::Eq, |x| {
                x.iter().map(|v| v * v).sum::<f64>() - 40.0
            }),
        ],
    );
    let result = run(
        &mut objective,
        &[1.0, 5.0, 5.0, 1.0],
        box_bounds(1.0, 5.0, 4),
        1e-10,
    );
    let x = [1.0, 4.742_999_4, 3.821_150_3, 1.379_408_2];
    assert_solved(&result, &x, 17.014_017_3, 1e-7);
    assert!(max_violation(&objective, &result.x) <= 1e-8);
}

/// The box `[0, 1]^2` meets `x0 + x1 >= 2` only at `(1, 1)`.
#[test]
fn a_single_feasible_point_is_found() {
    let mut objective = Constrained::new(
        |x| (x[0] - 3.0).powi(2) + (x[1] + 1.0).powi(2),
        vec![(ConstraintKind::Ineq, |x| x[0] + x[1] - 2.0)],
    );
    let result = run(&mut objective, &[0.2, 0.3], box_bounds(0.0, 1.0, 2), 1e-10);
    assert_solved(&result, &[1.0, 1.0], 8.0, 1e-9);
}

#[test]
fn contradictory_constraints_are_infeasible() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0] + x[1] * x[1],
        vec![
            (ConstraintKind::Ineq, |x| x[0] - 1.0),
            (ConstraintKind::Ineq, |x| -x[0]),
        ],
    );
    let result = run(&mut objective, &[0.5, 0.5], None, 1e-6);
    assert_eq!(result.status, Termination::Infeasible);
    assert!(!result.success);
    assert_eq!(
        result.message,
        "the constraints are infeasible; maximum constraint violation 5.000e-1"
    );
}

/// The second equality is twice the first, so the equality Jacobian has rank
/// one everywhere and only the relaxed subproblem can take a step.
#[test]
fn a_rank_deficient_equality_jacobian_is_solved_to_feasibility() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0] + 2.0 * x[1] * x[1],
        vec![
            (ConstraintKind::Eq, |x| x[0] + x[1] - 1.0),
            (ConstraintKind::Eq, |x| 2.0 * x[0] + 2.0 * x[1] - 2.0),
        ],
    );
    let result = run(&mut objective, &[3.0, -1.0], None, 1e-10);
    assert!(result.success, "{result:?}");
    assert!(max_violation(&objective, &result.x) <= 1e-8, "{result:?}");
    assert!((result.x[0] - 2.0 / 3.0).abs() <= 1e-6, "{result:?}");
}

#[test]
fn the_callback_cancels_the_run() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0] + 2.0 * x[1] * x[1] + 3.0 * x[2] * x[2],
        vec![(ConstraintKind::Eq, |x| x[0] + x[1] + x[2] - 1.0)],
    );
    objective.stop_after = Some(1);
    let result = run(&mut objective, &[1.0, 1.0, 1.0], None, 1e-10);
    assert_eq!(result.status, Termination::Cancelled);
    assert_eq!(result.nit, 1);
}

#[test]
fn a_nonfinite_constraint_ends_the_run_at_the_last_iterate() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0],
        vec![(ConstraintKind::Ineq, |x| {
            if x[0] < 2.0 { f64::NAN } else { x[0] - 1.0 }
        })],
    );
    let result = run(&mut objective, &[3.0], None, 1e-10);
    assert_eq!(result.status, Termination::Nonfinite);
    assert_eq!(result.x, [3.0]);
    assert_eq!(result.fun, 9.0);
}

#[test]
fn constraint_evaluations_are_not_charged() {
    let mut objective = Constrained::new(
        |x| x[0] * x[0] + x[1] * x[1],
        vec![(ConstraintKind::Eq, |x| x[0] + x[1] - 1.0)],
    );
    let result = run(&mut objective, &[0.0, 0.0], None, 1e-10);
    assert!(result.success);
    assert_eq!(result.nfev, objective.value_calls);
    assert!(objective.constraint_calls > 0);
}
