//! Longstaff-Schwartz path pricer for early-exercise options.
//!
//! Port of `ql/methods/montecarlo/longstaffschwartzpathpricer.hpp:52-211`, the
//! two-phase least-squares Monte Carlo core (Longstaff and Schwartz, 2001).
//! During calibration the pricer buffers every path it is shown; [`calibrate`]
//! then walks the exercise dates backwards, regressing the discounted realized
//! future cashflow of the in-the-money paths against the basis system to get a
//! continuation value per date. Every path is then priced against that fit.
//!
//! Divergences from `longstaffschwartzpathpricer.hpp`, all deliberate:
//! - **fixed to [`Path`] with `State = Real`, not generic over the path type**:
//!   C++ templates on `PathType` and resolves the state through
//!   `EarlyExerciseTraits` (`:52-55`). [`GeneralLinearLeastSquares`] needs a
//!   `Copy` regressor and this stack ports the single-factor engine only. The
//!   `MultiPath` form is instantiated in QuantLib
//!   (`mcamericanbasketengine.hpp:64`), so the basket engine needs this
//!   generalized before it can be ported.
//! - **the mutable calibration state sits behind `RefCell`/`Cell`**: the C++
//!   `operator()` is `const` and mutates through `mutable` members (`:75,80`).
//!   [`PathPricer::price`] likewise takes `&self`, and the engine holds the
//!   pricer as a [`Shared`], which has no `&mut` projection. So does
//!   [`calibrate`], which additionally returns [`QlResult`] because the
//!   regression is fallible under D4 where C++ throws.
//! - **[`exercise_probability`] returns [`QlResult`]**: [`MeanStdDev::mean`]
//!   fails on an empty sample set, reachable by calling the accessor before any
//!   path is priced. A bare `Real` would need a panic or a silent `0.0`, and
//!   D10 forbids the silent fallback.
//!
//! Deferred, omitted visibly rather than accepted and ignored:
//! - **`post_processing`** (`:67-70`, called at `:155,198`): an empty hook with
//!   no override anywhere in `ql/`. Omitting it also drops the
//!   `state(paths_[j], i)` calls at `:150,193`, which only build its arguments.
//!
//! Taking the buffered paths out of the cell up front (the C++ swap-and-release
//! at `:201-203`) means an `Err` out of the regression drops the buffer while
//! `calibration_phase` is still set, where C++ unwinds with `paths_` intact. The
//! caller aborts on that `Err`, so no recovery machinery is warranted.
//!
//! [`calibrate`]: LongstaffSchwartzPathPricer::calibrate
//! [`exercise_probability`]: LongstaffSchwartzPathPricer::exercise_probability

use std::cell::{Cell, RefCell};

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::array::Array;
use crate::math::generallinearleastsquares::GeneralLinearLeastSquares;
use crate::math::statistics::{IncrementalStatistics, MeanStdDev, Statistics};
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{EarlyExercisePathPricer, Path, PathPricer};
use crate::require;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::{DiscountFactor, Real, Size};

/// Prices an early-exercisable instrument by least-squares Monte Carlo.
pub struct LongstaffSchwartzPathPricer {
    path_pricer: Shared<dyn EarlyExercisePathPricer<Path, State = Real>>,
    v: Vec<Box<dyn Fn(Real) -> Real>>,
    df: Vec<DiscountFactor>,
    len: Size,
    calibration_phase: Cell<bool>,
    coeff: RefCell<Vec<Array>>,
    paths: RefCell<Vec<Path>>,
    exercise_probability: RefCell<IncrementalStatistics>,
}

impl LongstaffSchwartzPathPricer {
    /// Builds the pricer over the exercise `times` (`:87-99`). Grid index `0` is
    /// today and `len - 1` is maturity, so the interior exercise indices are
    /// `len - 2` down to `1` and `coeff[i - 1]` holds the fit for exercise index
    /// `i`. `df[i]` is the one-step discount `P(times[i+1]) / P(times[i])`
    /// (`:95-98`).
    ///
    /// # Errors
    ///
    /// Returns an error when `times` holds fewer than two points, the term
    /// structure handle is empty, or a discount lookup is out of range.
    pub fn new(
        times: &TimeGrid,
        path_pricer: Shared<dyn EarlyExercisePathPricer<Path, State = Real>>,
        term_structure: &Handle<dyn YieldTermStructure>,
    ) -> QlResult<Self> {
        let len = times.size();
        require!(len >= 2, "at least two exercise times required");

        let curve = term_structure.current_link()?;
        let t = times.times();
        let mut df = Vec::with_capacity(len - 1);
        for i in 0..len - 1 {
            df.push(curve.discount(t[i + 1], false)? / curve.discount(t[i], false)?);
        }

        Ok(LongstaffSchwartzPathPricer {
            v: path_pricer.basis_system(),
            path_pricer,
            df,
            len,
            calibration_phase: Cell::new(true),
            coeff: RefCell::new(vec![Array::new(); len - 2]),
            paths: RefCell::new(Vec::new()),
            exercise_probability: RefCell::new(IncrementalStatistics::new()),
        })
    }

    /// Fits the continuation values against the buffered calibration paths and
    /// enters the pricing phase (`:142-206`).
    ///
    /// # Errors
    ///
    /// Returns an error when a regression fails.
    pub fn calibrate(&self) -> QlResult<()> {
        let paths = std::mem::take(&mut *self.paths.borrow_mut());
        let mut prices: Vec<Real> = paths
            .iter()
            .map(|path| self.path_pricer.value(path, self.len - 1))
            .collect();

        for i in (1..self.len - 1).rev() {
            let mut itm: Vec<(Size, Real, Real)> = Vec::new();
            let mut x: Vec<Real> = Vec::new();
            let mut y: Vec<Real> = Vec::new();
            for (j, path) in paths.iter().enumerate() {
                let exercise = self.path_pricer.value(path, i);
                if exercise > 0.0 {
                    let state = self.path_pricer.state(path, i);
                    itm.push((j, state, exercise));
                    x.push(state);
                    y.push(self.df[i] * prices[j]);
                }
            }

            let fit = if self.v.len() <= x.len() {
                GeneralLinearLeastSquares::new(&x, &y, &self.v)?
                    .coefficients()
                    .clone()
            } else {
                Array::with_size(self.v.len())
            };

            for price in prices.iter_mut() {
                *price *= self.df[i];
            }
            for (j, state, exercise) in itm {
                if self.continuation(&fit, state) < exercise {
                    prices[j] = exercise;
                }
            }
            self.coeff.borrow_mut()[i - 1] = fit;
        }

        self.calibration_phase.set(false);
        Ok(())
    }

    /// The share of priced paths that were exercised, early or at maturity
    /// (`:208-211`).
    ///
    /// # Errors
    ///
    /// Returns an error when no path has been priced yet.
    pub fn exercise_probability(&self) -> QlResult<Real> {
        self.exercise_probability.borrow().mean()
    }

    fn continuation(&self, coeff: &Array, state: Real) -> Real {
        (0..self.v.len()).map(|l| coeff[l] * self.v[l](state)).sum()
    }

    fn coefficients(&self, i: Size) -> Array {
        self.coeff.borrow()[i - 1].clone()
    }
}

impl PathPricer<Path> for LongstaffSchwartzPathPricer {
    /// Buffers `path` and reports `0.0` during calibration; otherwise runs the
    /// backward induction against the fitted coefficients (`:101-140`).
    fn price(&self, path: &Path) -> Real {
        if self.calibration_phase.get() {
            self.paths.borrow_mut().push(path.clone());
            return 0.0;
        }

        let mut price = self.path_pricer.value(path, self.len - 1);
        let mut exercised = price > 0.0;

        for i in (1..self.len - 1).rev() {
            price *= self.df[i];

            let exercise = self.path_pricer.value(path, i);
            if exercise > 0.0 {
                let state = self.path_pricer.state(path, i);
                if self.continuation(&self.coefficients(i), state) < exercise {
                    price = exercise;
                    exercised = true;
                }
            }
        }

        self.exercise_probability
            .borrow_mut()
            .add(if exercised { 1.0 } else { 0.0 })
            .expect("a unit-weighted 0/1 indicator is a valid sample");

        price * self.df[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::methods::montecarlo::{LsmBasisSystem, PolynomialType};
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::Time;

    const STRIKE: Real = 100.0;
    const TOL: Real = 1e-10;

    /// An American put over the spot: exercise value is the payoff, the
    /// regressor is the spot, and the basis system is `{1, x}`.
    struct AmericanPut;

    impl EarlyExercisePathPricer<Path> for AmericanPut {
        type State = Real;

        fn value(&self, path: &Path, t: Size) -> Real {
            (STRIKE - path[t]).max(0.0)
        }

        fn state(&self, path: &Path, t: Size) -> Real {
            path[t]
        }

        fn basis_system(&self) -> Vec<Box<dyn Fn(Real) -> Real>> {
            LsmBasisSystem::path_basis_system(1, PolynomialType::Monomial)
        }
    }

    fn grid() -> TimeGrid {
        TimeGrid::new(3.0, 3).unwrap()
    }

    fn path(spots: [Real; 4]) -> Path {
        Path::new(grid(), Array::from(spots)).unwrap()
    }

    /// A continuously compounded `ln 2` flat curve, so every one-year step
    /// discounts by exactly one half and the hand arithmetic stays exact.
    fn half_step_curve() -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            Date::new(15, Month::June, 2026),
            (2.0 as Time).ln(),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn pricer() -> LongstaffSchwartzPathPricer {
        LongstaffSchwartzPathPricer::new(&grid(), shared(AmericanPut), &half_step_curve()).unwrap()
    }

    /// The fixture the hand arithmetic below is derived from. P3 is out of the
    /// money at exercise index 2 and in the money at index 1, so its index-2
    /// discount reaches the index-1 regression only if the roll-back discounts
    /// EVERY path rather than the in-the-money ones alone.
    fn calibration_paths() -> [Path; 3] {
        [
            path([100.0, 90.0, 80.0, 96.0]),
            path([100.0, 98.0, 90.0, 108.0]),
            path([100.0, 94.0, 110.0, 76.0]),
        ]
    }

    fn calibrated() -> LongstaffSchwartzPathPricer {
        let lsm = pricer();
        for p in calibration_paths() {
            lsm.price(&p);
        }
        lsm.calibrate().unwrap();
        lsm
    }

    /// Calibration-phase pricing buffers the path and reports nothing.
    #[test]
    fn the_calibration_phase_buffers_and_reports_zero() {
        let lsm = pricer();
        for (n, p) in calibration_paths().into_iter().enumerate() {
            assert_eq!(lsm.price(&p), 0.0);
            assert_eq!(lsm.paths.borrow().len(), n + 1);
        }
        assert!(lsm.calibration_phase.get());
        assert!(lsm.exercise_probability().is_err(), "nothing priced yet");
    }

    /// The hand-computed regression, with `K = 100`, every step discounting by
    /// one half, and terminal payoffs `[4, 0, 24]`.
    ///
    /// Index 2: exercise `[20, 10, 0]`, so the fit runs over P1 and P2 with
    /// `x = [80, 90]`, `y = [0.5*4, 0.5*0] = [2, 0]`. Two points against two
    /// basis functions fit exactly: `b = (0 - 2)/(90 - 80) = -0.2`,
    /// `a = 2 + 0.2*80 = 18`. Rolling back gives prices `[2, 0, 12]`; P1 and P2
    /// exercise (continuations 2 and 0 against 20 and 10) and P3 keeps its
    /// carried 12, leaving `[20, 10, 12]`.
    ///
    /// Index 1: exercise `[10, 2, 6]`, all in the money, so the fit runs over
    /// `x = [90, 98, 94]`, `y = [10, 5, 6]`. With `xbar = 94`, `ybar = 7`,
    /// `Sxx = 32`, `Sxy = -20`: `b = -0.625`, `a = 7 + 0.625*94 = 65.75`. Had
    /// P3 skipped the index-2 discount its `y` would be 12, lifting `a` to
    /// 67.75.
    #[test]
    fn calibrate_reproduces_the_hand_computed_coefficients() {
        let lsm = calibrated();
        let late = lsm.coefficients(2);
        let early = lsm.coefficients(1);

        assert!((late[0] - 18.0).abs() < TOL, "got {}", late[0]);
        assert!((late[1] + 0.2).abs() < TOL, "got {}", late[1]);
        assert!((early[0] - 65.75).abs() < TOL, "got {}", early[0]);
        assert!((early[1] + 0.625).abs() < TOL, "got {}", early[1]);
        assert!(!lsm.calibration_phase.get());
        assert!(lsm.paths.borrow().is_empty(), "the buffer is released");
    }

    /// P1 against those coefficients: terminal 4, halved to 2 at index 2 where
    /// the continuation `18 - 0.2*80 = 2` loses to the exercise value 20;
    /// halved to 10 at index 1 where `65.75 - 0.625*90 = 9.5` loses to 10; one
    /// more halving to 5.
    ///
    /// P2 is out of the money at maturity, exercises into 10 at index 2
    /// (continuation 0), then holds at index 1 where the continuation 4.5 beats
    /// the exercise value 2, so `10 * 0.5 * 0.5 = 2.5`.
    ///
    /// P3 never exercises early: out of the money at index 2, and at index 1
    /// `65.75 - 0.625*94 = 7` beats the exercise value 6, so its terminal 24
    /// just takes three halvings to 3.
    #[test]
    fn the_pricing_phase_returns_the_hand_computed_prices() {
        let lsm = calibrated();
        let [p1, p2, p3] = calibration_paths();

        assert!((lsm.price(&p1) - 5.0).abs() < TOL);
        assert!((lsm.price(&p2) - 2.5).abs() < TOL);
        assert!((lsm.price(&p3) - 3.0).abs() < TOL);
    }

    /// P3 is in the money at maturity but never exercises early, so a port that
    /// counted early exercises alone would report 0.5 here instead of 0.75.
    #[test]
    fn the_exercise_probability_counts_a_terminal_exercise() {
        let lsm = calibrated();
        for p in calibration_paths() {
            lsm.price(&p);
        }
        lsm.price(&path([100.0, 130.0, 140.0, 150.0]));

        assert!((lsm.exercise_probability().unwrap() - 0.75).abs() < TOL);
    }

    /// A continuation equal to the exercise value must NOT exercise (`:128,188`
    /// compare strictly). The tie is set exactly rather than fitted, since a
    /// fitted one would rest on rounding: coefficients `{0, 1}` make the
    /// continuation the spot itself, and a spot of 50 against a strike of 100
    /// puts both sides on exactly 50. The path is out of the money at index 2,
    /// so its terminal 40 halves twice to 10 before reaching the tie; holding
    /// returns `10 * 0.5 = 5`, exercising would return `50 * 0.5 = 25`.
    #[test]
    fn a_continuation_equal_to_the_exercise_value_holds() {
        let lsm = pricer();
        lsm.calibrate().unwrap();
        lsm.coeff.borrow_mut()[0] = Array::from([0.0, 1.0]);

        assert!((lsm.price(&path([100.0, 50.0, 110.0, 60.0])) - 5.0).abs() < TOL);
    }

    /// Calibrating off a single path leaves one in-the-money sample against two
    /// basis functions at both exercise indices, so the fit is skipped and the
    /// coefficients zeroed; every in-the-money path then exercises, a zero
    /// continuation losing to any positive exercise value. P2 shows it: it
    /// exercises into 10 at index 2 as before, but at index 1 the zero
    /// continuation now loses to the exercise value 2, pricing it at 1 not 2.5.
    #[test]
    fn too_few_itm_paths_zero_the_coefficients_and_force_exercise() {
        let lsm = pricer();
        lsm.price(&path([100.0, 90.0, 80.0, 96.0]));
        lsm.calibrate().unwrap();

        assert_eq!(lsm.coefficients(1), Array::with_size(2));
        assert_eq!(lsm.coefficients(2), Array::with_size(2));

        assert!((lsm.price(&path([100.0, 98.0, 90.0, 108.0])) - 1.0).abs() < TOL);
    }
}
