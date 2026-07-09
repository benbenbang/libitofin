//! Term structure with an added spread on the instantaneous forward rate.
//!
//! Port of `ql/termstructures/yield/forwardspreadedtermstructure.hpp`: a
//! constant spread on the instantaneous forwards shifts the continuously
//! compounded zero yields by the same amount, so the C++ class derives from
//! `ZeroYieldStructure` and implements `zeroYieldImpl` as the underlying
//! continuous zero rate plus the spread; this port wires the same derivation
//! into [`ZeroYieldStructure`]. The structure remains linked to the original
//! curve and the spread quote: changes in either propagate to observers, and
//! the extrapolation flag re-syncs to the underlying curve's on every
//! notification, as in C++.
//!
//! ## Divergences from QuantLib
//!
//! Reference date, times and inspectors delegate to the underlying handle;
//! an empty handle yields `None`/`Err` (and
//! [`max_date`](crate::termstructures::TermStructure::max_date) the null
//! date) where C++ dereferences a null pointer.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable, ResetThenNotify};
use crate::quotes::Quote;
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::yields::ZeroYieldStructure;
use crate::termstructures::yields::zerospreadedtermstructure::spawn_extrapolation_sync;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Natural, Rate, Time};

/// Term structure with an added spread on the instantaneous forward rate.
pub struct ForwardSpreadedTermStructure {
    base: Shared<TermStructureBase>,
    original: Handle<dyn YieldTermStructure>,
    spread: Handle<dyn Quote>,
    _listener: SharedMut<ResetThenNotify>,
}

impl ForwardSpreadedTermStructure {
    /// Spreads the instantaneous forwards of `original` by `spread`,
    /// registering with both handles.
    pub fn new(
        original: Handle<dyn YieldTermStructure>,
        spread: Handle<dyn Quote>,
    ) -> ForwardSpreadedTermStructure {
        let base = shared(TermStructureBase::new(None));
        let listener = spawn_extrapolation_sync(&base, &original, &spread);
        ForwardSpreadedTermStructure {
            base,
            original,
            spread,
            _listener: listener,
        }
    }
}

impl AsObservable for ForwardSpreadedTermStructure {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ForwardSpreadedTermStructure {
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

impl ZeroYieldStructure for ForwardSpreadedTermStructure {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        let original = self.original.current_link()?;
        let zero = original.zero_rate(t, Compounding::Continuous, Frequency::NoFrequency, true)?;
        Ok(zero.rate() + self.spread.current_link()?.value()?)
    }
}

impl YieldTermStructure for ForwardSpreadedTermStructure {
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

    /// Port of `testFSpreaded` (test-suite/termstructures.cpp): the spreaded
    /// instantaneous forward equals the underlying forward plus the spread.
    #[test]
    fn spreaded_forward_is_the_underlying_plus_the_spread() {
        let tolerance = 1.0e-10;
        let curve = flat_curve(0.06);
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ForwardSpreadedTermStructure::new(
            Handle::new(curve.clone()),
            Handle::new(spread.clone() as Shared<dyn Quote>),
        );
        let test_date = curve.reference_date().unwrap() + 1800;
        let forward = curve
            .forward_rate_between(
                test_date,
                test_date,
                curve.day_counter().unwrap(),
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap();
        let spreaded_forward = spreaded
            .forward_rate_between(
                test_date,
                test_date,
                spreaded.day_counter().unwrap(),
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap();
        assert!(
            (forward.rate() - (spreaded_forward.rate() - spread.value().unwrap())).abs()
                < tolerance,
            "unable to reproduce forward from spreaded curve"
        );
    }

    /// A constant forward spread scales every discount by exp(-spread * t).
    #[test]
    fn discounts_scale_by_the_exponential_spread() {
        let curve = flat_curve(0.06);
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ForwardSpreadedTermStructure::new(
            Handle::new(curve.clone()),
            Handle::new(spread as Shared<dyn Quote>),
        );
        for t in [0.5_f64, 2.0, 7.5] {
            let df = spreaded.discount(t, false).unwrap();
            let expected = curve.discount(t, false).unwrap() * (-0.01 * t).exp();
            assert!((df - expected).abs() < 1.0e-15);
        }
        assert_eq!(spreaded.discount(0.0, false).unwrap(), 1.0);
    }

    /// Port of `testFSpreadedObs`: relinking the underlying handle and
    /// changing the spread quote both notify observers.
    #[test]
    fn relink_and_spread_changes_notify_observers() {
        let handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let spread = shared(SimpleQuote::new(0.01));
        let spreaded = ForwardSpreadedTermStructure::new(
            handle.handle(),
            Handle::new(spread.clone() as Shared<dyn Quote>),
        );
        let flag = Flag::new();
        spreaded.observable().register_observer(&as_observer(&flag));

        handle.link_to(flat_curve(0.06));
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
    }
}
