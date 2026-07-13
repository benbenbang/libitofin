//! Bootstrap-helper base for curve construction.
//!
//! Port of `ql/termstructures/bootstraphelper.hpp`. A bootstrap helper prices
//! one market instrument against the curve being built and reports the error
//! between the market quote and the curve-implied quote; the bootstrap solves
//! each curve node until that error is zero.
//!
//! ## Divergences from QuantLib
//!
//! - C++ `BootstrapHelper<TS>` is a template over the term-structure type; the
//!   only instantiation the yield-curve bootstrap needs is
//!   `BootstrapHelper<YieldTermStructure>`, typedef'd `RateHelper`
//!   (`ratehelpers.hpp:47`). This port specializes to [`YieldTermStructure`]
//!   directly: [`RateHelper`] is the trait, with no `TS` parameter. A second
//!   term-structure family (a default-probability bootstrap) would generalize
//!   it then.
//! - The `AcyclicVisitor` `accept` hook is omitted; no visitor is ported.
//!
//! ## The ownership and observation contract
//!
//! A helper is handed the curve that is bootstrapping it through
//! [`set_term_structure`](RateHelper::set_term_structure), and holds it as a
//! [`Weak`] back-pointer. This is deliberate and load-bearing:
//!
//! 1. **Non-owning.** The curve owns its helpers; a strong [`Shared`] here
//!    would close a reference cycle and leak. C++ stores a raw `TS*` and links
//!    the concrete helper's pricing handle with a `null_deleter` shared_ptr
//!    (`ratehelpers.cpp:217`) precisely to say "borrowed, not owned"; [`Weak`]
//!    is the faithful analogue.
//! 2. **Not observed.** The helper does **not** register as an observer of the
//!    curve (C++'s `observer = false`). During a bootstrap the solver moves the
//!    curve while reading the helper; observing it back would loop forever.
//!    That is why a concrete [`implied_quote`](RateHelper::implied_quote) forces
//!    its own recalculation rather than relying on a notification. Storing a
//!    plain [`Weak`] with no `register_observer` call structurally enforces this.
//!
//! The helper *does* observe its own quote (the market rate being fitted): a
//! quote change notifies the helper's observers, which is how a re-quote
//! triggers a re-bootstrap.

use std::cell::Cell;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::Real;

/// Shared state of every bootstrap helper (`BootstrapHelper<TS>`'s members).
///
/// A concrete helper embeds one, hands it back through
/// [`base`](RateHelper::base), and inherits the whole [`RateHelper`] surface
/// from it; only [`implied_quote`](RateHelper::implied_quote) is left abstract.
/// Dates use the null [`Date`] as the "unset" sentinel, mirroring the C++
/// accessor fallback chain.
pub struct BootstrapHelperBase {
    quote: Handle<dyn Quote>,
    term_structure: RefCell<Option<Weak<dyn YieldTermStructure>>>,
    earliest_date: Cell<Date>,
    latest_date: Cell<Date>,
    maturity_date: Cell<Date>,
    latest_relevant_date: Cell<Date>,
    pillar_date: Cell<Date>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
    relative: Option<Shared<RelativeState>>,
}

/// The evaluation-date bookkeeping a [`RelativeDateRateHelper`] adds
/// (`RelativeDateBootstrapHelper`'s `evaluationDate_`/`updateDates_`): shared
/// between the base and the forwarder's reset closure so the closure can read
/// and advance the last-seen date without a self-reference.
struct RelativeState {
    settings: Shared<Settings<Date>>,
    evaluation_date: Cell<Option<Date>>,
    update_dates: bool,
}

impl BootstrapHelperBase {
    fn assemble(
        quote: Handle<dyn Quote>,
        observable: Shared<Observable>,
        forwarder: SharedMut<ResetThenNotify>,
        relative: Option<Shared<RelativeState>>,
    ) -> BootstrapHelperBase {
        quote.register_observer(&(forwarder.clone() as SharedMut<dyn Observer>));
        BootstrapHelperBase {
            quote,
            term_structure: RefCell::new(None),
            earliest_date: Cell::new(Date::null()),
            latest_date: Cell::new(Date::null()),
            maturity_date: Cell::new(Date::null()),
            latest_relevant_date: Cell::new(Date::null()),
            pillar_date: Cell::new(Date::null()),
            observable,
            forwarder,
            relative,
        }
    }

    /// Base for a helper with a fixed date schedule (the `RateHelper` typedef).
    ///
    /// Registers the helper's forwarding observer with `quote`, so a quote
    /// change re-broadcasts through [`observable`](Self::observable) - the port
    /// of the C++ constructor's `registerWith(quote_)`.
    pub fn new(quote: Handle<dyn Quote>) -> BootstrapHelperBase {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        Self::assemble(quote, observable, forwarder, None)
    }

    /// Base for a helper whose date schedule is relative to the evaluation date
    /// (the `RelativeDateRateHelper` typedef).
    ///
    /// In addition to observing the quote, the helper registers with the
    /// evaluation date (when `update_dates`) and, on a date change, runs
    /// `on_eval_change` *before* broadcasting - so observers reading the
    /// helper's dates during the notification see the rebuilt schedule. This is
    /// the port of `RelativeDateBootstrapHelper::update()`, whose control flow
    /// (reinitialize when the date moved, then notify) lives in the base while
    /// the reinitialization itself is the concrete `initializeDates()`. A
    /// concrete helper supplies `on_eval_change` as a closure that calls its own
    /// [`initialize_dates`](RelativeDateRateHelper::initialize_dates), typically
    /// through a weak self-reference.
    pub fn new_relative(
        quote: Handle<dyn Quote>,
        settings: Shared<Settings<Date>>,
        update_dates: bool,
        on_eval_change: Box<dyn Fn()>,
    ) -> BootstrapHelperBase {
        let relative = shared(RelativeState {
            settings: Shared::clone(&settings),
            evaluation_date: Cell::new(settings.evaluation_date()),
            update_dates,
        });
        let observable = shared(Observable::new());
        let forwarder = ResetThenNotify::broadcasting(Shared::clone(&observable), {
            let relative = Shared::clone(&relative);
            move || {
                if relative.update_dates {
                    let current = relative.settings.evaluation_date();
                    if current != relative.evaluation_date.get() {
                        relative.evaluation_date.set(current);
                        on_eval_change();
                    }
                }
            }
        });
        if update_dates {
            settings.register_eval_date_observer(&(forwarder.clone() as SharedMut<dyn Observer>));
        }
        Self::assemble(quote, observable, forwarder, Some(relative))
    }

    /// The market quote the helper fits the curve to.
    pub fn quote(&self) -> &Handle<dyn Quote> {
        &self.quote
    }

    /// The quote's current value, or an error if the handle is empty.
    pub fn quote_value(&self) -> QlResult<Real> {
        self.quote.current_link()?.value()
    }

    /// Stores the curve that is bootstrapping this helper as a non-owning
    /// back-pointer.
    ///
    /// The curve is held [`Weak`] (never observed), the two halves of the
    /// module's ownership contract: the curve owns its helpers, and the solver
    /// moves the curve while reading the helper.
    pub fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        *self.term_structure.borrow_mut() = Some(Shared::downgrade(term_structure));
    }

    /// The bootstrapping curve, or an error if none was set or it has been
    /// dropped (the port of C++'s `QL_REQUIRE(termStructure_ != nullptr)`).
    pub fn term_structure(&self) -> QlResult<Shared<dyn YieldTermStructure>> {
        match self
            .term_structure
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
        {
            Some(term_structure) => Ok(term_structure),
            None => crate::fail!("term structure not set to this instance of bootstrap helper"),
        }
    }

    /// The evaluation date last seen by a relative-date helper, if any.
    pub fn evaluation_date(&self) -> Option<Date> {
        self.relative
            .as_ref()
            .and_then(|relative| relative.evaluation_date.get())
    }

    /// Whether a relative-date helper rebuilds its schedule on date changes.
    pub fn update_dates(&self) -> bool {
        self.relative
            .as_ref()
            .is_some_and(|relative| relative.update_dates)
    }

    /// The earliest date data are needed at to price the instrument.
    pub fn earliest_date(&self) -> Date {
        self.earliest_date.get()
    }

    /// The instrument's maturity, falling back to the latest relevant date.
    pub fn maturity_date(&self) -> Date {
        let maturity = self.maturity_date.get();
        if maturity == Date::null() {
            self.latest_relevant_date()
        } else {
            maturity
        }
    }

    /// The latest date data are needed at, falling back to the latest date.
    pub fn latest_relevant_date(&self) -> Date {
        let latest_relevant = self.latest_relevant_date.get();
        if latest_relevant == Date::null() {
            self.latest_date()
        } else {
            latest_relevant
        }
    }

    /// The pillar date, falling back to the latest date.
    pub fn pillar_date(&self) -> Date {
        let pillar = self.pillar_date.get();
        if pillar == Date::null() {
            self.latest_date()
        } else {
            pillar
        }
    }

    /// The latest date, falling back to the pillar-date field.
    ///
    /// The fallback reads the pillar-date *field* rather than
    /// [`pillar_date`](Self::pillar_date), matching C++ and keeping the mutual
    /// fallback with `pillar_date` from recursing.
    pub fn latest_date(&self) -> Date {
        let latest = self.latest_date.get();
        if latest == Date::null() {
            self.pillar_date.get()
        } else {
            latest
        }
    }

    /// Sets the earliest date (from a concrete `initialize_dates`).
    pub fn set_earliest_date(&self, date: Date) {
        self.earliest_date.set(date);
    }

    /// Sets the latest date.
    pub fn set_latest_date(&self, date: Date) {
        self.latest_date.set(date);
    }

    /// Sets the maturity date.
    pub fn set_maturity_date(&self, date: Date) {
        self.maturity_date.set(date);
    }

    /// Sets the latest relevant date.
    pub fn set_latest_relevant_date(&self, date: Date) {
        self.latest_relevant_date.set(date);
    }

    /// Sets the pillar date.
    pub fn set_pillar_date(&self, date: Date) {
        self.pillar_date.set(date);
    }

    /// The observable the helper broadcasts quote and date changes through.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }

    /// The helper's forwarding observer, for registering with further
    /// observables.
    pub fn observer(&self) -> SharedMut<dyn Observer> {
        self.forwarder.clone() as SharedMut<dyn Observer>
    }
}

/// Bootstrap helper for the yield-curve bootstrap (`RateHelper`).
///
/// Mirrors `BootstrapHelper<YieldTermStructure>`: a concrete helper embeds a
/// [`BootstrapHelperBase`], returns it from [`base`](Self::base), and supplies
/// [`implied_quote`](Self::implied_quote); the rest of the interface is derived
/// from the base.
pub trait RateHelper: AsObservable {
    /// The embedded shared state.
    fn base(&self) -> &BootstrapHelperBase;

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
    /// A concrete helper that hands the curve to a pricing handle overrides
    /// this to relink that handle first, then delegates here.
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
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

/// Bootstrap helper whose date schedule is relative to the evaluation date
/// (`RelativeDateRateHelper`).
///
/// Deposit, FRA, swap and OIS helpers derive from this: their schedule is
/// rebuilt whenever the global evaluation date moves. The concrete helper
/// implements [`initialize_dates`](Self::initialize_dates) and, in its
/// constructor, builds the base with
/// [`BootstrapHelperBase::new_relative`], passing a closure that calls
/// `initialize_dates` so the base can rerun it when the date changes.
pub trait RelativeDateRateHelper: RateHelper {
    /// Rebuilds the helper's date schedule off the current evaluation date.
    fn initialize_dates(&self);
}

/// Orders helpers by pillar date (`detail::BootstrapHelperSorter`).
///
/// The comparator the bootstrap sorts its helpers with before solving; exposed
/// as a slice sort, the idiomatic Rust equivalent of the C++ functor.
pub fn sort_by_pillar_date(helpers: &mut [Shared<dyn RateHelper>]) {
    helpers.sort_by(compare_by_pillar_date);
}

/// Compares two helpers by pillar date, for use with `sort_by`.
pub fn compare_by_pillar_date(
    left: &Shared<dyn RateHelper>,
    right: &Shared<dyn RateHelper>,
) -> Ordering {
    left.pillar_date().cmp(&right.pillar_date())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    fn quote_handle(value: Real) -> (Shared<SimpleQuote>, Handle<dyn Quote>) {
        let quote = shared(SimpleQuote::new(value));
        let handle = Handle::new(Shared::clone(&quote) as Shared<dyn Quote>);
        (quote, handle)
    }

    /// Minimal fixed-schedule helper: a fixed implied quote and settable dates.
    struct StubHelper {
        base: BootstrapHelperBase,
        implied: Cell<Real>,
    }

    impl StubHelper {
        fn new(quote: Handle<dyn Quote>, implied: Real) -> StubHelper {
            StubHelper {
                base: BootstrapHelperBase::new(quote),
                implied: Cell::new(implied),
            }
        }
    }

    impl AsObservable for StubHelper {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl RateHelper for StubHelper {
        fn base(&self) -> &BootstrapHelperBase {
            &self.base
        }

        fn implied_quote(&self) -> QlResult<Real> {
            Ok(self.implied.get())
        }
    }

    fn a_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn a_curve() -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            a_date(),
            0.03,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    #[test]
    fn quote_error_is_market_minus_implied() {
        let (_quote, handle) = quote_handle(0.05);
        let helper = StubHelper::new(handle, 0.02);
        assert_eq!(helper.quote_error().unwrap(), 0.05 - 0.02);
    }

    #[test]
    fn quote_change_notifies_helper_observers() {
        let (quote, handle) = quote_handle(0.05);
        let helper = StubHelper::new(handle, 0.0);
        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        quote.set_value(0.06);
        assert!(Flag::is_up(&flag));
    }

    #[test]
    fn set_term_structure_does_not_own_the_curve() {
        let (_quote, handle) = quote_handle(0.05);
        let helper = StubHelper::new(handle, 0.0);

        let curve = a_curve();
        helper.set_term_structure(&curve);
        assert!(helper.base().term_structure().is_ok());

        drop(curve);
        let result = helper.base().term_structure();
        assert!(
            result.is_err(),
            "the weak back-pointer must not keep the curve alive"
        );
        assert!(
            result
                .err()
                .is_some_and(|err| err.message().contains("term structure not set"))
        );
    }

    #[test]
    fn curve_change_does_not_notify_the_helper() {
        let (_quote, helper_handle) = quote_handle(0.05);
        let helper = StubHelper::new(helper_handle, 0.0);

        let curve_quote = shared(SimpleQuote::new(0.03));
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
            a_date(),
            Handle::new(Shared::clone(&curve_quote) as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        helper.set_term_structure(&curve);

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        curve_quote.set_value(0.04);
        assert!(
            !Flag::is_up(&flag),
            "helper must not observe the bootstrapping curve"
        );
    }

    #[test]
    fn date_accessors_follow_the_fallback_chain() {
        let (_quote, handle) = quote_handle(0.05);
        let helper = StubHelper::new(handle, 0.0);
        let base = helper.base();

        assert_eq!(base.pillar_date(), Date::null());

        let pillar = a_date();
        base.set_pillar_date(pillar);
        assert_eq!(base.latest_date(), pillar);
        assert_eq!(base.latest_relevant_date(), pillar);
        assert_eq!(base.maturity_date(), pillar);

        let maturity = pillar + 30;
        base.set_maturity_date(maturity);
        assert_eq!(base.maturity_date(), maturity);
        assert_eq!(base.latest_relevant_date(), pillar);
    }

    #[test]
    fn sorter_orders_by_pillar_date() {
        let make = |pillar: Date| -> Shared<dyn RateHelper> {
            let (_quote, handle) = quote_handle(0.0);
            let helper = StubHelper::new(handle, 0.0);
            helper.base().set_pillar_date(pillar);
            shared(helper) as Shared<dyn RateHelper>
        };
        let mut helpers = vec![make(a_date() + 90), make(a_date()), make(a_date() + 30)];

        sort_by_pillar_date(&mut helpers);

        assert_eq!(helpers[0].pillar_date(), a_date());
        assert_eq!(helpers[1].pillar_date(), a_date() + 30);
        assert_eq!(helpers[2].pillar_date(), a_date() + 90);
    }

    /// Relative-date stub: `initialize_dates` sets the earliest date from the
    /// evaluation date, so a date change is visible through the accessor.
    struct RelativeStub {
        base: BootstrapHelperBase,
    }

    impl RelativeStub {
        fn new(quote: Handle<dyn Quote>, settings: Shared<Settings<Date>>) -> Shared<RelativeStub> {
            Shared::new_cyclic(|weak: &Weak<RelativeStub>| {
                let weak = weak.clone();
                let on_eval_change = Box::new(move || {
                    if let Some(helper) = weak.upgrade() {
                        helper.initialize_dates();
                    }
                });
                RelativeStub {
                    base: BootstrapHelperBase::new_relative(quote, settings, true, on_eval_change),
                }
            })
        }
    }

    impl AsObservable for RelativeStub {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl RateHelper for RelativeStub {
        fn base(&self) -> &BootstrapHelperBase {
            &self.base
        }

        fn implied_quote(&self) -> QlResult<Real> {
            Ok(0.0)
        }
    }

    impl RelativeDateRateHelper for RelativeStub {
        fn initialize_dates(&self) {
            if let Some(date) = self.base.evaluation_date() {
                self.base.set_earliest_date(date);
            }
        }
    }

    #[test]
    fn evaluation_date_change_reinitializes_a_relative_helper() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let helper = RelativeStub::new(quote_handle(0.05).1, Shared::clone(&settings));

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        let moved = Date::new(16, Month::January, 2026);
        settings.set_evaluation_date(moved);

        assert!(Flag::is_up(&flag), "date change must notify observers");
        assert_eq!(
            helper.earliest_date(),
            moved,
            "date change must rerun initialize_dates"
        );
    }

    #[test]
    fn unchanged_evaluation_date_leaves_the_schedule_alone() {
        let settings = shared(Settings::new());
        let start = Date::new(15, Month::January, 2026);
        settings.set_evaluation_date(start);
        let helper = RelativeStub::new(quote_handle(0.05).1, Shared::clone(&settings));

        settings.set_evaluation_date(start);
        assert_eq!(
            helper.earliest_date(),
            Date::null(),
            "an unchanged date must not rebuild the schedule"
        );
    }
}
