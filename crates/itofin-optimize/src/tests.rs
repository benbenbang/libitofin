use crate::{
    Bounds, Common, Converged, Counters, Flow, Halt, InvalidInput, IterationState, Method,
    Minimize, MinimizeError, NelderMeadOptions, Objective, Problem, Termination, minimize,
};
use std::convert::Infallible;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum ProbeError {
    #[error("boom")]
    Boom,
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    value_fails: bool,
    callback: Result<Flow, ProbeError>,
}

fn probe(value_fails: bool, callback: Result<Flow, ProbeError>) -> Probe {
    Probe {
        value_fails,
        callback,
    }
}

impl Objective for Probe {
    type Error = ProbeError;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        if self.value_fails {
            return Err(ProbeError::Boom);
        }
        Ok(x[0])
    }

    fn callback(&mut self, _state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        self.callback
    }
}

#[test]
fn a_closure_is_an_objective() {
    let mut objective = |x: &[f64]| -> Result<f64, ProbeError> { Ok(x[1]) };
    assert_eq!(objective.value(&[1.0, 2.0]).unwrap(), 2.0);
}

#[test]
fn an_objective_supplies_no_gradient_by_default() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let mut out = [0.0; 2];
    assert!(!objective.gradient(&[1.0, 2.0], &mut out).unwrap());
}

#[test]
fn an_objective_error_is_carried_by_value() {
    let error: MinimizeError<ProbeError> = MinimizeError::Objective(ProbeError::Boom);
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.downcast_ref::<ProbeError>(), Some(&ProbeError::Boom));
}

#[test]
fn an_invalid_input_converts_into_a_minimize_error() {
    let error: MinimizeError<ProbeError> = InvalidInput::EmptyX0.into();
    assert_eq!(error.to_string(), "invalid input: x0 must not be empty");
}

#[test]
fn only_a_convergence_is_a_success() {
    assert!(Termination::Converged(Converged::GTol).is_success());
    assert!(!Termination::Cancelled.is_success());
    assert!(!Termination::Nonfinite.is_success());
}

#[test]
fn a_result_derives_its_success_and_message_from_its_status() {
    let result = Minimize::new(
        vec![1.0],
        0.5,
        3,
        7,
        2,
        Termination::Converged(Converged::XTol),
    );
    assert!(result.success);
    assert_eq!(result.message, "converged: step below the x tolerance");
    assert_eq!((result.nit, result.nfev, result.njev), (3, 7, 2));
}

fn problem() -> Problem {
    Problem {
        x0: vec![1.0, 2.0],
        bounds: None,
    }
}

fn bounded(lower: Vec<f64>, upper: Vec<f64>) -> Problem {
    Problem {
        x0: vec![1.0, 2.0],
        bounds: Some(Bounds { lower, upper }),
    }
}

fn common(maxiter: Option<usize>, maxfev: Option<usize>, tol: Option<f64>) -> Common {
    Common {
        maxiter,
        maxfev,
        tol,
    }
}

#[test]
fn x0_must_not_be_empty() {
    let problem = Problem {
        x0: Vec::new(),
        bounds: None,
    };
    assert_eq!(problem.validate(), Err(InvalidInput::EmptyX0));
}

#[test]
fn x0_must_be_finite() {
    let problem = Problem {
        x0: vec![1.0, f64::INFINITY],
        bounds: None,
    };
    let expected = InvalidInput::NonfiniteX0 { index: 1 };
    assert_eq!(problem.validate(), Err(expected));
}

#[test]
fn an_unbounded_problem_is_valid() {
    assert_eq!(problem().validate(), Ok(()));
}

#[test]
fn bounds_must_carry_one_entry_per_coordinate() {
    let expected = InvalidInput::BoundsLength {
        expected: 2,
        found: 1,
    };
    assert_eq!(bounded(vec![0.0], vec![3.0, 3.0]).validate(), Err(expected));
}

#[test]
fn a_nan_bound_is_rejected() {
    let problem = bounded(vec![0.0, f64::NAN], vec![3.0, 3.0]);
    assert_eq!(problem.validate(), Err(InvalidInput::NanBound { index: 1 }));
}

#[test]
fn an_infinite_bound_leaves_that_side_open() {
    let problem = bounded(vec![f64::NEG_INFINITY, 0.0], vec![3.0, f64::INFINITY]);
    assert_eq!(problem.validate(), Ok(()));
}

#[test]
fn a_lower_bound_must_not_exceed_its_upper_bound() {
    let problem = bounded(vec![0.0, 4.0], vec![3.0, 3.0]);
    let expected = InvalidInput::BoundsOrder { index: 1 };
    assert_eq!(problem.validate(), Err(expected));
}

#[test]
fn a_budget_must_be_positive_when_given() {
    let maxiter = InvalidInput::NotPositive { option: "maxiter" };
    assert_eq!(common(Some(0), None, None).validate(), Err(maxiter));
    let maxfev = InvalidInput::NotPositive { option: "maxfev" };
    assert_eq!(common(None, Some(0), None).validate(), Err(maxfev));
    assert_eq!(common(Some(1), Some(1), Some(1e-8)).validate(), Ok(()));
    assert_eq!(Common::default().validate(), Ok(()));
}

#[test]
fn a_shared_tolerance_must_be_finite_and_positive() {
    for tol in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let expected = InvalidInput::NotFinitePositive { option: "tol" };
        assert_eq!(common(None, None, Some(tol)).validate(), Err(expected));
    }
}

#[test]
fn a_method_rejects_an_option_it_does_not_support() {
    let method = Method::NelderMead(NelderMeadOptions::default());
    assert_eq!(method.name(), "Nelder-Mead");
    assert!(!method.supports_bounds());
    assert_eq!(method.validate(&problem()), Ok(()));
    let expected = InvalidInput::Unsupported {
        method: "Nelder-Mead",
        option: "bounds",
    };
    let bounded = bounded(vec![0.0, 0.0], vec![3.0, 3.0]);
    assert_eq!(method.validate(&bounded), Err(expected));
}

fn nelder_mead(options: NelderMeadOptions) -> Method {
    Method::NelderMead(options)
}

fn tolerances(xatol: Option<f64>, fatol: Option<f64>) -> NelderMeadOptions {
    NelderMeadOptions {
        xatol,
        fatol,
        ..NelderMeadOptions::default()
    }
}

fn simplex(points: Vec<Vec<f64>>) -> NelderMeadOptions {
    NelderMeadOptions {
        initial_simplex: Some(points),
        ..NelderMeadOptions::default()
    }
}

#[test]
fn a_nelder_mead_tolerance_must_be_finite_and_nonnegative() {
    let rejected = [
        ("xatol", tolerances(Some(f64::NAN), None)),
        ("xatol", tolerances(Some(f64::INFINITY), None)),
        ("fatol", tolerances(None, Some(-1.0))),
    ];
    for (option, options) in rejected {
        let expected = InvalidInput::NotFiniteNonnegative { option };
        assert_eq!(nelder_mead(options).validate(&problem()), Err(expected));
    }
}

#[test]
fn a_zero_nelder_mead_tolerance_is_legal() {
    let method = nelder_mead(tolerances(Some(0.0), Some(0.0)));
    assert_eq!(method.validate(&problem()), Ok(()));
}

#[test]
fn an_initial_simplex_must_carry_one_point_more_than_coordinates() {
    let too_few = simplex(vec![vec![0.0, 0.0], vec![1.0, 0.0]]);
    let expected = InvalidInput::SimplexPointCount {
        expected: 3,
        found: 2,
    };
    assert_eq!(nelder_mead(too_few).validate(&problem()), Err(expected));
    let too_many = simplex(vec![vec![0.0, 0.0]; 4]);
    let expected = InvalidInput::SimplexPointCount {
        expected: 3,
        found: 4,
    };
    assert_eq!(nelder_mead(too_many).validate(&problem()), Err(expected));
}

#[test]
fn every_initial_simplex_point_needs_one_coordinate_per_dimension() {
    let options = simplex(vec![vec![0.0, 0.0], vec![1.0], vec![0.0, 1.0]]);
    let expected = InvalidInput::SimplexPointLength {
        point: 1,
        expected: 2,
        found: 1,
    };
    assert_eq!(nelder_mead(options).validate(&problem()), Err(expected));
}

#[test]
fn every_initial_simplex_coordinate_must_be_finite() {
    let options = simplex(vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, f64::NAN]]);
    let expected = InvalidInput::NonfiniteSimplex { point: 2, index: 1 };
    assert_eq!(nelder_mead(options).validate(&problem()), Err(expected));
}

#[test]
fn a_well_formed_initial_simplex_is_valid() {
    let options = simplex(vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]]);
    assert_eq!(nelder_mead(options).validate(&problem()), Ok(()));
}

#[test]
fn bounds_that_admit_no_finite_coordinate_are_rejected() {
    let positive = bounded(vec![f64::INFINITY, 0.0], vec![f64::INFINITY, 3.0]);
    let expected = InvalidInput::InfeasibleBound { index: 0 };
    assert_eq!(positive.validate(), Err(expected));
    let negative = bounded(vec![0.0, f64::NEG_INFINITY], vec![3.0, f64::NEG_INFINITY]);
    let expected = InvalidInput::InfeasibleBound { index: 1 };
    assert_eq!(negative.validate(), Err(expected));
}

#[test]
fn an_open_side_and_a_fixed_coordinate_stay_valid() {
    let bounds = bounded(vec![f64::NEG_INFINITY, 1.0], vec![f64::INFINITY, 1.0]);
    assert_eq!(bounds.validate(), Ok(()));
}

#[test]
fn nelder_mead_options_leave_every_tolerance_to_be_resolved() {
    let options = NelderMeadOptions::default();
    assert_eq!(options.xatol, None);
    assert_eq!(options.fatol, None);
    assert!(!options.adaptive);
    assert_eq!(options.initial_simplex, None);
    assert_eq!(nelder_mead(options).validate(&problem()), Ok(()));
}

fn step<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    x: &[f64],
    fun: &mut f64,
) -> Result<(), Halt<O::Error>> {
    *fun = counters.value(objective, x)?;
    counters.end_iteration(objective, x, *fun)
}

fn run<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    problem.validate()?;
    common.validate()?;
    let mut counters = Counters::new(common);
    let x = problem.x0.clone();
    let mut fun = f64::NAN;
    loop {
        match step(&mut counters, objective, &x, &mut fun) {
            Ok(()) => {}
            Err(Halt::Terminated(status)) => {
                let (nit, nfev, njev) = (counters.nit(), counters.nfev(), counters.njev());
                return Ok(Minimize::new(x, fun, nit, nfev, njev, status));
            }
            Err(Halt::Failed(error)) => return Err(error),
        }
    }
}

#[test]
fn an_objective_error_reaches_the_caller_by_value() {
    let mut objective = probe(true, Ok(Flow::Continue));
    let error = run(&mut objective, &problem(), &Common::default()).unwrap_err();
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
}

#[test]
fn a_failing_callback_is_an_objective_error() {
    let mut objective = probe(false, Err(ProbeError::Boom));
    let error = run(&mut objective, &problem(), &Common::default()).unwrap_err();
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
}

#[test]
fn a_stopping_callback_cancels_the_run() {
    let mut objective = probe(false, Ok(Flow::Stop));
    let result = run(&mut objective, &problem(), &Common::default()).unwrap();
    assert_eq!(result.status, Termination::Cancelled);
    assert_eq!(result.nit, 1);
    assert!(!result.success);
    assert_eq!(result.message, "stopped by the callback");
}

#[test]
fn an_exhausted_evaluation_budget_stops_with_max_evaluations() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let result = run(&mut objective, &problem(), &common(None, Some(3), None)).unwrap();
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert_eq!(result.nfev, 3);
    assert_eq!(result.fun, 1.0);
}

#[test]
fn an_exhausted_iteration_budget_stops_with_max_iterations() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let result = run(&mut objective, &problem(), &common(Some(2), None, None)).unwrap();
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(result.nit, 2);
    assert_eq!(result.njev, 0);
}

#[test]
fn an_invalid_input_stops_the_run_before_any_evaluation() {
    let mut objective = probe(true, Ok(Flow::Continue));
    let empty = Problem {
        x0: Vec::new(),
        bounds: None,
    };
    let error = run(&mut objective, &empty, &Common::default()).unwrap_err();
    assert!(matches!(
        error,
        MinimizeError::InvalidInput(InvalidInput::EmptyX0)
    ));
}

#[test]
fn a_closure_runs_through_the_counters() {
    let mut objective = |x: &[f64]| -> Result<f64, ProbeError> { Ok(x[1]) };
    let result = run(&mut objective, &problem(), &common(None, Some(1), None)).unwrap();
    assert_eq!(result.fun, 2.0);
    assert_eq!(result.nfev, 1);
}

fn solve(
    objective: impl FnMut(&[f64]) -> f64,
    x0: Vec<f64>,
    options: NelderMeadOptions,
    common: Common,
) -> Minimize {
    let mut objective = objective;
    let mut wrapped = move |x: &[f64]| -> Result<f64, Infallible> { Ok(objective(x)) };
    let problem = Problem { x0, bounds: None };
    minimize(
        &mut wrapped,
        &problem,
        &Method::NelderMead(options),
        &common,
    )
    .expect("an infallible objective cannot fail")
}

fn with_defaults<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    common: Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    let method = Method::NelderMead(NelderMeadOptions::default());
    minimize(objective, problem, &method, &common)
}

fn sphere(x: &[f64]) -> f64 {
    x.iter().map(|value| value * value).sum()
}

fn rosenbrock(x: &[f64]) -> f64 {
    100.0 * (x[1] - x[0] * x[0]).powi(2) + (1.0 - x[0]).powi(2)
}

fn beale(x: &[f64]) -> f64 {
    (1.5 - x[0] + x[0] * x[1]).powi(2)
        + (2.25 - x[0] + x[0] * x[1] * x[1]).powi(2)
        + (2.625 - x[0] + x[0] * x[1].powi(3)).powi(2)
}

fn tight() -> NelderMeadOptions {
    NelderMeadOptions {
        xatol: Some(1e-10),
        fatol: Some(1e-12),
        ..NelderMeadOptions::default()
    }
}

fn generous() -> Common {
    common(Some(20_000), Some(20_000), None)
}

#[test]
fn a_sphere_is_minimized_at_the_origin() {
    for x0 in [vec![1.0, -2.0], vec![1.0, -2.0, 3.0, 0.5, -1.5]] {
        let result = solve(sphere, x0, NelderMeadOptions::default(), generous());
        assert_eq!(result.status, Termination::Converged(Converged::XTol));
        assert!(result.success);
        for coordinate in &result.x {
            assert!(coordinate.abs() < 1e-4, "{:?}", result.x);
        }
    }
}

#[test]
fn rosenbrock_reaches_its_valley_floor() {
    let result = solve(rosenbrock, vec![-1.2, 1.0], tight(), generous());
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(result.fun < 1e-8, "{}", result.fun);
    assert!((result.x[0] - 1.0).abs() < 1e-4);
    assert!((result.x[1] - 1.0).abs() < 1e-4);
}

#[test]
fn a_non_smooth_objective_still_converges() {
    let absolute = |x: &[f64]| x.iter().map(|value| value.abs()).sum();
    let result = solve(
        absolute,
        vec![1.3, -0.7],
        NelderMeadOptions::default(),
        generous(),
    );
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(result.fun < 1e-3, "{}", result.fun);
}

#[test]
fn beale_reaches_its_known_minimum() {
    let result = solve(beale, vec![1.0, 1.0], tight(), generous());
    assert!(result.fun < 1e-8, "{}", result.fun);
    assert!((result.x[0] - 3.0).abs() < 1e-4);
    assert!((result.x[1] - 0.5).abs() < 1e-4);
}

#[test]
fn an_infinite_barrier_is_a_legal_worse_vertex() {
    let barrier = |x: &[f64]| {
        if sphere(x) > 1.0 {
            f64::INFINITY
        } else {
            (x[0] - 0.3).powi(2) + (x[1] + 0.2).powi(2)
        }
    };
    let result = solve(
        barrier,
        vec![0.5, 0.5],
        NelderMeadOptions::default(),
        generous(),
    );
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(result.fun.is_finite());
    assert!((result.x[0] - 0.3).abs() < 1e-3);
    assert!((result.x[1] + 0.2).abs() < 1e-3);
}

#[test]
fn adaptive_coefficients_minimize_a_ten_dimensional_sphere() {
    let options = NelderMeadOptions {
        adaptive: true,
        ..NelderMeadOptions::default()
    };
    let result = solve(sphere, vec![0.6; 10], options, generous());
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(result.fun < 1e-6, "{}", result.fun);
}

#[derive(Debug, Default)]
struct Recorder {
    points: Vec<Vec<f64>>,
    callbacks: usize,
}

impl Objective for Recorder {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.points.push(x.to_vec());
        Ok(sphere(x))
    }

    fn callback(&mut self, _state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        self.callbacks += 1;
        Ok(Flow::Continue)
    }
}

fn record(options: NelderMeadOptions, common: Common) -> (Recorder, Minimize) {
    let mut recorder = Recorder::default();
    let problem = Problem {
        x0: vec![1.0, -2.0],
        bounds: None,
    };
    let result = minimize(
        &mut recorder,
        &problem,
        &Method::NelderMead(options),
        &common,
    )
    .expect("an infallible objective cannot fail");
    (recorder, result)
}

#[test]
fn the_counters_report_what_the_objective_and_the_callback_saw() {
    let (recorder, result) = record(NelderMeadOptions::default(), generous());
    assert_eq!(result.nfev, recorder.points.len());
    assert_eq!(result.nit, recorder.callbacks);
    assert_eq!(result.njev, 0);
}

#[test]
fn an_explicit_initial_simplex_is_the_starting_simplex() {
    let points = vec![vec![0.5, 0.5], vec![0.9, 0.4], vec![0.4, 0.9]];
    let (recorder, result) = record(simplex(points.clone()), generous());
    assert_eq!(recorder.points[..3], points[..]);
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
}

#[test]
fn the_first_simplex_evaluation_is_not_an_iteration() {
    let (_, result) = record(NelderMeadOptions::default(), common(Some(1), None, None));
    assert_eq!(result.nit, 1);
    assert!(result.nfev > 3);
}

#[test]
fn an_evaluation_budget_stops_at_max_evaluations() {
    let (_, result) = record(tight(), common(None, Some(25), None));
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert_eq!(result.nfev, 25);
    assert!(!result.success);
}

#[test]
fn an_iteration_budget_stops_at_max_iterations() {
    let (_, result) = record(tight(), common(Some(7), None, None));
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(result.nit, 7);
}

#[test]
fn a_callback_that_stops_cancels_the_run() {
    let mut objective = probe(false, Ok(Flow::Stop));
    let result = with_defaults(&mut objective, &problem(), Common::default())
        .expect("a cancellation is not an error");
    assert_eq!(result.status, Termination::Cancelled);
    assert_eq!(result.nit, 1);
    assert!(!result.success);
}

#[test]
fn a_failing_callback_reaches_the_caller_by_value() {
    let mut objective = probe(false, Err(ProbeError::Boom));
    let error = with_defaults(&mut objective, &problem(), Common::default())
        .expect_err("the callback fails");
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
}

#[test]
fn a_failing_objective_reaches_the_caller_by_value() {
    let mut objective = probe(true, Ok(Flow::Continue));
    let error = with_defaults(&mut objective, &problem(), Common::default())
        .expect_err("the objective fails");
    assert!(matches!(error, MinimizeError::Objective(ProbeError::Boom)));
}

#[test]
fn bounds_are_rejected_before_any_evaluation() {
    let mut objective = probe(true, Ok(Flow::Continue));
    let bounded = bounded(vec![0.0, 0.0], vec![3.0, 3.0]);
    let error = with_defaults(&mut objective, &bounded, Common::default())
        .expect_err("Nelder-Mead does not support bounds");
    let expected = InvalidInput::Unsupported {
        method: "Nelder-Mead",
        option: "bounds",
    };
    assert!(matches!(error, MinimizeError::InvalidInput(found) if found == expected));
}

struct NonfiniteAfter {
    calls: usize,
    limit: usize,
    after: f64,
}

impl Objective for NonfiniteAfter {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.calls += 1;
        if self.calls > self.limit {
            return Ok(self.after);
        }
        Ok(sphere(x))
    }
}

#[test]
fn a_nan_value_ends_the_run_at_the_best_finite_vertex() {
    let mut objective = NonfiniteAfter {
        calls: 0,
        limit: 12,
        after: f64::NAN,
    };
    let problem = Problem {
        x0: vec![2.0, 2.0],
        bounds: None,
    };
    let result = with_defaults(&mut objective, &problem, Common::default())
        .expect("a nonfinite value is a status, not an error");
    assert_eq!(result.status, Termination::Nonfinite);
    assert!(!result.success);
    assert_eq!(result.nfev, 13);
    assert!(result.fun.is_finite());
    assert!(result.fun < sphere(&[2.0, 2.0]));
    assert_eq!(result.fun, sphere(&result.x));
}

#[test]
fn a_nan_at_the_starting_point_returns_that_point() {
    let mut objective = NonfiniteAfter {
        calls: 0,
        limit: 0,
        after: f64::NAN,
    };
    let x0 = vec![1.0, 2.0];
    let problem = Problem {
        x0: x0.clone(),
        bounds: None,
    };
    let result = with_defaults(&mut objective, &problem, Common::default())
        .expect("a nonfinite value is a status, not an error");
    assert_eq!(result.status, Termination::Nonfinite);
    assert_eq!(result.x, x0);
    assert!(result.fun.is_nan());
    assert_eq!(result.nfev, 1);
    assert!(!result.success);
}

#[test]
fn a_shared_tolerance_fills_only_the_tolerances_left_unset() {
    let loose = solve(
        sphere,
        vec![2.0, 2.0],
        NelderMeadOptions::default(),
        common(None, None, Some(0.5)),
    );
    let explicit = NelderMeadOptions {
        xatol: Some(1e-10),
        ..NelderMeadOptions::default()
    };
    let mixed = solve(
        sphere,
        vec![2.0, 2.0],
        explicit,
        common(None, None, Some(0.5)),
    );
    let fallback = solve(
        sphere,
        vec![2.0, 2.0],
        NelderMeadOptions::default(),
        Common::default(),
    );
    let by_value = NelderMeadOptions {
        fatol: Some(1e-10),
        ..NelderMeadOptions::default()
    };
    let tightened = solve(
        sphere,
        vec![2.0, 2.0],
        by_value,
        common(None, None, Some(0.5)),
    );
    assert_eq!(tightened.status, Termination::Converged(Converged::XTol));
    assert!(
        tightened.x.iter().all(|value| value.abs() < 1e-4),
        "{:?}",
        tightened.x
    );
    assert!(loose.nit < fallback.nit, "{} {}", loose.nit, fallback.nit);
    assert!(fallback.nit < mixed.nit, "{} {}", fallback.nit, mixed.nit);
    assert!(
        mixed.x.iter().all(|value| value.abs() < 1e-9),
        "{:?}",
        mixed.x
    );
}

#[test]
fn unset_budgets_take_the_scipy_default_of_two_hundred_per_coordinate() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let result = with_defaults(&mut objective, &problem(), Common::default())
        .expect("a descending objective never converges");
    assert_eq!(result.nfev, 400);
    assert_eq!(result.status, Termination::MaxEvaluations);
}

#[test]
fn setting_one_budget_leaves_the_other_unlimited() {
    let mut objective = probe(false, Ok(Flow::Continue));
    let result = with_defaults(&mut objective, &problem(), common(Some(500), None, None))
        .expect("a descending objective never converges");
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(result.nit, 500);
    assert!(result.nfev > 400, "{}", result.nfev);
}

#[test]
fn a_shrink_rescues_a_simplex_that_straddles_the_valley() {
    let straddling = simplex(vec![vec![-1.2, 1.0], vec![2.0, 4.0], vec![-2.0, 4.0]]);
    let result = solve(rosenbrock, vec![-1.2, 1.0], straddling, generous());
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(result.fun < 1e-8, "{}", result.fun);
}

#[test]
fn a_negative_infinity_vertex_ends_the_run_at_the_best_finite_point() {
    let cliff = |x: &[f64]| {
        if x[0] > 1.01 {
            f64::NEG_INFINITY
        } else {
            x[0] * x[0]
        }
    };
    let result = solve(
        cliff,
        vec![1.0],
        NelderMeadOptions::default(),
        Common::default(),
    );
    assert_eq!(result.status, Termination::Nonfinite);
    assert!(!result.success);
    assert_eq!(result.x, vec![1.0]);
    assert_eq!(result.fun, 1.0);
    assert_eq!(result.nfev, 2);
}

#[test]
fn a_negative_infinity_after_progress_keeps_the_best_finite_point() {
    let mut objective = NonfiniteAfter {
        calls: 0,
        limit: 12,
        after: f64::NEG_INFINITY,
    };
    let problem = Problem {
        x0: vec![2.0, 2.0],
        bounds: None,
    };
    let result = with_defaults(&mut objective, &problem, Common::default())
        .expect("a nonfinite value is a status, not an error");
    assert_eq!(result.status, Termination::Nonfinite);
    assert!(!result.success);
    assert_eq!(result.nfev, 13);
    assert!(result.fun.is_finite());
    assert!(result.fun < sphere(&[2.0, 2.0]));
    assert_eq!(result.fun, sphere(&result.x));
}

#[test]
fn an_infinite_shared_tolerance_is_rejected_before_any_evaluation() {
    let mut recorder = Recorder::default();
    let problem = Problem {
        x0: vec![1.0, -2.0],
        bounds: None,
    };
    let error = with_defaults(
        &mut recorder,
        &problem,
        common(None, None, Some(f64::INFINITY)),
    )
    .expect_err("an infinite tolerance is not a tolerance");
    let expected = InvalidInput::NotFinitePositive { option: "tol" };
    assert!(matches!(error, MinimizeError::InvalidInput(found) if found == expected));
    assert!(recorder.points.is_empty());
}
