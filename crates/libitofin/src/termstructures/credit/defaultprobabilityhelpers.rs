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

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::instrument::Instrument;
use crate::instruments::{CdsTerms, CreditDefaultSwap, ProtectionSide};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::credit::MidPointCdsEngine;
use crate::quotes::Quote;
use crate::require;
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
    fn initialize_dates(&self);
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

/// The terms a [`SpreadCdsHelper`] defaults when they are not quoted.
///
/// One field per defaulted argument of the C++ `CdsHelper` constructor
/// (`defaultprobabilityhelpers.hpp:90-94`); [`Default`] carries their C++
/// values. The `model` argument has no field: only
/// [`Midpoint`](crate::pricingengines::credit::MidPointCdsEngine) is ported, the
/// ISDA arm of `resetEngine` (`defaultprobabilityhelpers.cpp:143-148`) staying
/// deferred within EPIC Credit (#676).
pub struct CdsHelperTerms {
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
            settles_accrual: true,
            pays_at_default_time: true,
            start_date: None,
            last_period_day_counter: None,
            rebates_accrual: true,
        }
    }
}

/// A spread-quoted CDS as a credit bootstrap helper (`SpreadCdsHelper`,
/// `defaultprobabilityhelpers.hpp:128`).
///
/// The helper prices a par CDS on its own schedule against the curve being
/// bootstrapped and reports that contract's fair spread as
/// [`implied_quote`](DefaultProbabilityHelper::implied_quote); the bootstrap
/// drives `quoted spread - fair spread` to zero.
///
/// The C++ `CdsHelper` base (`defaultprobabilityhelpers.hpp:47-126`) has no
/// separate type here. It exists in C++ only to share state and
/// `initializeDates` with `UpfrontCdsHelper`, and that sibling is deferred
/// (#676), so its state and `initializeDates` live directly in this struct.
/// Porting `UpfrontCdsHelper` means factoring out the fields below plus
/// [`initialize_dates`](RelativeDateDefaultProbabilityHelper::initialize_dates)
/// and [`set_term_structure`](DefaultProbabilityHelper::set_term_structure),
/// leaving only `reset_engine` and `implied_quote` per subclass.
pub struct SpreadCdsHelper {
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
    /// Rejects the three CDS date-generation rules. Their maturity comes from
    /// `cdsMaturity` (`cpp:87`), which is not ported, and taking the other arm
    /// for them would silently produce a schedule ending on the wrong date; that
    /// branch is deferred within EPIC Credit (#676).
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
        require!(
            !matches!(
                rule,
                DateGeneration::CDS | DateGeneration::CDS2015 | DateGeneration::OldCDS
            ),
            "the post-Big-Bang date-generation rules need cdsMaturity, which is not ported yet \
             (defaultprobabilityhelpers.cpp:85-88)"
        );
        Ok(Shared::new_cyclic(|weak: &Weak<SpreadCdsHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                    helper.reset_engine();
                }
            });
            let base = BootstrapHelperBase::new_relative(
                running_spread,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            discount_curve.register_observer(&base.observer());
            let helper = SpreadCdsHelper {
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
                settings,
                schedule: RefCell::new(Schedule::from_dates(Vec::new())),
                protection_start: Cell::new(Date::null()),
                probability: RelinkableHandle::empty(),
                swap: RefCell::new(Err(engine_not_reset())),
            };
            helper.initialize_dates();
            helper
        }))
    }

    /// The date protection starts, `settlement_days` past the evaluation date
    /// (`cpp:79`).
    pub fn protection_start(&self) -> Date {
        self.protection_start.get()
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
        *self.swap.borrow_mut() = self.build_swap();
    }

    /// The par contract the helper prices: a protection-buyer CDS on a notional
    /// of 100 paying a 1% running spread (`cpp:138-141`), under a fresh midpoint
    /// engine over the helper's own probability handle (`cpp:151-152`).
    ///
    /// C++ passes its `evaluationDate_` as the contract's trade date; this
    /// leaves the trade date to the contract, which deduces `protection start -
    /// 1` for a pre-Big-Bang rule (`creditdefaultswap.rs:365-371`). The two
    /// differ only when `settlement_days` is zero, and only in the accrual
    /// rebate and upfront payment, both zero-amount flows on a contract with no
    /// upfront: the deduced date keeps the helper clear of the rebate
    /// arithmetic deferred by #689, which the C++ date would enter with nothing
    /// to show for it.
    fn build_swap(&self) -> QlResult<CreditDefaultSwap> {
        let mut swap = CreditDefaultSwap::with_terms(
            ProtectionSide::Buyer,
            100.0,
            0.01,
            self.schedule.borrow().clone(),
            self.payment_convention,
            self.day_counter.clone(),
            CdsTerms {
                settles_accrual: self.settles_accrual,
                pays_at_default_time: self.pays_at_default_time,
                protection_start: Some(self.protection_start.get()),
                last_period_day_counter: self.last_period_day_counter.clone(),
                rebates_accrual: self.rebates_accrual,
                ..CdsTerms::default()
            },
            Shared::clone(&self.settings),
        )?;
        let engine = MidPointCdsEngine::new(
            self.probability.handle(),
            self.recovery_rate,
            self.discount_curve.clone(),
            None,
            Shared::clone(&self.settings),
        );
        swap.base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        Ok(swap)
    }
}

impl AsObservable for SpreadCdsHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl DefaultProbabilityHelper for SpreadCdsHelper {
    fn base(&self) -> &DefaultProbabilityHelperBase {
        &self.base
    }

    /// The contract's fair spread (`impliedQuote`, `cpp:132-135`).
    ///
    /// The recalculation is forced rather than left to the cache: the helper
    /// weak-links the curve handle the engine prices over, so a node the
    /// bootstrap has just moved does not notify the contract, and a plain
    /// `fair_spread` would answer from the previous iteration's results.
    fn implied_quote(&self) -> QlResult<Real> {
        let mut swap = self.swap.borrow_mut();
        let swap = swap.as_mut().map_err(|error| error.clone())?;
        swap.recalculate()?;
        swap.fair_spread()
    }

    /// Records the curve, hands it to the engine, and rebuilds the contract
    /// (`setTermStructure`, `cpp:60-68`).
    ///
    /// The engine's handle is linked weakly, the port of the C++
    /// `linkTo(..., false)` at `cpp:63-65`: the curve owns this helper, which
    /// owns the contract, which owns the engine, and a strong link would close
    /// that ring.
    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        self.base.set_term_structure(term_structure);
        self.probability
            .link_to_weak(Shared::downgrade(term_structure));
        self.reset_engine();
    }
}

impl RelativeDateDefaultProbabilityHelper for SpreadCdsHelper {
    /// Rebuilds the schedule off the current evaluation date (`initializeDates`,
    /// `cpp:75-108`).
    ///
    /// Protection starts `settlement_days` past the evaluation date and, absent
    /// an explicit start, the schedule starts there too, rolled to a business
    /// day. The maturity is the tenor past that same reference date - the
    /// `cdsMaturity` arm above it (`cpp:86-88`) covers only the three CDS rules,
    /// which the constructor rejects; `TwentiethIMM` takes this arm.
    ///
    /// The earliest date is the schedule's first date and the latest its last,
    /// rolled. C++ then adds a day under the ISDA model (`cpp:105-106`); the
    /// midpoint model, the only one ported, does not.
    fn initialize_dates(&self) {
        let evaluation_date = self
            .base
            .evaluation_date()
            .expect("a relative-date helper always tracks an evaluation date");
        let protection_start = evaluation_date + self.settlement_days;
        self.protection_start.set(protection_start);

        let mut start_date = self.start_date.unwrap_or(protection_start);
        if self.rule != DateGeneration::CDS && self.rule != DateGeneration::CDS2015 {
            start_date = self.calendar.adjust(start_date, self.payment_convention);
        }
        let reference_date = match self.start_date {
            Some(date) => date + self.settlement_days,
            None => protection_start,
        };
        let end_date = reference_date + self.tenor;

        let schedule = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_frequency(self.frequency)
            .with_calendar(self.calendar.clone())
            .with_convention(self.payment_convention)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(self.rule)
            .build();

        self.base.set_earliest_date(schedule.date(0));
        self.base.set_latest_date(
            self.calendar
                .adjust(schedule.date(schedule.len() - 1), self.payment_convention),
        );
        *self.schedule.borrow_mut() = schedule;
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
}
