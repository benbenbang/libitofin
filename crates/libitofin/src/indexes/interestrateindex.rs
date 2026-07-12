//! The interest-rate index base.
//!
//! Port of `ql/indexes/interestrateindex.{hpp,cpp}`. `InterestRateIndex`
//! refines [`Index`] with a family name, tenor, fixing lag, currency and day
//! counter, the fixing/value-date algebra, and the fixing decision tree where a
//! historical fixing (D11 store) and a forecast curve meet. It stays abstract:
//! [`maturity_date`](InterestRateIndex::maturity_date) and
//! [`forecast_fixing`](InterestRateIndex::forecast_fixing) are the pure-virtual
//! members a concrete index (an `IborIndex`) supplies.
//!
//! Shared state lives in [`InterestRateIndexBase`], the analogue of
//! `CouponBase`: a concrete index embeds one, hands it back through
//! [`base`](InterestRateIndex::base), and the blanket
//! `impl<T: InterestRateIndex> Index for T` answers the whole [`Index`] surface
//! from it - so [`fixing`](Index::fixing), in particular, is written once and
//! cannot be re-derived wrongly per index (the lesson of the `Coupon`/`CashFlow`
//! blanket).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::indexes::index::Index;
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate};
use crate::{fail, require};

/// Shared state of every interest-rate index (`InterestRateIndex`'s members).
///
/// Built by [`new`](InterestRateIndexBase::new), which normalizes the tenor and
/// composes the index name exactly as the C++ constructor does, then registers
/// the index's forwarding observer with both the evaluation date and its own
/// fixing history (the two `registerWith` calls). Downstream observers register
/// with [`observable`](InterestRateIndexBase::observable); a change to either
/// source is re-broadcast through it, the port of `Index::update`.
pub struct InterestRateIndexBase {
    family_name: String,
    tenor: Period,
    fixing_days: Natural,
    currency: Currency,
    day_counter: DayCounter,
    fixing_calendar: Calendar,
    name: String,
    settings: Shared<Settings<Date>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl InterestRateIndexBase {
    /// Builds the shared state, wiring the index's observation of the
    /// evaluation date and its fixing history.
    ///
    /// The tenor is normalized as in the C++ constructor (a whole number of
    /// months becomes years, days left alone) and the name composed the same
    /// way (`ON`/`TN`/`SN` for a one-day tenor at 0/1/2 fixing days, otherwise
    /// the short period, then the day-counter name).
    pub fn new(
        family_name: String,
        tenor: Period,
        fixing_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        let tenor = normalize_tenor(tenor);
        let name = compose_name(&family_name, tenor, fixing_days, &day_counter);

        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        settings.register_eval_date_observer(&observer);
        settings.register_fixing_observer(&name, &observer);

        InterestRateIndexBase {
            family_name,
            tenor,
            fixing_days,
            currency,
            day_counter,
            fixing_calendar,
            name,
            settings,
            observable,
            forwarder,
        }
    }

    /// The observable the index broadcasts its changes through.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }

    /// The forwarding observer the index registers with its dependencies.
    ///
    /// Construction wires it to the evaluation date and the fixing history; a
    /// concrete index additionally registers it with its forwarding-curve
    /// handle, so that relinking or changing the curve re-broadcasts through
    /// [`observable`](InterestRateIndexBase::observable) - the port of
    /// `IborIndex`'s `registerWith(termStructure_)`.
    pub(crate) fn observer(&self) -> SharedMut<dyn Observer> {
        self.forwarder.clone() as SharedMut<dyn Observer>
    }
}

/// A whole number of months becomes years, mirroring the C++ constructor's
/// deliberately partial normalization (days are left untouched).
fn normalize_tenor(tenor: Period) -> Period {
    if tenor.units() == TimeUnit::Months && tenor.length() % 12 == 0 {
        Period::new(tenor.length() / 12, TimeUnit::Years)
    } else {
        tenor
    }
}

/// Composes the index name as `InterestRateIndex`'s constructor does.
fn compose_name(
    family_name: &str,
    tenor: Period,
    fixing_days: Natural,
    day_counter: &DayCounter,
) -> String {
    let period = if tenor == Period::new(1, TimeUnit::Days) {
        match fixing_days {
            0 => "ON".to_string(),
            1 => "TN".to_string(),
            2 => "SN".to_string(),
            _ => format!("{tenor}"),
        }
    } else {
        format!("{tenor}")
    };
    format!("{family_name}{period} {}", day_counter.name())
}

/// The interest-rate index interface (`InterestRateIndex`).
///
/// A concrete index supplies [`base`](InterestRateIndex::base) and the two
/// abstract calculations, [`maturity_date`](InterestRateIndex::maturity_date)
/// and [`forecast_fixing`](InterestRateIndex::forecast_fixing); the inspectors
/// and the [`fixing_date`](InterestRateIndex::fixing_date) /
/// [`value_date`](InterestRateIndex::value_date) algebra are provided, and can
/// be overridden by conventions that need it (the C++ `virtual` on those two).
pub trait InterestRateIndex {
    /// The embedded shared state.
    fn base(&self) -> &InterestRateIndexBase;

    /// The maturity date of the loan fixed on `value_date` (pure virtual in
    /// C++: the concrete index applies its tenor and convention).
    fn maturity_date(&self, value_date: Date) -> QlResult<Date>;

    /// The forecast fixing at `fixing_date` from the index's forwarding curve
    /// (pure virtual in C++).
    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate>;

    /// The family name (e.g. `Euribor`).
    fn family_name(&self) -> &str {
        &self.base().family_name
    }

    /// The tenor (normalized at construction).
    fn tenor(&self) -> Period {
        self.base().tenor
    }

    /// The number of fixing (settlement) days.
    fn fixing_days(&self) -> Natural {
        self.base().fixing_days
    }

    /// The index currency.
    fn currency(&self) -> &Currency {
        &self.base().currency
    }

    /// The index day counter.
    fn day_counter(&self) -> &DayCounter {
        &self.base().day_counter
    }

    /// The fixing date for a given `value_date`: `value_date` moved back
    /// `fixing_days` business days on the fixing calendar.
    fn fixing_date(&self, value_date: Date) -> Date {
        let base = self.base();
        base.fixing_calendar.advance(
            value_date,
            -(base.fixing_days as Integer),
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )
    }

    /// The value date for a given `fixing_date`: `fixing_date` moved forward
    /// `fixing_days` business days. Requires a valid fixing date, as in C++.
    fn value_date(&self, fixing_date: Date) -> QlResult<Date> {
        let base = self.base();
        require!(
            base.fixing_calendar.is_business_day(fixing_date),
            "{fixing_date:?} is not a valid fixing date"
        );
        Ok(base.fixing_calendar.advance(
            fixing_date,
            base.fixing_days as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        ))
    }
}

/// The whole [`Index`] surface, answered from an interest-rate index's base -
/// the port of `InterestRateIndex`'s `Index`-interface overrides, including the
/// `interestrateindex.cpp:63` fixing decision tree.
impl<T: InterestRateIndex> Index for T {
    fn name(&self) -> String {
        self.base().name.clone()
    }

    fn fixing_calendar(&self) -> Calendar {
        self.base().fixing_calendar.clone()
    }

    fn is_valid_fixing_date(&self, fixing_date: Date) -> bool {
        self.base().fixing_calendar.is_business_day(fixing_date)
    }

    fn settings(&self) -> &Settings<Date> {
        &self.base().settings
    }

    fn observable(&self) -> &Observable {
        &self.base().observable
    }

    fn fixing(&self, fixing_date: Date, forecast_todays_fixing: bool) -> QlResult<Rate> {
        require!(
            self.is_valid_fixing_date(fixing_date),
            "Fixing date {fixing_date:?} is not valid"
        );

        let today = match self.settings().evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: an index fixing needs a reference date"),
        };

        if fixing_date > today || (fixing_date == today && forecast_todays_fixing) {
            return self.forecast_fixing(fixing_date);
        }

        if fixing_date < today || self.settings().enforces_todays_historic_fixings() {
            return match self.settings().fixing(&self.name(), fixing_date) {
                Some(rate) => Ok(rate),
                None => fail!("Missing {} fixing for {fixing_date:?}", self.name()),
            };
        }

        if let Some(rate) = self.settings().fixing(&self.name(), fixing_date) {
            return Ok(rate);
        }
        self.forecast_fixing(fixing_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A concrete interest-rate index whose forecast is a fixed constant, so a
    /// test can tell a forecast (`FORECAST`) from a stored fixing by value.
    const FORECAST: Rate = 0.05;

    struct TestRateIndex {
        base: InterestRateIndexBase,
    }

    impl TestRateIndex {
        fn euribor(fixing_days: Natural, settings: Shared<Settings<Date>>) -> Self {
            TestRateIndex {
                base: InterestRateIndexBase::new(
                    "Euribor".into(),
                    Period::new(6, TimeUnit::Months),
                    fixing_days,
                    Currency::eur(),
                    Target::new(),
                    Actual360::new(),
                    settings,
                ),
            }
        }
    }

    impl InterestRateIndex for TestRateIndex {
        fn base(&self) -> &InterestRateIndexBase {
            &self.base
        }
        fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
            Ok(self.base.fixing_calendar.advance_by_period(
                value_date,
                self.base.tenor,
                BusinessDayConvention::Following,
                false,
            ))
        }
        fn forecast_fixing(&self, _fixing_date: Date) -> QlResult<Rate> {
            Ok(FORECAST)
        }
    }

    #[test]
    fn tenor_normalizes_and_name_is_composed() {
        let settings = shared(Settings::<Date>::new());
        let base = InterestRateIndexBase::new(
            "Euribor".into(),
            Period::new(12, TimeUnit::Months),
            2,
            Currency::eur(),
            Target::new(),
            Actual360::new(),
            settings,
        );
        // 12 Months collapses to 1 Year; name is family + short period + dc name.
        assert_eq!(base.tenor, Period::new(1, TimeUnit::Years));
        assert_eq!(base.name, "Euribor1Y Actual/360");
    }

    #[test]
    fn one_day_tenor_names_on_tn_sn_by_fixing_days() {
        let day = Period::new(1, TimeUnit::Days);
        let dc = Actual360::new();
        assert_eq!(compose_name("Eonia", day, 0, &dc), "EoniaON Actual/360");
        assert_eq!(compose_name("Eonia", day, 1, &dc), "EoniaTN Actual/360");
        assert_eq!(compose_name("Eonia", day, 2, &dc), "EoniaSN Actual/360");
        assert_eq!(compose_name("Eonia", day, 3, &dc), "Eonia1D Actual/360");
    }

    #[test]
    fn fixing_and_value_date_round_trip_across_a_weekend() {
        let settings = shared(Settings::<Date>::new());
        let index = TestRateIndex::euribor(2, settings);
        // Monday 15 June 2026; two business days back crosses the weekend to
        // Thursday 11 June, and forward again returns to the Monday.
        let value_date = Date::new(15, Month::June, 2026);
        let fixing_date = index.fixing_date(value_date);
        assert_eq!(fixing_date, Date::new(11, Month::June, 2026));
        assert_eq!(index.value_date(fixing_date).unwrap(), value_date);
    }

    #[test]
    fn value_date_rejects_an_invalid_fixing_date() {
        let settings = shared(Settings::<Date>::new());
        let index = TestRateIndex::euribor(2, settings);
        // Saturday 13 June 2026 is not a business day.
        assert!(index.value_date(Date::new(13, Month::June, 2026)).is_err());
    }

    #[test]
    fn fixing_on_an_invalid_date_is_an_error() {
        let today = Date::new(15, Month::June, 2026);
        let index = TestRateIndex::euribor(2, settings_on(today));
        assert!(
            index
                .fixing(Date::new(13, Month::June, 2026), false)
                .is_err()
        );
    }

    #[test]
    fn fixing_without_an_evaluation_date_is_an_error() {
        let settings = shared(Settings::<Date>::new());
        let index = TestRateIndex::euribor(2, settings);
        assert!(
            index
                .fixing(Date::new(15, Month::June, 2026), false)
                .is_err()
        );
    }

    #[test]
    fn future_date_forecasts() {
        let today = Date::new(15, Month::June, 2026);
        let index = TestRateIndex::euribor(2, settings_on(today));
        let tomorrow = Date::new(16, Month::June, 2026);
        assert_eq!(index.fixing(tomorrow, false).unwrap(), FORECAST);
    }

    #[test]
    fn today_forecasts_when_asked_even_with_a_stored_fixing() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = TestRateIndex::euribor(2, settings.clone());
        settings.add_fixing(&index.base.name, today, 0.01).unwrap();
        assert_eq!(index.fixing(today, true).unwrap(), FORECAST);
    }

    #[test]
    fn today_reads_a_stored_fixing_when_not_forecasting() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = TestRateIndex::euribor(2, settings.clone());
        settings.add_fixing(&index.base.name, today, 0.01).unwrap();
        assert_eq!(index.fixing(today, false).unwrap(), 0.01);
    }

    #[test]
    fn today_forecasts_when_no_stored_fixing_and_not_enforced() {
        let today = Date::new(15, Month::June, 2026);
        let index = TestRateIndex::euribor(2, settings_on(today));
        assert_eq!(index.fixing(today, false).unwrap(), FORECAST);
    }

    #[test]
    fn today_missing_fixing_is_an_error_when_enforced() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        settings.set_enforces_todays_historic_fixings(true);
        let index = TestRateIndex::euribor(2, settings);
        assert!(index.fixing(today, false).is_err());
    }

    #[test]
    fn today_reads_a_stored_fixing_when_enforced() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        settings.set_enforces_todays_historic_fixings(true);
        let index = TestRateIndex::euribor(2, settings.clone());
        settings.add_fixing(&index.base.name, today, 0.01).unwrap();
        assert_eq!(index.fixing(today, false).unwrap(), 0.01);
    }

    #[test]
    fn past_stored_fixing_is_returned() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = TestRateIndex::euribor(2, settings.clone());
        let past = Date::new(12, Month::June, 2026);
        settings.add_fixing(&index.base.name, past, 0.01).unwrap();
        assert_eq!(index.fixing(past, false).unwrap(), 0.01);
    }

    #[test]
    fn past_missing_fixing_is_an_error() {
        let today = Date::new(15, Month::June, 2026);
        let index = TestRateIndex::euribor(2, settings_on(today));
        let past = Date::new(12, Month::June, 2026);
        assert!(index.fixing(past, false).is_err());
    }

    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    #[test]
    fn adding_a_fixing_notifies_the_index() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = TestRateIndex::euribor(2, settings.clone());
        let flag = shared_mut(Flag { up: false });
        index
            .base
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        settings
            .add_fixing(&index.base.name, Date::new(12, Month::June, 2026), 0.01)
            .unwrap();
        assert!(flag.borrow().up);
    }

    #[test]
    fn changing_the_evaluation_date_notifies_the_index() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = TestRateIndex::euribor(2, settings.clone());
        let flag = shared_mut(Flag { up: false });
        index
            .base
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        settings.set_evaluation_date(Date::new(16, Month::June, 2026));
        assert!(flag.borrow().up);
    }
}
