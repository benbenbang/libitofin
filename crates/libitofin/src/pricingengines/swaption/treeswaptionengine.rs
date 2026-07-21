//! Numerical lattice engine for swaptions.
//!
//! Port of `ql/pricingengines/swaption/treeswaptionengine.{hpp,cpp}` together
//! with the folded-in `ql/pricingengines/latticeshortratemodelengine.hpp`.
//! [`TreeSwaptionEngine`] prices a [`Swaption`](crate::instruments::Swaption)
//! under [`HullWhite`] on a short-rate tree: it builds a [`DiscretizedSwaption`],
//! fits a [`TimeGrid`] over its mandatory times, grows the model tree on that
//! grid, then rolls the swaption back to the first exercise and reads its present
//! value (`treeswaptionengine.cpp:51`).
//!
//! # Model binding (documented deferral)
//! C++'s engine is `LatticeShortRateModelEngine<Swaption::arguments,
//! Swaption::results>`, generic over any `ShortRateModel`. As the
//! [`JamshidianSwaptionEngine`](super::JamshidianSwaptionEngine) precedent does,
//! the port binds concretely to [`HullWhite`] - the only ported model that grows
//! a tree - so `model.tree(grid)` is a direct call, not a virtual dispatch.
//!
//! # `LatticeShortRateModelEngine` folded in, not built
//! The generic C++ base stores `time_steps` / `time_grid` / `lattice` and has an
//! `update()` that rebuilds the lattice from a fixed grid. With a single concrete
//! consumer, no generic base is built: `time_steps` is a field here and the tree
//! is grown per `calculate()`. The fixed-`TimeGrid`-at-construction variant (and
//! its `update()` rebuild), the `Handle<ShortRateModel>` ctor and the engine-level
//! `termStructure_` fallback for a non-term-structure-consistent model
//! (`treeswaptionengine.cpp:63-70`, the `else` branch) are DEFERRED: Hull-White is
//! always term-structure consistent, so only the `tsmodel` branch is live.
//!
//! # Divergences from QuantLib
//! - **Settings on the ctor (D5).** [`DiscretizedSwap`](super::DiscretizedSwap)
//!   reads `includeTodaysCashFlows` from what C++ takes off the `Settings`
//!   singleton. With no singleton the engine holds a [`Settings`] handle and
//!   threads it into the [`DiscretizedSwaption`], exactly as #464 threads it.
//! - **`Result`, not UB, on a degenerate exercise.** C++ dereferences
//!   `stoppingTimes.back()` and `find_if(>= 0)` unchecked; the port returns
//!   [`QlResult`] errors for an empty or all-negative exercise schedule.

use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::instrument::InstrumentResults;
use crate::instruments::{SettlementMethod, SwaptionArguments, SwaptionEngine};
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::lattice::Lattice;
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::time::date::Date;
use crate::types::{Size, Time};
use crate::{fail, require};

use super::DiscretizedSwaption;

/// Numerical lattice engine for swaptions under [`HullWhite`]
/// (`treeswaptionengine.hpp:44`).
pub struct TreeSwaptionEngine {
    base: SwaptionEngine,
    model: SharedMut<HullWhite>,
    time_steps: Size,
    settings: Shared<Settings<Date>>,
}

impl TreeSwaptionEngine {
    /// Builds the engine over a Hull-White `model` with a fixed step count
    /// (`treeswaptionengine.cpp:26` + `latticeshortratemodelengine.hpp:56`). The
    /// engine observes the model, so a model change invalidates a swaption priced
    /// by it.
    ///
    /// # Errors
    /// `time_steps` must be positive (`latticeshortratemodelengine.hpp:60`).
    pub fn new(
        model: SharedMut<HullWhite>,
        time_steps: Size,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<TreeSwaptionEngine> {
        require!(
            time_steps > 0,
            "timeSteps must be positive, {time_steps} not allowed"
        );
        let base = SwaptionEngine::new(SwaptionArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(TreeSwaptionEngine {
            base,
            model,
            time_steps,
            settings,
        })
    }
}

impl AsObservable for TreeSwaptionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for TreeSwaptionEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `calculate()` (`treeswaptionengine.cpp:51`): guard the settlement method,
    /// take the reference date / day counter from the model's term structure,
    /// build the [`DiscretizedSwaption`], grow the tree over its mandatory times,
    /// then roll back to the first exercise and read the present value.
    fn calculate(&mut self) -> QlResult<()> {
        require!(
            self.base.arguments().settlement_method != SettlementMethod::ParYieldCurve,
            "cash settled (ParYieldCurve) swaptions not priced with TreeSwaptionEngine"
        );

        let model = self.model.borrow();
        let (reference_date, day_counter) = {
            let curve = model.term_structure().current_link()?;
            (curve.reference_date()?, curve.require_day_counter()?)
        };

        let (mut swaption, stopping_times) = {
            let args = self.base.arguments();
            let Some(exercise) = args.exercise.as_ref() else {
                fail!("exercise not set");
            };
            let stopping_times: Vec<Time> = exercise
                .dates()
                .iter()
                .map(|&date| day_counter.year_fraction(reference_date, date))
                .collect();
            let swaption =
                DiscretizedSwaption::new(args, reference_date, &day_counter, &self.settings)?;
            (swaption, stopping_times)
        };

        let times = swaption.mandatory_times();
        let grid = TimeGrid::with_mandatory_times(&times, self.time_steps)?;
        let lattice: Shared<dyn Lattice> = shared(model.tree(grid)?);
        drop(model);

        let Some(&last) = stopping_times.last() else {
            fail!("swaption has no exercise dates");
        };
        swaption.initialize(Shared::clone(&lattice), last)?;

        let Some(next_exercise) = stopping_times.iter().copied().find(|&t| t >= 0.0) else {
            fail!("swaption has no non-negative exercise time");
        };
        swaption.rollback(next_exercise)?;

        self.base.results_mut().value = Some(swaption.present_value()?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    use crate::exercise::{EuropeanExercise, Exercise, ExerciseType};
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::instrument::Instrument;
    use crate::instruments::{
        FixedVsFloatingSwap, FixedVsFloatingSwapArguments, SettlementType, SwapType, Swaption,
        VanillaSwap,
    };
    use crate::interestrate::Compounding;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::pricingengines::swaption::JamshidianSwaptionEngine;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::{MakeSchedule, Schedule};
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Rate, Real};

    // ------------------------------------------------------------------
    // Convergence fixture: reuse the on-main JamshidianSwaptionEngine oracle
    // fixture (jamshidianswaptionengine.rs), whose payer/receiver NPVs are
    // already pinned against C++ to 1e-8. Flat 3% curve, HW(0.05, 0.01), a
    // 5Y annual/semiannual swap starting 2028, European exercise 2027.
    // ------------------------------------------------------------------

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

    fn conv_swap(settings: &Shared<Settings<Date>>, swap_type: SwapType) -> FixedVsFloatingSwap {
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
            0.0,
            Actual360::new(),
            None,
            Shared::clone(settings),
        )
        .unwrap()
        .into_fixed_vs_floating()
    }

    fn european(date: Date) -> Shared<dyn Exercise> {
        shared(EuropeanExercise::new(date)) as Shared<dyn Exercise>
    }

    /// A multi-date Bermudan/American exercise, standing in for the unported
    /// `BermudanExercise`, so the multi-exercise loop and optionality can be
    /// pinned (mirrors the Jamshidian test's `StubExercise`).
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

    /// Prices the fixture swaption with the on-main Jamshidian engine (the
    /// trusted European reference).
    fn jamshidian_npv(swap_type: SwapType) -> Real {
        let settings = settings();
        let swap = shared_mut(conv_swap(&settings, swap_type));
        let mut swaption = Swaption::new(
            swap,
            european(Date::new(15, Month::January, 2027)),
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&settings),
        );
        let engine =
            shared_mut(JamshidianSwaptionEngine::new(hw_model())) as SharedMut<dyn PricingEngine>;
        swaption.base_mut().set_pricing_engine(engine);
        swaption.npv().unwrap()
    }

    /// Prices the fixture swaption with the tree engine at `steps`, on the given
    /// exercise schedule.
    fn tree_npv(swap_type: SwapType, steps: Size, exercise: Shared<dyn Exercise>) -> Real {
        let settings = settings();
        let swap = shared_mut(conv_swap(&settings, swap_type));
        let mut swaption = Swaption::new(
            swap,
            exercise,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&settings),
        );
        let engine = shared_mut(TreeSwaptionEngine::new(hw_model(), steps, settings).unwrap())
            as SharedMut<dyn PricingEngine>;
        swaption.base_mut().set_pricing_engine(engine);
        swaption.npv().unwrap()
    }

    /// PRIMARY / HARD GATE: a European payer swaption priced on the Hull-White
    /// tree converges to the on-main Jamshidian price as the step count grows.
    ///
    /// The exercise-payoff kink makes trinomial convergence non-monotone: moderate
    /// counts sit on a ~2.5e-3 hump (grid alignment of the exercise/coupon nodes)
    /// and the error only settles below 1e-3 in the tail. So the gate spans the
    /// hump - a coarse count of 50 against a fine count of 1100 - and asserts the
    /// fine error is inside 1e-3 AND well below the coarse error (a ~10x drop). The
    /// tail is monotone toward the reference (n=200..1600: rel 2.5e-3 -> 1.3e-4),
    /// which is the convergence proof; a real bias would plateau instead.
    ///
    /// The receiver arm (the `OptionType::Call` branch) is not re-gated here - it
    /// is anchored twice already: #464 pins the receiver swap NPV on the tree and
    /// that it negates the payer, and the Jamshidian oracle pins the receiver arm
    /// to C++. A single cheap moderate-count price guards a receiver-specific
    /// wiring slip.
    #[test]
    fn tree_converges_to_jamshidian_european() {
        let reference = jamshidian_npv(SwapType::Payer);
        let exercise = || european(Date::new(15, Month::January, 2027));
        let e_coarse = (tree_npv(SwapType::Payer, 50, exercise()) - reference).abs() / reference;
        let e_fine = (tree_npv(SwapType::Payer, 1100, exercise()) - reference).abs() / reference;
        assert!(
            e_fine < 1.0e-3,
            "payer: tree(1100) rel err {e_fine} vs jamshidian {reference}"
        );
        assert!(
            e_fine < e_coarse,
            "payer: error must shrink 50->1100: coarse {e_coarse} fine {e_fine}"
        );

        // Receiver insurance: the OptionType::Call branch prices near its own
        // Jamshidian reference at a moderate (plateau) count.
        let reference_r = jamshidian_npv(SwapType::Receiver);
        let e_r = (tree_npv(SwapType::Receiver, 300, exercise()) - reference_r).abs() / reference_r;
        assert!(
            e_r < 5.0e-3,
            "receiver: tree(300) rel err {e_r} vs jamshidian {reference_r}"
        );
    }

    /// COMMITTED INVARIANT: a Bermudan swaption (three exercise opportunities)
    /// is worth at least its European counterpart (a single exercise at the same
    /// first date) on the same underlying - more optionality cannot cost value.
    /// Also exercises the multi-exercise loop the cached diagnostic leans on.
    #[test]
    fn bermudan_dominates_european() {
        let euro = tree_npv(
            SwapType::Payer,
            120,
            european(Date::new(15, Month::January, 2027)),
        );
        let bermudan = tree_npv(
            SwapType::Payer,
            120,
            shared(StubExercise {
                exercise_type: ExerciseType::Bermudan,
                dates: vec![
                    Date::new(15, Month::January, 2027),
                    Date::new(15, Month::January, 2028),
                    Date::new(15, Month::January, 2029),
                ],
            }) as Shared<dyn Exercise>,
        );
        assert!(
            bermudan >= euro - 1.0e-9,
            "bermudan {bermudan} must dominate european {euro}"
        );
    }

    /// The `time_steps > 0` guard (`latticeshortratemodelengine.hpp:60`).
    #[test]
    fn rejects_non_positive_time_steps() {
        let err = TreeSwaptionEngine::new(hw_model(), 0, settings())
            .err()
            .expect("time_steps == 0 must be rejected");
        assert_eq!(err.message(), "timeSteps must be positive, 0 not allowed");
    }

    /// The `ParYieldCurve` cash-settlement guard (`treeswaptionengine.cpp:53`)
    /// fires first, before any model or argument read.
    #[test]
    fn rejects_par_yield_cash_settlement() {
        let mut engine = TreeSwaptionEngine::new(hw_model(), 50, settings()).unwrap();
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<SwaptionArguments>()
            .expect("engine carries SwaptionArguments");
        args.settlement_method = SettlementMethod::ParYieldCurve;
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "cash settled (ParYieldCurve) swaptions not priced with TreeSwaptionEngine"
        );
    }

    // ------------------------------------------------------------------
    // DIAGNOSTIC (NOT a merge gate): QuantLib's only tree-swaption test,
    // bermudanswaption.cpp:112 testCachedValues. HW(0.048696, 0.0058904),
    // a 1y-into-5y payer Bermudan on a nominal of 1000, exercisable on each
    // fixed accrual-start date, TreeSwaptionEngine(model, 50). Cached ITM/
    // ATM/OTM = 42.2402 / 12.9032 / 2.49758 (the non-par / indexed arm;
    // usingAtParCoupons = false). Reproduced to a LOOSE band only: the exact
    // 1e-4 gate needs the deferred date-snapping (which shifts the semiannual
    // floating resets that fall within a week of the annual exercise dates)
    // plus near-bit-exact tree detail. A miss here is not a bug (see the
    // discretizedswaption.rs deferral note); the deltas are printed.
    // ------------------------------------------------------------------

    const BERM_A: Real = 0.048696;
    const BERM_SIGMA: Real = 0.0058904;
    const BERM_NOMINAL: Real = 1000.0;

    fn berm_curve(settlement: Date) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            settlement,
            0.04875825,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// `CommonVars::makeSwap` (`bermudanswaption.cpp:81`): a 1y-into-5y payer
    /// swap, annual/unadjusted fixed vs semiannual/modified-following float, on a
    /// nominal of 1000, priced by a discounting engine so `fair_rate` resolves.
    fn berm_swap(
        settings: &Shared<Settings<Date>>,
        calendar: &Calendar,
        settlement: Date,
        curve: &Handle<dyn YieldTermStructure>,
        fixed_rate: Rate,
    ) -> SharedMut<FixedVsFloatingSwap> {
        let start = calendar.advance(
            settlement,
            1,
            TimeUnit::Years,
            BusinessDayConvention::Following,
            false,
        );
        let maturity = calendar.advance(
            start,
            5,
            TimeUnit::Years,
            BusinessDayConvention::Following,
            false,
        );
        let fixed_schedule = MakeSchedule::new()
            .from(start)
            .to(maturity)
            .with_frequency(Frequency::Annual)
            .with_calendar(calendar.clone())
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(false)
            .build();
        let float_schedule = MakeSchedule::new()
            .from(start)
            .to(maturity)
            .with_frequency(Frequency::Semiannual)
            .with_calendar(calendar.clone())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .end_of_month(false)
            .build();
        let index: Shared<IborIndex> =
            shared(Euribor::six_months(curve.clone(), Shared::clone(settings)));
        let swap = shared_mut(
            VanillaSwap::new(
                SwapType::Payer,
                BERM_NOMINAL,
                fixed_schedule,
                fixed_rate,
                Thirty360::with_convention(Convention::BondBasis),
                float_schedule,
                index,
                0.0,
                Actual360::new(),
                None,
                Shared::clone(settings),
            )
            .unwrap()
            .into_fixed_vs_floating(),
        );
        let engine = shared_mut(DiscountingSwapEngine::new(
            curve.clone(),
            None,
            None,
            None,
            Shared::clone(settings),
        )) as SharedMut<dyn PricingEngine>;
        swap.borrow_mut().base_mut().set_pricing_engine(engine);
        swap
    }

    fn berm_swaption_npv(
        swap: SharedMut<FixedVsFloatingSwap>,
        exercise_dates: Vec<Date>,
        model: SharedMut<HullWhite>,
        settings: &Shared<Settings<Date>>,
    ) -> Real {
        let mut swaption = Swaption::new(
            swap,
            shared(StubExercise {
                exercise_type: ExerciseType::Bermudan,
                dates: exercise_dates,
            }) as Shared<dyn Exercise>,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(settings),
        );
        let engine =
            shared_mut(TreeSwaptionEngine::new(model, 50, Shared::clone(settings)).unwrap())
                as SharedMut<dyn PricingEngine>;
        swaption.base_mut().set_pricing_engine(engine);
        swaption.npv().unwrap()
    }

    #[test]
    fn cached_bermudan_diagnostic() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, Month::February, 2002));
        settings.set_using_at_par_coupons(false);
        let calendar = Target::new();
        let settlement = calendar.advance(
            Date::new(15, Month::February, 2002),
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let curve = berm_curve(settlement);

        let atm_rate = berm_swap(&settings, &calendar, settlement, &curve, 0.0)
            .borrow_mut()
            .fair_rate()
            .unwrap();

        // Exercise on each fixed-coupon accrual-start date (bermudanswaption.cpp:141).
        let atm_swap = berm_swap(&settings, &calendar, settlement, &curve, atm_rate);
        let mut atm_args = FixedVsFloatingSwapArguments::default();
        atm_swap.borrow().setup_arguments(&mut atm_args).unwrap();
        let exercise_dates = atm_args.fixed_reset_dates.clone();
        assert_eq!(exercise_dates.len(), 5, "five annual fixed accrual starts");

        let cases = [
            ("ITM", 0.8 * atm_rate, 42.2402_f64),
            ("ATM", atm_rate, 12.9032),
            ("OTM", 1.2 * atm_rate, 2.49758),
        ];
        for (label, rate, cached) in cases {
            let swap = berm_swap(&settings, &calendar, settlement, &curve, rate);
            let model = HullWhite::new(curve.clone(), BERM_A, BERM_SIGMA).unwrap();
            let npv = berm_swaption_npv(swap, exercise_dates.clone(), model, &settings);
            let rel = (npv - cached).abs() / cached;
            eprintln!(
                "[cached-bermudan diagnostic] {label}: tree {npv:.6} vs cached {cached} (rel {rel:.3e})"
            );
            assert!(
                rel < 1.0e-2,
                "{label}: tree {npv} vs cached {cached} (rel {rel}) outside the loose diagnostic band"
            );
        }
    }
}
