//! Flat interest-rate curve.
//!
//! Port of `ql/termstructures/yield/flatforward.{hpp,cpp}`: a curve quoting
//! one forward rate for every maturity, backed by a quote handle or a plain
//! value.
//!
//! C++'s `LazyObject` half becomes a cached [`InterestRate`] invalidated by
//! quote notifications: the curve's observer clears the cache *before*
//! passing the notification on, so observers reading the curve during the
//! same notification wave see fresh values. The value-backed constructors
//! wrap the rate in an unshared [`SimpleQuote`] like the C++ ones; the
//! subscription they add is inert since nothing else can change that quote.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::{Compounding, InterestRate};
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Natural, Rate, Time};

/// Observer half of a flat curve (the C++ `FlatForward::update()`): drops the
/// cached rate, then behaves like the term-structure base updater.
struct RateInvalidator {
    rate: SharedMut<Option<InterestRate>>,
    updater: SharedMut<dyn Observer>,
}

impl Observer for RateInvalidator {
    fn update(&mut self) {
        self.rate.borrow_mut().take();
        self.updater.borrow_mut().update();
    }
}

/// Flat interest-rate curve.
pub struct FlatForward {
    base: TermStructureBase,
    forward: Handle<dyn Quote>,
    compounding: Compounding,
    frequency: Frequency,
    rate: SharedMut<Option<InterestRate>>,
    _listener: SharedMut<RateInvalidator>,
}

impl FlatForward {
    fn assemble(
        base: TermStructureBase,
        forward: Handle<dyn Quote>,
        compounding: Compounding,
        frequency: Frequency,
    ) -> FlatForward {
        let rate = shared_mut(None);
        let listener = shared_mut(RateInvalidator {
            rate: SharedMut::clone(&rate),
            updater: base.updater(),
        });
        forward.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        FlatForward {
            base,
            forward,
            compounding,
            frequency,
            rate,
            _listener: listener,
        }
    }

    fn wrap(value: Rate) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    /// Quote-backed curve with a fixed reference date.
    pub fn new(
        reference_date: Date,
        forward: Handle<dyn Quote>,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
    ) -> FlatForward {
        let base = TermStructureBase::with_reference_date(reference_date, None, Some(day_counter));
        Self::assemble(base, forward, compounding, frequency)
    }

    /// Value-backed curve with a fixed reference date.
    pub fn with_rate(
        reference_date: Date,
        forward: Rate,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
    ) -> FlatForward {
        Self::new(
            reference_date,
            Self::wrap(forward),
            day_counter,
            compounding,
            frequency,
        )
    }

    /// Quote-backed curve whose reference date moves off the evaluation date.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        forward: Handle<dyn Quote>,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
        settings: SharedMut<Settings<Date>>,
    ) -> QlResult<FlatForward> {
        let base =
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings)?;
        Ok(Self::assemble(base, forward, compounding, frequency))
    }

    /// Value-backed curve whose reference date moves off the evaluation date.
    pub fn moving_with_rate(
        settlement_days: Natural,
        calendar: Calendar,
        forward: Rate,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
        settings: SharedMut<Settings<Date>>,
    ) -> QlResult<FlatForward> {
        Self::moving(
            settlement_days,
            calendar,
            Self::wrap(forward),
            day_counter,
            compounding,
            frequency,
            settings,
        )
    }

    /// The compounding convention of the quoted rate.
    pub fn compounding(&self) -> Compounding {
        self.compounding
    }

    /// The compounding frequency of the quoted rate.
    pub fn compounding_frequency(&self) -> Frequency {
        self.frequency
    }

    fn flat_rate(&self) -> QlResult<InterestRate> {
        if let Some(rate) = self.rate.borrow().clone() {
            return Ok(rate);
        }
        let value = self.forward.current_link()?.value()?;
        let day_counter = self
            .base
            .day_counter()
            .expect("a flat forward curve is constructed with a day counter");
        let rate = InterestRate::new(value, day_counter, self.compounding, self.frequency)?;
        *self.rate.borrow_mut() = Some(rate.clone());
        Ok(rate)
    }
}

impl AsObservable for FlatForward {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for FlatForward {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl YieldTermStructure for FlatForward {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.flat_rate()?.discount_factor(t)
    }
}
