//! Shared test harness for the concrete 1-D solvers.
//!
//! Port of the generic `test_solver` machinery in `test-suite/solvers.cpp`: every
//! bracketing solver is exercised against the same known-root functions and the
//! same driver-contract checks, parameterised by a factory that produces a fresh
//! solver. Each solver's test module just calls these with its own constructor.

use crate::math::solver1d::{DerivativeSolver, Solver1D, func1d};
use crate::types::Real;
use std::cell::Cell;

/// `f1(x) = x^2 - 1`: increasing, single root at `x = 1`.
pub fn f1(x: Real) -> Real {
    x * x - 1.0
}
/// `f2(x) = 1 - x^2`: decreasing, single root at `x = 1`.
pub fn f2(x: Real) -> Real {
    1.0 - x * x
}
/// `f3(x) = atan(x - 1)`: single root at `x = 1`.
pub fn f3(x: Real) -> Real {
    (x - 1.0).atan()
}
/// `f1'(x) = 2x`.
pub fn d1(x: Real) -> Real {
    2.0 * x
}
/// `f2'(x) = -2x`.
pub fn d2(x: Real) -> Real {
    -2.0 * x
}
/// `f3'(x) = 1 / (1 + (x - 1)^2)`.
pub fn d3(x: Real) -> Real {
    let u = x - 1.0;
    1.0 / (1.0 + u * u)
}
/// `f1''(x) = 2`.
pub fn dd1(_x: Real) -> Real {
    2.0
}
/// `f2''(x) = -2`.
pub fn dd2(_x: Real) -> Real {
    -2.0
}
/// `f3''(x) = -2(x - 1) / (1 + (x - 1)^2)^2`.
pub fn dd3(x: Real) -> Real {
    let u = x - 1.0;
    let d = 1.0 + u * u;
    -2.0 * u / (d * d)
}

const ACCURACIES: [Real; 3] = [1.0e-4, 1.0e-6, 1.0e-8];

/// Port of `test_solver`: the solver must find the root at `x = 1` of `f1` and
/// `f2` (guessing from either side, auto-bracketing and pre-bracketed) and `f3`.
pub fn check_finds_known_roots<S: Solver1D>(make: impl Fn() -> S) {
    for f in [f1 as fn(Real) -> Real, f2] {
        for guess in [0.5, 1.5] {
            for acc in ACCURACIES {
                let root = make().solve(f, acc, guess, 0.1).unwrap();
                assert!(
                    (root - 1.0).abs() <= acc,
                    "auto: guess={guess} acc={acc} root={root}"
                );
                let root = make().solve_bracketed(f, acc, guess, 0.0, 2.0).unwrap();
                assert!(
                    (root - 1.0).abs() <= acc,
                    "bracketed: guess={guess} acc={acc} root={root}"
                );
            }
        }
    }
    for acc in ACCURACIES {
        let root = make().solve(f3, acc, 1.00001, 0.1).unwrap();
        assert!((root - 1.0).abs() <= acc, "f3: acc={acc} root={root}");
    }
}

/// Port of `test_last_call_with_root`: the solver's final function call must be
/// made at the value it returns. The stateful `Probe` records the last argument
/// it saw (exercising the `FnMut` path); the chained offsets give roots 5, 4, 3, 2.
pub fn check_last_call_with_root<S: Solver1D>(make: impl Fn() -> S) {
    let mins = [3.0, 2.25, 1.5, 1.0];
    let maxs = [7.0, 5.75, 4.5, 3.0];
    let steps = [0.2, 0.2, 0.1, 0.1];
    let offsets = [25.0, 11.0, 5.0, 1.0];
    let guesses = [4.5, 4.5, 2.5, 2.5];
    let accuracy = 1.0e-6;

    for bracketed in [false, true] {
        let argument = Cell::new(0.0);
        for i in 0..4 {
            let previous = argument.get();
            let probe = |x: Real| {
                argument.set(x);
                previous + offsets[i] - x * x
            };
            let result = if bracketed {
                make()
                    .solve_bracketed(probe, accuracy, guesses[i], mins[i], maxs[i])
                    .unwrap()
            } else {
                make().solve(probe, accuracy, guesses[i], steps[i]).unwrap()
            };
            assert!(
                (result - argument.get()).abs() <= 2.0 * Real::EPSILON,
                "bracketed={bracketed} i={i}: result={result} last_arg={}",
                argument.get()
            );
        }
    }
}

/// The driver rejects malformed inputs: non-positive accuracy, an unbracketed
/// range, and a guess outside the range.
pub fn check_rejects_invalid_inputs<S: Solver1D>(make: impl Fn() -> S) {
    assert!(make().solve(f1, 0.0, 0.5, 0.1).is_err());
    assert!(make().solve_bracketed(f1, 1e-8, 2.5, 2.0, 3.0).is_err());
    assert!(make().solve_bracketed(f1, 1e-8, 5.0, 0.0, 2.0).is_err());
}

/// Configured domain bounds are honoured: a bracket past the upper bound is
/// rejected, while a bounded auto-bracketing search still finds the root.
pub fn check_honours_bounds<S: Solver1D>(make: impl Fn() -> S) {
    let mut solver = make();
    solver.set_upper_bound(2.0);
    assert!(solver.solve_bracketed(f1, 1e-8, 1.5, 0.0, 3.0).is_err());

    let mut solver = make();
    solver.set_lower_bound(0.0);
    solver.set_upper_bound(5.0);
    let root = solver.solve(f1, 1e-10, 0.5, 0.1).unwrap();
    assert!((root - 1.0).abs() <= 1e-9, "root={root}");
}

// --- Derivative-based solvers (Newton, NewtonSafe, ...) ---

/// Port of `test_solver` for a [`DerivativeSolver`]: the well-behaved known roots
/// of `f1` and `f2`, paired with their derivatives, that every derivative solver
/// must find. (The `f3 = atan(x - 1)`, `guess = 1.00001` case forces a step out
/// of the bracket and so belongs to the safe-stepping solvers - see NewtonSafe.)
pub fn check_derivative_solver_finds_roots<S: DerivativeSolver>(make: impl Fn() -> S) {
    let cases = [(f1 as fn(Real) -> Real, d1 as fn(Real) -> Real), (f2, d2)];
    for (f, d) in cases {
        for guess in [0.5, 1.5] {
            for acc in ACCURACIES {
                let root = make().solve(func1d(f, d), acc, guess, 0.1).unwrap();
                assert!(
                    (root - 1.0).abs() <= acc,
                    "auto: guess={guess} acc={acc} root={root}"
                );
                let root = make()
                    .solve_bracketed(func1d(f, d), acc, guess, 0.0, 2.0)
                    .unwrap();
                assert!(
                    (root - 1.0).abs() <= acc,
                    "bracketed: guess={guess} acc={acc} root={root}"
                );
            }
        }
    }
}

/// Port of `test_last_call_with_root` for a [`DerivativeSolver`]. The Probe gets
/// its CORRECT derivative `-2x` (QuantLib's fixture declares `2x` and relies on
/// the NewtonSafe fallback to absorb it; pure Newton needs the true derivative).
pub fn check_derivative_last_call<S: DerivativeSolver>(make: impl Fn() -> S) {
    let mins = [3.0, 2.25, 1.5, 1.0];
    let maxs = [7.0, 5.75, 4.5, 3.0];
    let steps = [0.2, 0.2, 0.1, 0.1];
    let offsets = [25.0, 11.0, 5.0, 1.0];
    let guesses = [4.5, 4.5, 2.5, 2.5];
    let accuracy = 1.0e-6;

    for bracketed in [false, true] {
        let argument = Cell::new(0.0);
        for i in 0..4 {
            let previous = argument.get();
            let value = |x: Real| {
                argument.set(x);
                previous + offsets[i] - x * x
            };
            let derivative = |x: Real| -2.0 * x;
            let g = func1d(value, derivative);
            let result = if bracketed {
                make()
                    .solve_bracketed(g, accuracy, guesses[i], mins[i], maxs[i])
                    .unwrap()
            } else {
                make().solve(g, accuracy, guesses[i], steps[i]).unwrap()
            };
            assert!(
                (result - argument.get()).abs() <= 2.0 * Real::EPSILON,
                "bracketed={bracketed} i={i}: result={result} last_arg={}",
                argument.get()
            );
        }
    }
}

/// The shared driver rejects malformed inputs for a [`DerivativeSolver`] too.
pub fn check_derivative_rejects<S: DerivativeSolver>(make: impl Fn() -> S) {
    assert!(make().solve(func1d(f1, d1), 0.0, 0.5, 0.1).is_err());
    assert!(
        make()
            .solve_bracketed(func1d(f1, d1), 1e-8, 2.5, 2.0, 3.0)
            .is_err()
    );
    assert!(
        make()
            .solve_bracketed(func1d(f1, d1), 1e-8, 5.0, 0.0, 2.0)
            .is_err()
    );
}

/// Port of `test_solver`'s `f3 = atan(x - 1)`, `guess = 1.00001` case, which
/// forces a Newton step out of the bracket. Only safe (bisection-fallback)
/// derivative solvers survive it; pure Newton errors instead.
pub fn check_safe_derivative_solver<S: DerivativeSolver>(make: impl Fn() -> S) {
    for acc in ACCURACIES {
        let root = make().solve(func1d(f3, d3), acc, 1.00001, 0.1).unwrap();
        assert!(
            (root - 1.0).abs() <= acc,
            "f3 stress: acc={acc} root={root}"
        );
    }
}
