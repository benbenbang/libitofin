//! The Jamshidian swaption engine.
//!
//! Port of `ql/pricingengines/swaption/jamshidianswaptionengine.{hpp,cpp}`:
//! prices a European swaption under the one-factor [`HullWhite`] model via the
//! Jamshidian decomposition. It finds the critical short rate `rStar` at which
//! the underlying coupon bond equals its strike, then values the swaption as a
//! portfolio of options on the individual fixed-leg zero-coupon bonds, each
//! priced by the model's `discountBondOption`
//! (`jamshidianswaptionengine.cpp:57-128`).
//!
//! ## Model binding (documented deferral)
//!
//! C++'s engine is `GenericModelEngine<OneFactorAffineModel, ...>`
//! (`jamshidianswaptionengine.hpp:44`), generic over any one-factor affine
//! model. The port binds concretely to [`HullWhite`]: `discountBondOption` lives
//! on no trait on main - [`OneFactorAffineModel`] carries only `a`/`b`/
//! `discount_bond` and its base `AffineModel` only `discount_bond_factors` (both
//! left that way for the #377 Vasicek/CIR slice, whose infallible `Real` return
//! would also collide with this engine's [`QlResult`] path). Hull-White (#391) is
//! the only model that provides it, so the engine takes a
//! [`SharedMut<HullWhite>`] (the shape [`HullWhite::new`] returns). A generic
//! Jamshidian over every affine model waits for a `DiscountBondOption` trait, a
//! later ticket.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! - **Calibration**: `CalibratedModel::calibrate`, the `CalibrationHelper`/
//!   `SwaptionHelper` family and the `testCachedHullWhite*` oracles they drive
//!   (`shortratemodels.cpp`) are the fit-loop path deferred with the short-rate
//!   dynamics (#377); this engine prices at fixed `a`/`sigma`.
//! - **The non-term-structure-consistent-model fallback**: C++ carries an
//!   optional engine-level `termStructure_` for a model that cannot supply its
//!   own curve (`jamshidianswaptionengine.cpp:82-85`, the `else` branch).
//!   Hull-White is always term-structure consistent, so only the `tsmodel`
//!   branch is live; the fallback is not ported.
//! - **The model-present guard** (`jamshidianswaptionengine.cpp:72`): the ctor
//!   takes the model by value, so an absent model is structurally impossible and
//!   the guard has no Rust counterpart.
//! - **Bermudan / non-European exercise and `ParYieldCurve` cash settlement**:
//!   the engine [`QlResult`]-rejects them, which is the C++ behaviour
//!   (`QL_REQUIRE`), not a stub.
//!
//! ## Divergences from QuantLib
//!
//! - **The zero-spread guard reads the per-coupon spreads.** C++ checks
//!   `arguments_.swap->spread() == 0` (`jamshidianswaptionengine.cpp:67`), a
//!   scalar off the swap handle; the Rust [`SwaptionArguments`] carries neither a
//!   swap handle nor a scalar spread, so the port rejects the first non-zero
//!   entry of `swap_arguments.floating_spreads` (`fixedvsfloatingswap.rs:110`)
//!   and reports it in the same `"non zero spread (<s>) not allowed"` message.
//! - **The constant-nominal guard reads the `Option`.** C++ checks
//!   `arguments_.nominal != Null<Real>` (`:69`); the Rust arguments expose the
//!   constant nominal as `swap_arguments.nominal: Option<Real>` (`Some` only when
//!   the swap's nominals are constant, `fixedvsfloatingswap.cpp` population), so
//!   the port rejects `None` with the same message.
//! - **Observation.** C++ `registerWith(model_)` (in the `GenericModelEngine`
//!   base) so a model change invalidates. The port registers the engine as an
//!   observer of the model's observable through the
//!   [`CalibratedModelHolder`](crate::models::model::CalibratedModelHolder) seam;
//!   since Hull-White already observes its own curve handle, this gives the
//!   curve -> model -> engine -> swaption invalidation chain (the same idiom the
//!   Black engine follows for its vol/curve handles). The engine-level
//!   `termStructure_` registration has no live counterpart (see the deferral).

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instrument::InstrumentResults;
use crate::instruments::{SettlementMethod, SwapType, SwaptionArguments, SwaptionEngine};
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::models::shortrate::onefactormodel::OneFactorAffineModel;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::shared::SharedMut;
use crate::types::{Real, Time};
use crate::{fail, require};

/// Swaption engine using Jamshidian's decomposition under [`HullWhite`]
/// (`jamshidianswaptionengine.hpp:44`).
pub struct JamshidianSwaptionEngine {
    base: SwaptionEngine,
    model: SharedMut<HullWhite>,
}

impl JamshidianSwaptionEngine {
    /// Builds the engine over a Hull-White `model`
    /// (`jamshidianswaptionengine.hpp:52`). The engine observes the model's
    /// observable, so a model change invalidates a swaption priced by it.
    pub fn new(model: SharedMut<HullWhite>) -> JamshidianSwaptionEngine {
        let base = SwaptionEngine::new(SwaptionArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        JamshidianSwaptionEngine { base, model }
    }
}

impl AsObservable for JamshidianSwaptionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for JamshidianSwaptionEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        // Read and guard the arguments, then release the borrow before pricing.
        let (exercise_date, value_time_date, swap_type, nominal, fixed_coupons, fixed_pay_dates) = {
            let arguments = self.base.arguments();
            let swap = &arguments.swap_arguments;

            require!(
                arguments.settlement_method != SettlementMethod::ParYieldCurve,
                "cash settled (ParYieldCurve) swaptions not priced with JamshidianSwaptionEngine"
            );

            let Some(exercise) = arguments.exercise.as_ref() else {
                fail!("exercise not set");
            };
            require!(
                exercise.exercise_type() == ExerciseType::European,
                "cannot use the Jamshidian decomposition on exotic swaptions"
            );

            if let Some(spread) = swap.floating_spreads.iter().copied().find(|&s| s != 0.0) {
                fail!("non zero spread ({spread}) not allowed");
            }

            let Some(nominal) = swap.nominal else {
                fail!("non-constant nominals are not supported yet");
            };
            let Some(swap_type) = swap.swap_type else {
                fail!("swap type not set");
            };
            let Some(&value_time_date) = swap.fixed_reset_dates.first() else {
                fail!("swap has no fixed coupons");
            };

            (
                exercise.dates()[0],
                value_time_date,
                swap_type,
                nominal,
                swap.fixed_coupons.clone(),
                swap.fixed_pay_dates.clone(),
            )
        };

        let model = self.model.borrow();
        let (reference_date, day_counter) = {
            let curve = model.term_structure().current_link()?;
            (curve.reference_date()?, curve.require_day_counter()?)
        };

        let mut amounts = fixed_coupons;
        let Some(last) = amounts.last_mut() else {
            fail!("swap has no fixed coupons");
        };
        *last += nominal;

        let maturity = day_counter.year_fraction(reference_date, exercise_date);
        let value_time = day_counter.year_fraction(reference_date, value_time_date);
        let fixed_pay_times: Vec<Time> = fixed_pay_dates
            .iter()
            .map(|&date| day_counter.year_fraction(reference_date, date))
            .collect();

        // rStarFinder: value(x) = nominal - sum_i amounts[i] * P(maturity, payTime_i, x) / B(x),
        // with B(x) = P(maturity, valueTime, x), solved for the critical rate rStar
        // (`jamshidianswaptionengine.cpp:27-55`, `:101-107`).
        let rstar = {
            let finder = |x: Real| -> Real {
                let b = model.discount_bond(maturity, value_time, x);
                let mut value = nominal;
                for (amount, &pay_time) in amounts.iter().zip(fixed_pay_times.iter()) {
                    value -= amount * model.discount_bond(maturity, pay_time, x) / b;
                }
                value
            };
            let mut solver = Brent::new()
                .with_max_evaluations(10_000)
                .with_lower_bound(-10.0)
                .with_upper_bound(10.0);
            solver.solve_bracketed(finder, 1.0e-8, 0.05, -10.0, 10.0)?
        };

        // Decompose: a payer swaption is a put on the coupon bond, a receiver a
        // call; each strike is B-normalised at the solved rStar and the option
        // value is scaled by the coupon amount (`:109-127`).
        let w = if swap_type == SwapType::Payer {
            OptionType::Put
        } else {
            OptionType::Call
        };
        let b = model.discount_bond(maturity, value_time, rstar);
        let mut value = 0.0;
        for (amount, &pay_time) in amounts.iter().zip(fixed_pay_times.iter()) {
            let strike = model.discount_bond(maturity, pay_time, rstar) / b;
            let dbo =
                model.discount_bond_option_with_start(w, strike, maturity, value_time, pay_time)?;
            value += amount * dbo;
        }
        drop(model);

        self.base.results_mut().value = Some(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    use crate::exercise::{EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::instrument::Instrument;
    use crate::instruments::{
        FixedVsFloatingSwap, SettlementType, Swaption, SwaptionArguments, VanillaSwap,
    };
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::shared::{Shared, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::{MakeSchedule, Schedule};

    /// The shared fixture: evaluation and curve reference date 15 Jan 2026, a
    /// flat 3% continuously-compounded `Actual365Fixed` curve, and a Hull-White
    /// model at `a = 0.05`, `sigma = 0.01`. Reproduced verbatim in the C++
    /// generator (scratchpad `jamgen.cpp`).
    const A: Real = 0.05;
    const SIGMA: Real = 0.01;
    const NOMINAL: Real = 100.0;
    const FIXED_RATE: Real = 0.03;

    fn settings() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        settings
    }

    fn flat_curve() -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            Date::new(15, Month::January, 2026),
            0.03,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn hw_model() -> SharedMut<HullWhite> {
        HullWhite::new(flat_curve(), A, SIGMA).unwrap()
    }

    fn schedule(from: Date, to: Date, frequency: Frequency) -> Schedule {
        MakeSchedule::new()
            .from(from)
            .to(to)
            .with_frequency(frequency)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(false)
            .build()
    }

    /// A five-year annual fixed / semiannual Euribor6M swap starting 15 Jan 2028,
    /// the underlying of the fixture swaption. The floating leg is priced by no
    /// one (the Jamshidian engine reads only the fixed leg, the nominal and the
    /// exercise), so its only constraint is a zero spread.
    fn fixture_swap(
        settings: &Shared<Settings<Date>>,
        swap_type: SwapType,
        spread: Real,
    ) -> FixedVsFloatingSwap {
        let index: Shared<IborIndex> =
            shared(Euribor::six_months(flat_curve(), Shared::clone(settings)));
        VanillaSwap::new(
            swap_type,
            NOMINAL,
            schedule(
                Date::new(15, Month::January, 2028),
                Date::new(15, Month::January, 2033),
                Frequency::Annual,
            ),
            FIXED_RATE,
            Thirty360::with_convention(Convention::BondBasis),
            schedule(
                Date::new(15, Month::January, 2028),
                Date::new(15, Month::January, 2033),
                Frequency::Semiannual,
            ),
            index,
            spread,
            Actual360::new(),
            None,
            Shared::clone(settings),
        )
        .unwrap()
        .into_fixed_vs_floating()
    }

    /// The fixture swaption: exercise 15 Jan 2027 (a one-year start delay before
    /// the 15 Jan 2028 swap start, so the 5-arg `discountBondOption` with a
    /// distinct `bondStart` is exercised), physically settled.
    fn fixture_swaption(swap_type: SwapType, spread: Real) -> Swaption {
        let settings = settings();
        let model = hw_model();
        let swap = shared_mut(fixture_swap(&settings, swap_type, spread));
        let mut swaption = Swaption::new(
            swap,
            shared(EuropeanExercise::new(Date::new(15, Month::January, 2027)))
                as Shared<dyn Exercise>,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&settings),
        );
        let engine =
            shared_mut(JamshidianSwaptionEngine::new(model)) as SharedMut<dyn PricingEngine>;
        swaption.base_mut().set_pricing_engine(engine);
        swaption
    }

    /// A non-European exercise, standing in for the (unported) American/Bermudan
    /// exercises so the European-only guard can be pinned.
    struct StubExercise {
        exercise_type: ExerciseType,
        dates: Vec<Date>,
    }

    impl Exercise for StubExercise {
        fn exercise_type(&self) -> ExerciseType {
            self.exercise_type
        }
        fn dates(&self) -> &[Date] {
            &self.dates
        }
    }

    fn set_args(engine: &mut JamshidianSwaptionEngine, f: impl FnOnce(&mut SwaptionArguments)) {
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<SwaptionArguments>()
            .expect("engine carries SwaptionArguments");
        f(args);
    }

    /// The `ParYieldCurve` cash-settlement guard
    /// (`jamshidianswaptionengine.cpp:59`) fires first, before any other read.
    #[test]
    fn rejects_par_yield_cash_settlement() {
        let mut engine = JamshidianSwaptionEngine::new(hw_model());
        set_args(&mut engine, |args| {
            args.settlement_method = SettlementMethod::ParYieldCurve;
        });
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "cash settled (ParYieldCurve) swaptions not priced with JamshidianSwaptionEngine"
        );
    }

    /// The European-only guard (`:63`) rejects an American exercise.
    #[test]
    fn rejects_non_european_exercise() {
        let mut engine = JamshidianSwaptionEngine::new(hw_model());
        set_args(&mut engine, |args| {
            args.exercise = Some(shared(StubExercise {
                exercise_type: ExerciseType::American,
                dates: vec![Date::new(15, Month::January, 2027)],
            }) as Shared<dyn Exercise>);
        });
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "cannot use the Jamshidian decomposition on exotic swaptions"
        );
    }

    /// The zero-spread guard (`:67`) reports the offending per-coupon spread, the
    /// Rust mapping of C++'s scalar `swap->spread()`.
    #[test]
    fn rejects_non_zero_spread() {
        let mut engine = JamshidianSwaptionEngine::new(hw_model());
        set_args(&mut engine, |args| {
            args.exercise = Some(
                shared(EuropeanExercise::new(Date::new(15, Month::January, 2027)))
                    as Shared<dyn Exercise>,
            );
            args.swap_arguments.floating_spreads = vec![0.0, 0.001];
        });
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "non zero spread (0.001) not allowed"
        );
    }

    /// The constant-nominal guard (`:69`) rejects an absent (non-constant)
    /// nominal, the Rust mapping of C++'s `nominal != Null<Real>`.
    #[test]
    fn rejects_non_constant_nominal() {
        let mut engine = JamshidianSwaptionEngine::new(hw_model());
        set_args(&mut engine, |args| {
            args.exercise = Some(
                shared(EuropeanExercise::new(Date::new(15, Month::January, 2027)))
                    as Shared<dyn Exercise>,
            );
            args.swap_arguments.nominal = None;
        });
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "non-constant nominals are not supported yet"
        );
    }

    /// A valid payer swaption prices to a finite, strictly positive NPV: the
    /// rStar solve converges and the decomposition runs end to end. The exact
    /// value is pinned against C++ in the cached-NPV oracle.
    #[test]
    fn prices_a_valid_swaption_to_a_finite_npv() {
        let mut swaption = fixture_swaption(SwapType::Payer, 0.0);
        let npv = swaption.npv().unwrap();
        assert!(npv.is_finite(), "npv not finite: {npv}");
        assert!(
            npv > 0.0,
            "a payer swaption on this fixture should be positive: {npv}"
        );
    }
}
