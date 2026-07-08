//! Implied term structure at a given date in the future.
//!
//! Port of `ql/termstructures/yield/impliedtermstructure.hpp`: a curve that
//! re-bases another [`YieldTermStructure`] handle to a future reference date,
//! so that `discount(t)` is the underlying discount from the implied
//! reference date onward. The structure remains linked to the original one:
//! changes in the underlying handle propagate to observers and refresh the
//! cached re-basing factors, and the extrapolation flag re-syncs to the
//! underlying curve's on every notification, as in C++.
//!
//! ## Divergences from QuantLib
//!
//! - The cached reference time and discount use an `Option` cleared on
//!   notification where C++ uses `Null<T>` sentinels.
//! - Inspectors on an empty underlying handle return `None`/`Err` (and
//!   [`max_date`](crate::termstructures::TermStructure::max_date) the null
//!   date) where C++ dereferences a null pointer.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer, deliver};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{DiscountFactor, Natural, Time};

/// Observer half of an implied curve (the C++ `ImpliedTermStructure::update()`):
/// drops the cached re-basing factors, re-syncs the extrapolation flag to the
/// underlying curve, then behaves like the term-structure base updater.
struct CacheInvalidator {
    cache: SharedMut<Option<(Time, DiscountFactor)>>,
    base: Shared<TermStructureBase>,
    original: Handle<dyn YieldTermStructure>,
    updater: SharedMut<dyn Observer>,
}

impl Observer for CacheInvalidator {
    fn update(&mut self) {
        self.cache.borrow_mut().take();
        sync_extrapolation(&self.base, &self.original);
        deliver(&self.updater);
    }
}

fn sync_extrapolation(base: &TermStructureBase, original: &Handle<dyn YieldTermStructure>) {
    if let Ok(original) = original.current_link() {
        if original.allows_extrapolation() {
            base.enable_extrapolation();
        } else {
            base.disable_extrapolation();
        }
    }
}

/// Implied term structure at a given date in the future.
///
/// The given date will be the implied reference date; day counter, calendar,
/// settlement days and maximum date are the underlying curve's.
pub struct ImpliedTermStructure {
    base: Shared<TermStructureBase>,
    original: Handle<dyn YieldTermStructure>,
    cache: SharedMut<Option<(Time, DiscountFactor)>>,
    _listener: SharedMut<CacheInvalidator>,
}

impl ImpliedTermStructure {
    /// Re-bases `original` to `reference_date`, registering with the handle
    /// and adopting the underlying curve's extrapolation setting.
    pub fn new(
        original: Handle<dyn YieldTermStructure>,
        reference_date: Date,
    ) -> ImpliedTermStructure {
        let base = shared(TermStructureBase::with_reference_date(
            reference_date,
            None,
            None,
        ));
        sync_extrapolation(&base, &original);
        let cache = shared_mut(None);
        let listener = shared_mut(CacheInvalidator {
            cache: SharedMut::clone(&cache),
            base: Shared::clone(&base),
            original: original.clone(),
            updater: base.updater(),
        });
        original.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        ImpliedTermStructure {
            base,
            original,
            cache,
            _listener: listener,
        }
    }
}

impl AsObservable for ImpliedTermStructure {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ImpliedTermStructure {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.original
            .current_link()
            .map(|curve| curve.max_date())
            .unwrap_or_else(|_| Date::null())
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.original
            .current_link()
            .ok()
            .and_then(|curve| curve.day_counter())
    }

    fn calendar(&self) -> Option<Calendar> {
        self.original
            .current_link()
            .ok()
            .and_then(|curve| curve.calendar())
    }

    fn settlement_days(&self) -> QlResult<Natural> {
        self.original.current_link()?.settlement_days()
    }
}

impl YieldTermStructure for ImpliedTermStructure {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        let original = self.original.current_link()?;
        let cached = *self.cache.borrow();
        let (ref_time, ref_df) = match cached {
            Some(pair) => pair,
            None => {
                let reference = self.reference_date()?;
                let day_counter = self.require_day_counter()?;
                let ref_time = day_counter.year_fraction(original.reference_date()?, reference);
                let ref_df = original.discount_date(reference, true)?;
                *self.cache.borrow_mut() = Some((ref_time, ref_df));
                (ref_time, ref_df)
            }
        };
        Ok(original.discount(t + ref_time, true)? / ref_df)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::interestrate::Compounding;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_curve(quote: Shared<SimpleQuote>) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::new(
            today(),
            Handle::new(quote as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }

    /// Port of `testImplied` (test-suite/termstructures.cpp): the discount to
    /// a date past the implied reference factorizes as
    /// `discount(testDate) = discount(newSettlement) * implied(testDate)`.
    #[test]
    fn implied_discount_reproduces_the_underlying_discount() {
        let tolerance = 1.0e-10;
        let quote = shared(SimpleQuote::new(0.0));
        let curve = flat_curve(quote.clone());
        let new_settlement = today() + 1097;
        let test_date = new_settlement + 1800;
        let implied = ImpliedTermStructure::new(Handle::new(curve.clone()), new_settlement);
        for i in 1..4 {
            quote.set_value(f64::from(i) / 100.0);
            let base_discount = curve.discount_date(new_settlement, false).unwrap();
            let discount = curve.discount_date(test_date, false).unwrap();
            let implied_discount = implied.discount_date(test_date, false).unwrap();
            assert!(
                (discount - base_discount * implied_discount).abs() < tolerance,
                "unable to reproduce discount from implied curve at rate {i}%"
            );
        }
    }

    /// Port of `testImpliedObs`: relinking the underlying handle notifies
    /// observers of the implied curve.
    #[test]
    fn relinking_the_underlying_notifies_observers() {
        let handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let implied = ImpliedTermStructure::new(handle.handle(), today() + 1097);
        let flag = Flag::new();
        implied.observable().register_observer(&as_observer(&flag));

        handle.link_to(flat_curve(shared(SimpleQuote::new(0.03))));

        assert!(
            Flag::is_up(&flag),
            "observer was not notified of term structure change"
        );
    }

    #[test]
    fn inspectors_delegate_to_the_underlying_curve() {
        let curve = flat_curve(shared(SimpleQuote::new(0.03)));
        let reference = today() + 30;
        let implied = ImpliedTermStructure::new(Handle::new(curve.clone()), reference);

        assert_eq!(implied.reference_date().unwrap(), reference);
        assert_eq!(implied.max_date(), curve.max_date());
        assert_eq!(
            implied.day_counter().unwrap().name(),
            curve.day_counter().unwrap().name()
        );
        assert!(implied.calendar().is_none());
        assert!(implied.settlement_days().is_err());
    }

    #[test]
    fn empty_handle_errors_instead_of_pricing() {
        let handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let implied = ImpliedTermStructure::new(handle.handle(), today() + 30);
        assert!(implied.day_counter().is_none());
        assert_eq!(implied.max_date(), Date::null());
        assert!(implied.discount(1.0, true).is_err());
    }

    #[test]
    fn notifications_resync_the_extrapolation_flag() {
        let quote = shared(SimpleQuote::new(0.03));
        let curve = flat_curve(quote.clone());
        let implied = ImpliedTermStructure::new(Handle::new(curve.clone()), today() + 30);
        assert!(!implied.allows_extrapolation());

        curve.enable_extrapolation();
        quote.set_value(0.04);

        assert!(
            implied.allows_extrapolation(),
            "extrapolation flag must re-sync to the underlying curve"
        );
    }
}
