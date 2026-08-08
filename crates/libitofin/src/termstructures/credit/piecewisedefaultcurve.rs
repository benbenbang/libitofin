//! Piecewise-bootstrapped credit term structure.
//!
//! Port of `ql/termstructures/credit/piecewisedefaultcurve.hpp:53`. A
//! [`PiecewiseDefaultCurve`] is built from a set of
//! [`DefaultProbabilityHelper`]s whose maturities mark the segment boundaries;
//! each node is solved so the helper reprices its quote off the curve, by the
//! same [`IterativeBootstrap`] the yield side runs.
//!
//! This is the credit twin of
//! [`PiecewiseYieldCurve`](crate::termstructures::yields::PiecewiseYieldCurve)
//! and mirrors it field for field: the laziness contract, the pre-set
//! `calculated` flag, the observer registration on every helper and the
//! [`PiecewiseCurve`] surface the bootstrap drives are all the same.
//!
//! ## Laziness and bootstrap re-entrancy
//!
//! C++ derives from `Traits::curve<Interpolator>::type` *and* `LazyObject`
//! (`:53-60`), and its `survivalProbabilityImpl` / `defaultDensityImpl` /
//! `hazardRateImpl` each call `calculate()` before delegating to the base curve
//! (`:259-276`). Rust has no inheritance, so the node storage lives here and the
//! reads run the same conversions over it, through the free functions
//! `survival_probability_from_nodes` and `hazard_rate_from_nodes` the plain
//! [`InterpolatedHazardRateCurve`](crate::termstructures::credit::interpolatedhazardratecurve::InterpolatedHazardRateCurve)
//! reads its own nodes with.
//!
//! The pre-set `calculated` flag ([`LazyObject::new(true)`](LazyObject::new)) is
//! what breaks the bootstrap cycle, and the cycle here is tighter than on the
//! yield side: mid-bootstrap a
//! [`SpreadCdsHelper`](crate::termstructures::credit::defaultprobabilityhelpers::SpreadCdsHelper)
//! reprices its contract, whose engine reads *this* curve, which re-enters
//! [`calculate`](PiecewiseDefaultCurve::calculate). The re-entrant call finds
//! the calculation already running, returns immediately, and the read answers
//! off the partially solved node prefix - which is exactly what the bootstrap
//! needs, since a helper only ever reads up to its own pillar.
//!
//! ## Scope
//!
//! The `Traits` type parameter carries the C++ spelling
//! (`PiecewiseDefaultCurve<HazardRate, BackwardFlat>`) but only
//! [`HazardRate`] is wired: every impl below is written at that instantiation,
//! so a curve on the yield traits will not compile - the Rust counterpart of
//! C++'s `Traits::curve<I>::type` failing to be a
//! `DefaultProbabilityTermStructure`. `DefaultDensity` and
//! `SurvivalProbability` need their own base curves and follow within EPIC
//! Credit (#676); adding them means relaxing these bounds.
//!
//! Jump quotes are not ported, per the
//! [`defaulttermstructure`](crate::termstructures::credit::defaulttermstructure)
//! divergence (#676), so the four C++ constructors collapse to
//! [`new`](PiecewiseDefaultCurve::new).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::Interpolator;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::termstructures::credit::defaultprobabilityhelpers::DefaultProbabilityHelper;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::hazardratestructure::HazardRateStructure;
use crate::termstructures::credit::interpolatedhazardratecurve::{
    hazard_rate_from_nodes, survival_probability_from_nodes,
};
use crate::termstructures::credit::probabilitytraits::HazardRate;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Rate, Real, Time};

/// Feeds a helper-quote, discount-curve or evaluation-date notification into
/// the curve's lazy core: it invalidates the bootstrap cache and re-broadcasts
/// to the curve's own observers (the port of `registerWithObservables` +
/// `LazyObject::update`), as the yield curve's own updater does.
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

/// Default-probability term structure bootstrapped from credit helpers.
///
/// `T` is the curve-shape traits ([`HazardRate`]) and `I` the interpolation
/// factory (`BackwardFlat`). The node data lives in a `RefCell` the bootstrap
/// mutates and the survival/hazard lookups read back.
pub struct PiecewiseDefaultCurve<T: BootstrapTraits, I: Interpolator> {
    base: TermStructureBase,
    instruments: Vec<Shared<dyn DefaultProbabilityHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn DefaultProbabilityTermStructure>,
    _traits: PhantomData<fn() -> T>,
}

impl<I: Interpolator + 'static> PiecewiseDefaultCurve<HazardRate, I> {
    /// Builds a curve over `instruments` with a fixed `reference_date` (the C++
    /// reference-date constructor, `piecewisedefaultcurve.hpp:68-79`).
    /// Construction is cheap; the bootstrap runs on first use.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    pub fn new(
        reference_date: Date,
        instruments: Vec<Shared<dyn DefaultProbabilityHelper>>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Shared<PiecewiseDefaultCurve<HazardRate, I>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseDefaultCurve<HazardRate, I>>| {
            let self_weak: Weak<dyn DefaultProbabilityTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseDefaultCurve {
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

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        Ok(curve)
    }

    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `piecewisedefaultcurve.hpp:279-283`). The lazy
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

    /// The node hazard rates, after bootstrapping.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, hazard rate) nodes, after bootstrapping.
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseDefaultCurve<HazardRate, I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseDefaultCurve<HazardRate, I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve; a
        // bootstrap failure is surfaced by the reads, so fall back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.base.reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> HazardRateStructure for PiecewiseDefaultCurve<HazardRate, I> {
    fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        hazard_rate_from_nodes(data.interpolation()?, t)
    }
}

impl<I: Interpolator + 'static> DefaultProbabilityTermStructure
    for PiecewiseDefaultCurve<HazardRate, I>
{
    /// Opts the bootstrapped curve into the downcast seam, for the same reason
    /// its yield-side twin does
    /// ([`PiecewiseYieldCurve`](crate::termstructures::yields::piecewiseyieldcurve::PiecewiseYieldCurve)):
    /// C++ casts a `PiecewiseDefaultCurve<HazardRate, BackwardFlat>` to its
    /// `InterpolatedHazardRateCurve<BackwardFlat>` base
    /// (`isdacdsengine.cpp:136-141`), which composition cannot reproduce.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        self.calculate()?;
        let data = self.data.borrow();
        survival_probability_from_nodes(data.interpolation()?, t)
    }

    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        self.default_density_from_hazard_rate(t)
    }

    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.hazard_rate_curve_impl(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseDefaultCurve<HazardRate, I> {
    type Traits = HazardRate;
    type Interp = I;
    type TS = dyn DefaultProbabilityTermStructure;
    type Helper = dyn DefaultProbabilityHelper;

    fn instruments(&self) -> &[Shared<dyn DefaultProbabilityHelper>] {
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

    fn term_structure_shared(&self) -> QlResult<Shared<dyn DefaultProbabilityTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Oracle: `defaultprobabilitycurves.cpp` `testFlatHazardConsistency`
    //! (`:320`) through `testBootstrapFromSpread<HazardRate, BackwardFlat>`
    //! (`:152-224`) and `testBootstrapFromUpfront<HazardRate, BackwardFlat>`
    //! (`:230-317`), `testSingleInstrumentBootstrap` (`:344-378`) and
    //! `testUpfrontBootstrap` (`:380-395`).
    //!
    //! The round trip is self-consistent - every helper's own contract is
    //! rebuilt and repriced off the bootstrapped curve and must reproduce its
    //! input spread - so there are no external numbers to transcribe.
    //!
    //! Two deliberate departures from the C++ fixture, both from D5:
    //!
    //! - C++ takes `today` from `Settings::instance().evaluationDate()`, which
    //!   the test suite's global fixture sets from the clock. Each test here
    //!   owns its `Settings`, so the evaluation date is a fixed TARGET business
    //!   day, asserted rather than assumed.
    //! - The `SavedSettings` guard around the `includeTodaysCashFlows` write
    //!   (`:196-198`) is not ported: the flag lives on a `Settings` this test
    //!   owns outright, so there is no global to restore.

    use super::*;
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{CdsTerms, CreditDefaultSwap, ProtectionSide, cds_maturity};
    use crate::interestrate::Compounding;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::credit::{MidPointCdsEngine, isda_node_grid};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::credit::defaultprobabilityhelpers::{
        CdsHelperTerms, SpreadCdsHelper, UpfrontCdsHelper,
    };
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::schedule::Schedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Natural, Rate};

    const QUOTES: [Real; 4] = [0.005, 0.006, 0.007, 0.009];
    const TENORS: [i32; 4] = [1, 2, 3, 5];
    const RECOVERY_RATE: Real = 0.4;
    const TOLERANCE: Real = 1.0e-6;

    /// The evaluation date, standing in for the C++ global fixture's. A TARGET
    /// business day, and one whose yearly anniversaries miss the IMM twentieths
    /// the schedules roll to - see
    /// [`the_helper_and_the_round_trip_agree_on_the_imm_maturity`].
    fn today() -> Date {
        Date::new(9, Month::June, 2006)
    }

    fn settings_at(evaluation_date: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(evaluation_date);
        settings
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    /// `FlatForward(today, 0.06, Actual360())` (`:172-173`), whose C++ defaults
    /// are continuous compounding at annual frequency.
    fn discount_curve(reference_date: Date) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference_date,
            0.06,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn spread_helper(
        quote: Real,
        tenor: Period,
        settlement_days: Integer,
        discount: &Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> Shared<dyn DefaultProbabilityHelper> {
        SpreadCdsHelper::new(
            Handle::new(shared(SimpleQuote::new(quote)) as Shared<dyn Quote>),
            tenor,
            settlement_days,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::Following,
            DateGeneration::TwentiethIMM,
            day_counter(),
            RECOVERY_RATE,
            discount.clone(),
            Shared::clone(settings),
        )
        .expect("TwentiethIMM is an accepted date-generation rule")
            as Shared<dyn DefaultProbabilityHelper>
    }

    /// The round-trip contract's schedule (`:203-206`): it starts at the rolled
    /// protection start, as the helper's does, but ends a tenor past `today`
    /// where the helper's ends a tenor past the protection start.
    fn round_trip_schedule(today: Date, tenor: Period) -> Schedule {
        let calendar = Target::new();
        let start_date = calendar.adjust(today + SETTLEMENT_DAYS, BusinessDayConvention::Following);
        Schedule::new(
            start_date,
            today + tenor,
            Period::try_from(Frequency::Quarterly).expect("a quarterly period"),
            calendar,
            BusinessDayConvention::Following,
            BusinessDayConvention::Unadjusted,
            DateGeneration::TwentiethIMM,
            false,
            Date::null(),
            Date::null(),
        )
    }

    fn rolled_back_date(schedule: &Schedule) -> Date {
        Target::new().adjust(
            schedule.date(schedule.len() - 1),
            BusinessDayConvention::Following,
        )
    }

    const SETTLEMENT_DAYS: Integer = 1;

    struct Fixture {
        settings: Shared<Settings<Date>>,
        discount: Handle<dyn YieldTermStructure>,
        helpers: Vec<Shared<dyn DefaultProbabilityHelper>>,
        curve: Shared<PiecewiseDefaultCurve<HazardRate, BackwardFlat>>,
    }

    /// The four-pillar fixture of `testBootstrapFromSpread` (`:154-186`).
    fn fixture() -> Fixture {
        assert!(
            Target::new().is_business_day(today()),
            "the fixed evaluation date must be a TARGET business day, as the \
             C++ fixture's clock-derived one is rolled to be"
        );
        let settings = settings_at(today());
        // C++ sets this before the first pricing (`:198`); the helpers price
        // their own contracts inside the bootstrap, so it must be in place
        // before any read triggers it.
        settings.set_include_todays_cash_flows(Some(true));
        let discount = discount_curve(today());

        let helpers: Vec<Shared<dyn DefaultProbabilityHelper>> = QUOTES
            .iter()
            .zip(TENORS)
            .map(|(quote, n)| {
                spread_helper(
                    *quote,
                    Period::new(n, TimeUnit::Years),
                    SETTLEMENT_DAYS,
                    &discount,
                    &settings,
                )
            })
            .collect();

        let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            today(),
            helpers.clone(),
            day_counter(),
            BackwardFlat,
        )
        .unwrap();

        Fixture {
            settings,
            discount,
            helpers,
            curve,
        }
    }

    /// `testBootstrapFromSpread<HazardRate, BackwardFlat>` (`:200-223`): each
    /// pillar's CDS, rebuilt from the market conventions and repriced off the
    /// bootstrapped curve, returns its own input spread to 1e-6.
    #[test]
    fn bootstrapped_curve_reproduces_the_input_cds_spreads() {
        let fixture = fixture();
        let curve: Handle<dyn DefaultProbabilityTermStructure> = Handle::new(Shared::clone(
            &fixture.curve,
        )
            as Shared<dyn DefaultProbabilityTermStructure>);

        for (quote, n) in QUOTES.iter().zip(TENORS) {
            let tenor = Period::new(n, TimeUnit::Years);
            let protection_start = today() + SETTLEMENT_DAYS;
            let mut cds = CreditDefaultSwap::with_terms(
                ProtectionSide::Buyer,
                1.0,
                *quote,
                round_trip_schedule(today(), tenor),
                BusinessDayConvention::Following,
                day_counter(),
                CdsTerms {
                    protection_start: Some(protection_start),
                    ..CdsTerms::default()
                },
                Shared::clone(&fixture.settings),
            )
            .unwrap();
            let engine = MidPointCdsEngine::new(
                curve.clone(),
                RECOVERY_RATE,
                fixture.discount.clone(),
                None,
                Shared::clone(&fixture.settings),
            );
            cds.base_mut()
                .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

            let computed = cds.fair_spread().unwrap();
            assert!(
                (computed - quote).abs() <= TOLERANCE,
                "failed to reproduce the fair spread for the {n}Y credit-default swap: \
                 computed {computed}, input {quote}"
            );
        }
    }

    /// The bootstrapped curve is what the ISDA engine is handed, so it must be
    /// introspectable through the downcast seam
    /// (`isdacdsengine.cpp:136-141`). C++ gets there by inheritance - a
    /// `PiecewiseDefaultCurve<HazardRate, BackwardFlat>` *is* an
    /// `InterpolatedHazardRateCurve<BackwardFlat>` - which this port's
    /// composition cannot reproduce, so the arm is named for the piecewise
    /// curve in its own right. The fixture's discount curve is flat and
    /// contributes nothing, leaving the grid as the solved pillars alone.
    #[test]
    fn bootstrapped_curve_feeds_the_isda_node_grid() {
        let fixture = fixture();
        let curve: Handle<dyn DefaultProbabilityTermStructure> = Handle::new(Shared::clone(
            &fixture.curve,
        )
            as Shared<dyn DefaultProbabilityTermStructure>);

        let pillars = fixture.curve.dates().expect("the bootstrap succeeds");
        assert_eq!(
            pillars.len(),
            TENORS.len() + 1,
            "the reference date plus a pillar per tenor"
        );

        let grid = isda_node_grid(&fixture.discount, &curve, today() + 10_000)
            .expect("a bootstrapped backward-flat hazard-rate curve is an ISDA curve");
        assert_eq!(grid, pillars);
    }

    /// The maturity the round trip compares on is only the helper's because
    /// `TwentiethIMM` snaps both to the same twentieth: the helper's schedule
    /// ends a tenor past `today + settlementDays` (`defaultprobabilityhelpers
    /// .cpp:90-92`) and the round trip's a tenor past `today` (`:205`).
    ///
    /// `next_twentieth` rolls to the twentieth *on or after* its argument
    /// (`schedule.rs:956-958`), so the two agree everywhere except when
    /// `today + n*Years` lands exactly on an IMM twentieth - there the helper's
    /// one-day-later variant rolls a full quarter and the round trip would
    /// silently compare a three-month-shorter contract against the curve node.
    /// This asserts the agreement instead of documenting it, so a future
    /// evaluation date on that boundary fails loudly.
    #[test]
    fn the_helper_and_the_round_trip_agree_on_the_imm_maturity() {
        let fixture = fixture();
        for (helper, n) in fixture.helpers.iter().zip(TENORS) {
            let schedule = round_trip_schedule(today(), Period::new(n, TimeUnit::Years));
            assert_eq!(
                helper.latest_date(),
                rolled_back_date(&schedule),
                "the {n}Y helper's pillar and the round-trip contract's maturity diverge"
            );
        }
    }

    /// `testSingleInstrumentBootstrap` (`:344-378`): one helper is enough,
    /// because `BackwardFlat` needs a single point (`flat.rs`), so the two nodes
    /// a lone pillar lays down clear the driver's required-points guard
    /// (`iterativebootstrap.rs:153-158`). C++ asserts nothing beyond the
    /// bootstrap completing; the node count is added here to pin what makes it
    /// complete.
    #[test]
    fn a_single_helper_bootstraps() {
        let settings = settings_at(today());
        let discount = discount_curve(today());
        let helper = spread_helper(
            0.005,
            Period::new(2, TimeUnit::Years),
            0,
            &discount,
            &settings,
        );
        let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            today(),
            vec![helper],
            day_counter(),
            BackwardFlat,
        )
        .unwrap();

        curve.calculate().unwrap();
        assert_eq!(
            curve.dates().unwrap().len(),
            2,
            "a lone pillar lays down the reference node and its own"
        );
    }

    /// Construction lays down no nodes and runs no solver; the first read
    /// bootstraps; a quote move invalidates the cache and the next read
    /// re-bootstraps to a wider curve (the C++ `LazyObject` contract that keeps
    /// the bootstrap out of the constructor).
    #[test]
    fn the_bootstrap_is_lazy_and_reruns_on_a_quote_change() {
        let settings = settings_at(today());
        let discount = discount_curve(today());
        let quote = shared(SimpleQuote::new(0.005));
        let helper = SpreadCdsHelper::new(
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            Period::new(5, TimeUnit::Years),
            SETTLEMENT_DAYS,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::Following,
            DateGeneration::TwentiethIMM,
            day_counter(),
            RECOVERY_RATE,
            discount,
            Shared::clone(&settings),
        )
        .unwrap();
        let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            today(),
            vec![Shared::clone(&helper) as Shared<dyn DefaultProbabilityHelper>],
            day_counter(),
            BackwardFlat,
        )
        .unwrap();

        assert!(!curve.lazy.borrow().is_calculated());
        let first = curve
            .survival_probability_date(helper.latest_date(), false)
            .unwrap();
        assert!(curve.lazy.borrow().is_calculated());
        assert!(first < 1.0 && first > 0.0);

        quote.set_value(0.02);
        assert!(!curve.lazy.borrow().is_calculated());
        let second = curve
            .survival_probability_date(helper.latest_date(), false)
            .unwrap();
        assert!(
            second < first,
            "a wider spread must lower the survival probability: {second} vs {first}"
        );
    }

    const UPFRONT_QUOTES: [Real; 4] = [0.01, 0.02, 0.04, 0.06];
    const UPFRONT_TENORS: [i32; 4] = [2, 3, 5, 7];
    const RUNNING_SPREAD: Rate = 0.05;
    const UPFRONT_SETTLEMENT_DAYS: Natural = 3;

    /// The upfront fixture's conventions (`:234-247`), which differ from the
    /// spread one's in more than the quotation: the contracts roll on
    /// `DateGeneration::CDS` under `ModifiedFollowing`, accrue on `Actual360`
    /// with the last period including its end date, and pay a fixed 5% coupon
    /// while the quote is the upfront.
    fn upfront_helper(
        quote: Real,
        tenor: Period,
        discount: &Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> Shared<dyn DefaultProbabilityHelper> {
        UpfrontCdsHelper::with_terms(
            Handle::new(shared(SimpleQuote::new(quote)) as Shared<dyn Quote>),
            RUNNING_SPREAD,
            tenor,
            SETTLEMENT_DAYS,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::ModifiedFollowing,
            DateGeneration::CDS,
            Actual360::new(),
            RECOVERY_RATE,
            discount.clone(),
            UPFRONT_SETTLEMENT_DAYS,
            CdsHelperTerms {
                last_period_day_counter: Some(Actual360::with_last_day(true)),
                ..CdsHelperTerms::default()
            },
            Shared::clone(settings),
        )
        .expect("the CDS rule rolls every tenor quoted here")
            as Shared<dyn DefaultProbabilityHelper>
    }

    /// The round-trip contract's schedule (`:290-291`), from the protection
    /// start to the rolled CDS maturity.
    fn upfront_schedule(today: Date, tenor: Period) -> Schedule {
        Schedule::new(
            today + SETTLEMENT_DAYS,
            cds_maturity(today, tenor, DateGeneration::CDS)
                .unwrap()
                .expect("a live CDS maturity"),
            Period::try_from(Frequency::Quarterly).expect("a quarterly period"),
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            BusinessDayConvention::Unadjusted,
            DateGeneration::CDS,
            false,
            Date::null(),
            Date::null(),
        )
    }

    /// The four upfront helpers of `testBootstrapFromUpfront` (`:252-266`) and
    /// the curve they bootstrap.
    fn upfront_fixture(settings: &Shared<Settings<Date>>) -> Fixture {
        let discount = discount_curve(today());
        let helpers: Vec<Shared<dyn DefaultProbabilityHelper>> = UPFRONT_QUOTES
            .iter()
            .zip(UPFRONT_TENORS)
            .map(|(quote, n)| {
                upfront_helper(*quote, Period::new(n, TimeUnit::Years), &discount, settings)
            })
            .collect();
        let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            today(),
            helpers.clone(),
            day_counter(),
            BackwardFlat,
        )
        .unwrap();
        Fixture {
            settings: Shared::clone(settings),
            discount,
            helpers,
            curve,
        }
    }

    /// The reprice loop of `testBootstrapFromUpfront` (`:277-315`): each
    /// pillar's contract, rebuilt from the market conventions and priced off
    /// the bootstrapped curve, must return its own input upfront.
    ///
    /// The `includeTodaysCashFlows` write and its unwind are the C++ block's
    /// (`:278-281`), kept around the reprice alone so that what the flag holds
    /// outside it is the caller's business.
    fn assert_the_upfronts_reproduce(fixture: &Fixture) {
        let curve: Handle<dyn DefaultProbabilityTermStructure> = Handle::new(Shared::clone(
            &fixture.curve,
        )
            as Shared<dyn DefaultProbabilityTermStructure>);
        let restore = fixture.settings.include_todays_cash_flows();
        fixture.settings.set_include_todays_cash_flows(Some(true));

        for (quote, n) in UPFRONT_QUOTES.iter().zip(UPFRONT_TENORS) {
            let tenor = Period::new(n, TimeUnit::Years);
            let mut cds = CreditDefaultSwap::with_upfront_and_terms(
                ProtectionSide::Buyer,
                1.0,
                *quote,
                RUNNING_SPREAD,
                upfront_schedule(today(), tenor),
                BusinessDayConvention::ModifiedFollowing,
                Actual360::new(),
                CdsTerms {
                    protection_start: Some(today() + SETTLEMENT_DAYS),
                    upfront_date: Some(Target::new().advance(
                        today(),
                        UPFRONT_SETTLEMENT_DAYS as Integer,
                        TimeUnit::Days,
                        BusinessDayConvention::ModifiedFollowing,
                        false,
                    )),
                    last_period_day_counter: Some(Actual360::with_last_day(true)),
                    trade_date: Some(today()),
                    ..CdsTerms::default()
                },
                Shared::clone(&fixture.settings),
            )
            .unwrap();
            let engine = MidPointCdsEngine::new(
                curve.clone(),
                RECOVERY_RATE,
                fixture.discount.clone(),
                Some(true),
                Shared::clone(&fixture.settings),
            );
            cds.base_mut()
                .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

            let computed = cds.fair_upfront().unwrap();
            assert!(
                (computed - quote).abs() <= TOLERANCE,
                "failed to reproduce the fair upfront for the {n}Y credit-default swap: \
                 computed {computed}, input {quote}"
            );
        }
        fixture.settings.set_include_todays_cash_flows(restore);
    }

    /// `testBootstrapFromUpfront<HazardRate, BackwardFlat>` (`:230-317`).
    #[test]
    fn bootstrapped_curve_reproduces_the_input_cds_upfronts() {
        assert_the_upfronts_reproduce(&upfront_fixture(&settings_at(today())));
    }

    /// `testUpfrontBootstrap` (`:380-395`): the bootstrap runs with the
    /// caller's flag switched off and leaves it switched off afterwards.
    ///
    /// The bootstrap is forced before the reprice, which runs under a flag of
    /// its own: a curve first read inside that loop would be bootstrapped under
    /// the loop's `true`, and the assertion below would pin the loop's unwind
    /// rather than the helper's. C++ leaves it to the loop, so this is the
    /// stricter of the two.
    ///
    /// What this pins is the unwind, not the write. The C++ comment claims a
    /// `false` flag "would prevent the upfront from being used", but the flag
    /// only decides flows dated *on* the evaluation date
    /// (`cashflow.rs:94-113`), and this fixture has none: the upfront and its
    /// rebate settle three business days out and the first coupon later still.
    /// Removing the write from `implied_quote` leaves both upfront tests green.
    /// The write is ported for fidelity and pinned for its unwind
    /// (`the_cash_flow_flag_is_restored_when_the_contract_cannot_price`,
    /// defaultprobabilityhelpers.rs); no oracle here discriminates it.
    #[test]
    fn the_upfront_bootstrap_leaves_the_cash_flow_flag_as_it_found_it() {
        let settings = settings_at(today());
        settings.set_include_todays_cash_flows(Some(false));
        let fixture = upfront_fixture(&settings);

        fixture.curve.calculate().unwrap();
        assert_eq!(
            settings.include_todays_cash_flows(),
            Some(false),
            "the helper's own write must be unwound, not left on the caller's settings"
        );
        assert_the_upfronts_reproduce(&fixture);
        assert_eq!(settings.include_todays_cash_flows(), Some(false));
    }
}
