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
//!   `Discount` (`LogLinear`/`Linear`) and `ZeroYield` (`Linear`) conventions
//!   are wired; the spline interpolators are deferred (they need the global
//!   convergence loop, unported).
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
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
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
/// `T` is the curve-shape traits (`Discount`); `I` is the interpolation factory
/// (`LogLinear`, `Linear`). The node data lives in a `RefCell` the bootstrap
/// mutates and the discount lookup reads back.
pub struct PiecewiseYieldCurve<T: BootstrapTraits, I: Interpolator> {
    base: TermStructureBase,
    instruments: Vec<Shared<dyn RateHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn YieldTermStructure>,
    _traits: PhantomData<fn() -> T>,
}

impl<T: BootstrapTraits + 'static, I: Interpolator + 'static> PiecewiseYieldCurve<T, I> {
    /// Builds a curve over `instruments` with a fixed `reference_date` (the C++
    /// reference-date constructor). Construction is cheap; the bootstrap runs
    /// on first use.
    pub fn new(
        reference_date: Date,
        instruments: Vec<Shared<dyn RateHelper>>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Shared<PiecewiseYieldCurve<T, I>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseYieldCurve<T, I>>| {
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
                bootstrap: IterativeBootstrap::new(),
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

impl<T: BootstrapTraits, I: Interpolator> AsObservable for PiecewiseYieldCurve<T, I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<T: BootstrapTraits + 'static, I: Interpolator + 'static> TermStructure
    for PiecewiseYieldCurve<T, I>
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

impl<T: BootstrapTraits + 'static, I: Interpolator + 'static> YieldTermStructure
    for PiecewiseYieldCurve<T, I>
{
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

impl<T: BootstrapTraits + 'static, I: Interpolator + 'static> PiecewiseCurve
    for PiecewiseYieldCurve<T, I>
{
    type Traits = T;
    type Interp = I;

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

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
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
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::bootstraptraits::{Discount, ForwardRate, ZeroYield};
    use crate::termstructures::yields::{DepositRateHelper, SwapRateHelper};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
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
        T: BootstrapTraits + 'static,
        I: Interpolator + Default + 'static,
    >() -> Shared<PiecewiseYieldCurve<T, I>> {
        let vars = common_vars();
        let curve = PiecewiseYieldCurve::<T, I>::new(
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
                (estimated - expected).abs() <= TOLERANCE,
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
                (estimated - expected).abs() <= TOLERANCE,
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
}
