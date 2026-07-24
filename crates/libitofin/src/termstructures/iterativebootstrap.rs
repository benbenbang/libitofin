//! Iterative piecewise-curve bootstrap.
//!
//! Port of `ql/termstructures/iterativebootstrap.hpp`. Given a set of rate
//! helpers, the bootstrap solves the curve node at each pillar so that the
//! helper repricing off the curve reproduces its market quote
//! (`helper.quote_error() == 0`).
//!
//! ## What is ported, and what is provably dead for this path
//!
//! The wired conventions (`Discount`, `ZeroYield`, `ForwardRate`) all run on
//! *local* interpolators - `LogLinear`, `Linear`, `BackwardFlat` - whose value
//! at a point depends only on the bracketing nodes. Several branches of the C++
//! algorithm exist only for *global* interpolators (splines) and are dead here:
//!
//! - **The outer convergence loop** (`iterativebootstrap.hpp:363-387`). It
//!   re-solves every node until the curve stops moving, and runs only when
//!   `loopRequired_` is set. `loopRequired_` starts as `Interpolator::global`
//!   (false for local) and is only forced true when a pillar date precedes the
//!   helper's latest-relevant date (`:206`). Every helper in scope here pins
//!   its pillar at its latest-relevant date, so the loop never runs: the
//!   bootstrap is a single forward pass. This port makes that single pass
//!   explicit.
//! - **The `Linear` interpolation fallback** (`:296-308`). When the target
//!   interpolation cannot yet span the solved prefix, a *global* interpolator
//!   falls back to `Linear`; a *local* one rethrows (`:302-303`). `LogLinear`
//!   and `Linear` need only two points and always span a prefix of length
//!   >= 2, so the fallback is unreachable. Not ported.
//! - **Bound-widening retries and `dontThrow`** (`maxAttempts > 1`, `:336-351`).
//!   Optional robustness features off by default; non-convergence is instead an
//!   explicit `Err` (D10), never a silent partial curve.
//!
//! Both solvers the C++ uses are wired: `Brent` runs the first (fresh) pass and
//! `FiniteDifferenceNewtonSafe` runs a re-bootstrap seeded from a still-valid
//! previous curve (`:318-322`), the path a quote change takes.

use std::cell::RefCell;

use crate::errors::{QlError, QlResult};
use crate::math::interpolations::Interpolator;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::math::solvers1d::finitedifferencenewtonsafe::FiniteDifferenceNewtonSafe;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{RateHelper, sort_by_pillar_date};
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
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

    /// The rate helpers whose quotes the curve is bootstrapped to.
    fn instruments(&self) -> &[Shared<dyn RateHelper>];

    /// The interpolation factory.
    fn interpolator(&self) -> &Self::Interp;

    /// The mutable node storage the bootstrap writes into.
    fn curve_data(&self) -> &RefCell<CurveData<Self::Interp>>;

    /// The stopping accuracy for the per-node root search.
    fn accuracy(&self) -> Real;

    /// The curve's reference date (the first curve node).
    fn reference_date(&self) -> QlResult<Date>;

    /// The year fraction from the reference date to `date`.
    fn time_from_reference(&self, date: Date) -> QlResult<Time>;

    /// A strong handle to the curve as a yield term structure, to hand to the
    /// helpers so they price against it (C++'s `setTermStructure(this)`).
    fn term_structure_shared(&self) -> QlResult<Shared<dyn YieldTermStructure>>;
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
    /// `calculate`, single-pass for the local-interpolator scope).
    pub fn calculate<C: PiecewiseCurve>(&self, curve: &C) -> QlResult<()> {
        let mut helpers: Vec<Shared<dyn RateHelper>> = curve.instruments().to_vec();
        let n = helpers.len();
        require!(n > 0, "no bootstrap helpers given");
        sort_by_pillar_date(&mut helpers);

        let first_date = curve.reference_date()?;
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
        }

        // Install the pillars, seeding the values from a still-valid previous
        // solution when its shape matches, otherwise resetting to the traits'
        // initial value (`:212-218`).
        let valid_data = {
            let mut cd = curve.curve_data().borrow_mut();
            let reuse = cd.is_valid() && cd.data().len() == nodes;
            cd.set_pillars(dates, times);
            if !reuse {
                cd.reset_data(C::Traits::initial_value(), nodes);
            }
            cd.set_max_date(max_date);
            reuse
        };

        // Hand the curve to each alive helper and reject invalid quotes.
        let term_structure = curve.term_structure_shared()?;
        for helper in helpers.iter().take(n).skip(first_alive) {
            helper.base().quote_value()?;
            helper.set_term_structure(&term_structure);
        }

        let accuracy = self.accuracy.unwrap_or_else(|| curve.accuracy());

        for (i, j) in (1..).zip(first_alive..n) {
            let (min, max, guess) = {
                let cd = curve.curve_data().borrow();
                let min = C::Traits::min_value_after(i, cd.times(), cd.data(), valid_data);
                let max = C::Traits::max_value_after(i, cd.times(), cd.data(), valid_data);
                let mut guess = C::Traits::guess(i, cd.times(), cd.data(), valid_data);
                // Nudge a guess that sits on or past a bracket end back inside
                // it (`:290-293`).
                if guess >= max {
                    guess = max - (max - min) / 5.0;
                } else if guess <= min {
                    guess = min + (max - min) / 5.0;
                }
                (min, max, guess)
            };

            let helper = &helpers[j];
            let error_slot: RefCell<Option<QlError>> = RefCell::new(None);
            let error = |g: Real| -> Real {
                match node_error::<C>(curve, i, helper, g) {
                    Ok(value) => value,
                    Err(err) => {
                        *error_slot.borrow_mut() = Some(err);
                        Real::NAN
                    }
                }
            };

            let solved = if valid_data {
                FiniteDifferenceNewtonSafe::new().solve_bracketed(error, accuracy, guess, min, max)
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

            // Pin the solved value and rebuild the prefix so the final curve
            // holds the root exactly, not the solver's last trial point.
            let mut cd = curve.curve_data().borrow_mut();
            C::Traits::update_guess(cd.data_mut(), root, i);
            cd.rebuild(curve.interpolator(), i)?;
        }

        curve.curve_data().borrow_mut().set_valid(true);
        Ok(())
    }
}

/// Writes a trial value into node `i`, rebuilds the solved prefix, and returns
/// the helper's quote error. The mutable borrow of the node storage is dropped
/// before the helper reprices, so the helper can read the same curve back
/// without a `RefCell` conflict.
fn node_error<C: PiecewiseCurve>(
    curve: &C,
    i: Size,
    helper: &Shared<dyn RateHelper>,
    guess: Real,
) -> QlResult<Real> {
    {
        let mut cd = curve.curve_data().borrow_mut();
        C::Traits::update_guess(cd.data_mut(), guess, i);
        cd.rebuild(curve.interpolator(), i)?;
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
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::yields::DepositRateHelper;
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
    }

    impl StubCurve {
        fn new(reference: Date, instruments: Vec<Shared<dyn RateHelper>>) -> Shared<StubCurve> {
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
