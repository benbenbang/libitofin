//! Rate helpers for the yield-curve bootstrap.
//!
//! Port of `ql/termstructures/yield/ratehelpers.{hpp,cpp}`. This module holds
//! [`DepositRateHelper`], the short end of the curve; the FRA and futures
//! helpers (`FraRateHelper`, `FuturesRateHelper`) are deferred to a later
//! ticket.

use std::cell::Cell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, shared};
use crate::termstructures::bootstraphelper::{
    BootstrapHelperBase, RateHelper, RelativeDateRateHelper,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::types::Real;

/// Bootstrap helper over a deposit rate (`DepositRateHelper`).
///
/// A deposit borrows at the quoted rate from spot to spot-plus-tenor;
/// [`implied_quote`](RateHelper::implied_quote) re-derives that rate from the
/// bootstrapping curve's discount factors between the value and maturity dates.
///
/// The load-bearing mechanism is the cloned index: the constructor re-curves
/// the supplied index onto the helper's *own* [`RelinkableHandle`] with
/// [`IborIndex::clone_with`] (`ratehelpers.cpp:206`), so the helper forecasts
/// off the curve it is being bootstrapped against rather than whatever curve
/// the index was handed. [`set_term_structure`](RateHelper::set_term_structure)
/// weak-links that handle to the bootstrapping curve, non-owning and unobserved
/// (the `null_deleter`/`observer = false` of `ratehelpers.cpp:217`).
pub struct DepositRateHelper {
    base: BootstrapHelperBase,
    index: IborIndex,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    fixing_date: Cell<Date>,
}

impl DepositRateHelper {
    /// A deposit helper fitting `quote`, an explicit market-rate handle, with
    /// its schedule taken from `index` (the C++ `DepositRateHelper(rate, i)`
    /// with `rate` a `Handle<Quote>`, `ratehelpers.cpp:195`).
    pub fn new(quote: Handle<dyn Quote>, index: &IborIndex) -> Shared<DepositRateHelper> {
        Self::build(quote, index)
    }

    /// A deposit helper fitting a fixed `rate`, wrapped in a [`SimpleQuote`]
    /// (the `rate`-as-`Rate` arm of the same C++ variant constructor).
    pub fn from_rate(rate: Real, index: &IborIndex) -> Shared<DepositRateHelper> {
        let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
        Self::build(quote, index)
    }

    fn build(quote: Handle<dyn Quote>, source_index: &IborIndex) -> Shared<DepositRateHelper> {
        let settings = source_index.base().settings().clone();
        Shared::new_cyclic(|weak: &Weak<DepositRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let index = source_index.clone_with(term_structure_handle.handle());
            let base = BootstrapHelperBase::new_relative(quote, settings, true, on_eval_change);
            let helper = DepositRateHelper {
                base,
                index,
                term_structure_handle,
                fixing_date: Cell::new(Date::null()),
            };
            helper.initialize_dates();
            helper
        })
    }
}

impl AsObservable for DepositRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for DepositRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The deposit rate implied by the current curve.
    ///
    /// The forecast flag is forced true (`iborIndex_->fixing(fixingDate_, true)`,
    /// `ratehelpers.cpp:213`): the helper prices off the curve, never off a
    /// stored fixing.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        self.index.fixing(self.fixing_date.get(), true)
    }

    /// Weak-links the helper's own pricing handle to the bootstrapping curve,
    /// then records the curve on the base - both non-owning and unobserved
    /// (`ratehelpers.cpp:216`).
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for DepositRateHelper {
    /// Rebuilds the schedule off the current evaluation date
    /// (`initializeDates`, `ratehelpers.cpp:228`): the reference date is the
    /// evaluation date rolled to a business day, the earliest (value) date is
    /// spot from there, the fixing date the value date rolled back, and the
    /// maturity the value date advanced by the tenor. Pillar, latest and
    /// latest-relevant dates all equal the maturity.
    ///
    /// The value- and maturity-date arithmetic is calendar rolling on an
    /// already-adjusted business day and so cannot fail; the `expect` documents
    /// that invariant.
    fn initialize_dates(&self) {
        let evaluation_date = self
            .base
            .evaluation_date()
            .expect("a relative-date helper always tracks an evaluation date");
        let reference_date = self
            .index
            .fixing_calendar()
            .adjust(evaluation_date, BusinessDayConvention::Following);
        let earliest = self
            .index
            .value_date(reference_date)
            .expect("value date of an adjusted business day is valid");
        self.fixing_date.set(self.index.fixing_date(earliest));
        let maturity = self
            .index
            .maturity_date(earliest)
            .expect("maturity date of a value date is valid");

        self.base.set_earliest_date(earliest);
        self.base.set_maturity_date(maturity);
        self.base.set_pillar_date(maturity);
        self.base.set_latest_date(maturity);
        self.base.set_latest_relevant_date(maturity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::{currency::Currency, types::Rate};

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn euribor(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        IborIndex::new(
            "Euribor".into(),
            tenor,
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            forwarding,
            settings,
        )
    }

    fn flat_curve(reference: Date, rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// Standalone oracle: off a flat continuously-compounded curve the implied
    /// deposit rate is the closed-form simple forward `(exp(r*t) - 1)/t` over
    /// the helper's value-to-maturity window, and equals the index fixing path.
    #[test]
    fn implied_quote_matches_closed_form_deposit_rate() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let rate = 0.03;
        let curve = flat_curve(today(), rate);
        helper.set_term_structure(&curve);

        let d1 = helper.earliest_date();
        let d2 = helper.maturity_date();
        let t = Actual360::new().year_fraction(d1, d2);
        let implied = helper.implied_quote().unwrap();

        let closed_form = ((rate * t).exp() - 1.0) / t;
        assert!((implied - closed_form).abs() < 1e-12);
    }

    /// `initializeDates` derives earliest/maturity/pillar from the index
    /// conventions off the evaluation date (`ratehelpers.cpp:228`).
    #[test]
    fn initialize_dates_follows_the_index_conventions() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let reference = source
            .fixing_calendar()
            .adjust(today(), BusinessDayConvention::Following);
        let earliest = source.value_date(reference).unwrap();
        let maturity = source.maturity_date(earliest).unwrap();

        assert_eq!(helper.earliest_date(), earliest);
        assert!(earliest > today(), "the value date is spot, past today");
        assert_eq!(helper.maturity_date(), maturity);
        assert_eq!(helper.pillar_date(), maturity);
        assert_eq!(helper.latest_relevant_date(), maturity);
    }

    /// The clone mechanism: the helper forecasts off its OWN handle (the curve
    /// it is bootstrapped against), leaving the source index - here on an empty
    /// handle - untouched.
    #[test]
    fn helper_prices_off_its_own_handle_not_the_source_index() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);
        let implied_low = helper.implied_quote().unwrap();

        let curve_high = flat_curve(today(), 0.06);
        helper.set_term_structure(&curve_high);
        let implied_high = helper.implied_quote().unwrap();

        assert!(
            implied_high > implied_low,
            "relinking the helper's handle moves its implied quote"
        );
        assert!(
            source.forecast_fixing(helper.earliest_date()).is_err(),
            "the source index's own empty handle is untouched"
        );
    }

    /// `quote_error` is market minus implied.
    #[test]
    fn quote_error_is_market_minus_implied() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.05, &source);

        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);

        let implied = helper.implied_quote().unwrap();
        assert!((helper.quote_error().unwrap() - (0.05 - implied)).abs() < 1e-15);
    }

    /// An evaluation-date change reruns `initializeDates` and notifies observers.
    #[test]
    fn evaluation_date_change_reinitializes_dates() {
        let settings = settings_on(today());
        let source = euribor(
            Period::new(6, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        let helper = DepositRateHelper::from_rate(0.02, &source);
        let before = helper.earliest_date();

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        let moved = today() + 30;
        settings.set_evaluation_date(moved);

        assert!(Flag::is_up(&flag), "date change must notify observers");
        assert!(
            helper.earliest_date() > before,
            "date change must rerun initialize_dates"
        );
    }
}
