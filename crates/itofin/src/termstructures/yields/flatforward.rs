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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn today() -> Date {
        Date::new(17, Month::May, 1998)
    }

    #[test]
    fn flat_curve_reproduces_the_continuous_discounts_european_options_use() {
        let q = FlatForward::with_rate(
            today(),
            0.04,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        let r = FlatForward::with_rate(
            today(),
            0.06,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        for days in [90, 180, 360, 720] {
            let t = Time::from(days) / 360.0;
            let df_q = q.discount_date(today() + days, false).unwrap();
            let df_r = r.discount_date(today() + days, false).unwrap();
            assert!((df_q - (-0.04 * t).exp()).abs() < 1.0e-15);
            assert!((df_r - (-0.06 * t).exp()).abs() < 1.0e-15);
        }
        assert_eq!(q.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn zero_and_forward_rates_are_flat_at_the_quoted_rate() {
        let curve = FlatForward::with_rate(
            today(),
            0.06,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        for t in [0.25, 1.0, 7.5] {
            let zero = curve
                .zero_rate(t, Compounding::Continuous, Frequency::Annual, false)
                .unwrap();
            assert!((zero.rate() - 0.06).abs() < 1.0e-12);
        }
        let forward = curve
            .forward_rate(0.5, 2.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - 0.06).abs() < 1.0e-12);
        let instantaneous = curve
            .forward_rate(1.0, 1.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((instantaneous.rate() - 0.06).abs() < 1.0e-9);
    }

    #[test]
    fn compounded_quotes_discount_with_their_own_convention() {
        let curve = FlatForward::with_rate(
            today(),
            0.06,
            Actual360::new(),
            Compounding::Compounded,
            Frequency::Semiannual,
        );
        assert_eq!(curve.compounding(), Compounding::Compounded);
        assert_eq!(curve.compounding_frequency(), Frequency::Semiannual);
        let df = curve.discount(1.0, false).unwrap();
        assert!((df - 1.0 / (1.0_f64 + 0.06 / 2.0).powi(2)).abs() < 1.0e-15);
    }

    #[test]
    fn quote_change_notifies_observers_and_refreshes_the_rate() {
        let quote = shared(SimpleQuote::new(0.05));
        let curve = FlatForward::new(
            today(),
            Handle::new(quote.clone() as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        assert!((curve.discount(2.0, false).unwrap() - (-0.10_f64).exp()).abs() < 1.0e-15);

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        quote.set_value(0.07);

        assert!(
            Flag::is_up(&flag),
            "quote change must reach curve observers"
        );
        assert!((curve.discount(2.0, false).unwrap() - (-0.14_f64).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn observers_reading_during_the_notification_see_the_fresh_rate() {
        struct Reader {
            curve: Shared<FlatForward>,
            seen: SharedMut<Option<DiscountFactor>>,
        }
        impl Observer for Reader {
            fn update(&mut self) {
                *self.seen.borrow_mut() = Some(self.curve.discount(1.0, false).unwrap());
            }
        }

        let quote = shared(SimpleQuote::new(0.05));
        let curve = shared(FlatForward::new(
            today(),
            Handle::new(quote.clone() as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        curve.discount(1.0, false).unwrap();

        let seen = shared_mut(None);
        let reader = shared_mut(Reader {
            curve: curve.clone(),
            seen: SharedMut::clone(&seen),
        });
        curve
            .observable()
            .register_observer(&(reader.clone() as SharedMut<dyn Observer>));

        quote.set_value(0.07);

        let seen = seen.borrow().expect("reader must have been notified");
        assert!(
            (seen - (-0.07_f64).exp()).abs() < 1.0e-15,
            "mid-notification read returned a stale discount ({seen})"
        );
    }

    #[test]
    fn relinking_the_handle_switches_the_curve_to_the_new_quote() {
        let relinkable = crate::quotes::make_quote_handle(0.05);
        let curve = FlatForward::new(
            today(),
            relinkable.handle(),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        assert!((curve.discount(1.0, false).unwrap() - (-0.05_f64).exp()).abs() < 1.0e-15);

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        relinkable.link_to(shared(SimpleQuote::new(0.08)));

        assert!(Flag::is_up(&flag), "relink must reach curve observers");
        assert!((curve.discount(1.0, false).unwrap() - (-0.08_f64).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn moving_curve_follows_the_evaluation_date() {
        let settings = shared_mut(Settings::new());
        settings
            .borrow_mut()
            .set_evaluation_date(Date::new(15, Month::January, 2026));
        let curve = FlatForward::moving_with_rate(
            2,
            Target::new(),
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
            settings.clone(),
        )
        .unwrap();
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(19, Month::January, 2026)
        );

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        settings
            .borrow_mut()
            .set_evaluation_date(Date::new(16, Month::January, 2026));

        assert!(Flag::is_up(&flag));
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(20, Month::January, 2026)
        );
        let df = curve
            .discount_date(Date::new(20, Month::January, 2027), false)
            .unwrap();
        assert!((df - (-0.05_f64 * 365.0 / 360.0).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn empty_or_invalid_quotes_error_instead_of_pricing() {
        let curve = FlatForward::new(
            today(),
            Handle::empty(),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        assert!(curve.discount(1.0, false).is_err());

        let unset = shared(SimpleQuote::default());
        let curve = FlatForward::new(
            today(),
            Handle::new(unset as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        assert!(curve.discount(1.0, false).is_err());
    }
}
