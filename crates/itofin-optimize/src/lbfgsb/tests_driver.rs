use crate::{
    Bounds, Common, Converged, FiniteDifference, Flow, InvalidInput, IterationState, LbfgsbOptions,
    Method, MinimizeError, Objective, Problem, Termination, minimize,
};
use std::convert::Infallible;

fn run<O: Objective>(
    objective: &mut O,
    x0: Vec<f64>,
    bounds: Bounds,
    options: LbfgsbOptions,
) -> Result<crate::Minimize, MinimizeError<O::Error>> {
    minimize(
        objective,
        &Problem {
            x0,
            bounds: Some(bounds),
        },
        &Method::Lbfgsb(options),
        &Common::default(),
    )
}

struct Rosenbrock {
    analytic: bool,
    bounds: Bounds,
}

impl Objective for Rosenbrock {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        assert!(
            x.iter()
                .enumerate()
                .all(|(i, &xi)| xi >= self.bounds.lower[i] && xi <= self.bounds.upper[i])
        );
        Ok(x.windows(2)
            .map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                100.0 * (b - a * a).powi(2) + (1.0 - a).powi(2)
            })
            .sum())
    }

    fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
        if self.analytic {
            out.fill(0.0);
            for i in 0..x.len() - 1 {
                let (a, b) = (x[i], x[i + 1]);
                out[i] += -400.0 * a * (b - a * a) - 2.0 * (1.0 - a);
                out[i + 1] += 200.0 * (b - a * a);
            }
        }
        Ok(self.analytic)
    }
}

#[test]
fn face_solution_needs_projected_path_and_keeps_all_probes_in_bounds() {
    let bounds = Bounds {
        lower: vec![0.0, -2.0],
        upper: vec![0.5, 2.0],
    };
    let mut objective = Rosenbrock {
        analytic: false,
        bounds: bounds.clone(),
    };
    let result = run(
        &mut objective,
        vec![-1.2, 1.0],
        bounds,
        LbfgsbOptions::default(),
    )
    .unwrap();
    assert!(result.success, "{result:?}");
    assert!(result.x[0] <= 0.5 && result.x[0] > 0.49, "{result:?}");
    assert!((result.x[1] - 0.25).abs() < 1e-3, "{result:?}");
    assert!((result.fun - 0.25).abs() < 1e-4, "{result:?}");
    objective.analytic = true;
    let mut gradient = [0.0; 2];
    objective.gradient(&result.x, &mut gradient).unwrap();
    let projected = if gradient[0] < 0.0 && result.x[0] == 0.5 {
        0.0
    } else {
        gradient[0].abs()
    };
    assert!(projected.max(gradient[1].abs()) < 1e-3, "{result:?}");
}

#[test]
fn rosenbrock_inside_box_and_extended_dimension_converge() {
    for n in [2, 25] {
        let bounds = Bounds {
            lower: vec![-2.0; n],
            upper: vec![2.0; n],
        };
        let mut objective = Rosenbrock {
            analytic: true,
            bounds: bounds.clone(),
        };
        let x0: Vec<f64> = (0..n)
            .map(|i| if i % 2 == 0 { -1.2 } else { 1.0 })
            .collect();
        let options = LbfgsbOptions {
            maxcor: Some(5),
            ..LbfgsbOptions::default()
        };
        let result = run(&mut objective, x0, bounds, options).unwrap();
        assert!(result.success && result.fun < 1e-6, "n={n} {result:?}");
    }
}

#[test]
fn a_linear_objective_accepts_the_limiting_face_step() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1.0],
    };
    let mut objective = |x: &[f64]| -> Result<f64, Infallible> {
        assert!((0.0..=1.0).contains(&x[0]));
        Ok(-x[0])
    };
    let result = run(&mut objective, vec![0.5], bounds, LbfgsbOptions::default()).unwrap();
    assert_eq!(result.x, vec![1.0]);
    assert_eq!(result.status, Termination::Converged(Converged::GTol));
}

#[test]
fn method_rejects_constraints_and_invalid_options_before_evaluation() {
    struct Constrained;
    impl Objective for Constrained {
        type Error = Infallible;
        fn value(&mut self, _: &[f64]) -> Result<f64, Self::Error> {
            panic!("must not evaluate")
        }
        fn constraint_count(&self) -> usize {
            1
        }
    }
    let problem = Problem {
        x0: vec![0.0],
        bounds: None,
    };
    let result = minimize(
        &mut Constrained,
        &problem,
        &Method::Lbfgsb(LbfgsbOptions::default()),
        &Common::default(),
    );
    assert!(matches!(
        result,
        Err(MinimizeError::InvalidInput(InvalidInput::Unsupported {
            method: "L-BFGS-B",
            option: "constraints"
        }))
    ));
    let result = minimize(
        &mut Constrained,
        &problem,
        &Method::Lbfgsb(LbfgsbOptions {
            maxcor: Some(0),
            ..LbfgsbOptions::default()
        }),
        &Common::default(),
    );
    assert!(matches!(
        result,
        Err(MinimizeError::InvalidInput(InvalidInput::NotPositive {
            option: "maxcor"
        }))
    ));
}

struct Stop;
impl Objective for Stop {
    type Error = Infallible;
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        Ok((x[0] - 2.0).powi(2))
    }
    fn callback(&mut self, _: &IterationState<'_>) -> Result<Flow, Self::Error> {
        Ok(Flow::Stop)
    }
}

#[test]
fn callback_and_evaluation_budget_keep_the_last_accepted_point() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![3.0],
    };
    let cancelled = run(
        &mut Stop,
        vec![0.0],
        bounds.clone(),
        LbfgsbOptions::default(),
    )
    .unwrap();
    assert_eq!(cancelled.status, Termination::Cancelled);
    assert_eq!(cancelled.nit, 1);
    assert!(cancelled.fun < 4.0);
    let mut objective = |x: &[f64]| -> Result<f64, Infallible> { Ok((x[0] - 2.0).powi(2)) };
    let budget = Common {
        maxfev: Some(1),
        ..Common::default()
    };
    let exhausted = minimize(
        &mut objective,
        &Problem {
            x0: vec![0.0],
            bounds: Some(bounds),
        },
        &Method::Lbfgsb(LbfgsbOptions {
            finite_difference: FiniteDifference::Central,
            ..LbfgsbOptions::default()
        }),
        &budget,
    )
    .unwrap();
    assert_eq!(exhausted.status, Termination::MaxEvaluations);
    assert_eq!(exhausted.nfev, 1);
    assert_eq!(exhausted.x, vec![0.0]);
}
