//! Piecewise-bootstrapped yield term structure.
//!
//! Port of `ql/termstructures/yield/piecewiseyieldcurve.hpp`. A
//! [`PiecewiseYieldCurve`] is built from a set of rate helpers whose maturities
//! mark the segment boundaries; each node is solved so the helper reprices its
//! quote off the curve (see [`iterativebootstrap`](crate::termstructures::iterativebootstrap)).
//!
//! ## Laziness (bootstrap in `perform_calculations`, not the constructor)
//!
//! The curve embeds a [`LazyObject`], exactly as C++ inherits one
//! (`piecewiseyieldcurve.hpp:63`). Construction is cheap: it lays out no nodes
//! and runs no solver. The first read that needs the curve
//! ([`discount`](YieldTermStructure::discount) or [`max_date`](TermStructure::max_date))
//! calls [`calculate`](PiecewiseYieldCurve::calculate), which runs the bootstrap
//! once and caches it. A helper-quote or evaluation-date change notifies the
//! curve, invalidates the cache, and the next read re-bootstraps. Bootstrapping
//! in the constructor would break that observability contract, so it is done in
//! `perform_calculations` (here, [`calculate`](Self::calculate)'s closure).
//!
//! The `LazyObject`'s pre-set `calculated` flag is what breaks bootstrap
//! recursion: while the bootstrap runs, a helper reads the curve's discount,
//! which re-enters `calculate`; the flag is already set, so the re-entrant call
//! returns immediately and reads the partially built curve, mirroring the C++
//! `calculated_ = true` guard.
//!
//! ## Scope and deferrals
//!
//! - Generic over the interpolator; the traits are a type parameter. The
//!   `Discount` (`LogLinear`/`Linear`), `ZeroYield` (`Linear`) and `ForwardRate`
//!   (`Linear`/`BackwardFlat`) conventions are wired; the spline interpolators
//!   are deferred (they need the global convergence loop, unported).
//! - `MultiCurveBootstrapProvider` (`ql/termstructures/multicurve.hpp:36`), a
//!   marker base used only for a `dynamic_pointer_cast`, is dropped.
//! - Jump quotes (`jumps`/`jumpDates`) are not ported, following the
//!   [`YieldTermStructure`] precedent.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::Interpolator;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraphelper::RateHelper;
use crate::termstructures::bootstraptraits::{CurveData, YieldBootstrapTraits};
use crate::termstructures::iterativebootstrap::{Bootstrap, IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{DiscountFactor, Real, Time};

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's
/// own observers (the port of `registerWithObservables` + `LazyObject::update`).
struct CurveUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for CurveUpdater {
    fn update(&mut self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }
}

/// Yield term structure bootstrapped from rate helpers.
///
/// `T` is the curve-shape traits (`Discount`, `ZeroYield`, `ForwardRate`); `I`
/// is the interpolation factory (`LogLinear`, `Linear`, `BackwardFlat`); `B`
/// is the bootstrap algorithm ([`IterativeBootstrap`] by default, or
/// [`LocalBootstrap`](crate::termstructures::localbootstrap::LocalBootstrap)),
/// mirroring the C++ `Bootstrap` template parameter. `B` is deliberately
/// unbounded here - the `where B: Bootstrap<Self>` obligation lives on the
/// impl blocks that call it, so the struct's `T`/`I` inference stays free of
/// the recursive bound. The node data lives in a `RefCell` the bootstrap
/// mutates and the discount lookup reads back.
pub struct PiecewiseYieldCurve<T: YieldBootstrapTraits, I: Interpolator, B = IterativeBootstrap> {
    base: TermStructureBase,
    instruments: Vec<Shared<dyn RateHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: B,
    accuracy: Real,
    self_weak: Weak<dyn YieldTermStructure>,
    _traits: PhantomData<fn() -> T>,
}

impl<T: YieldBootstrapTraits + 'static, I: Interpolator + 'static, B: 'static>
    PiecewiseYieldCurve<T, I, B>
where
    B: Bootstrap<Self>,
{
    /// Builds a curve over `instruments` with a fixed `reference_date` (the C++
    /// reference-date constructor) and a default-configured bootstrap.
    /// Construction is cheap; the bootstrap runs on first use.
    pub fn new(
        reference_date: Date,
        instruments: Vec<Shared<dyn RateHelper>>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Shared<PiecewiseYieldCurve<T, I, B>>>
    where
        B: Default,
    {
        Self::with_bootstrap(
            reference_date,
            instruments,
            day_counter,
            interpolator,
            B::default(),
        )
    }

    /// Builds the curve with an explicitly configured bootstrap (the C++
    /// constructor's trailing `bootstrap` argument).
    pub fn with_bootstrap(
        reference_date: Date,
        instruments: Vec<Shared<dyn RateHelper>>,
        day_counter: DayCounter,
        interpolator: I,
        bootstrap: B,
    ) -> QlResult<Shared<PiecewiseYieldCurve<T, I, B>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseYieldCurve<T, I, B>>| {
            let self_weak: Weak<dyn YieldTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseYieldCurve {
                base: TermStructureBase::with_reference_date(
                    reference_date,
                    None,
                    Some(day_counter),
                ),
                instruments,
                interpolator,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap,
                accuracy: 1.0e-12,
                self_weak,
                _traits: PhantomData,
            }
        });

        // Register the curve as an observer of every helper, so a quote or
        // evaluation-date change invalidates the bootstrap (C++'s
        // `bootstrap_.setup(this)` -> `registerWithObservables`).
        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        // Helpers the bootstrap owns rather than fits - GlobalBootstrap's
        // additional helpers - register too (`globalbootstrap.hpp:219-220`).
        for observable in curve.bootstrap.additional_observables() {
            observable.register_observer(&observer);
        }
        Ok(curve)
    }

    /// Runs the bootstrap if the cache is stale, caching the result. The lazy
    /// core is not borrowed while the bootstrap runs, so a helper reading the
    /// curve mid-bootstrap re-enters here and returns on the pre-set flag.
    pub fn calculate(&self) -> QlResult<()> {
        if self.lazy.borrow().is_calculated() {
            return Ok(());
        }
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.bootstrap.calculate(self);
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    /// The node times, after bootstrapping.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node values (discount factors for `Discount`), after bootstrapping.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, value) nodes, after bootstrapping.
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<T: YieldBootstrapTraits, I: Interpolator, B> AsObservable for PiecewiseYieldCurve<T, I, B> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<T: YieldBootstrapTraits + 'static, I: Interpolator + 'static, B: 'static> TermStructure
    for PiecewiseYieldCurve<T, I, B>
where
    B: Bootstrap<Self>,
{
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve; a
        // bootstrap failure is surfaced by `discount`, so fall back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.base.reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<T: YieldBootstrapTraits + 'static, I: Interpolator + 'static, B: 'static> YieldTermStructure
    for PiecewiseYieldCurve<T, I, B>
where
    B: Bootstrap<Self>,
{
    /// Opts the bootstrapped curve into the downcast seam.
    ///
    /// C++ reaches these node dates through inheritance - a
    /// `PiecewiseYieldCurve<Discount, LogLinear>` *is* an
    /// `InterpolatedDiscountCurve<LogLinear>`, so the engine's
    /// `dynamic_pointer_cast` to the base succeeds
    /// (`isdacdsengine.cpp:113-116`). This port composes instead of inheriting,
    /// so the piecewise curve must be named at the downcast site in its own
    /// right; see
    /// [`isda_node_grid`](crate::pricingengines::credit::isda_node_grid).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Runs the bootstrap before the range check, so `max_date` reflects the
    /// solved curve (the C++ `discountImpl`/`maxDate` both call `calculate`).
    fn discount(&self, t: Time, extrapolate: bool) -> QlResult<DiscountFactor> {
        self.calculate()?;
        self.check_range_time(t, extrapolate)?;
        self.discount_impl(t)
    }

    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        let data = self.data.borrow();
        T::discount_from_nodes(data.interpolation()?, t)
    }
}

impl<T: YieldBootstrapTraits + 'static, I: Interpolator + 'static, B: 'static> PiecewiseCurve
    for PiecewiseYieldCurve<T, I, B>
{
    type Traits = T;
    type Interp = I;
    type TS = dyn YieldTermStructure;
    type Helper = dyn RateHelper;

    fn instruments(&self) -> &[Shared<dyn RateHelper>] {
        &self.instruments
    }

    fn interpolator(&self) -> &I {
        &self.interpolator
    }

    fn curve_data(&self) -> &RefCell<CurveData<I>> {
        &self.data
    }

    fn accuracy(&self) -> Real {
        self.accuracy
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.base.reference_date()
    }

    /// Computed off the base directly rather than through
    /// [`TermStructure::time_from_reference`]: the `TermStructure` impl
    /// carries the `B: Bootstrap<Self>` obligation (its `max_date` runs the
    /// bootstrap), and routing through it would put that recursive bound on
    /// this impl too, which every `Bootstrap<C>` implementation depends on.
    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        let Some(day_counter) = self.base.day_counter() else {
            crate::fail!("no day counter provided for this term structure");
        };
        Ok(day_counter.year_fraction(self.base.reference_date()?, date))
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn YieldTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Oracle: `piecewiseyieldcurve.cpp` `testCurveConsistency` (tolerance
    //! 1e-9), the **deposits** (`:364-378`) and **swaps** (`:379-403`) sections
    //! only. The round-trip is self-consistent: each instrument is repriced off
    //! the bootstrapped curve and must reproduce its input quote, so there are
    //! no external numbers. The bond, FRA and futures sections and the
    //! `testBMACurveConsistency` half need helpers deferred to #343 and are not
    //! ported here.

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::indexes::iborindex::IborIndex;
    use crate::indexes::index::Index;
    use crate::instruments::MakeVanillaSwap;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::math::interpolations::linear::Linear;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::pricingengines::credit::isda_node_grid;
    use crate::quotes::{FuturesConvAdjustmentQuote, Quote, SimpleQuote, make_quote_handle};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::bootstraptraits::{
        Discount, ForwardRate, SimpleZeroYield, ZeroYield,
    };
    use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::{
        DepositRateHelper, FraRateHelper, FuturesRateHelper, InterpolatedSimpleZeroCurve, Pillar,
        SwapRateHelper,
    };
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Day, Month, Year};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::imm;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Rate;

    // (n, units, rate-in-percent), transcribed from piecewiseyieldcurve.cpp.
    const DEPOSIT_DATA: [(i32, TimeUnit, Rate); 6] = [
        (1, TimeUnit::Weeks, 4.559),
        (1, TimeUnit::Months, 4.581),
        (2, TimeUnit::Months, 4.573),
        (3, TimeUnit::Months, 4.557),
        (6, TimeUnit::Months, 4.496),
        (9, TimeUnit::Months, 4.490),
    ];

    const SWAP_DATA: [(i32, TimeUnit, Rate); 15] = [
        (1, TimeUnit::Years, 4.54),
        (2, TimeUnit::Years, 4.63),
        (3, TimeUnit::Years, 4.75),
        (4, TimeUnit::Years, 4.86),
        (5, TimeUnit::Years, 4.99),
        (6, TimeUnit::Years, 5.11),
        (7, TimeUnit::Years, 5.23),
        (8, TimeUnit::Years, 5.33),
        (9, TimeUnit::Years, 5.41),
        (10, TimeUnit::Years, 5.47),
        (12, TimeUnit::Years, 5.60),
        (15, TimeUnit::Years, 5.75),
        (20, TimeUnit::Years, 5.89),
        (25, TimeUnit::Years, 5.95),
        (30, TimeUnit::Years, 5.96),
    ];

    const TOLERANCE: Real = 1.0e-9;

    struct CommonVars {
        settings: Shared<Settings<Date>>,
        today: Date,
        settlement: Date,
        instruments: Vec<Shared<dyn RateHelper>>,
    }

    fn common_vars() -> CommonVars {
        common_vars_on(Date::new(15, Month::June, 2026))
    }

    /// The same fixture at an explicit evaluation date, the port of the C++
    /// `CommonVars(Date)` constructor (`piecewiseyieldcurve.cpp:167`).
    /// `testGlobalBootstrapVariables` pins its evaluation date to make the
    /// solve reproducible.
    fn common_vars_on(evaluation_date: Date) -> CommonVars {
        let calendar = Target::new();
        let today = calendar.adjust(evaluation_date, BusinessDayConvention::Following);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let mut instruments: Vec<Shared<dyn RateHelper>> = Vec::new();
        for (n, units, rate) in DEPOSIT_DATA {
            let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
            let index = Euribor::new(Period::new(n, units), Handle::empty(), settings.clone())
                .expect("deposit tenor is valid");
            instruments.push(DepositRateHelper::new(quote, &index) as Shared<dyn RateHelper>);
        }
        for (n, units, rate) in SWAP_DATA {
            let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
            let euribor6m = Euribor::six_months(Handle::empty(), settings.clone());
            instruments.push(SwapRateHelper::new(
                quote,
                Period::new(n, units),
                calendar.clone(),
                Frequency::Annual,
                BusinessDayConvention::Unadjusted,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor6m,
            ) as Shared<dyn RateHelper>);
        }

        CommonVars {
            settings,
            today,
            settlement,
            instruments,
        }
    }

    fn euribor6m_on(
        handle: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> Shared<IborIndex> {
        shared(Euribor::six_months(handle, settings))
    }

    /// The port of `testCurveConsistency<Traits, I, IterativeBootstrap>`,
    /// deposits + swaps only. Generic over the traits so the same round-trip
    /// checks the `Discount`, `ZeroYield` and `ForwardRate` conventions; the
    /// bootstrapped curve is returned so a convention that stores rates can
    /// additionally assert on its solved nodes.
    fn check_curve_consistency<
        T: YieldBootstrapTraits + 'static,
        I: Interpolator + Default + 'static,
    >() -> Shared<PiecewiseYieldCurve<T, I>> {
        check_curve_consistency_with::<T, I, IterativeBootstrap>(TOLERANCE)
    }

    /// The fully generic `testCurveConsistency<Traits, I, Bootstrap>`: also
    /// names the bootstrap algorithm and the repricing tolerance (the C++
    /// harness takes both; `LocalBootstrap` runs at 1e-6 where the iterative
    /// arms run at 1e-9).
    fn check_curve_consistency_with<T, I, B>(
        tolerance: Real,
    ) -> Shared<PiecewiseYieldCurve<T, I, B>>
    where
        T: YieldBootstrapTraits + 'static,
        I: Interpolator + Default + 'static,
        B: Bootstrap<PiecewiseYieldCurve<T, I, B>> + Default + 'static,
    {
        let vars = common_vars();
        let curve = PiecewiseYieldCurve::<T, I, B>::new(
            vars.settlement,
            vars.instruments.clone(),
            Actual360::new(),
            I::default(),
        )
        .unwrap();
        let handle: Handle<dyn YieldTermStructure> =
            Handle::new(Shared::clone(&curve) as Shared<dyn YieldTermStructure>);

        // deposits: a fresh index on the curve handle reprices its own rate
        for (n, units, rate) in DEPOSIT_DATA {
            let index = Euribor::new(Period::new(n, units), handle.clone(), vars.settings.clone())
                .expect("deposit tenor is valid");
            let estimated = index.fixing(vars.today, false).unwrap();
            let expected = rate / 100.0;
            assert!(
                (estimated - expected).abs() <= tolerance,
                "{n} {units:?} deposit: estimated {estimated} vs expected {expected}"
            );
        }

        // swaps: a spot-starting vanilla swap on the curve handle is at par
        let euribor6m = euribor6m_on(handle.clone(), vars.settings.clone());
        for (n, units, rate) in SWAP_DATA {
            let mut swap = MakeVanillaSwap::new(
                Period::new(n, units),
                Shared::clone(&euribor6m),
                Some(0.0),
                Period::new(0, TimeUnit::Days),
                vars.settings.clone(),
            )
            .with_effective_date(vars.settlement)
            .with_discounting_term_structure(handle.clone())
            .with_fixed_leg_day_count(Thirty360::with_convention(Convention::BondBasis))
            .with_fixed_leg_tenor(Period::try_from(Frequency::Annual).unwrap())
            .with_fixed_leg_convention(BusinessDayConvention::Unadjusted)
            .with_fixed_leg_termination_date_convention(BusinessDayConvention::Unadjusted)
            .build()
            .unwrap();

            let estimated = swap.fixed_vs_floating_mut().fair_rate().unwrap();
            let expected = rate / 100.0;
            assert!(
                (estimated - expected).abs() <= tolerance,
                "{n} {units:?} swap: estimated {estimated} vs expected {expected}"
            );
        }

        curve
    }

    /// `testLogLinearDiscountConsistency` -> `<Discount, LogLinear>`
    /// (`piecewiseyieldcurve.cpp:676,683`). The `testBMACurveConsistency` half
    /// (`:684`) needs `BMASwapRateHelper` (#343) and is skipped.
    #[test]
    fn log_linear_discount_consistency() {
        check_curve_consistency::<Discount, LogLinear>();
    }

    /// `testLinearDiscountConsistency` -> `<Discount, Linear>`
    /// (`piecewiseyieldcurve.cpp:687,694`). The BMA half (`:695`) is skipped.
    #[test]
    fn linear_discount_consistency() {
        check_curve_consistency::<Discount, Linear>();
    }

    /// `testLinearZeroConsistency` -> `<ZeroYield, Linear>`
    /// (`piecewiseyieldcurve.cpp:698,705`). The BMA half (`:706`) is skipped.
    ///
    /// The consistency round-trip only prices instruments at exact solved
    /// nodes, so it cannot see the reference node: `ZeroYield::update_guess`
    /// mirrors the first solved rate into node `[0]` (the C++ `i==1 -> data[0]`
    /// write), and no repriced instrument covers the `(0, t1)` segment where
    /// that node shapes the curve. Assert it directly, or a missing mirror would
    /// leave node `[0]` at `initial_value` and still pass green.
    #[test]
    fn linear_zero_consistency() {
        let curve = check_curve_consistency::<ZeroYield, Linear>();
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference zero rate must mirror the first solved pillar"
        );
    }

    /// The `SimpleZeroYield` arm of the same harness: `<SimpleZeroYield,
    /// Linear>` under `IterativeBootstrap`. Upstream has no
    /// `testSimpleZeroConsistency` - `SimpleZeroYield` only appears in
    /// `testGlobalBootstrap` (`piecewiseyieldcurve.cpp:1486`) - so this arm is
    /// the harness applied to a fourth convention rather than a named port.
    ///
    /// What it sees: the whole `BootstrapTraits` surface the driver calls -
    /// `initial_value`, `guess`, `update_guess`, and both bracket bounds,
    /// whose lower one is floored at `-1/t + 1e-8` for every pillar past a
    /// year on this fresh pass (the floor sets the bracket edge there; the
    /// solved rates themselves never approach it). That floor is load-bearing
    /// for this arm, not decoration: stripping the `max` drops the 3Y pillar's
    /// lower bound back to `-maxRate = -1`, where `1/(1 + z*t)` has already
    /// passed its pole, and the bootstrap fails with "root not bracketed:
    /// f[-1, 1] -> [-1.0299..., -0.3081...]".
    ///
    /// What it does NOT see: `transform_direct`/`transform_inverse`, which
    /// `IterativeBootstrap` never calls, and - on its own - the SIMPLE in
    /// simply compounded. A repricing round trip only constrains the curve's
    /// discount function at the helper dates, and the bootstrap would reach
    /// the same discounts through `exp(-z*t)` by solving for different node
    /// values. The pillar assertion below is what pins the conversion: it
    /// reads the raw node back and requires the curve's own discount to be
    /// `1/(1 + z*t)` of it.
    #[test]
    fn linear_simple_zero_consistency() {
        let curve = check_curve_consistency::<SimpleZeroYield, Linear>();
        let data = curve.data().unwrap();
        let times = curve.times().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference zero rate must mirror the first solved pillar"
        );
        for k in [1, 5, times.len() - 1] {
            let discount = curve.discount(times[k], false).unwrap();
            let expected = 1.0 / (1.0 + data[k] * times[k]);
            assert!(
                (discount - expected).abs() < 1.0e-12,
                "pillar {k}: discount {discount} vs simple 1/(1 + z*t) {expected}"
            );
        }
    }

    /// Ties [`InterpolatedSimpleZeroCurve`] to the traits that name it
    /// (`bootstraptraits.hpp:317`). The piecewise curve never instantiates the
    /// module - it hands its own node holder to
    /// `SimpleZeroYield::discount_from_nodes` - so nothing else in the suite
    /// would catch the two drifting apart. Rebuilt from the bootstrapped
    /// nodes, the module must reproduce the same discount factors at a pillar,
    /// between pillars, and past the last node, where both continue the last
    /// instantaneous forward flat. Both sides are queried with extrapolation
    /// allowed: the piecewise curve's maximum date is the latest helper date
    /// and the module's is its last pillar, so a far probe is in range on
    /// neither reliably.
    #[test]
    fn the_curve_module_reproduces_the_bootstrapped_simple_zero_nodes() {
        let bootstrapped = check_curve_consistency::<SimpleZeroYield, Linear>();
        let module = InterpolatedSimpleZeroCurve::<Linear>::new(
            bootstrapped.dates().unwrap(),
            bootstrapped.data().unwrap(),
            Actual360::new(),
            None,
        )
        .unwrap();

        let times = bootstrapped.times().unwrap();
        let last = *times.last().unwrap();
        let probes = [
            times[1] * 0.5,
            times[1],
            (times[3] + times[4]) / 2.0,
            last,
            last + 5.0,
        ];
        for t in probes {
            let expected = bootstrapped.discount(t, true).unwrap();
            let discount = module.discount(t, true).unwrap();
            assert!(
                (discount - expected).abs() < 1.0e-12,
                "t {t}: module {discount} vs bootstrapped {expected}"
            );
        }
    }

    /// `testSplineZeroConsistency` -> `<ZeroYield, Cubic>`
    /// (`piecewiseyieldcurve.cpp:709,716`). The BMA half (`:721`) is skipped.
    /// The bootstrap runs the convergence loop here: `Cubic` is global, so
    /// every pillar solve moves the whole curve and the driver re-solves all
    /// nodes until the largest per-pass change is within the bootstrap
    /// accuracy.
    ///
    /// D10 divergence: C++ runs `Cubic(Spline, monotonic, SecondDerivative 0)`
    /// (`:718-720`) - a monotone-filtered natural spline - while the Rust
    /// `Cubic` interpolator is the Kruger scheme, non-monotonic
    /// (`cubic.rs:712,736`). Both are *global* cubics, so both drive the
    /// bootstrap through the convergence loop this test exercises, and the
    /// oracle is a self-repricing round-trip with no cached C++ number, so the
    /// scheme choice is fidelity-neutral here. A Spline-monotonic interpolator
    /// is deliberately not added.
    ///
    /// The node `[0]` assertion has the same rationale as
    /// [`linear_zero_consistency`]: `ZeroYield::update_guess` mirrors the
    /// first solved rate into the reference node and no repriced instrument
    /// covers `(0, t1)` to catch a missing mirror.
    #[test]
    fn spline_zero_consistency() {
        use crate::math::interpolations::cubic::Cubic;

        let curve = check_curve_consistency::<ZeroYield, Cubic>();
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference zero rate must mirror the first solved pillar"
        );
    }

    /// `testLinearForwardConsistency` -> `<ForwardRate, Linear>`
    /// (`piecewiseyieldcurve.cpp:728,735`). The BMA half (`:736`) is skipped.
    /// The node `[0]` assertion has the same rationale as
    /// [`linear_zero_consistency`]: `ForwardRate::update_guess` mirrors the
    /// first solved forward into the reference node and no repriced instrument
    /// covers `(0, t1)` to catch a missing mirror.
    #[test]
    fn linear_forward_consistency() {
        let curve = check_curve_consistency::<ForwardRate, Linear>();
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference forward must mirror the first solved pillar"
        );
    }

    /// `testFlatForwardConsistency` -> `<ForwardRate, BackwardFlat>`
    /// (`piecewiseyieldcurve.cpp:747,754`). The BMA half (`:755`) is skipped.
    #[test]
    fn flat_forward_consistency() {
        let curve = check_curve_consistency::<ForwardRate, BackwardFlat>();
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference forward must mirror the first solved pillar"
        );
    }

    /// `testConvexMonotoneForwardConsistency` -> `<ForwardRate, ConvexMonotone>`
    /// (`piecewiseyieldcurve.cpp:772,777`). The BMA half (`:779`) needs
    /// `BMASwapRateHelper` (#343) and is skipped.
    ///
    /// The first non-Cubic global interpolator through the convergence loop:
    /// `ConvexMonotone` reads the solved nodes as discrete forwards (ignoring
    /// node `[0]`), so every pillar solve re-shapes the neighbouring sections
    /// and the bootstrap re-solves to convergence. The node `[0]` assertion
    /// pins the `update_guess` mirror as in [`linear_forward_consistency`];
    /// the interpolation itself never reads that node.
    #[test]
    fn convex_monotone_forward_consistency() {
        use crate::math::interpolations::convexmonotone::ConvexMonotone;

        let curve = check_curve_consistency::<ForwardRate, ConvexMonotone>();
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference forward must mirror the first solved pillar"
        );
    }

    /// `testLocalBootstrapConsistency` ->
    /// `<ForwardRate, ConvexMonotone, LocalBootstrap>` at tolerance 1e-6
    /// (`piecewiseyieldcurve.cpp:783,788`). The BMA half (`:790-791`) needs
    /// `BMASwapRateHelper` (#343) and is skipped.
    ///
    /// The looser tolerance is the C++ harness's own: each localised
    /// least-squares window stops at the bootstrap accuracy rather than at a
    /// per-node root. Note the limits of this oracle: each grow step solves a
    /// square `localisation x localisation` system, so the curve reprices its
    /// own strip under ANY window size and the test cannot discriminate the
    /// localisation choice, the solver start point or the `EndCriteria`
    /// literals - those are transcription-verified against
    /// `localbootstrap.hpp` instead. What it does pin is the window/node
    /// bookkeeping (`initial_data_pt`, the window slice, the
    /// `DATA_SIZE_ADJUSTMENT` offset): corrupting any of those leaves an
    /// already-final node revised by a later window, and an earlier helper
    /// de-reprices past 1e-6.
    #[test]
    fn local_bootstrap_consistency() {
        use crate::math::interpolations::convexmonotone::ConvexMonotone;
        use crate::termstructures::localbootstrap::LocalBootstrap;

        check_curve_consistency_with::<ForwardRate, ConvexMonotone, LocalBootstrap>(1.0e-6);
    }

    /// The `GlobalBootstrap` arm of the consistency harness:
    /// `<Discount, LogLinear, GlobalBootstrap>` at 1e-9 (and a
    /// `<ZeroYield, Linear>` arm below). The minimal single-curve core has no
    /// direct C++ oracle - `testGlobalBootstrapPenalty` and
    /// `testGlobalBootstrap` are ported alongside the penalty terms and the
    /// additional restrictions in `globalbootstrap.rs`, and the one remaining
    /// upstream test (`piecewiseyieldcurve.cpp:1486`) exercises the
    /// still-deferred additional variables - so the round-trip stands in.
    /// Unlike [`local_bootstrap_consistency`] this oracle is NOT vacuous: the
    /// system is exactly determined (21 helper residuals over 21 interior
    /// nodes, no free window parameter), so the zero-residual root is locally
    /// unique and repricing every helper to 1e-9 pins the full simultaneous
    /// solve - the grid layout, the cost wiring, the guess mapping and the
    /// transform round trip.
    ///
    /// What it does NOT pin (the transcription-note precedent of
    /// [`local_bootstrap_consistency`]): the oracle is blind to the transform
    /// SPACE. A self-consistent wrong pair (identity for BOTH
    /// `transform_direct` and `transform_inverse` on `Discount`) converges to
    /// the same correct discount factors and still passes, so the exp/log
    /// pair is transcription-verified against `bootstraptraits.hpp:106-113`
    /// rather than oracle-verified. Confirmed by stubbing (#949), one
    /// transform at a time:
    ///
    /// - `transform_direct` alone broken (exp -> identity, inverse kept log):
    ///   the log-space guess is written into the node vector as-is, node 1
    ///   goes negative (`ln` of the 1W discount, about -0.00097), and this
    ///   arm FAILS with "log-linear interpolation requires positive y values,
    ///   got y[1] = -0.0009717..." - the oracle sees `transform_direct` on
    ///   every cost evaluation.
    /// - `transform_inverse` alone broken (log -> identity, direct kept exp):
    ///   the arm still PASSES. The inverse only seeds the start point, so the
    ///   ~0.99-discount guesses land at exp(0.99) ~ 2.69 - a bad start, not a
    ///   broken mapping - and the unconstrained LM walks back to the same
    ///   root within its 1000-iteration budget. `transform_inverse` is
    ///   therefore transcription-verified only, like the space choice.
    #[test]
    fn global_bootstrap_discount_consistency() {
        use crate::termstructures::globalbootstrap::GlobalBootstrap;

        check_curve_consistency_with::<Discount, LogLinear, GlobalBootstrap>(TOLERANCE);
    }

    /// The rate-storing `GlobalBootstrap` arm: `<ZeroYield, Linear>` at 1e-9.
    /// The `ZeroYield` transforms are identity upstream
    /// (`bootstraptraits.hpp:197-204`), so this arm pins the driver on a
    /// convention where the optimizer works directly in node space. The node
    /// `[0]` assertion has the same rationale as [`linear_zero_consistency`]:
    /// `update_guess` must mirror the first solved rate into the reference
    /// node under the global solve too.
    #[test]
    fn global_bootstrap_zero_consistency() {
        use crate::termstructures::globalbootstrap::GlobalBootstrap;

        let curve = check_curve_consistency_with::<ZeroYield, Linear, GlobalBootstrap>(TOLERANCE);
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference zero rate must mirror the first solved pillar"
        );
    }

    /// The `SimpleZeroYield` arm of the global harness: `<SimpleZeroYield,
    /// Linear, GlobalBootstrap>` at 1e-9. This is the only combination in the
    /// suite whose transforms actually depend on the node time, so it is what
    /// pins the `times[i + 1]` plumbing through all three call sites
    /// (`globalbootstrap.rs`'s cost evaluation, guess build and solution pin):
    /// an off-by-one there reaches `times[0] == 0`, the floor `-1/t` is
    /// infinite, and the solve breaks rather than converging elsewhere.
    ///
    /// It also pins that the transform pair is a working bijection on a real
    /// solve - the guess `ln(z - floor)` is well defined and the direct map
    /// lands back on admissible rates. It does NOT pin the C++ numbers: the
    /// behavioural oracle for `SimpleZeroYield` under `GlobalBootstrap` is
    /// `testGlobalBootstrap` (`piecewiseyieldcurve.cpp:1486`), which arrives
    /// with the additional restrictions in #976. Like the two arms above, the
    /// round trip is also blind to a self-consistent wrong transform SPACE;
    /// the constant is pinned in `bootstraptraits.rs`'s transform test.
    #[test]
    fn global_bootstrap_simple_zero_consistency() {
        use crate::termstructures::globalbootstrap::GlobalBootstrap;

        let curve =
            check_curve_consistency_with::<SimpleZeroYield, Linear, GlobalBootstrap>(TOLERANCE);
        let data = curve.data().unwrap();
        assert_eq!(
            data[0], data[1],
            "the reference zero rate must mirror the first solved pillar"
        );
    }

    /// The bootstrapped forward curve must be introspectable through the
    /// downcast seam (`isdacdsengine.cpp:117-120`), the arm a
    /// `PiecewiseYieldCurve<ForwardRate, BackwardFlat>` reaches in C++ by
    /// inheriting `InterpolatedForwardCurve<BackwardFlat>`. The flat credit
    /// curve contributes nothing, leaving the grid as the solved pillars alone.
    #[test]
    fn bootstrapped_forward_curve_feeds_the_isda_node_grid() {
        let curve = check_curve_consistency::<ForwardRate, BackwardFlat>();
        let pillars = curve.dates().unwrap();
        let credit = Handle::new(shared(FlatHazardRate::with_rate(
            pillars[0],
            0.01,
            Actual360::new(),
        )) as Shared<dyn DefaultProbabilityTermStructure>);

        let grid = isda_node_grid(
            &Handle::new(curve as Shared<dyn YieldTermStructure>),
            &credit,
            pillars[0] + 10_000,
        )
        .expect("a bootstrapped backward-flat forward curve is an ISDA curve");
        assert_eq!(grid, pillars);
    }

    /// Builds the mixed market strip - deposits (1W/1M/3M), a 3-month IMM
    /// future, a 9x15 FRA and swaps (2Y/3Y/5Y) - shared by the positive
    /// bootstrap test and the global-interpolator rejection test. Returns the
    /// settlement date (the curve reference) and the helper set.
    fn build_mixed_strip() -> (Date, Vec<Shared<dyn RateHelper>>) {
        use crate::instruments::FuturesType;
        use crate::termstructures::yields::{FraRateHelper, FuturesRateHelper, Pillar};
        use crate::time::imm;

        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let mut helpers: Vec<Shared<dyn RateHelper>> = Vec::new();
        for (n, units, rate) in [
            (1, TimeUnit::Weeks, 0.04559),
            (1, TimeUnit::Months, 0.04581),
            (3, TimeUnit::Months, 0.04557),
        ] {
            let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
            let index = Euribor::new(Period::new(n, units), Handle::empty(), settings.clone())
                .expect("deposit tenor is valid");
            helpers.push(DepositRateHelper::new(quote, &index) as Shared<dyn RateHelper>);
        }

        let imm_start = imm::next_date(settlement, false);
        let price = Handle::new(shared(SimpleQuote::new(95.5)) as Shared<dyn Quote>);
        let futures = FuturesRateHelper::new(
            price,
            imm_start,
            3,
            calendar.clone(),
            BusinessDayConvention::ModifiedFollowing,
            false,
            Actual360::new(),
            Handle::empty(),
            FuturesType::Imm,
        )
        .expect("the next IMM date is a valid futures start");
        helpers.push(futures as Shared<dyn RateHelper>);

        let fra_index = Euribor::six_months(Handle::empty(), settings.clone());
        let fra_quote = Handle::new(shared(SimpleQuote::new(0.046)) as Shared<dyn Quote>);
        let fra = FraRateHelper::new(
            fra_quote,
            Period::new(9, TimeUnit::Months),
            &fra_index,
            true,
            Pillar::LastRelevantDate,
        );
        helpers.push(fra as Shared<dyn RateHelper>);

        for (n, units, rate) in [
            (2, TimeUnit::Years, 0.0463),
            (3, TimeUnit::Years, 0.0475),
            (5, TimeUnit::Years, 0.0499),
        ] {
            let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
            let euribor6m = Euribor::six_months(Handle::empty(), settings.clone());
            helpers.push(SwapRateHelper::new(
                quote,
                Period::new(n, units),
                calendar.clone(),
                Frequency::Annual,
                BusinessDayConvention::Unadjusted,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor6m,
            ) as Shared<dyn RateHelper>);
        }

        (settlement, helpers)
    }

    /// A valid mixed market strip - deposits (1W/1M/3M), a 3-month IMM future,
    /// a 9x15 FRA and swaps (2Y/3Y/5Y) - bootstraps cleanly and every
    /// instrument reprices its own quote off the solved curve to 1e-9. The strip
    /// is arranged so pillar dates are distinct and latest-relevant dates are
    /// strictly monotone (the two ordering invariants `IterativeBootstrap`
    /// enforces, `iterativebootstrap.rs:136-145`); the futures window overlaps
    /// the 3M deposit in time but its pillar still sorts after, which the
    /// bootstrap accepts. The reprice is the bootstrap's own self-consistency
    /// residual: its value here is confirming the single-forward-pass property
    /// holds across a mixed strip, that solving the later swap nodes does not
    /// disturb the deposit/futures/FRA repricing.
    #[test]
    fn mixed_strip_bootstraps() {
        let (settlement, helpers) = build_mixed_strip();

        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            settlement,
            helpers.clone(),
            Actual360::new(),
            LogLinear,
        )
        .unwrap();

        // Force the bootstrap before repricing: each helper is linked to the
        // curve during the solve, so implied_quote reads the solved curve only
        // after calculate has run.
        let nodes = curve.dates().unwrap();
        assert_eq!(
            nodes.len(),
            helpers.len() + 1,
            "one curve node per helper plus the reference"
        );

        let mut worst = 0.0_f64;
        for helper in &helpers {
            let implied = helper.implied_quote().unwrap();
            let quote = helper.base().quote_value().unwrap();
            let error = (implied - quote).abs();
            worst = worst.max(error);
            assert!(
                error <= TOLERANCE,
                "mixed strip reprice: implied {implied} vs quote {quote} (err {error})"
            );
        }
        assert!(
            worst <= TOLERANCE,
            "worst mixed-strip reprice error {worst}"
        );
    }

    /// The global-interpolator counterpart of `mixed_strip_bootstraps`,
    /// converted from the pre-#543 rejection pin: the SAME mixed strip under
    /// `Cubic` now bootstraps through the ported convergence loop
    /// (`iterativebootstrap.hpp:257,363-387`) and every instrument reprices
    /// its own quote off the solved curve. Before the loop landed this strip
    /// was rejected up front with a deferral pointer at #543.
    ///
    /// The traits are `Discount` rather than the original `ForwardRate`: a
    /// piecewise cubic on instantaneous forwards is the configuration upstream
    /// itself disables as unstable (`testSplineForwardConsistency`,
    /// `piecewiseyieldcurve.cpp:750-769`, commented out `//Unstable`), and the
    /// port reproduces that instability faithfully - see
    /// [`unstable_forward_cubic_hits_the_iteration_cap`]. The original test
    /// only needed *some* traits to pin the rejection; the positive claim
    /// needs a configuration that genuinely converges.
    #[test]
    fn global_interpolator_bootstraps_through_the_convergence_loop() {
        use crate::math::interpolations::cubic::Cubic;

        let (settlement, helpers) = build_mixed_strip();

        let curve = PiecewiseYieldCurve::<Discount, Cubic>::new(
            settlement,
            helpers.clone(),
            Actual360::new(),
            Cubic,
        )
        .unwrap();

        let nodes = curve.dates().unwrap();
        assert_eq!(nodes.len(), helpers.len() + 1);

        for helper in &helpers {
            let implied = helper.implied_quote().unwrap();
            let quote = helper.base().quote_value().unwrap();
            let error = (implied - quote).abs();
            assert!(
                error <= TOLERANCE,
                "cubic mixed strip reprice: implied {implied} vs quote {quote} (err {error})"
            );
        }
    }

    /// `testSplineForwardConsistency` (`piecewiseyieldcurve.cpp:750-769`) is
    /// NOT ported: upstream keeps it commented out as `//Unstable`, and this
    /// port reproduces exactly that instability, documented here visibly
    /// rather than silently dropped. In its place this test pins the
    /// iteration-cap exit of the convergence loop (`iterativebootstrap.hpp:
    /// 376-383`, the `dontThrow=false` branch): the mixed strip under
    /// `<ForwardRate, Cubic>` settles into a period-2 cycle (the per-pass
    /// change plateaus near 2.2e-3 and never converges), so the bootstrap
    /// exhausts `Traits::max_iterations` passes and returns the ported
    /// non-convergence error instead of a silently mispriced curve.
    #[test]
    fn unstable_forward_cubic_hits_the_iteration_cap() {
        use crate::math::interpolations::cubic::Cubic;

        let (settlement, helpers) = build_mixed_strip();

        let curve = PiecewiseYieldCurve::<ForwardRate, Cubic>::new(
            settlement,
            helpers,
            Actual360::new(),
            Cubic,
        )
        .unwrap();

        let err = curve.dates().unwrap_err();
        assert!(
            err.message().contains("convergence not reached after"),
            "expected the ported non-convergence error, got: {}",
            err.message()
        );
    }

    /// A genuine duplicate pillar - two 3M deposits on the same index reduce to
    /// one pillar date - is rejected at bootstrap (query) time with the ported
    /// message, matching QuantLib's `QL_REQUIRE` throw
    /// (`iterativebootstrap.hpp:190-191`, `iterativebootstrap.rs:136-139`). This
    /// documents that the throw is faithful, not a defect: the only dedup in
    /// QuantLib lives in the separate, unported `GlobalBootstrap`.
    #[test]
    fn duplicate_pillar_is_rejected() {
        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let index = Euribor::new(Period::new(3, TimeUnit::Months), Handle::empty(), settings)
            .expect("deposit tenor is valid");
        let helpers: Vec<Shared<dyn RateHelper>> = vec![
            DepositRateHelper::from_rate(0.04557, &index) as Shared<dyn RateHelper>,
            DepositRateHelper::from_rate(0.04600, &index) as Shared<dyn RateHelper>,
        ];
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            settlement,
            helpers,
            Actual360::new(),
            LogLinear,
        )
        .unwrap();

        let err = curve.dates().unwrap_err();
        assert!(
            err.message()
                .contains("more than one instrument with pillar"),
            "expected the ported duplicate-pillar message, got: {}",
            err.message()
        );
    }

    /// Laziness: constructing the curve runs no bootstrap; the first discount
    /// does; a quote change invalidates and the next read re-bootstraps (the
    /// `testObservability` contract that forbids bootstrapping in the ctor).
    #[test]
    fn bootstrap_is_lazy_and_reruns_on_quote_change() {
        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let quote = shared(SimpleQuote::new(0.04557));
        let index = Euribor::new(
            Period::new(3, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        )
        .unwrap();
        let helper = DepositRateHelper::new(
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            &index,
        );
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            settlement,
            vec![Shared::clone(&helper) as Shared<dyn RateHelper>],
            Actual360::new(),
            LogLinear,
        )
        .unwrap();

        // cheap construction: no nodes laid out yet
        assert!(!curve.lazy.borrow().is_calculated());

        let df1 = curve.discount_date(helper.maturity_date(), false).unwrap();
        assert!(curve.lazy.borrow().is_calculated());
        assert!(df1 < 1.0 && df1 > 0.0);

        // a quote change invalidates the cache and re-bootstraps to a new curve
        quote.set_value(0.06);
        assert!(!curve.lazy.borrow().is_calculated());
        let df2 = curve.discount_date(helper.maturity_date(), false).unwrap();
        assert!(
            df2 < df1,
            "a higher deposit rate discounts more: {df2} vs {df1}"
        );
    }

    /// D1 for the helpers the BOOTSTRAP owns rather than fits
    /// (`globalbootstrap.hpp:219-220`): a `GlobalBootstrap` additional helper
    /// is registered with the curve through
    /// [`Bootstrap::additional_observables`], so a change to its quote
    /// invalidates the cache exactly as an instrument's does.
    ///
    /// The re-bootstrapped curve is then IDENTICAL, and deliberately so: an
    /// additional helper contributes no residual, and the driver only
    /// validates its quote (`hpp:348`) before handing it the curve. Its market
    /// quote reaches the solve solely through whatever the penalty reads, and
    /// here there is no penalty. So the pair of assertions is the whole
    /// contract - the notification must arrive, and it must not move the
    /// answer. The quote is moved from -0.004 to 0.05, orders of magnitude
    /// more than the 1e-12 agreement asserted, so a port that let it into the
    /// residual vector could not pass the second half.
    ///
    /// Bit-equality is not asserted: the second solve warm-restarts from the
    /// stored solution and rewrites every node through the transform pair, so
    /// the last digits may differ.
    ///
    /// Two additional helpers are registered and the quote moved is the
    /// SECOND one's: with a single helper the arm cannot tell "all of them"
    /// from "the first of them" (gate-probed - a registration truncated to
    /// the first helper passed the single-helper form).
    #[test]
    fn a_global_bootstrap_additional_helper_is_observed() {
        use crate::termstructures::globalbootstrap::GlobalBootstrap;

        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let index = Euribor::new(Period::new(3, TimeUnit::Months), Handle::empty(), settings)
            .expect("deposit tenor is valid");

        let first = FraRateHelper::from_months(
            Handle::new(shared(SimpleQuote::new(-0.004)) as Shared<dyn Quote>),
            3,
            &index,
            true,
            Pillar::LastRelevantDate,
        ) as Shared<dyn RateHelper>;
        let quote = shared(SimpleQuote::new(-0.004));
        let second = FraRateHelper::from_months(
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            6,
            &index,
            true,
            Pillar::LastRelevantDate,
        ) as Shared<dyn RateHelper>;
        let curve = PiecewiseYieldCurve::<Discount, LogLinear, _>::with_bootstrap(
            settlement,
            vec![DepositRateHelper::from_rate(0.04557, &index) as Shared<dyn RateHelper>],
            Actual360::new(),
            LogLinear,
            GlobalBootstrap::with_penalties(
                vec![first, second],
                None,
                None,
                None,
                Vec::new(),
                |_, _| Vec::new(),
            ),
        )
        .expect("a one-deposit strip builds a curve");

        let before = curve.data().expect("the strip solves");
        assert!(curve.lazy.borrow().is_calculated());

        quote.set_value(0.05);
        assert!(
            !curve.lazy.borrow().is_calculated(),
            "the second additional helper's quote must invalidate the curve"
        );

        let after = curve.data().expect("the strip re-solves");
        assert_eq!(before.len(), after.len());
        for (i, (old, new)) in before.iter().zip(&after).enumerate() {
            assert!(
                (old - new).abs() < 1.0e-12,
                "node {i} moved on an additional helper's quote: {old} vs {new}"
            );
        }
    }
    /// The curve-level half of the `registerAsObserver = false` contract
    /// (`piecewiseyieldcurve.cpp:1516-1519`): a futures helper holding its
    /// convexity quote through [`Handle::new_unregistered`] does not let a
    /// volatility change invalidate the bootstrapped curve, while the SAME
    /// setup through the ordinary [`Handle::new`] does.
    ///
    /// This is what makes a joint solve over the volatility possible at all -
    /// an observing handle clears the lazy flag on every optimizer step, and
    /// the penalty's reprice then re-enters `calculate`.
    ///
    /// The pair is built with a FIXED volatility and no additional variables,
    /// deliberately: the observing arm cannot be run under a variables solve,
    /// because that is precisely the configuration the C++ comment says breaks
    /// bootstrapping. Only the handle constructor differs between the two
    /// halves.
    fn futures_curve_on(
        observing: bool,
    ) -> (
        Shared<SimpleQuote>,
        Shared<PiecewiseYieldCurve<Discount, LogLinear>>,
    ) {
        use crate::instruments::FuturesType;

        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(25, Month::September, 2019),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let index = shared(Euribor::three_months(Handle::empty(), settings.clone()));
        let futures_date = imm::next_date(today, true);
        let price = Handle::new(shared(SimpleQuote::new(95.419)) as Shared<dyn Quote>);
        let volatility = shared(SimpleQuote::new(0.2));
        let conv_adj = shared(
            FuturesConvAdjustmentQuote::new(
                &index,
                futures_date,
                price.clone(),
                Handle::new(Shared::clone(&volatility) as Shared<dyn Quote>),
                make_quote_handle(0.03).handle(),
                settings,
            )
            .expect("the index resolves the maturity of an IMM date"),
        ) as Shared<dyn Quote>;
        let conv_adj = if observing {
            Handle::new(conv_adj)
        } else {
            Handle::new_unregistered(conv_adj)
        };
        let helper =
            FuturesRateHelper::from_index(price, futures_date, &index, conv_adj, FuturesType::Imm)
                .expect("an IMM date builds a futures helper")
                as Shared<dyn RateHelper>;

        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            settlement,
            vec![helper],
            Actual365Fixed::new(),
            LogLinear,
        )
        .expect("a one-futures strip builds a curve");
        (volatility, curve)
    }

    #[test]
    fn an_unregistered_convexity_handle_survives_a_volatility_change() {
        let (volatility, curve) = futures_curve_on(false);
        curve.data().expect("the one-futures strip solves");
        assert!(curve.lazy.borrow().is_calculated());

        volatility.set_value(0.3);

        assert!(
            curve.lazy.borrow().is_calculated(),
            "an unregistered convexity handle must not invalidate the curve"
        );
    }

    #[test]
    fn an_observing_convexity_handle_invalidates_the_curve() {
        let (volatility, curve) = futures_curve_on(true);
        curve.data().expect("the one-futures strip solves");
        assert!(curve.lazy.borrow().is_calculated());

        volatility.set_value(0.3);

        assert!(
            !curve.lazy.borrow().is_calculated(),
            "an observing convexity handle must invalidate the curve"
        );
    }
    /// The IMM futures quotes of `CommonVars` (`piecewiseyieldcurve.cpp:116`,
    /// turned into prices at `:266`): `100 - rate`.
    const IMM_FUTURES_PRICE: [Real; 3] = [95.419, 95.427, 95.443];

    /// The 22 pillar dates of the plain curve, from the C++ dylib run.
    const REF_PILLAR: [(Day, Month, Year); 22] = [
        (27, Month::September, 2019),
        (4, Month::October, 2019),
        (28, Month::October, 2019),
        (27, Month::November, 2019),
        (27, Month::December, 2019),
        (27, Month::March, 2020),
        (29, Month::June, 2020),
        (28, Month::September, 2020),
        (27, Month::September, 2021),
        (27, Month::September, 2022),
        (27, Month::September, 2023),
        (27, Month::September, 2024),
        (29, Month::September, 2025),
        (28, Month::September, 2026),
        (27, Month::September, 2027),
        (27, Month::September, 2028),
        (27, Month::September, 2029),
        (29, Month::September, 2031),
        (27, Month::September, 2034),
        (27, Month::September, 2039),
        (27, Month::September, 2044),
        (27, Month::September, 2049),
    ];

    /// Port of `testGlobalBootstrapVariables`
    /// (`piecewiseyieldcurve.cpp:1486-1545`): a `PiecewiseYieldCurve<Discount,
    /// LogLinear, GlobalBootstrap>` over the `CommonVars` strip, and a second
    /// one where the first swap is replaced by three IMM futures whose
    /// convexity volatility is fitted AS PART OF the same global solve.
    ///
    /// The joint system is SQUARE: 23 interior nodes plus one volatility
    /// variable against 6 deposit residuals, 14 swap residuals, 3 futures
    /// residuals and 1 penalty term. The removed first swap carries no residual
    /// of its own - it is an additional helper, and reaches the solve only
    /// through the penalty `1e4 * quote_error`, which is what ties the futures
    /// strip back to the swap level the volatility has to reproduce.
    ///
    /// TWO HANDLES, TWO BEHAVIOURS, and the distinction is load-bearing. The
    /// volatility handle INSIDE each convexity quote is the ordinary observing
    /// one, so a trial volatility drops the quote's cached bias and the next
    /// residual is computed at the new one. Only the futures helper's handle TO
    /// the convexity quote is unregistered, so that same write does not
    /// invalidate the curve mid-solve.
    ///
    /// The reference numbers come from a harness run against a locally built
    /// QuantLib dylib with `usingAtParCoupons()` set, since the `.cpp` prints
    /// none: the two pillar lists, the solved volatility, and the pillar
    /// discounts of both curves. The C++ arms are a pillar-list inequality and
    /// a repricing comparison at `QL_CHECK_CLOSE` 1e-6, which is a PERCENTAGE
    /// tolerance and so 1e-8 relative; the port asserts 1e-9, and the dylib's
    /// own worst disagreement between the two curves is 2.5e-15 against this
    /// port's 2.0e-15.
    ///
    /// The SOLVED-VOLATILITY pin is not in the C++ test and is what carries
    /// this oracle. Both upstream arms are blind to the convexity model: the
    /// joint solve reprices every original instrument for whatever volatility
    /// the futures leg is fitted at, so the discount comparison passes on a
    /// bias timed from the wrong reference date, and passes even when the
    /// volatility never moves off its initial guess. Probed both ways - a bias
    /// referenced to settlement rather than to the evaluation date leaves the
    /// discount arm green and shifts the fitted volatility to 0.0993827, and an
    /// unregistered inner volatility handle leaves it stuck at exactly 1.0.
    #[test]
    fn global_bootstrap_variables_fit_a_futures_convexity_volatility() {
        use crate::indexes::interestrateindex::InterestRateIndex;
        use crate::instruments::FuturesType;
        use crate::termstructures::globalbootstrap::GlobalBootstrap;
        use crate::termstructures::globalbootstrapvars::SimpleQuoteVariables;

        let vars = common_vars_on(Date::new(25, Month::September, 2019));
        let curve = PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::new(
            vars.settlement,
            vars.instruments.clone(),
            Actual365Fixed::new(),
            LogLinear,
        )
        .expect("the plain strip builds a curve");

        let mut helpers = vars.instruments.clone();
        let first_swap = helpers.remove(DEPOSIT_DATA.len());

        let index = shared(Euribor::three_months(
            Handle::empty(),
            vars.settings.clone(),
        ));
        let volatility = shared(SimpleQuote::new(None));
        let mean_reversion = make_quote_handle(0.03);
        let mut futures_date = vars.today;
        for price in IMM_FUTURES_PRICE {
            futures_date = imm::next_date(futures_date, true);
            if index.fixing_date(futures_date) < vars.today {
                futures_date = imm::next_date(futures_date, true);
            }
            let price = Handle::new(shared(SimpleQuote::new(price)) as Shared<dyn Quote>);
            let conv_adj = shared(
                FuturesConvAdjustmentQuote::new(
                    &index,
                    futures_date,
                    price.clone(),
                    Handle::new(Shared::clone(&volatility) as Shared<dyn Quote>),
                    mean_reversion.handle(),
                    vars.settings.clone(),
                )
                .expect("the index resolves the maturity of an IMM date"),
            ) as Shared<dyn Quote>;
            helpers.push(
                FuturesRateHelper::from_index(
                    price,
                    futures_date,
                    &index,
                    Handle::new_unregistered(conv_adj),
                    FuturesType::Imm,
                )
                .expect("an IMM date builds a futures helper")
                    as Shared<dyn RateHelper>,
            );
        }

        let penalty_helper = Shared::clone(&first_swap);
        let curve_futures = PiecewiseYieldCurve::<Discount, LogLinear, _>::with_bootstrap(
            vars.settlement,
            helpers,
            Actual365Fixed::new(),
            LogLinear,
            GlobalBootstrap::with_grid_independent_penalties(
                vec![Shared::clone(&first_swap)],
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                move || {
                    let error = penalty_helper
                        .quote_error()
                        .expect("the first swap reprices off the trial curve");
                    vec![1.0e4 * error]
                },
            )
            .with_additional_variables(Box::new(
                SimpleQuoteVariables::new(vec![Shared::clone(&volatility)], vec![1.0], vec![0.0])
                    .expect("one guess and one bound for one quote"),
            )),
        )
        .expect("the futures strip builds a curve");

        let pillars: Vec<Date> = REF_PILLAR
            .iter()
            .map(|(day, month, year)| Date::new(*day, *month, *year))
            .collect();
        assert_eq!(
            curve.dates().expect("the plain strip solves"),
            pillars,
            "the plain curve's pillar list"
        );

        // The futures curve drops the first swap's pillar (28 Sep 2020) and
        // gains the three futures maturities, so 24 dates against 22.
        let mut futures_pillars: Vec<Date> = pillars
            .iter()
            .copied()
            .filter(|date| *date != Date::new(28, Month::September, 2020))
            .chain([
                Date::new(18, Month::March, 2020),
                Date::new(18, Month::June, 2020),
                Date::new(17, Month::September, 2020),
            ])
            .collect();
        futures_pillars.sort_unstable();
        assert_eq!(futures_pillars.len(), 24);
        assert_eq!(
            curve_futures
                .dates()
                .expect("the futures strip solves")
                .clone(),
            futures_pillars,
            "the futures curve's pillar list"
        );

        let solved = volatility.value().expect("the volatility is solved");
        assert!(
            (solved - 0.098_615_063_557_902_12).abs() < 1.0e-8,
            "the fitted convexity volatility is {solved}"
        );
        assert!(
            (solved - 1.0).abs() > 0.5,
            "the volatility never moved off its initial guess of 1.0"
        );

        let mut worst = 0.0_f64;
        for helper in &vars.instruments {
            let pillar = helper.pillar_date();
            let plain = curve
                .discount_date(pillar, false)
                .expect("the plain curve discounts its own pillar");
            let with_futures = curve_futures
                .discount_date(pillar, false)
                .expect("the futures curve discounts the plain pillar");
            let relative = (plain - with_futures).abs() / plain;
            worst = worst.max(relative);
            assert!(
                relative < 1.0e-9,
                "{pillar} discounts to {plain} without futures and {with_futures} with them"
            );
        }
        assert!(worst < 1.0e-9, "worst relative discount gap is {worst}");
    }
}
