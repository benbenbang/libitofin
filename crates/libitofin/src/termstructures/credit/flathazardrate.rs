//! Flat hazard-rate curve.
//!
//! Port of `ql/termstructures/credit/flathazardrate.{hpp,cpp}`: a credit curve
//! quoting one hazard rate for every maturity, backed by a quote handle or a
//! plain value, with the closed-form survival probability `exp(-h t)`
//! (`flathazardrate.hpp:72-74`).
//!
//! ## Divergences from QuantLib
//!
//! - The C++ constructors register with the quote only on the two
//!   `Handle<Quote>` overloads (`flathazardrate.cpp:32,47`); the two `Rate`
//!   overloads wrap the value in a fresh `SimpleQuote` and skip the
//!   registration (`flathazardrate.cpp:39,55`). This port registers uniformly
//!   for all four, keeping the idiom of its direct sibling
//!   [`FlatForward`](crate::termstructures::yields::FlatForward). The C++ skip
//!   is an unobservable optimization rather than a behaviour: the wrapped quote
//!   is private and unreachable, so nothing can ever call `set_value` on it and
//!   the subscription can never fire.
//! - Unlike [`FlatForward`](crate::termstructures::yields::FlatForward) this
//!   curve caches nothing, since C++ reads the quote live on every
//!   `hazardRateImpl` (`flathazardrate.hpp:59`). The subscription therefore
//!   resets no state and exists only to forward quote notifications to the
//!   structure's own observers, which a consumer caching off this curve needs.
//! - The moving constructors take an explicit
//!   [`Settings`] handle rather than reading a global evaluation date (D5), so
//!   [`reference_date`](TermStructure::reference_date) returns an `Err` when no
//!   evaluation date is set instead of falling back to the system clock.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::hazardratestructure::HazardRateStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Probability, Rate, Real, Time};

/// Flat hazard-rate curve.
pub struct FlatHazardRate {
    base: TermStructureBase,
    hazard_rate: Handle<dyn Quote>,
    _listener: SharedMut<ResetThenNotify>,
}

impl FlatHazardRate {
    fn assemble(base: TermStructureBase, hazard_rate: Handle<dyn Quote>) -> FlatHazardRate {
        let listener = ResetThenNotify::delivering(base.updater(), || {});
        hazard_rate.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        FlatHazardRate {
            base,
            hazard_rate,
            _listener: listener,
        }
    }

    fn wrap(value: Rate) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    /// Quote-backed curve with a fixed reference date.
    pub fn new(
        reference_date: Date,
        hazard_rate: Handle<dyn Quote>,
        day_counter: DayCounter,
    ) -> FlatHazardRate {
        let base = TermStructureBase::with_reference_date(reference_date, None, Some(day_counter));
        Self::assemble(base, hazard_rate)
    }

    /// Value-backed curve with a fixed reference date.
    pub fn with_rate(
        reference_date: Date,
        hazard_rate: Rate,
        day_counter: DayCounter,
    ) -> FlatHazardRate {
        Self::new(reference_date, Self::wrap(hazard_rate), day_counter)
    }

    /// Quote-backed curve whose reference date moves off the evaluation date.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        hazard_rate: Handle<dyn Quote>,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> FlatHazardRate {
        let base =
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings);
        Self::assemble(base, hazard_rate)
    }

    /// Value-backed curve whose reference date moves off the evaluation date.
    pub fn moving_with_rate(
        settlement_days: Natural,
        calendar: Calendar,
        hazard_rate: Rate,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> FlatHazardRate {
        Self::moving(
            settlement_days,
            calendar,
            Self::wrap(hazard_rate),
            day_counter,
            settings,
        )
    }

    fn hazard_rate_value(&self) -> QlResult<Rate> {
        self.hazard_rate.current_link()?.value()
    }
}

impl AsObservable for FlatHazardRate {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for FlatHazardRate {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl HazardRateStructure for FlatHazardRate {
    fn hazard_rate_curve_impl(&self, _t: Time) -> QlResult<Rate> {
        self.hazard_rate_value()
    }
}

impl DefaultProbabilityTermStructure for FlatHazardRate {
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        Ok((-self.hazard_rate_value()? * t).exp())
    }

    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        self.default_density_from_hazard_rate(t)
    }

    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.hazard_rate_curve_impl(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Flag, as_observer};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::timeunit::TimeUnit;

    const HAZARD_RATE: Rate = 0.0100;
    const TOLERANCE: Real = 1.0e-10;
    const N: usize = 20;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn handle(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
        Handle::new(quote.clone() as Shared<dyn Quote>)
    }

    /// C++ `calendar.advance(d, 1, Years)`, whose defaults are `Following` and
    /// `endOfMonth = false` (`ql/time/calendar.hpp:146-150`).
    fn one_year_on(calendar: &Calendar, d: Date) -> Date {
        calendar.advance(
            d,
            1,
            TimeUnit::Years,
            BusinessDayConvention::Following,
            false,
        )
    }

    /// `testFlatHazardRate` (`defaultprobabilitycurves.cpp:118-149`): the
    /// default probability is `1 - exp(-h t)` at twenty annual maturities.
    ///
    /// C++ measures every `t` from `startDate`, which is pinned to `today` at
    /// :131 and never reassigned inside the loop, while `endDate` walks forward
    /// cumulatively.
    #[test]
    fn flat_hazard_rate_reproduces_the_closed_form_default_probability() {
        let quote = shared(SimpleQuote::new(HAZARD_RATE));
        let day_counter = Actual360::new();
        let calendar = Target::new();
        let start_date = today();
        let curve = FlatHazardRate::new(today(), handle(&quote), day_counter.clone());

        let mut end_date = start_date;
        for _ in 0..N {
            end_date = one_year_on(&calendar, end_date);
            let t = day_counter.year_fraction(start_date, end_date);
            let probability = 1.0 - (-HAZARD_RATE * t).exp();
            let computed = curve.default_probability(t, false).unwrap();
            assert!(
                (probability - computed).abs() <= TOLERANCE,
                "failed to reproduce probability for flat hazard rate at t = {t}: \
                 calculated {computed}, expected {probability}"
            );
        }
    }

    /// `testDefaultProbability` (`defaultprobabilitycurves.cpp:55-116`): the
    /// two-argument default probability is the difference of the one-argument
    /// ones, and the time-argument overloads agree with the date-argument ones.
    #[test]
    fn default_probabilities_are_self_consistent_across_dates_and_times() {
        let quote = shared(SimpleQuote::new(HAZARD_RATE));
        let day_counter = Actual360::new();
        let calendar = Target::new();
        let curve = FlatHazardRate::new(today(), handle(&quote), day_counter.clone());

        let mut end_date = today();
        for _ in 0..N {
            let start_date = end_date;
            end_date = one_year_on(&calendar, end_date);

            let p_start = curve.default_probability_date(start_date, false).unwrap();
            let p_end = curve.default_probability_date(end_date, false).unwrap();
            let p_between_computed = curve
                .default_probability_between_dates(start_date, end_date, false)
                .unwrap();
            let p_between = p_end - p_start;
            assert!(
                (p_between - p_between_computed).abs() <= TOLERANCE,
                "failed to reproduce probability(d1, d2): \
                 calculated {p_between_computed}, expected {p_between}"
            );

            let t2 = day_counter.year_fraction(today(), end_date);
            let time_probability = curve.default_probability(t2, false).unwrap();
            assert!(
                (time_probability - p_end).abs() <= TOLERANCE,
                "single-time probability {time_probability} and single-date \
                 probability {p_end} do not match"
            );

            let t1 = day_counter.year_fraction(today(), start_date);
            let time_probability = curve.default_probability_between(t1, t2, false).unwrap();
            assert!(
                (time_probability - p_between_computed).abs() <= TOLERANCE,
                "double-time probability {time_probability} and double-date \
                 probability {p_between_computed} do not match"
            );
        }
    }

    /// Neither ported oracle reads the density or the hazard rate, so the
    /// closed forms behind `hazardRateImpl` (`flathazardrate.hpp:59`) and the
    /// adapter's `h(t) S(t)` are pinned directly.
    #[test]
    fn density_and_hazard_rate_match_their_closed_forms() {
        let curve = FlatHazardRate::with_rate(today(), HAZARD_RATE, Actual360::new());
        for t in [0.0_f64, 0.5, 1.0, 5.0, 20.0] {
            let survival = (-HAZARD_RATE * t).exp();
            assert!((curve.survival_probability(t, false).unwrap() - survival).abs() <= TOLERANCE);
            assert!(
                (curve.default_density(t, false).unwrap() - HAZARD_RATE * survival).abs()
                    <= TOLERANCE
            );
            assert!((curve.hazard_rate(t, false).unwrap() - HAZARD_RATE).abs() <= TOLERANCE);
        }
    }

    #[test]
    fn quote_change_notifies_observers_and_refreshes_the_curve() {
        let quote = shared(SimpleQuote::new(HAZARD_RATE));
        let curve = FlatHazardRate::new(today(), handle(&quote), Actual360::new());
        assert!(
            (curve.survival_probability(2.0, false).unwrap() - (-0.02_f64).exp()).abs()
                <= TOLERANCE
        );

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        quote.set_value(0.0200);

        assert!(
            Flag::is_up(&flag),
            "quote change must reach curve observers"
        );
        assert!(
            (curve.survival_probability(2.0, false).unwrap() - (-0.04_f64).exp()).abs()
                <= TOLERANCE
        );
        assert!((curve.hazard_rate(1.0, false).unwrap() - 0.0200).abs() <= TOLERANCE);
    }

    #[test]
    fn moving_curve_follows_the_evaluation_date() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let curve = FlatHazardRate::moving_with_rate(
            2,
            Target::new(),
            HAZARD_RATE,
            Actual360::new(),
            settings.clone(),
        );
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(19, Month::January, 2026)
        );

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        settings.set_evaluation_date(Date::new(16, Month::January, 2026));

        assert!(Flag::is_up(&flag));
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(20, Month::January, 2026)
        );
        let survival = curve
            .survival_probability_date(Date::new(20, Month::January, 2027), false)
            .unwrap();
        assert!((survival - (-HAZARD_RATE * 365.0 / 360.0).exp()).abs() <= TOLERANCE);
    }

    #[test]
    fn empty_or_unset_quotes_error_instead_of_pricing() {
        let curve = FlatHazardRate::new(today(), Handle::empty(), Actual360::new());
        assert!(curve.survival_probability(1.0, false).is_err());
        assert!(curve.hazard_rate(1.0, false).is_err());

        let unset = shared(SimpleQuote::default());
        let curve = FlatHazardRate::new(
            today(),
            Handle::new(unset as Shared<dyn Quote>),
            Actual360::new(),
        );
        assert!(curve.default_density(1.0, false).is_err());
    }
}
