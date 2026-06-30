//! Abstract 1-D solver.
//!
//! Port of `ql/math/solver1d.hpp`. QuantLib's CRTP base owns the bracketing
//! driver and two `solve` overloads; each concrete solver supplies a `solveImpl`
//! that refines an already-bracketed root. We map that to a [`Solver1D`] trait
//! whose provided `solve` / `solve_bracketed` methods run the shared driver and
//! statically dispatch to [`solve_impl`](Solver1D::solve_impl), with the per-call
//! working data carried in a [`Solver1DState`].

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::types::Real;

/// The default evaluation cap for the bracketing search and refinement,
/// matching QuantLib's `MAX_FUNCTION_EVALUATIONS`. Concrete solvers start from
/// this and may override it via [`Solver1D::set_max_evaluations`].
pub const DEFAULT_MAX_EVALUATIONS: usize = 100;

/// Configuration shared by every 1-D solver, mirroring the state QuantLib keeps
/// on the `Solver1D` base: the evaluation cap and the optional domain bounds the
/// bracketing search is clamped to.
#[derive(Clone, Copy, Debug)]
pub struct SolverConfig {
    /// Maximum function evaluations for the bracketing search and refinement.
    pub max_evaluations: usize,
    /// If set, the search never evaluates `f` below this point.
    pub lower_bound: Option<Real>,
    /// If set, the search never evaluates `f` above this point.
    pub upper_bound: Option<Real>,
}

impl SolverConfig {
    /// The default config: [`DEFAULT_MAX_EVALUATIONS`] and no domain bounds.
    pub fn new() -> Self {
        SolverConfig {
            max_evaluations: DEFAULT_MAX_EVALUATIONS,
            lower_bound: None,
            upper_bound: None,
        }
    }

    /// Clamp `x` into the enforced domain (QuantLib's `enforceBounds_`).
    fn enforce_bounds(&self, x: Real) -> Real {
        if let Some(lb) = self.lower_bound
            && x < lb
        {
            return lb;
        }
        if let Some(ub) = self.upper_bound
            && x > ub
        {
            return ub;
        }
        x
    }
}

impl Default for SolverConfig {
    fn default() -> Self {
        SolverConfig::new()
    }
}

/// The working state threaded from the bracketing driver into
/// [`Solver1D::solve_impl`].
///
/// On entry to `solve_impl` the driver guarantees that `x_min`/`x_max` form a
/// valid bracket, `fx_min`/`fx_max` hold `f` there, and `root` is a guess
/// strictly inside the bracket.
#[derive(Clone, Copy, Debug, Default)]
pub struct Solver1DState {
    /// The current root estimate (a valid guess on entry).
    pub root: Real,
    /// The bracket `[x_min, x_max]`.
    pub x_min: Real,
    pub x_max: Real,
    /// `f` at the bracket ends.
    pub fx_min: Real,
    pub fx_max: Real,
    /// Function evaluations performed so far.
    pub evaluation_number: usize,
}

/// A 1-D root finder for a continuous function.
///
/// Implementors store a [`SolverConfig`], expose it via `config` / `config_mut`,
/// and supply [`solve_impl`](Solver1D::solve_impl); they get the shared `solve` /
/// `solve_bracketed` drivers, the cap accessors and bound enforcement for free.
pub trait Solver1D {
    /// The shared configuration (evaluation cap, domain bounds).
    fn config(&self) -> &SolverConfig;

    /// Mutable access to the shared configuration.
    fn config_mut(&mut self) -> &mut SolverConfig;

    /// The evaluation cap for the bracketing search and the refinement step.
    fn max_evaluations(&self) -> usize {
        self.config().max_evaluations
    }

    /// Set the evaluation cap.
    fn set_max_evaluations(&mut self, evaluations: usize) {
        self.config_mut().max_evaluations = evaluations;
    }

    /// Restrict the search to `x >= lower_bound` (QuantLib's `setLowerBound`).
    fn set_lower_bound(&mut self, lower_bound: Real) {
        self.config_mut().lower_bound = Some(lower_bound);
    }

    /// Restrict the search to `x <= upper_bound` (QuantLib's `setUpperBound`).
    fn set_upper_bound(&mut self, upper_bound: Real) {
        self.config_mut().upper_bound = Some(upper_bound);
    }

    /// Refine the already-bracketed root of `f` (invariants per [`Solver1DState`])
    /// to the given `accuracy`. `f` is `FnMut` because QuantLib functors may carry
    /// mutable state (e.g. recording the last argument).
    ///
    /// # Errors
    ///
    /// Errors if the refinement exceeds [`max_evaluations`](Self::max_evaluations).
    fn solve_impl<F>(
        &mut self,
        f: &mut F,
        accuracy: Real,
        state: &mut Solver1DState,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real;

    /// Find a zero of `f` near `guess`, auto-bracketing by scanning outward in
    /// steps of `step`.
    ///
    /// # Errors
    ///
    /// Returns an error if `accuracy <= 0`, or if no bracket is found within
    /// [`max_evaluations`](Self::max_evaluations) evaluations.
    fn solve<F>(&mut self, mut f: F, accuracy: Real, guess: Real, step: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        match bracket_by_stepping(self.config(), &mut f, guess, step)? {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(mut st) => self.solve_impl(&mut f, accuracy, &mut st),
        }
    }

    /// Find a zero of `f` in the caller-supplied bracket `[x_min, x_max]`, with
    /// `guess` as the starting point.
    ///
    /// # Errors
    ///
    /// Returns an error if `accuracy <= 0`, the range is invalid or falls outside
    /// the enforced domain bounds, the bracket does not straddle a zero, or
    /// `guess` is not strictly inside it.
    fn solve_bracketed<F>(
        &mut self,
        mut f: F,
        accuracy: Real,
        guess: Real,
        x_min: Real,
        x_max: Real,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        match bracket_given(self.config(), &mut f, guess, x_min, x_max)? {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(mut st) => self.solve_impl(&mut f, accuracy, &mut st),
        }
    }
}

/// Outcome of the bracketing phase shared by every solver's `solve` entry points.
pub(crate) enum Bracketed {
    /// An endpoint already lies (within tolerance) at the root.
    Root(Real),
    /// A valid bracket prepared for refinement by `solve_impl`.
    Ready(Solver1DState),
}

/// Auto-bracket a root of `f` near `guess` by scanning outward in steps of
/// `step`, clamping every expansion to `config`'s domain bounds (QuantLib's
/// `Solver1D::solve` step routine).
pub(crate) fn bracket_by_stepping<F>(
    config: &SolverConfig,
    f: &mut F,
    guess: Real,
    step: Real,
) -> QlResult<Bracketed>
where
    F: FnMut(Real) -> Real,
{
    let growth_factor = 1.6;
    let mut flipflop: i32 = -1;
    let mut st = Solver1DState {
        root: guess,
        ..Default::default()
    };
    st.fx_max = f(st.root);

    // monotonically crescent bias, as in optionValue(volatility)
    if close(st.fx_max, 0.0) {
        return Ok(Bracketed::Root(st.root));
    } else if st.fx_max > 0.0 {
        st.x_min = config.enforce_bounds(st.root - step);
        st.fx_min = f(st.x_min);
        st.x_max = st.root;
    } else {
        st.x_min = st.root;
        st.fx_min = st.fx_max;
        st.x_max = config.enforce_bounds(st.root + step);
        st.fx_max = f(st.x_max);
    }

    st.evaluation_number = 2;
    while st.evaluation_number <= config.max_evaluations {
        if st.fx_min * st.fx_max <= 0.0 {
            if close(st.fx_min, 0.0) {
                return Ok(Bracketed::Root(st.x_min));
            }
            if close(st.fx_max, 0.0) {
                return Ok(Bracketed::Root(st.x_max));
            }
            st.root = (st.x_max + st.x_min) / 2.0;
            return Ok(Bracketed::Ready(st));
        }
        if st.fx_min.abs() < st.fx_max.abs() {
            st.x_min = config.enforce_bounds(st.x_min + growth_factor * (st.x_min - st.x_max));
            st.fx_min = f(st.x_min);
        } else if st.fx_min.abs() > st.fx_max.abs() {
            st.x_max = config.enforce_bounds(st.x_max + growth_factor * (st.x_max - st.x_min));
            st.fx_max = f(st.x_max);
        } else if flipflop == -1 {
            st.x_min = config.enforce_bounds(st.x_min + growth_factor * (st.x_min - st.x_max));
            st.fx_min = f(st.x_min);
            st.evaluation_number += 1;
            flipflop = 1;
        } else if flipflop == 1 {
            st.x_max = config.enforce_bounds(st.x_max + growth_factor * (st.x_max - st.x_min));
            st.fx_max = f(st.x_max);
            flipflop = -1;
        }
        st.evaluation_number += 1;
    }

    fail!(
        "unable to bracket root in {} function evaluations (last bracket attempt: f[{}, {}] -> [{}, {}])",
        config.max_evaluations,
        st.x_min,
        st.x_max,
        st.fx_min,
        st.fx_max
    )
}

/// Validate the caller-supplied bracket `[x_min, x_max]` against `config` and
/// `guess`, preparing it for refinement (QuantLib's bracketed `Solver1D::solve`).
pub(crate) fn bracket_given<F>(
    config: &SolverConfig,
    f: &mut F,
    guess: Real,
    x_min: Real,
    x_max: Real,
) -> QlResult<Bracketed>
where
    F: FnMut(Real) -> Real,
{
    let mut st = Solver1DState {
        x_min,
        x_max,
        ..Default::default()
    };
    if st.x_min >= st.x_max {
        fail!("invalid range: x_min ({x_min}) >= x_max ({x_max})");
    }
    if let Some(lb) = config.lower_bound
        && st.x_min < lb
    {
        fail!("x_min ({x_min}) < enforced lower bound ({lb})");
    }
    if let Some(ub) = config.upper_bound
        && st.x_max > ub
    {
        fail!("x_max ({x_max}) > enforced upper bound ({ub})");
    }

    st.fx_min = f(st.x_min);
    if close(st.fx_min, 0.0) {
        return Ok(Bracketed::Root(st.x_min));
    }
    st.fx_max = f(st.x_max);
    if close(st.fx_max, 0.0) {
        return Ok(Bracketed::Root(st.x_max));
    }
    st.evaluation_number = 2;

    if st.fx_min * st.fx_max >= 0.0 {
        fail!(
            "root not bracketed: f[{x_min}, {x_max}] -> [{}, {}]",
            st.fx_min,
            st.fx_max
        );
    }
    if guess <= st.x_min {
        fail!("guess ({guess}) < x_min ({x_min})");
    }
    if guess >= st.x_max {
        fail!("guess ({guess}) > x_max ({x_max})");
    }

    st.root = guess;
    Ok(Bracketed::Ready(st))
}

/// A function paired with its first derivative, the input to derivative-based
/// solvers (mirroring QuantLib's functor with an `operator()` and a
/// `derivative()`). Build one from a pair of closures with [`func1d`].
///
/// The methods take `&mut self` so a stateful functor can be used, matching the
/// `FnMut` value closures the [`Solver1D`] drivers accept.
pub trait Function1D {
    /// `f(x)`.
    fn value(&mut self, x: Real) -> Real;
    /// `f'(x)`.
    fn derivative(&mut self, x: Real) -> Real;
}

/// Adapt a value closure and a derivative closure into a [`Function1D`].
pub fn func1d<F, D>(value: F, derivative: D) -> impl Function1D
where
    F: FnMut(Real) -> Real,
    D: FnMut(Real) -> Real,
{
    struct Pair<F, D> {
        value: F,
        derivative: D,
    }
    impl<F, D> Function1D for Pair<F, D>
    where
        F: FnMut(Real) -> Real,
        D: FnMut(Real) -> Real,
    {
        fn value(&mut self, x: Real) -> Real {
            (self.value)(x)
        }
        fn derivative(&mut self, x: Real) -> Real {
            (self.derivative)(x)
        }
    }
    Pair { value, derivative }
}

/// A [`Function1D`] that also exposes its second derivative, for solvers that use
/// curvature (Halley). Build one from three closures with [`func2d`].
pub trait Function2D: Function1D {
    /// `f''(x)`.
    fn second_derivative(&mut self, x: Real) -> Real;
}

/// Adapt value, first-derivative and second-derivative closures into a
/// [`Function2D`].
pub fn func2d<F, D, S>(value: F, derivative: D, second_derivative: S) -> impl Function2D
where
    F: FnMut(Real) -> Real,
    D: FnMut(Real) -> Real,
    S: FnMut(Real) -> Real,
{
    struct Triple<F, D, S> {
        value: F,
        derivative: D,
        second_derivative: S,
    }
    impl<F, D, S> Function1D for Triple<F, D, S>
    where
        F: FnMut(Real) -> Real,
        D: FnMut(Real) -> Real,
        S: FnMut(Real) -> Real,
    {
        fn value(&mut self, x: Real) -> Real {
            (self.value)(x)
        }
        fn derivative(&mut self, x: Real) -> Real {
            (self.derivative)(x)
        }
    }
    impl<F, D, S> Function2D for Triple<F, D, S>
    where
        F: FnMut(Real) -> Real,
        D: FnMut(Real) -> Real,
        S: FnMut(Real) -> Real,
    {
        fn second_derivative(&mut self, x: Real) -> Real {
            (self.second_derivative)(x)
        }
    }
    Triple {
        value,
        derivative,
        second_derivative,
    }
}

/// A 1-D root finder that uses the function's derivative (Newton and friends).
///
/// A separate contract from [`Solver1D`]: its refinement needs `f'`, so it takes
/// a [`Function1D`] rather than a bare value closure. It still reuses the shared
/// bracketing helpers for the auto-bracketing and bracket-validation phases.
pub trait DerivativeSolver {
    /// The shared configuration (evaluation cap, domain bounds).
    fn config(&self) -> &SolverConfig;

    /// Refine an already-bracketed root of `g` to the given `accuracy`, with the
    /// bracket invariants documented on [`Solver1DState`] guaranteed by the driver
    /// (the derivative-solver analogue of [`Solver1D::solve_impl`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the refinement exhausts the evaluation budget or the
    /// method cannot proceed (e.g. a pure Newton step leaves the bracket).
    fn refine<G: Function1D>(
        &self,
        g: &mut G,
        accuracy: Real,
        state: Solver1DState,
    ) -> QlResult<Real>;

    /// Find a zero of `g` near `guess`, auto-bracketing in steps of `step`.
    ///
    /// # Errors
    ///
    /// Returns an error if `accuracy <= 0`, no bracket is found, or [`refine`](Self::refine) fails.
    fn solve<G: Function1D>(
        &self,
        mut g: G,
        accuracy: Real,
        guess: Real,
        step: Real,
    ) -> QlResult<Real> {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        // Bind before matching so the value-closure's borrow of `g` is released
        // before `refine` takes `&mut g`.
        let bracketed = bracket_by_stepping(self.config(), &mut |x| g.value(x), guess, step)?;
        match bracketed {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(st) => self.refine(&mut g, accuracy, st),
        }
    }

    /// Find a zero of `g` in the caller-supplied bracket `[x_min, x_max]`.
    ///
    /// # Errors
    ///
    /// As for [`solve`](Self::solve), plus the bracket-validation errors of the
    /// shared driver.
    fn solve_bracketed<G: Function1D>(
        &self,
        mut g: G,
        accuracy: Real,
        guess: Real,
        x_min: Real,
        x_max: Real,
    ) -> QlResult<Real> {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        let bracketed = bracket_given(self.config(), &mut |x| g.value(x), guess, x_min, x_max)?;
        match bracketed {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(st) => self.refine(&mut g, accuracy, st),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::math::comparison::close;

    // A minimal bisection solver, defined here only to exercise the shared
    // driver (bracketing, bound enforcement, bracket validation, last-call
    // semantics) independently of any real algorithm. Production solvers live in
    // `solvers1d`.
    struct TestBisection {
        config: SolverConfig,
    }

    impl Solver1D for TestBisection {
        fn config(&self) -> &SolverConfig {
            &self.config
        }

        fn config_mut(&mut self) -> &mut SolverConfig {
            &mut self.config
        }

        fn solve_impl<F>(
            &mut self,
            f: &mut F,
            accuracy: Real,
            st: &mut Solver1DState,
        ) -> QlResult<Real>
        where
            F: FnMut(Real) -> Real,
        {
            while st.evaluation_number < self.max_evaluations() {
                let mid = 0.5 * (st.x_min + st.x_max);
                let fmid = f(mid);
                st.evaluation_number += 1;
                if 0.5 * (st.x_max - st.x_min).abs() <= accuracy || close(fmid, 0.0) {
                    return Ok(mid);
                }
                if fmid * st.fx_min < 0.0 {
                    st.x_max = mid;
                    st.fx_max = fmid;
                } else {
                    st.x_min = mid;
                    st.fx_min = fmid;
                }
            }
            fail!("bisection exceeded {} evaluations", self.max_evaluations())
        }
    }

    fn bisection() -> TestBisection {
        TestBisection {
            config: SolverConfig::new(),
        }
    }

    // f(x) = x^2 - 1, single positive root at x = 1.
    fn quadratic(x: Real) -> Real {
        x * x - 1.0
    }

    #[test]
    fn drivers_find_the_root() {
        // auto-bracketing from either side
        for guess in [0.3, 1.7] {
            let root = bisection().solve(quadratic, 1e-10, guess, 0.1).unwrap();
            assert!((root - 1.0).abs() <= 1e-9, "auto guess={guess} root={root}");
        }
        // caller-supplied bracket
        let root = bisection()
            .solve_bracketed(quadratic, 1e-10, 0.5, 0.0, 2.0)
            .unwrap();
        assert!((root - 1.0).abs() <= 1e-9, "bracketed root={root}");
        // x_max is exactly the root: short-circuit before solving
        assert_eq!(
            bisection()
                .solve_bracketed(quadratic, 1e-10, 0.5, 0.0, 1.0)
                .unwrap(),
            1.0
        );
    }

    #[test]
    fn drivers_reject_invalid_inputs() {
        let cases = [
            // (guess, x_min, x_max): inverted range, not straddling a zero, guess outside
            (1.0, 2.0, 0.0),
            (2.5, 2.0, 3.0),
            (5.0, 0.0, 2.0),
        ];
        for (guess, lo, hi) in cases {
            assert!(
                bisection()
                    .solve_bracketed(quadratic, 1e-8, guess, lo, hi)
                    .is_err(),
                "expected error for ({guess}, {lo}, {hi})"
            );
        }
        // non-positive accuracy
        assert!(bisection().solve(quadratic, 0.0, 0.5, 0.1).is_err());
        // bracket outside the enforced bounds (upper, then lower)
        let mut solver = bisection();
        solver.set_upper_bound(2.0);
        assert!(
            solver
                .solve_bracketed(quadratic, 1e-8, 1.5, 0.0, 3.0)
                .is_err()
        );
        let mut solver = bisection();
        solver.set_lower_bound(0.5);
        assert!(
            solver
                .solve_bracketed(quadratic, 1e-8, 0.75, 0.0, 2.0)
                .is_err()
        );
    }

    #[test]
    fn auto_bracketing_fails_within_the_evaluation_cap() {
        // 1 + x^2 has no real root; bracketing must give up, not loop forever.
        let mut solver = bisection();
        solver.set_max_evaluations(20);
        assert!(solver.solve(|x: Real| 1.0 + x * x, 1e-8, 0.5, 0.1).is_err());
    }

    #[test]
    fn auto_bracketing_never_evaluates_outside_the_bounds() {
        // The expanding bracket is clamped to [0, 5] before each evaluation, so f
        // is never sampled outside the domain. Root of x^2 - 2 is sqrt(2), inside.
        let lo = Cell::new(Real::INFINITY);
        let hi = Cell::new(Real::NEG_INFINITY);
        let f = |x: Real| {
            lo.set(lo.get().min(x));
            hi.set(hi.get().max(x));
            x * x - 2.0
        };
        let mut solver = bisection();
        solver.set_lower_bound(0.0);
        solver.set_upper_bound(5.0);
        let root = solver.solve(f, 1e-10, 0.5, 0.1).unwrap();
        assert!((root - 2.0_f64.sqrt()).abs() <= 1e-9, "root={root}");
        assert!(lo.get() >= 0.0, "evaluated below lower bound: {}", lo.get());
        assert!(hi.get() <= 5.0, "evaluated above upper bound: {}", hi.get());
    }
}
