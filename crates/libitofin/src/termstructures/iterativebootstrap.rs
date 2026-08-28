//! Iterative piecewise-curve bootstrap.
//!
//! Port of `ql/termstructures/iterativebootstrap.hpp`. Given a set of rate
//! helpers, the bootstrap solves the curve node at each pillar so that the
//! helper repricing off the curve reproduces its market quote
//! (`helper.quote_error() == 0`).
//!
//! ## What is ported, and what is deferred
//!
//! The driver mirrors `IterativeBootstrap::calculate`: a forward pass solves
//! the curve node at each pillar in maturity order, and an outer convergence
//! loop (`iterativebootstrap.hpp:257,363-387`) repeats that pass when the
//! interpolator is *global* (`Interpolator::GLOBAL`, true for `Cubic`) or when
//! a pillar date precedes its helper's latest-relevant date (`:205-207`).
//! Under a global interpolator every node depends on all others, so one pass
//! leaves the curve mispriced; the loop re-solves every node - each against
//! the full-range interpolation seeded by the previous pass - until the
//! largest node change of a pass is within the bootstrap accuracy. Exhausting
//! `Traits::max_iterations` passes without converging is an explicit `Err`
//! (the C++ `dontThrow=false` branch, `:376-383`). For the local
//! interpolators (`Linear`, `LogLinear`, `BackwardFlat`), whose wired helpers
//! pin the pillar at the latest-relevant date, neither condition fires and the
//! bootstrap stays a single forward pass.
//!
//! Two branches of the C++ algorithm are deliberately not ported:
//!
//! - **The `Linear` interpolation fallback** (`:296-308`), which substitutes
//!   `Linear` while a global interpolation cannot yet span the solved prefix.
//!   It is unreachable for the wired `Cubic` (`required_points()` is 2, so
//!   every prefix of length >= 2 builds) and type-incompatible with the
//!   monomorphized holder: `CurveData<I>` stores `Option<I::Output>`, which
//!   cannot hold a `LinearInterpolation` where a `CubicInterpolation` is
//!   expected - C++ only manages the swap through its type-erased
//!   `Interpolation`. Only an interpolator needing >= 4 points (Akima,
//!   Lagrange), none of which is wired, could ever reach it.
//! - **The robustness fallbacks** - `dontThrow`, `maxAttempts` bound-widening
//!   retries and the `validCurve_` invalidate-and-retry recursion
//!   (`:318-358`) - deferred to #941. Every solve failure and non-convergence
//!   here is an explicit `Err` (D4), never a silent partial curve.
//!
//! Both solvers the C++ uses are wired: `Brent` runs the first (fresh) pass and
//! `FiniteDifferenceNewtonSafe` runs a re-bootstrap seeded from a still-valid
//! previous curve (`:318-322`), the path a quote change takes.
//!
//! ## Duplicate pillars are rejected, not pruned (matching QuantLib)
//!
//! `IterativeBootstrap` hard-throws on two ordering violations: a duplicate
//! pillar date (`iterativebootstrap.hpp:190-191`) and a `latestRelevantDate`
//! that does not strictly advance (`:194-201`). This port reproduces both as the
//! two `require!` checks below (a duplicate pillar, then a non-monotone
//! latest-relevant date). That is faithful, not a defect: `IterativeBootstrap`
//! has **no** redundant-helper pruning upstream. The only dedup in QuantLib
//! lives in the separate, unported `GlobalBootstrap` (`globalbootstrap.hpp:288`),
//! and even there it dedups a date grid for a least-squares solve rather than
//! dropping a helper. So there is nothing to port. This corrects the original
//! #532 premise that the bootstrap "should prune redundant helpers"; the correct
//! behaviour for a genuine duplicate is the error, and callers must supply a
//! strip with distinct pillars. `GlobalBootstrap` itself (a global
//! least-squares fit) is a separate, still-unported slice.

use std::cell::RefCell;

use crate::errors::{QlError, QlResult};
use crate::math::interpolations::Interpolator;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::math::solvers1d::finitedifferencenewtonsafe::FiniteDifferenceNewtonSafe;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{BootstrapHelperShared, sort_by_pillar_date};
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::time::date::Date;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

/// The curve surface the bootstrap drives.
///
/// A piecewise curve implements this to expose its helpers, its mutable node
/// storage and the identity it hands to helpers so they price against it. The
/// bootstrap is the Rust stand-in for C++'s `friend` relationship: rather than
/// reaching into private members, it mutates the curve only through
/// [`curve_data`](Self::curve_data).
pub trait PiecewiseCurve {
    /// The curve-shape traits (`Discount`, ...).
    type Traits: BootstrapTraits;
    /// The interpolation factory.
    type Interp: Interpolator;
    /// The term-structure family the curve belongs to, handed to the helpers
    /// so they price against it.
    type TS: ?Sized;
    /// The bootstrap-helper family the curve is fitted to (`dyn RateHelper`
    /// for a yield curve). The association lives here rather than on the
    /// helper traits: those must stay object-safe as bare `dyn RateHelper`.
    type Helper: BootstrapHelperShared<TS = Self::TS> + ?Sized;

    /// The bootstrap helpers whose quotes the curve is bootstrapped to.
    fn instruments(&self) -> &[Shared<Self::Helper>];

    /// The interpolation factory.
    fn interpolator(&self) -> &Self::Interp;

    /// The mutable node storage the bootstrap writes into.
    fn curve_data(&self) -> &RefCell<CurveData<Self::Interp>>;

    /// The stopping accuracy for the per-node root search.
    fn accuracy(&self) -> Real;

    /// The curve's reference date.
    fn reference_date(&self) -> QlResult<Date>;

    /// The date of the first curve node (`Traits::initialDate`).
    ///
    /// C++ takes this from the traits struct, which for every yield and credit
    /// convention returns the term structure's reference date - hence the
    /// default here. An inflation curve overrides it with its base date, which
    /// *precedes* the reference date, so its first node sits at a negative
    /// time. The driver takes all three of its date decisions from this - the
    /// expired-instrument threshold, the first-alive scan and node 0
    /// (`iterativebootstrap.hpp:161-182`) - while `time_from_reference` stays
    /// anchored to the reference date.
    fn initial_date(&self) -> QlResult<Date> {
        self.reference_date()
    }

    /// The value seeded into the first curve node (`Traits::initialValue`).
    ///
    /// The mirror of [`initial_date`](Self::initial_date): C++ passes the term
    /// structure to `Traits::initialValue(const TS*)`, and while every yield and
    /// credit convention ignores that pointer - hence the default here,
    /// delegating to the traits constant - the year-on-year inflation
    /// convention reads the curve's base rate off it
    /// (`YoYInflationTraits::initialValue(t) = t->baseRate()`,
    /// `inflationtraits.hpp:129-131`, against the zero convention's constant at
    /// `:50-53`). Taking it from the curve rather than from the traits struct is
    /// what lets such a curve seed node 0 from its own state. It returns a
    /// `QlResult` because that base rate is itself fallible.
    fn initial_value(&self) -> QlResult<Real> {
        Ok(Self::Traits::initial_value())
    }

    /// The year fraction from the reference date to `date`.
    fn time_from_reference(&self, date: Date) -> QlResult<Time>;

    /// A strong handle to the curve as a term structure of its family, to hand
    /// to the helpers so they price against it (C++'s
    /// `setTermStructure(this)`).
    fn term_structure_shared(&self) -> QlResult<Shared<Self::TS>>;
}

/// A bootstrap algorithm driving a piecewise curve (the Rust counterpart of
/// C++'s `Bootstrap` template parameter of `PiecewiseYieldCurve`).
///
/// A curve stores its bootstrapper as a type parameter and hands itself to
/// [`calculate`](Self::calculate) from its lazy recalculation; the
/// implementations are [`IterativeBootstrap`] (the per-node root search) and
/// [`LocalBootstrap`](crate::termstructures::localbootstrap::LocalBootstrap)
/// (the localised least-squares fit for the convex-monotone spline).
pub trait Bootstrap<C: PiecewiseCurve> {
    /// Bootstraps `curve` in place (C++'s `Bootstrap::calculate`).
    fn calculate(&self, curve: &C) -> QlResult<()>;
}

/// The iterative bootstrap (`IterativeBootstrap`).
///
/// Carries the stopping accuracy override; the solvers and the traits come from
/// the curve. Defaults mirror the C++ constructor: accuracy taken from the
/// term structure, a single attempt per node, throw on non-convergence.
#[derive(Clone, Copy, Debug, Default)]
pub struct IterativeBootstrap {
    accuracy: Option<Real>,
}

impl IterativeBootstrap {
    /// The default bootstrap: accuracy from the curve, throw on failure.
    pub fn new() -> IterativeBootstrap {
        IterativeBootstrap { accuracy: None }
    }

    /// Bootstraps `curve` in place, solving every alive pillar (C++'s
    /// `calculate`): one forward pass for a local interpolator, passes to
    /// convergence for a global one.
    pub fn calculate<C: PiecewiseCurve>(&self, curve: &C) -> QlResult<()> {
        let mut helpers: Vec<Shared<C::Helper>> = curve.instruments().to_vec();
        let n = helpers.len();
        require!(n > 0, "no bootstrap helpers given");
        sort_by_pillar_date(&mut helpers);

        let first_date = curve.initial_date()?;
        let initial_value = curve.initial_value()?;
        require!(
            helpers[n - 1].pillar_date() > first_date,
            "all instruments expired"
        );
        let mut first_alive = 0usize;
        while helpers[first_alive].pillar_date() <= first_date {
            first_alive += 1;
        }
        let alive = n - first_alive;
        let nodes = alive + 1;
        let required = curve.interpolator().required_points();
        require!(
            nodes >= required,
            "not enough alive instruments: {alive} provided, {} required",
            required - 1
        );

        let mut dates = Vec::with_capacity(nodes);
        let mut times = Vec::with_capacity(nodes);
        dates.push(first_date);
        times.push(curve.time_from_reference(first_date)?);
        let mut max_date = first_date;
        let mut loop_required = C::Interp::GLOBAL;
        for (i, j) in (1..).zip(first_alive..n) {
            let pillar = helpers[j].pillar_date();
            require!(
                dates[i - 1] != pillar,
                "more than one instrument with pillar {pillar}"
            );
            let latest_relevant = helpers[j].latest_relevant_date();
            require!(
                latest_relevant > max_date,
                "instrument with pillar {pillar} has latest-relevant date \
                 {latest_relevant} before or equal to a previous instrument's ({max_date})"
            );
            dates.push(pillar);
            times.push(curve.time_from_reference(pillar)?);
            max_date = pillar.max(latest_relevant);
            // A pillar before the last relevant date forces the convergence
            // loop even for a local interpolator (`:205-207`).
            if pillar < latest_relevant {
                loop_required = true;
            }
        }

        // Install the pillars, seeding the values from a still-valid previous
        // solution when its shape matches, otherwise resetting to the curve's
        // initial value (`:212-218`).
        let mut valid_data = {
            let mut cd = curve.curve_data().borrow_mut();
            let reuse = cd.is_valid() && cd.data().len() == nodes;
            cd.set_pillars(dates, times);
            if !reuse {
                cd.reset_data(initial_value, nodes);
            }
            cd.set_max_date(max_date);
            reuse
        };

        // Hand the curve to each alive helper and reject invalid quotes.
        let term_structure = curve.term_structure_shared()?;
        for helper in helpers.iter().take(n).skip(first_alive) {
            helper.quote_value()?;
            helper.set_term_structure(&term_structure);
        }

        let accuracy = self.accuracy.unwrap_or_else(|| curve.accuracy());
        let max_iterations = C::Traits::max_iterations() - 1;
        let mut previous_data: Vec<Real> = Vec::new();

        for iteration in 0.. {
            // Snapshot the previous pass so the exit condition can measure how
            // much the curve moved (`:258-259`).
            if loop_required && valid_data {
                previous_data = curve.curve_data().borrow().data().to_vec();
            }

            for (i, j) in (1..).zip(first_alive..n) {
                let (min, max, guess) = {
                    let cd = curve.curve_data().borrow();
                    let min = C::Traits::min_value_after(i, cd.times(), cd.data(), valid_data);
                    let max = C::Traits::max_value_after(i, cd.times(), cd.data(), valid_data);
                    let mut guess = C::Traits::guess(i, cd.times(), cd.data(), valid_data);
                    // Nudge a guess that sits on or past a bracket end back
                    // inside it (`:290-293`).
                    if guess >= max {
                        guess = max - (max - min) / 5.0;
                    } else if guess <= min {
                        guess = min + (max - min) / 5.0;
                    }
                    (min, max, guess)
                };

                // On a fresh pass the interpolation is extended a point at a
                // time over the solved prefix; with valid data C++ keeps the
                // full-range interpolation and only updates it (`:295-309`),
                // so every re-solve spans all nodes.
                let upto = if valid_data { alive } else { i };
                let helper = &helpers[j];
                let error_slot: RefCell<Option<QlError>> = RefCell::new(None);
                let error = |g: Real| -> Real {
                    match node_error::<C>(curve, i, upto, helper, g) {
                        Ok(value) => value,
                        Err(err) => {
                            *error_slot.borrow_mut() = Some(err);
                            Real::NAN
                        }
                    }
                };

                let solved = if valid_data {
                    FiniteDifferenceNewtonSafe::new()
                        .solve_bracketed(error, accuracy, guess, min, max)
                } else {
                    Brent::new().solve_bracketed(error, accuracy, guess, min, max)
                };

                let root = match solved {
                    Ok(root) => root,
                    Err(solver_err) => {
                        if let Some(inner) = error_slot.into_inner() {
                            return Err(inner);
                        }
                        fail!(
                            "bootstrap failed at pillar {} (maturity {}): {}",
                            helper.pillar_date(),
                            helper.maturity_date(),
                            solver_err.message()
                        );
                    }
                };

                // Pin the solved value and rebuild so the final curve holds
                // the root exactly, not the solver's last trial point.
                let mut cd = curve.curve_data().borrow_mut();
                C::Traits::update_guess(cd.data_mut(), root, i);
                cd.rebuild(curve.interpolator(), upto)?;
            }

            if !loop_required {
                break;
            }

            // Exit condition (`:363-387`): converged when the largest node
            // move of a full pass is within the bootstrap accuracy.
            let mut change: Real = 0.0;
            if valid_data {
                let cd = curve.curve_data().borrow();
                let data = cd.data();
                for i in 1..=alive {
                    change = change.max((data[i] - previous_data[i]).abs());
                }
                if change <= accuracy {
                    break;
                }
            }

            require!(
                iteration != max_iterations,
                "convergence not reached after {iteration} iterations; \
                 last improvement {change}, required accuracy {accuracy}"
            );

            valid_data = true;
        }

        curve.curve_data().borrow_mut().set_valid(true);
        Ok(())
    }
}

impl<C: PiecewiseCurve> Bootstrap<C> for IterativeBootstrap {
    /// Delegates to the inherent [`IterativeBootstrap::calculate`], so the
    /// concrete curves that name their bootstrapper keep calling it without
    /// importing the trait.
    fn calculate(&self, curve: &C) -> QlResult<()> {
        IterativeBootstrap::calculate(self, curve)
    }
}

/// Writes a trial value into node `i`, rebuilds the interpolation over
/// `[0, upto]`, and returns the helper's quote error. `upto` is `i` on a fresh
/// pass (the solved prefix) and the last node when reusing valid data, where
/// C++ only updates the already full-range interpolation. The mutable borrow
/// of the node storage is dropped before the helper reprices, so the helper
/// can read the same curve back without a `RefCell` conflict.
fn node_error<C: PiecewiseCurve>(
    curve: &C,
    i: Size,
    upto: Size,
    helper: &Shared<C::Helper>,
    guess: Real,
) -> QlResult<Real> {
    {
        let mut cd = curve.curve_data().borrow_mut();
        C::Traits::update_guess(cd.data_mut(), guess, i);
        cd.rebuild(curve.interpolator(), upto)?;
    }
    helper.quote_error()
}

#[cfg(test)]
mod tests {
    use std::rc::Weak;

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::{Discount, YieldBootstrapTraits};
    use crate::termstructures::yields::DepositRateHelper;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::termstructures::{TermStructure, TermStructureBase};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::DiscountFactor;

    /// A minimal piecewise curve: no lazy wiring, just the node storage the
    /// bootstrap mutates and the discount lookup helpers read back. Exercises
    /// the bootstrap in isolation from the full `PiecewiseYieldCurve`.
    struct StubCurve {
        base: TermStructureBase,
        instruments: Vec<Shared<dyn RateHelper>>,
        interpolator: LogLinear,
        data: RefCell<CurveData<LogLinear>>,
        self_weak: Weak<dyn YieldTermStructure>,
        initial_date: Option<Date>,
        initial_value: Option<Real>,
    }

    impl StubCurve {
        fn new(reference: Date, instruments: Vec<Shared<dyn RateHelper>>) -> Shared<StubCurve> {
            StubCurve::build(reference, instruments, None, None)
        }

        /// A curve whose first node sits at `initial_date` rather than at the
        /// reference date, the way an inflation curve's base date does.
        fn with_initial_date(
            reference: Date,
            instruments: Vec<Shared<dyn RateHelper>>,
            initial_date: Date,
        ) -> Shared<StubCurve> {
            StubCurve::build(reference, instruments, Some(initial_date), None)
        }

        /// A curve that seeds node 0 from its own state rather than from the
        /// traits constant, the way a year-on-year inflation curve seeds it
        /// from its base rate.
        fn with_initial_value(
            reference: Date,
            instruments: Vec<Shared<dyn RateHelper>>,
            initial_value: Real,
        ) -> Shared<StubCurve> {
            StubCurve::build(reference, instruments, None, Some(initial_value))
        }

        fn build(
            reference: Date,
            instruments: Vec<Shared<dyn RateHelper>>,
            initial_date: Option<Date>,
            initial_value: Option<Real>,
        ) -> Shared<StubCurve> {
            Shared::new_cyclic(|weak: &Weak<StubCurve>| {
                let self_weak: Weak<dyn YieldTermStructure> = weak.clone();
                StubCurve {
                    base: TermStructureBase::with_reference_date(
                        reference,
                        None,
                        Some(Actual360::new()),
                    ),
                    instruments,
                    interpolator: LogLinear,
                    data: RefCell::new(CurveData::new()),
                    self_weak,
                    initial_date,
                    initial_value,
                }
            })
        }
    }

    impl AsObservable for StubCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for StubCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            self.data
                .borrow()
                .max_date()
                .unwrap_or_else(|| self.base.reference_date().expect("fixed reference date"))
        }
    }

    impl YieldTermStructure for StubCurve {
        fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
            let data = self.data.borrow();
            Discount::discount_from_nodes(data.interpolation()?, t)
        }
    }

    impl PiecewiseCurve for StubCurve {
        type Traits = Discount;
        type Interp = LogLinear;
        type TS = dyn YieldTermStructure;
        type Helper = dyn RateHelper;

        fn instruments(&self) -> &[Shared<dyn RateHelper>] {
            &self.instruments
        }

        fn interpolator(&self) -> &LogLinear {
            &self.interpolator
        }

        fn curve_data(&self) -> &RefCell<CurveData<LogLinear>> {
            &self.data
        }

        fn accuracy(&self) -> Real {
            1.0e-12
        }

        fn reference_date(&self) -> QlResult<Date> {
            self.base.reference_date()
        }

        fn initial_date(&self) -> QlResult<Date> {
            match self.initial_date {
                Some(date) => Ok(date),
                None => self.base.reference_date(),
            }
        }

        fn initial_value(&self) -> QlResult<Real> {
            match self.initial_value {
                Some(value) => Ok(value),
                None => Ok(Discount::initial_value()),
            }
        }

        fn time_from_reference(&self, date: Date) -> QlResult<Time> {
            TermStructure::time_from_reference(self, date)
        }

        fn term_structure_shared(&self) -> QlResult<Shared<dyn YieldTermStructure>> {
            self.self_weak
                .upgrade()
                .ok_or_else(|| QlError::new("curve dropped", file!(), line!()))
        }
    }

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn euribor(
        tenor: Period,
        settings: Shared<Settings<Date>>,
    ) -> crate::indexes::iborindex::IborIndex {
        Euribor::new(tenor, Handle::empty(), settings).expect("month tenor is valid")
    }

    /// The bootstrap solves each deposit node so the helper reprices its own
    /// quote off the curve: after bootstrapping, every quote error is zero to
    /// solver accuracy. This is the deposit round-trip in miniature.
    #[test]
    fn bootstrap_reproduces_deposit_quotes() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);

        let three_m = euribor(Period::new(3, TimeUnit::Months), settings.clone());
        let six_m = euribor(Period::new(6, TimeUnit::Months), settings.clone());
        let nine_m = euribor(Period::new(9, TimeUnit::Months), settings.clone());

        let h3 = DepositRateHelper::from_rate(0.04557, &three_m);
        let h6 = DepositRateHelper::from_rate(0.04496, &six_m);
        let h9 = DepositRateHelper::from_rate(0.04490, &nine_m);

        // All deposits spot-start at the same value date; use it as the curve
        // reference so discount(reference) = 1 aligns with the value date.
        let reference = h3.earliest_date();
        let instruments: Vec<Shared<dyn RateHelper>> = vec![
            Shared::clone(&h3) as Shared<dyn RateHelper>,
            Shared::clone(&h6) as Shared<dyn RateHelper>,
            Shared::clone(&h9) as Shared<dyn RateHelper>,
        ];

        let curve = StubCurve::new(reference, instruments);
        IterativeBootstrap::new().calculate(curve.as_ref()).unwrap();

        for helper in [&h3, &h6, &h9] {
            let error = helper.quote_error().unwrap();
            assert!(error.abs() < 1.0e-12, "deposit quote error {error}");
        }
    }

    /// The first node is laid down at the curve's `initial_date`, not at its
    /// reference date, and its time is still measured from the reference - so a
    /// curve whose first node precedes the reference (an inflation base date)
    /// gets `times[0] < 0`. Without this the regression suites would not move:
    /// the default `initial_date` makes the two dates equal everywhere else.
    #[test]
    fn the_first_node_sits_at_the_initial_date_which_may_precede_the_reference() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);

        let three_m = euribor(Period::new(3, TimeUnit::Months), settings.clone());
        let six_m = euribor(Period::new(6, TimeUnit::Months), settings.clone());

        let h3 = DepositRateHelper::from_rate(0.04557, &three_m);
        let h6 = DepositRateHelper::from_rate(0.04496, &six_m);

        let reference = h3.earliest_date();
        let base_date = reference - 90;
        let instruments: Vec<Shared<dyn RateHelper>> = vec![
            Shared::clone(&h3) as Shared<dyn RateHelper>,
            Shared::clone(&h6) as Shared<dyn RateHelper>,
        ];

        let curve = StubCurve::with_initial_date(reference, instruments, base_date);
        IterativeBootstrap::new().calculate(curve.as_ref()).unwrap();

        let data = curve.data.borrow();
        assert_eq!(data.dates()[0], base_date);
        assert!(data.times()[0] < 0.0, "times[0] = {}", data.times()[0]);
        assert_eq!(data.times()[0], -90.0 / 360.0);
        drop(data);

        for helper in [&h3, &h6] {
            let error = helper.quote_error().unwrap();
            assert!(error.abs() < 1.0e-12, "deposit quote error {error}");
        }
    }

    /// Node 0 is seeded through the curve's `initial_value` hook, not from the
    /// traits constant: a curve that overrides the hook sees its own value in
    /// the bootstrapped node vector. `Discount::update_guess` only ever writes
    /// nodes `1..`, so the seed survives the solve and the assertion reads it
    /// back after the driver has run to completion - which the quote errors
    /// confirm. This is what proves the hook is wired into the driver rather
    /// than merely defined: with the seed still taken from the traits, node 0
    /// would read back as `Discount::initial_value()`.
    #[test]
    fn node_zero_is_seeded_from_the_curves_initial_value_hook() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);

        let three_m = euribor(Period::new(3, TimeUnit::Months), settings.clone());
        let six_m = euribor(Period::new(6, TimeUnit::Months), settings.clone());

        let h3 = DepositRateHelper::from_rate(0.04557, &three_m);
        let h6 = DepositRateHelper::from_rate(0.04496, &six_m);

        let reference = h3.earliest_date();
        let instruments: Vec<Shared<dyn RateHelper>> = vec![
            Shared::clone(&h3) as Shared<dyn RateHelper>,
            Shared::clone(&h6) as Shared<dyn RateHelper>,
        ];

        let seed = 0.42;
        assert_ne!(seed, Discount::initial_value());

        let curve = StubCurve::with_initial_value(reference, instruments, seed);
        IterativeBootstrap::new().calculate(curve.as_ref()).unwrap();

        assert_eq!(curve.data.borrow().data()[0], seed);

        for helper in [&h3, &h6] {
            let error = helper.quote_error().unwrap();
            assert!(error.abs() < 1.0e-12, "deposit quote error {error}");
        }
    }

    #[test]
    fn empty_helper_set_is_rejected() {
        let today = Date::new(15, Month::June, 2026);
        let _ = settings_on(today);
        let curve = StubCurve::new(today, Vec::new());
        let err = IterativeBootstrap::new()
            .calculate(curve.as_ref())
            .unwrap_err();
        assert!(err.message().contains("no bootstrap helpers"));
    }
}
