//! Term structure with an added spread on the zero yield rate.
//!
//! Port of `ql/termstructures/yield/zerospreadedtermstructure.hpp`: the
//! spread is added to the underlying curve's zero rate in the configured
//! compounding convention, and the spreaded rate is converted back to the
//! continuous zero yield driving [`ZeroYieldStructure`]. The structure
//! remains linked to the original curve and the spread quote: changes in
//! either propagate to observers, and the extrapolation flag re-syncs to the
//! underlying curve's on every notification, as in C++.
//!
//! ## Divergences from QuantLib
//!
//! - Reference date, times and inspectors delegate to the underlying handle;
//!   an empty handle yields `None`/`Err` (and
//!   [`max_date`](crate::termstructures::TermStructure::max_date) the null
//!   date) where C++ dereferences a null pointer. Construction with an empty
//!   handle succeeds, matching `testCreateWithNullUnderlying`.
//! - The deprecated day-counter constructor is not ported.

use super::sync_extrapolation;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::{Compounding, InterestRate};
use crate::patterns::observable::{AsObservable, Observable, Observer, deliver};
use crate::quotes::Quote;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::yields::ZeroYieldStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Natural, Rate, Time};

/// Observer half of a spreaded curve (the C++ `update()` override): re-syncs
/// the extrapolation flag to the underlying curve, then behaves like the
/// term-structure base updater.
pub(super) struct ExtrapolationSync {
    base: Shared<TermStructureBase>,
    original: Handle<dyn YieldTermStructure>,
    updater: SharedMut<dyn Observer>,
}

impl Observer for ExtrapolationSync {
    fn update(&mut self) {
        sync_extrapolation(&self.base, &self.original);
        deliver(&self.updater);
    }
}

pub(super) fn spawn_extrapolation_sync(
    base: &Shared<TermStructureBase>,
    original: &Handle<dyn YieldTermStructure>,
    spread: &Handle<dyn Quote>,
) -> SharedMut<ExtrapolationSync> {
    sync_extrapolation(base, original);
    let listener = shared_mut(ExtrapolationSync {
        base: Shared::clone(base),
        original: original.clone(),
        updater: base.updater(),
    });
    original.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
    spread.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
    listener
}

/// Term structure with an added spread on the zero yield rate.
pub struct ZeroSpreadedTermStructure {
    base: Shared<TermStructureBase>,
    original: Handle<dyn YieldTermStructure>,
    spread: Handle<dyn Quote>,
    compounding: Compounding,
    frequency: Frequency,
    _listener: SharedMut<ExtrapolationSync>,
}

impl ZeroSpreadedTermStructure {
    /// Spreads `original` by `spread` on continuously compounded zero rates
    /// (the C++ default arguments).
    pub fn new(
        original: Handle<dyn YieldTermStructure>,
        spread: Handle<dyn Quote>,
    ) -> ZeroSpreadedTermStructure {
        Self::with_compounding(
            original,
            spread,
            Compounding::Continuous,
            Frequency::NoFrequency,
        )
    }

    /// Spreads `original` by `spread` on zero rates quoted with the given
    /// compounding convention.
    pub fn with_compounding(
        original: Handle<dyn YieldTermStructure>,
        spread: Handle<dyn Quote>,
        compounding: Compounding,
        frequency: Frequency,
    ) -> ZeroSpreadedTermStructure {
        let base = shared(TermStructureBase::new(None));
        let listener = spawn_extrapolation_sync(&base, &original, &spread);
        ZeroSpreadedTermStructure {
            base,
            original,
            spread,
            compounding,
            frequency,
            _listener: listener,
        }
    }
}

impl AsObservable for ZeroSpreadedTermStructure {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ZeroSpreadedTermStructure {
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

    fn reference_date(&self) -> QlResult<Date> {
        self.original.current_link()?.reference_date()
    }

    fn max_time(&self) -> QlResult<Time> {
        self.original.current_link()?.max_time()
    }
}

impl ZeroYieldStructure for ZeroSpreadedTermStructure {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        let original = self.original.current_link()?;
        let zero_rate = original.zero_rate(t, self.compounding, self.frequency, true)?;
        let spread = self.spread.current_link()?.value()?;
        let spreaded_rate = InterestRate::new(
            zero_rate.rate() + spread,
            zero_rate.day_counter().clone(),
            zero_rate.compounding(),
            zero_rate.frequency(),
        )?;
        Ok(spreaded_rate
            .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, t)?
            .rate())
    }
}

impl YieldTermStructure for ZeroSpreadedTermStructure {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.discount_from_zero_yield(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::quotes::SimpleQuote;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_curve(rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            today(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }

    /// Port of `testZSpreaded` (test-suite/termstructures.cpp): the spreaded
    /// zero rate equals the underlying zero rate plus the spread.
    #[test]
    fn spreaded_zero_rate_is_the_underlying_plus_the_spread() {
        let tolerance = 1.0e-10;
        let curve = flat_curve(0.06);
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ZeroSpreadedTermStructure::new(
            Handle::new(curve.clone()),
            Handle::new(spread.clone() as Shared<dyn Quote>),
        );
        let test_date = curve.reference_date().unwrap() + 1800;
        let day_counter = curve.day_counter().unwrap();
        let zero = curve
            .zero_rate_date(
                test_date,
                day_counter.clone(),
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap();
        let spreaded_zero = spreaded
            .zero_rate_date(
                test_date,
                day_counter,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap();
        assert!(
            (zero.rate() - (spreaded_zero.rate() - spread.value().unwrap())).abs() < tolerance,
            "unable to reproduce zero yield from spreaded curve"
        );
    }

    /// The compounding conversion path: the spread is added on compounded
    /// rates, then converted to the continuous zero yield.
    #[test]
    fn compounded_spread_converts_to_the_continuous_equivalent() {
        let curve = flat_curve(0.06);
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ZeroSpreadedTermStructure::with_compounding(
            Handle::new(curve.clone()),
            Handle::new(spread.clone() as Shared<dyn Quote>),
            Compounding::Compounded,
            Frequency::Annual,
        );
        let t = 2.0;
        let zero = curve
            .zero_rate(t, Compounding::Compounded, Frequency::Annual, false)
            .unwrap();
        let expected = InterestRate::new(
            zero.rate() + 0.01,
            zero.day_counter().clone(),
            zero.compounding(),
            zero.frequency(),
        )
        .unwrap()
        .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, t)
        .unwrap();
        let spreaded_zero = spreaded
            .zero_rate(t, Compounding::Continuous, Frequency::NoFrequency, false)
            .unwrap();
        assert!((spreaded_zero.rate() - expected.rate()).abs() < 1.0e-12);
    }

    /// Port of `testZSpreadedObs`: relinking the underlying handle and
    /// changing the spread quote both notify observers.
    #[test]
    fn relink_and_spread_changes_notify_observers() {
        let handle: RelinkableHandle<dyn YieldTermStructure> =
            RelinkableHandle::new(flat_curve(0.03));
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ZeroSpreadedTermStructure::new(
            handle.handle(),
            Handle::new(spread.clone() as Shared<dyn Quote>),
        );
        let flag = Flag::new();
        spreaded.observable().register_observer(&as_observer(&flag));

        handle.link_to(flat_curve(0.05));
        assert!(
            Flag::is_up(&flag),
            "observer was not notified of term structure change"
        );

        Flag::lower(&flag);
        spread.set_value(0.005);
        assert!(
            Flag::is_up(&flag),
            "observer was not notified of spread change"
        );
        let df = spreaded.discount(1.0, false).unwrap();
        assert!((df - (-(0.05 + 0.005_f64)).exp()).abs() < 1.0e-15);
    }

    /// Port of `testCreateWithNullUnderlying`: construction with an empty
    /// underlying handle succeeds, and the curve works once linked.
    #[test]
    fn creating_with_an_empty_underlying_succeeds() {
        let spread = shared(SimpleQuote::new(0.01));
        let underlying: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let spreaded = ZeroSpreadedTermStructure::new(
            underlying.handle(),
            Handle::new(spread as Shared<dyn Quote>),
        );
        assert!(spreaded.reference_date().is_err());
        assert!(spreaded.discount(1.0, true).is_err());

        underlying.link_to(flat_curve(0.06));
        assert_eq!(spreaded.reference_date().unwrap(), today());
        let df = spreaded.discount(1.0, false).unwrap();
        assert!((df - (-0.07_f64).exp()).abs() < 1.0e-15);
    }
}
