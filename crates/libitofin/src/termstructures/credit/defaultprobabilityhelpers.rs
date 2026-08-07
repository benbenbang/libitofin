//! Bootstrap helpers for default-probability term structures.
//!
//! Port of the two typedefs at the head of
//! `ql/termstructures/credit/defaultprobabilityhelpers.hpp:41-44`:
//! `DefaultProbabilityHelper` is `BootstrapHelper<DefaultProbabilityTermStructure>`
//! and `RelativeDateDefaultProbabilityHelper` is
//! `RelativeDateBootstrapHelper<DefaultProbabilityTermStructure>`. They are the
//! credit twins of
//! [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper) and
//! [`RelativeDateRateHelper`](crate::termstructures::bootstraphelper::RelativeDateRateHelper),
//! and they carry no behaviour of their own: everything is inherited from the
//! shared [`BootstrapHelperBase`], instantiated here over
//! [`DefaultProbabilityTermStructure`] instead of the yield curve.
//!
//! Where C++ gets both families from one class template, this port needs two
//! traits, because the yield layer is typed on the bare `dyn RateHelper` object
//! and a trait generic over its term structure cannot be made into one. The
//! shared driver reaches both through [`BootstrapHelperShared`], implemented
//! below on `dyn DefaultProbabilityHelper`.
//!
//! [`SpreadCdsHelper`] follows them: the running-spread CDS helper the credit
//! bootstrap is driven by. `UpfrontCdsHelper` (`defaultprobabilityhelpers.hpp:170`)
//! is not here yet and follows within EPIC Credit (#676).

use std::cell::{Cell, Ref, RefCell};
use std::rc::Weak;

use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::instrument::Instrument;
use crate::instruments::{CdsTerms, CreditDefaultSwap, PricingModel, ProtectionSide, cds_maturity};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::credit::MidPointCdsEngine;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, BootstrapHelperShared};
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::{MakeSchedule, Schedule};
use crate::types::{Integer, Real};
use crate::{fail, require};

/// The shared state of a credit bootstrap helper: a
/// [`BootstrapHelperBase`] whose back-pointer is a default-probability curve.
pub type DefaultProbabilityHelperBase = BootstrapHelperBase<dyn DefaultProbabilityTermStructure>;

/// Bootstrap helper for the credit-curve bootstrap
/// (`DefaultProbabilityHelper`).
///
/// Mirrors [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper)
/// exactly, over [`DefaultProbabilityTermStructure`]: a concrete helper embeds
/// a [`DefaultProbabilityHelperBase`], returns it from [`base`](Self::base) and
/// supplies [`implied_quote`](Self::implied_quote); the rest of the interface is
/// derived from the base. The same ownership contract holds - the curve is held
/// [`Weak`](std::rc::Weak) and never observed - since it is the one base that
/// enforces it.
pub trait DefaultProbabilityHelper: AsObservable {
    /// The embedded shared state.
    fn base(&self) -> &DefaultProbabilityHelperBase;

    /// The quote implied by the current curve, computed by the concrete helper.
    ///
    /// The helper does not observe the curve, so this must force any
    /// recalculation it needs itself rather than trusting a cached value.
    fn implied_quote(&self) -> QlResult<Real>;

    /// The market quote the helper fits the curve to.
    fn quote(&self) -> &Handle<dyn Quote> {
        self.base().quote()
    }

    /// The bootstrap's root: market quote minus implied quote, driven to zero.
    fn quote_error(&self) -> QlResult<Real> {
        Ok(self.base().quote_value()? - self.implied_quote()?)
    }

    /// Sets the curve being bootstrapped (non-owning, unobserved).
    ///
    /// A concrete helper that hands the curve to a pricing engine overrides
    /// this to relink that handle first, then delegates here.
    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        self.base().set_term_structure(term_structure);
    }

    /// The earliest date data are needed at.
    fn earliest_date(&self) -> Date {
        self.base().earliest_date()
    }

    /// The instrument's maturity date.
    fn maturity_date(&self) -> Date {
        self.base().maturity_date()
    }

    /// The latest date data are needed at.
    fn latest_relevant_date(&self) -> Date {
        self.base().latest_relevant_date()
    }

    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> Date {
        self.base().pillar_date()
    }

    /// The latest date, equal to the pillar date.
    fn latest_date(&self) -> Date {
        self.base().latest_date()
    }
}

/// Credit bootstrap helper whose date schedule is relative to the evaluation
/// date (`RelativeDateDefaultProbabilityHelper`).
///
/// `CdsHelper` derives from this: a CDS schedule is rebuilt whenever the
/// evaluation date moves. The concrete helper builds its base with
/// [`BootstrapHelperBase::new_relative`], passing a closure that calls
/// [`initialize_dates`](Self::initialize_dates).
pub trait RelativeDateDefaultProbabilityHelper: DefaultProbabilityHelper {
    /// Rebuilds the helper's date schedule off the current evaluation date.
    ///
    /// Fallible where the yield family's is not: a CDS schedule's maturity
    /// comes from [`cds_maturity`], which rejects a tenor it cannot roll, and
    /// the bootstrap must carry that out rather than unwrap it (D4).
    ///
    /// # Errors
    ///
    /// As the concrete helper's date rule.
    fn initialize_dates(&self) -> QlResult<()>;
}

/// The credit half of the driver bound. Like the yield impl, every method
/// routes through the [`DefaultProbabilityHelper`] trait rather than straight
/// to the base, so a concrete helper's override still runs.
impl BootstrapHelperShared for dyn DefaultProbabilityHelper {
    type TS = dyn DefaultProbabilityTermStructure;

    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        DefaultProbabilityHelper::set_term_structure(self, term_structure);
    }

    fn quote_value(&self) -> QlResult<Real> {
        self.base().quote_value()
    }

    fn quote_error(&self) -> QlResult<Real> {
        DefaultProbabilityHelper::quote_error(self)
    }

    fn pillar_date(&self) -> Date {
        DefaultProbabilityHelper::pillar_date(self)
    }

    fn latest_relevant_date(&self) -> Date {
        DefaultProbabilityHelper::latest_relevant_date(self)
    }

    fn maturity_date(&self) -> Date {
        DefaultProbabilityHelper::maturity_date(self)
    }
}

/// The terms a CDS bootstrap helper defaults when they are not quoted.
///
/// One field per defaulted argument of the C++ `CdsHelper` constructor
/// (`defaultprobabilityhelpers.hpp:90-95`); [`Default`] carries their C++
/// values.
pub struct CdsHelperTerms {
    /// The model the helper prices its contract under.
    pub model: PricingModel,
    /// Whether the accrued coupon is due on a default.
    pub settles_accrual: bool,
    /// Whether a default pays at default time rather than at the end of the
    /// accrual period.
    pub pays_at_default_time: bool,
    /// An explicit schedule start, for an off-the-run contract; the protection
    /// start when absent.
    pub start_date: Option<Date>,
    /// The day counter the last coupon accrues with, overriding the spread's.
    pub last_period_day_counter: Option<DayCounter>,
    /// Whether the protection seller rebates the accrued current coupon.
    pub rebates_accrual: bool,
}

impl Default for CdsHelperTerms {
    fn default() -> CdsHelperTerms {
        CdsHelperTerms {
            model: PricingModel::Midpoint,
            settles_accrual: true,
            pays_at_default_time: true,
            start_date: None,
            last_period_day_counter: None,
            rebates_accrual: true,
        }
    }
}

/// The state and dates every CDS bootstrap helper shares (`CdsHelper`,
/// `defaultprobabilityhelpers.hpp:48-128`).
///
/// C++ makes this an abstract class between the relative-date helper and the
/// two quoted contracts; here it is the state those contracts embed. A concrete
/// helper holds one, reports its [`base`](CdsHelperBase::base) as its own, and
/// supplies the two members C++ leaves virtual: the contract `reset_engine`
/// builds and [`implied_quote`](DefaultProbabilityHelper::implied_quote).
pub struct CdsHelperBase {
    base: DefaultProbabilityHelperBase,
    tenor: Period,
    settlement_days: Integer,
    calendar: Calendar,
    frequency: Frequency,
    payment_convention: BusinessDayConvention,
    rule: DateGeneration,
    day_counter: DayCounter,
    recovery_rate: Real,
    discount_curve: Handle<dyn YieldTermStructure>,
    settles_accrual: bool,
    pays_at_default_time: bool,
    start_date: Option<Date>,
    last_period_day_counter: Option<DayCounter>,
    rebates_accrual: bool,
    model: PricingModel,
    settings: Shared<Settings<Date>>,
    schedule: RefCell<Schedule>,
    protection_start: Cell<Date>,
    probability: RelinkableHandle<dyn DefaultProbabilityTermStructure>,
    swap: RefCell<QlResult<CreditDefaultSwap>>,
}

/// The state the helper is in before a curve has been handed to it: C++ leaves
/// `swap_` null there and would dereference it, where this reports the reason.
fn engine_not_reset() -> QlError {
    QlError::new(
        "the helper's credit default swap is built when the bootstrapping curve is set",
        file!(),
        line!(),
    )
}

impl CdsHelperBase {
    /// The C++ `CdsHelper` constructor (`defaultprobabilityhelpers.cpp:32-58`),
    /// less the `initializeDates` its callers run once the concrete helper
    /// exists to hold this.
    ///
    /// `on_eval_change` is the rebuild the base runs when the evaluation date
    /// moves; it reaches the concrete helper, since the contract it rebuilds is
    /// the subclass's to build.
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: Integer,
        calendar: Calendar,
        frequency: Frequency,
        payment_convention: BusinessDayConvention,
        rule: DateGeneration,
        day_counter: DayCounter,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        terms: CdsHelperTerms,
        settings: Shared<Settings<Date>>,
        on_eval_change: Box<dyn Fn()>,
    ) -> CdsHelperBase {
        let base = BootstrapHelperBase::new_relative(
            quote,
            Shared::clone(&settings),
            true,
            on_eval_change,
        );
        discount_curve.register_observer(&base.observer());
        CdsHelperBase {
            base,
            tenor,
            settlement_days,
            calendar,
            frequency,
            payment_convention,
            rule,
            day_counter,
            recovery_rate,
            discount_curve,
            settles_accrual: terms.settles_accrual,
            pays_at_default_time: terms.pays_at_default_time,
            start_date: terms.start_date,
            last_period_day_counter: terms.last_period_day_counter,
            rebates_accrual: terms.rebates_accrual,
            model: terms.model,
            settings,
            schedule: RefCell::new(Schedule::from_dates(Vec::new())),
            protection_start: Cell::new(Date::null()),
            probability: RelinkableHandle::empty(),
            swap: RefCell::new(Err(engine_not_reset())),
        }
    }

    /// The embedded bootstrap-helper state.
    pub fn base(&self) -> &DefaultProbabilityHelperBase {
        &self.base
    }

    /// The contract the helper prices (`swap()`, `hpp:98-100`), or the reason
    /// it could not be built.
    pub fn swap(&self) -> Ref<'_, QlResult<CreditDefaultSwap>> {
        self.swap.borrow()
    }

    /// The schedule the contract pays on (`schedule_`, `hpp:126`).
    pub fn schedule(&self) -> Ref<'_, Schedule> {
        self.schedule.borrow()
    }

    /// The date protection starts, `settlement_days` past the evaluation date
    /// (`cpp:79`).
    pub fn protection_start(&self) -> Date {
        self.protection_start.get()
    }

    /// The evaluation date the schedule is measured from.
    fn evaluation_date(&self) -> Date {
        self.base
            .evaluation_date()
            .expect("a relative-date helper always tracks an evaluation date")
    }

    /// Stores the rebuilt contract, or the reason it could not be built.
    fn set_swap(&self, swap: QlResult<CreditDefaultSwap>) {
        *self.swap.borrow_mut() = swap;
    }

    /// Records the curve and hands it to the engine (`setTermStructure`,
    /// `cpp:60-68`), leaving the contract rebuild to the caller.
    ///
    /// The engine's handle is linked weakly, the port of the C++
    /// `linkTo(..., false)` at `cpp:63-65`: the curve owns this helper, which
    /// owns the contract, which owns the engine, and a strong link would close
    /// that ring.
    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        self.base.set_term_structure(term_structure);
        self.probability
            .link_to_weak(Shared::downgrade(term_structure));
    }

    /// The terms the priced contract inherits from the helper.
    ///
    /// The trade date is the evaluation date, as C++ passes it
    /// (`cpp:141` and `cpp:201`), rather than the contract's own deduction: it
    /// is what the accrual rebate accrues to, so leaving it to the contract
    /// would rebate a different amount than the quoted market convention.
    fn contract_terms(&self) -> CdsTerms {
        CdsTerms {
            settles_accrual: self.settles_accrual,
            pays_at_default_time: self.pays_at_default_time,
            protection_start: Some(self.protection_start.get()),
            last_period_day_counter: self.last_period_day_counter.clone(),
            rebates_accrual: self.rebates_accrual,
            trade_date: Some(self.evaluation_date()),
            ..CdsTerms::default()
        }
    }

    /// Installs the model's engine over the helper's own probability handle
    /// (`resetEngine`'s switch, `cpp:143-153`).
    ///
    /// # Errors
    ///
    /// [`PricingModel::Isda`] needs the `IsdaCdsEngine` deferred to #783; the
    /// arm is refused rather than silently priced on the mid-point engine.
    fn install_engine(&self, swap: &mut CreditDefaultSwap) -> QlResult<()> {
        require!(
            self.model == PricingModel::Midpoint,
            "the ISDA arm of resetEngine (defaultprobabilityhelpers.cpp:143-148) needs the \
             IsdaCdsEngine, which is not ported yet (#783)"
        );
        let engine = MidPointCdsEngine::new(
            self.probability.handle(),
            self.recovery_rate,
            self.discount_curve.clone(),
            None,
            Shared::clone(&self.settings),
        );
        swap.base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        Ok(())
    }

    /// Rebuilds the schedule off the current evaluation date (`initializeDates`,
    /// `cpp:75-108`).
    ///
    /// Protection starts `settlement_days` past the evaluation date and, absent
    /// an explicit start, the schedule starts there too, rolled to a business
    /// day for every rule but the two post-Big-Bang ones. The maturity has two
    /// arms: the three CDS rules roll it off [`cds_maturity`] measured from the
    /// evaluation date (`cpp:86-88`), every other rule measures a tenor from the
    /// protection start (`cpp:90-92`) - a different reference date, not just a
    /// different roll.
    ///
    /// The earliest date is the schedule's first date and the latest its last,
    /// rolled, plus the day the ISDA model adds (`cpp:105-106`).
    ///
    /// # Errors
    ///
    /// If [`cds_maturity`] refuses the tenor, or rolls it to a contract that has
    /// already matured.
    fn initialize_dates(&self) -> QlResult<()> {
        let evaluation_date = self.evaluation_date();
        let protection_start = evaluation_date + self.settlement_days;
        self.protection_start.set(protection_start);

        let mut start_date = self.start_date.unwrap_or(protection_start);
        if self.rule != DateGeneration::CDS && self.rule != DateGeneration::CDS2015 {
            start_date = self.calendar.adjust(start_date, self.payment_convention);
        }

        let end_date = if matches!(
            self.rule,
            DateGeneration::CDS2015 | DateGeneration::CDS | DateGeneration::OldCDS
        ) {
            let reference_date = self.start_date.unwrap_or(evaluation_date);
            match cds_maturity(reference_date, self.tenor, self.rule)? {
                Some(date) => date,
                None => fail!(
                    "the CDS2015 contract quoted at a zero tenor on {reference_date} has already \
                     matured (creditdefaultswap.cpp:494)"
                ),
            }
        } else {
            let reference_date = match self.start_date {
                Some(date) => date + self.settlement_days,
                None => protection_start,
            };
            reference_date + self.tenor
        };

        let schedule = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_frequency(self.frequency)
            .with_calendar(self.calendar.clone())
            .with_convention(self.payment_convention)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(self.rule)
            .build();

        let mut latest_date = self
            .calendar
            .adjust(schedule.date(schedule.len() - 1), self.payment_convention);
        if self.model == PricingModel::Isda {
            latest_date += 1;
        }
        self.base.set_earliest_date(schedule.date(0));
        self.base.set_latest_date(latest_date);
        *self.schedule.borrow_mut() = schedule;
        Ok(())
    }
}

/// A spread-quoted CDS as a credit bootstrap helper (`SpreadCdsHelper`,
/// `defaultprobabilityhelpers.hpp:129`).
///
/// The helper prices a par CDS on its own schedule against the curve being
/// bootstrapped and reports that contract's fair spread as
/// [`implied_quote`](DefaultProbabilityHelper::implied_quote); the bootstrap
/// drives `quoted spread - fair spread` to zero.
pub struct SpreadCdsHelper {
    cds: CdsHelperBase,
}

impl SpreadCdsHelper {
    /// A helper on the C++ default terms (`defaultprobabilityhelpers.cpp:109`).
    ///
    /// The C++ quote is a `std::variant<Rate, Handle<Quote>>` (`hpp:129`); a
    /// quoted spread takes the `Rate` arm here as
    /// [`make_quote_handle(spread).handle()`](crate::quotes::make_quote_handle).
    ///
    /// # Errors
    ///
    /// As [`with_terms`](SpreadCdsHelper::with_terms).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        running_spread: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: Integer,
        calendar: Calendar,
        frequency: Frequency,
        payment_convention: BusinessDayConvention,
        rule: DateGeneration,
        day_counter: DayCounter,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<SpreadCdsHelper>> {
        SpreadCdsHelper::with_terms(
            running_spread,
            tenor,
            settlement_days,
            calendar,
            frequency,
            payment_convention,
            rule,
            day_counter,
            recovery_rate,
            discount_curve,
            CdsHelperTerms::default(),
            settings,
        )
    }

    /// A helper on the given `terms` (`defaultprobabilityhelpers.cpp:40-58`).
    ///
    /// The helper observes its quote and its discount curve (the constructor's
    /// `registerWith(discountCurve)`, `cpp:57`) and tracks the evaluation date,
    /// rebuilding its schedule and its contract whenever that date moves.
    ///
    /// # Errors
    ///
    /// If the date rule cannot roll the tenor to a maturity
    /// ([`initialize_dates`](CdsHelperBase::initialize_dates)).
    #[allow(clippy::too_many_arguments)]
    pub fn with_terms(
        running_spread: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: Integer,
        calendar: Calendar,
        frequency: Frequency,
        payment_convention: BusinessDayConvention,
        rule: DateGeneration,
        day_counter: DayCounter,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        terms: CdsHelperTerms,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<SpreadCdsHelper>> {
        let helper = Shared::new_cyclic(|weak: &Weak<SpreadCdsHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    match helper.cds.initialize_dates() {
                        Ok(()) => helper.reset_engine(),
                        Err(error) => helper.cds.set_swap(Err(error)),
                    }
                }
            });
            SpreadCdsHelper {
                cds: CdsHelperBase::new(
                    running_spread,
                    tenor,
                    settlement_days,
                    calendar,
                    frequency,
                    payment_convention,
                    rule,
                    day_counter,
                    recovery_rate,
                    discount_curve,
                    terms,
                    settings,
                    on_eval_change,
                ),
            }
        });
        helper.cds.initialize_dates()?;
        Ok(helper)
    }

    /// The date protection starts, `settlement_days` past the evaluation date
    /// (`cpp:79`).
    pub fn protection_start(&self) -> Date {
        self.cds.protection_start()
    }

    /// Rebuilds the priced contract and its engine (`resetEngine`, `cpp:137-153`).
    ///
    /// Called from [`set_term_structure`](DefaultProbabilityHelper::set_term_structure)
    /// and, after [`initialize_dates`](RelativeDateDefaultProbabilityHelper::initialize_dates),
    /// on an evaluation-date move - the two C++ call sites (`cpp:67` and
    /// `cpp:72`). The order matters on the second: the contract is built from the
    /// schedule and protection start, so rebuilding it first would price the
    /// stale schedule.
    ///
    /// C++ calls this on *every* notification `update()` carries, where the
    /// evaluation-date guard sits inside `RelativeDateBootstrapHelper::update()`;
    /// this port has that guard in the base and hooks the rebuild to it. The
    /// unconditional arm rebuilds an identical contract - its notional and
    /// spread are the fixed `100.0` and `0.01`, never the quote - and a discount
    /// or curve move still reaches the contract through the engine it observes.
    ///
    /// A contract that cannot be built is stored as the error and surfaces at
    /// [`implied_quote`](DefaultProbabilityHelper::implied_quote), since the C++
    /// signature this mirrors returns nothing.
    fn reset_engine(&self) {
        self.cds.set_swap(self.build_swap());
    }

    /// The par contract the helper prices: a protection-buyer CDS on a notional
    /// of 100 paying a 1% running spread (`cpp:138-141`), under a fresh midpoint
    /// engine over the helper's own probability handle (`cpp:151-152`).
    fn build_swap(&self) -> QlResult<CreditDefaultSwap> {
        let mut swap = CreditDefaultSwap::with_terms(
            ProtectionSide::Buyer,
            100.0,
            0.01,
            self.cds.schedule().clone(),
            self.cds.payment_convention,
            self.cds.day_counter.clone(),
            self.cds.contract_terms(),
            Shared::clone(&self.cds.settings),
        )?;
        self.cds.install_engine(&mut swap)?;
        Ok(swap)
    }
}

impl AsObservable for SpreadCdsHelper {
    fn observable(&self) -> &Observable {
        self.cds.base.observable()
    }
}

impl DefaultProbabilityHelper for SpreadCdsHelper {
    fn base(&self) -> &DefaultProbabilityHelperBase {
        self.cds.base()
    }

    /// The contract's fair spread (`impliedQuote`, `cpp:132-135`).
    ///
    /// The recalculation is forced rather than left to the cache: the helper
    /// weak-links the curve handle the engine prices over, so a node the
    /// bootstrap has just moved does not notify the contract, and a plain
    /// `fair_spread` would answer from the previous iteration's results.
    fn implied_quote(&self) -> QlResult<Real> {
        let mut swap = self.cds.swap.borrow_mut();
        let swap = swap.as_mut().map_err(|error| error.clone())?;
        swap.recalculate()?;
        swap.fair_spread()
    }

    /// Records the curve, hands it to the engine, and rebuilds the contract
    /// (`setTermStructure`, `cpp:60-68`).
    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        self.cds.set_term_structure(term_structure);
        self.reset_engine();
    }
}

impl RelativeDateDefaultProbabilityHelper for SpreadCdsHelper {
    fn initialize_dates(&self) -> QlResult<()> {
        self.cds.initialize_dates()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credit family satisfies the bound the bootstrap driver puts on
    /// `PiecewiseCurve::Helper`, so a credit piecewise curve can name
    /// `dyn DefaultProbabilityHelper` there against a
    /// `dyn DefaultProbabilityTermStructure` curve. The two associated types
    /// must agree, which is the whole point of the generalization, and only a
    /// paired instantiation checks it. This is a compile-time assertion: the
    /// credit bootstrap itself ports no behaviour yet.
    #[test]
    fn credit_helpers_satisfy_the_driver_bound() {
        fn accepts_driver_helper<H>()
        where
            H: BootstrapHelperShared<TS = dyn DefaultProbabilityTermStructure> + ?Sized,
        {
        }
        accepts_driver_helper::<dyn DefaultProbabilityHelper>();
    }

    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::timeunit::TimeUnit;

    /// A Monday, so the schedule's start is a business day and the roll the
    /// helper applies to it is visible as the identity it is.
    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn five_years() -> Period {
        Period::new(5, TimeUnit::Years)
    }

    fn settings_at(evaluation_date: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(evaluation_date);
        settings
    }

    fn discount(settlement: Date) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            settlement,
            0.03,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// A one-settlement-day helper on a five-year `TwentiethIMM` schedule, the
    /// pre-Big-Bang convention the ported date arm serves.
    fn helper(settings: &Shared<Settings<Date>>, terms: CdsHelperTerms) -> Shared<SpreadCdsHelper> {
        SpreadCdsHelper::with_terms(
            Handle::new(shared(SimpleQuote::new(0.01)) as Shared<dyn Quote>),
            five_years(),
            1,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::Following,
            DateGeneration::TwentiethIMM,
            Actual360::new(),
            0.4,
            discount(today()),
            terms,
            Shared::clone(settings),
        )
        .unwrap()
    }

    /// The schedule `initialize_dates` is expected to have built, derived from
    /// the two dates the C++ arm computes rather than read back off the helper.
    fn expected_schedule(start_date: Date, end_date: Date) -> Schedule {
        MakeSchedule::new()
            .from(Target::new().adjust(start_date, BusinessDayConvention::Following))
            .to(end_date)
            .with_frequency(Frequency::Quarterly)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(DateGeneration::TwentiethIMM)
            .build()
    }

    fn last_date(schedule: &Schedule) -> Date {
        schedule.date(schedule.len() - 1)
    }

    /// `initializeDates` (`defaultprobabilityhelpers.cpp:75-108`): protection
    /// starts `settlementDays` past the evaluation date, the schedule starts
    /// there rolled, and the maturity is the tenor past the protection start -
    /// the `cdsMaturity` arm (`cpp:86-88`) covers only the rejected CDS rules,
    /// so `TwentiethIMM` reaches this one.
    #[test]
    fn initialize_dates_spans_protection_start_to_the_tenor() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());
        let calendar = Target::new();

        let protection_start = today() + 1;
        assert_eq!(helper.protection_start(), protection_start);

        let schedule = expected_schedule(protection_start, protection_start + five_years());
        assert_eq!(
            helper.earliest_date(),
            calendar.adjust(protection_start, BusinessDayConvention::Following)
        );
        assert_eq!(helper.earliest_date(), schedule.date(0));
        assert_eq!(
            helper.latest_date(),
            calendar.adjust(last_date(&schedule), BusinessDayConvention::Following)
        );
    }

    /// The latest date is the rolled last coupon date and nothing more: C++ adds
    /// a day only under the ISDA model (`cpp:105-106`), which is not ported.
    /// Pillar and latest-relevant date follow it, which is what the bootstrap
    /// driver orders the helpers on.
    #[test]
    fn the_node_sits_on_the_rolled_maturity() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());

        let schedule = expected_schedule(today() + 1, today() + 1 + five_years());
        let rolled = Target::new().adjust(last_date(&schedule), BusinessDayConvention::Following);
        assert_eq!(helper.latest_date(), rolled);
        assert_eq!(helper.pillar_date(), rolled);
        assert_eq!(helper.latest_relevant_date(), rolled);
        assert_eq!(helper.maturity_date(), rolled);
    }

    /// An explicit start date replaces the protection start on both sides of the
    /// schedule, but the maturity is measured from `startDate + settlementDays`
    /// rather than from the start date itself (`cpp:90`) - the one place the two
    /// arms of that ternary differ by more than the branch they take.
    #[test]
    fn an_explicit_start_date_offsets_the_maturity_by_the_settlement_days() {
        let settings = settings_at(today());
        let start_date = Date::new(20, Month::March, 2026);
        let helper = helper(
            &settings,
            CdsHelperTerms {
                start_date: Some(start_date),
                ..CdsHelperTerms::default()
            },
        );

        let schedule = expected_schedule(start_date, start_date + 1 + five_years());
        assert_eq!(helper.earliest_date(), schedule.date(0));
        assert_eq!(
            helper.latest_date(),
            Target::new().adjust(last_date(&schedule), BusinessDayConvention::Following)
        );
        assert_eq!(helper.protection_start(), today() + 1);
    }

    /// The relative-date rebuild (`cpp:70-73`): a moved evaluation date reruns
    /// `initialize_dates` through the base's `new_relative` closure, so the whole
    /// schedule shifts with it.
    #[test]
    fn an_evaluation_date_move_rebuilds_the_schedule() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());
        let (earliest, latest) = (helper.earliest_date(), helper.latest_date());

        let moved = Date::new(15, Month::December, 2026);
        settings.set_evaluation_date(moved);

        assert_eq!(helper.protection_start(), moved + 1);
        assert!(helper.earliest_date() > earliest);
        assert!(helper.latest_date() > latest);
        assert_eq!(
            helper.earliest_date(),
            Target::new().adjust(moved + 1, BusinessDayConvention::Following)
        );
    }

    /// The `cdsMaturity` arm of `initializeDates` (`cpp:85-88`): under the three
    /// post-Big-Bang rules the maturity is rolled off the *evaluation date*,
    /// where the other arm measures a tenor from the protection start, and the
    /// schedule runs from the previous IMM twentieth to it.
    ///
    /// Every date here is derived by hand from the rule - the twentieth of
    /// March, June, September or December at or before the trade date, plus the
    /// tenor, plus a quarter - rather than read back from [`cds_maturity`]. The
    /// helper calls that function itself, so an equality against it would hold
    /// for whatever maturity it returned. The round-trip bootstrap cannot stand
    /// in either: it rebuilds its contract on the same conventions, so a shared
    /// wrong schedule still reprices to the quote.
    #[test]
    fn the_cds_rules_roll_the_maturity_off_the_evaluation_date() {
        let settings = settings_at(today());
        let calendar = Target::new();
        let anchor = Date::new(20, Month::March, 2026);

        for (years, maturity) in [
            (2, Date::new(20, Month::June, 2028)),
            (3, Date::new(20, Month::June, 2029)),
            (5, Date::new(20, Month::June, 2031)),
            (7, Date::new(20, Month::June, 2033)),
        ] {
            let helper = SpreadCdsHelper::new(
                Handle::new(shared(SimpleQuote::new(0.01)) as Shared<dyn Quote>),
                Period::new(years, TimeUnit::Years),
                1,
                calendar.clone(),
                Frequency::Quarterly,
                BusinessDayConvention::Following,
                DateGeneration::CDS,
                Actual360::new(),
                0.4,
                discount(today()),
                Shared::clone(&settings),
            )
            .unwrap();

            let schedule = helper.cds.schedule();
            assert_eq!(schedule.date(0), anchor);
            assert_eq!(last_date(&schedule), maturity);
            assert_eq!(helper.earliest_date(), anchor);
            assert_eq!(
                helper.latest_date(),
                calendar.adjust(maturity, BusinessDayConvention::Following)
            );
            assert_eq!(helper.protection_start(), today() + 1);
        }
    }

    /// `impliedQuote` (`cpp:132-135`) prices the helper's own contract against
    /// the curve it was handed, which `set_term_structure` weak-links into the
    /// engine and rebuilds the contract for (`cpp:60-68`).
    ///
    /// The forced recalculation is load-bearing, and this is what shows it: the
    /// weak link registers no observer on the curve
    /// ([`Link::link_weak`](crate::handle)), so moving the curve leaves the
    /// contract still flagged as calculated. A plain `fair_spread` would answer
    /// from that stale cache; the fresh number can only come from the
    /// `recalculate` this makes first. During the bootstrap it is the solver
    /// moving a curve node, which reaches the contract the same way: not at all.
    #[test]
    fn implied_quote_reprices_a_curve_it_does_not_observe() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());

        let hazard = shared(SimpleQuote::new(0.02));
        let curve: Shared<dyn DefaultProbabilityTermStructure> = shared(FlatHazardRate::new(
            today(),
            Handle::new(Shared::clone(&hazard) as Shared<dyn Quote>),
            Actual365Fixed::new(),
        ));
        helper.set_term_structure(&curve);

        let first = helper.implied_quote().unwrap();
        assert!(first.is_finite() && first > 0.0);
        assert_eq!(helper.implied_quote().unwrap(), first);

        hazard.set_value(0.05);
        assert!(
            helper.cds.swap().as_ref().unwrap().base().is_calculated(),
            "the weak link must leave the contract unnotified by the curve"
        );

        let second = helper.implied_quote().unwrap();
        assert!(
            second > first,
            "a higher hazard rate must widen the fair spread, not repeat {first}"
        );
    }

    /// The second `resetEngine` call site (`cpp:72`): the contract is rebuilt
    /// after the schedule is, so the helper prices the moved dates rather than
    /// the ones it was handed the curve on. Nothing else in the type system
    /// enforces that ordering, and a contract left behind still prices - just
    /// against the wrong schedule.
    #[test]
    fn an_evaluation_date_move_rebuilds_the_contract_too() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());
        let curve: Shared<dyn DefaultProbabilityTermStructure> = shared(FlatHazardRate::with_rate(
            today(),
            0.02,
            Actual365Fixed::new(),
        ));
        helper.set_term_structure(&curve);
        helper.implied_quote().unwrap();

        let moved = Date::new(15, Month::December, 2026);
        settings.set_evaluation_date(moved);

        let schedule = expected_schedule(moved + 1, moved + 1 + five_years());
        let swap = helper.cds.swap();
        let swap = swap.as_ref().unwrap();
        assert_eq!(swap.protection_start_date(), moved + 1);
        assert_eq!(swap.maturity(), last_date(&schedule));
    }

    /// `registerWith(discountCurve)` (`cpp:57`): the discount curve is the one
    /// input the helper prices against that it does observe - unlike the
    /// bootstrapping curve - so a move there re-broadcasts and reaches the curve
    /// being built. This is what makes it safe for the rebuild to hang off the
    /// evaluation date alone.
    #[test]
    fn a_discount_curve_move_notifies_the_helper() {
        let settings = settings_at(today());
        let rate = shared(SimpleQuote::new(0.03));
        let discount_curve = Handle::new(shared(FlatForward::new(
            today(),
            Handle::new(Shared::clone(&rate) as Shared<dyn Quote>),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let helper = SpreadCdsHelper::new(
            Handle::new(shared(SimpleQuote::new(0.01)) as Shared<dyn Quote>),
            five_years(),
            1,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::Following,
            DateGeneration::TwentiethIMM,
            Actual360::new(),
            0.4,
            discount_curve,
            Shared::clone(&settings),
        )
        .unwrap();

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        rate.set_value(0.04);
        assert!(Flag::is_up(&flag));
    }

    /// Before a curve arrives there is no contract to price, where C++ would
    /// dereference its null `swap_`.
    #[test]
    fn implied_quote_without_a_curve_reports_the_missing_contract() {
        let settings = settings_at(today());
        let helper = helper(&settings, CdsHelperTerms::default());
        assert!(
            helper
                .implied_quote()
                .err()
                .is_some_and(|error| error.message().contains("bootstrapping curve is set"))
        );
    }
}
