//! The three-calendar Ibor index.
//!
//! Port of `ql/indexes/ibor/custom.{hpp,cpp}`. [`CustomIborIndex`] is a
//! LIBOR-like index that takes separate fixing, value, and maturity calendars,
//! so each of the three date calculations rolls on its own calendar:
//!
//! - `fixingDate()` goes back on the value calendar, then adjusts `Preceding`
//!   on the fixing calendar;
//! - `valueDate()` advances on the value calendar and adjusts on the maturity
//!   calendar;
//! - `maturityDate()` advances on the maturity calendar.
//!
//! Typical LIBOR indexes use `fixingCalendar = valueCalendar = UK` with
//! `maturityCalendar = JoinHolidays(UK, CurrencyCalendar)` for non-EUR
//! currencies, and `fixingCalendar = JoinHolidays(UK, TARGET)` with
//! `valueCalendar = maturityCalendar = TARGET` for EUR.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::{InterestRateIndex, InterestRateIndexBase};
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate};

/// An [`IborIndex`] with separate fixing, value, and maturity calendars
/// (`ql/indexes/ibor/custom.hpp:28`).
///
/// The port keeps the C++ "is-an-`IborIndex`" relation as a newtype embedding
/// the configured [`IborIndex`] (the [`OvernightIndex`] precedent), carrying
/// the two extra calendars alongside; the base's single calendar plays the
/// fixing-calendar role. The three date methods C++ overrides
/// (`custom.cpp:23-41`) are overridden here on [`InterestRateIndex`].
///
/// [`forecast_fixing`](InterestRateIndex::forecast_fixing) cannot simply
/// delegate to the inner index: the C++ body lives on `IborIndex` but calls
/// `valueDate`/`maturityDate` virtually, resolving to this subclass's
/// three-calendar overrides. The override here re-derives both dates through
/// the overridden methods and delegates only the curve read
/// ([`forecast_fixing_between`](IborIndex::forecast_fixing_between)).
///
/// [`OvernightIndex`]: crate::indexes::iborindex::OvernightIndex
pub struct CustomIborIndex {
    ibor: Shared<IborIndex>,
    value_calendar: Calendar,
    maturity_calendar: Calendar,
}

impl CustomIborIndex {
    /// Builds the index over `forwarding`, mirroring the C++ constructor
    /// (`custom.cpp:8-21`): the base [`IborIndex`] takes `fixing_calendar` as
    /// its single calendar, and the value and maturity calendars are stored
    /// alongside.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        value_calendar: Calendar,
        maturity_calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        CustomIborIndex {
            ibor: shared(IborIndex::new(
                family_name,
                tenor,
                settlement_days,
                currency,
                fixing_calendar,
                convention,
                end_of_month,
                day_counter,
                forwarding,
                settings,
            )),
            value_calendar,
            maturity_calendar,
        }
    }

    /// The calendar value dates are advanced on (`valueCalendar`).
    pub fn value_calendar(&self) -> Calendar {
        self.value_calendar.clone()
    }

    /// The calendar maturity dates are advanced on (`maturityCalendar`).
    pub fn maturity_calendar(&self) -> Calendar {
        self.maturity_calendar.clone()
    }

    /// The convention applied when rolling the value date to maturity.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.ibor.business_day_convention()
    }

    /// Whether the maturity roll keeps to month ends.
    pub fn end_of_month(&self) -> bool {
        self.ibor.end_of_month()
    }

    /// Rebuilds this index onto a different forwarding curve, re-threading all
    /// three calendars along with every other configuration field (the C++
    /// `clone` override, `custom.cpp:43-49`).
    pub fn clone_with(&self, forwarding: Handle<dyn YieldTermStructure>) -> CustomIborIndex {
        CustomIborIndex {
            ibor: shared(self.ibor.clone_with(forwarding)),
            value_calendar: self.value_calendar.clone(),
            maturity_calendar: self.maturity_calendar.clone(),
        }
    }
}

impl InterestRateIndex for CustomIborIndex {
    fn base(&self) -> &InterestRateIndexBase {
        self.ibor.base()
    }

    /// The three-calendar fixing date (`custom.cpp:23-27`): back
    /// `fixing_days` business days on the value calendar, then a `Preceding`
    /// adjust on the fixing calendar - not the base's single `Following`
    /// advance.
    fn fixing_date(&self, value_date: Date) -> Date {
        let fixing_date = self.value_calendar.advance(
            value_date,
            -(self.fixing_days() as Integer),
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        self.fixing_calendar()
            .adjust(fixing_date, BusinessDayConvention::Preceding)
    }

    /// The three-calendar value date (`custom.cpp:29-36`): requires a valid
    /// fixing date on the fixing calendar, advances `fixing_days` business
    /// days on the value calendar, then adjusts `Following` on the maturity
    /// calendar.
    fn value_date(&self, fixing_date: Date) -> QlResult<Date> {
        require!(
            self.is_valid_fixing_date(fixing_date),
            "{fixing_date:?} is not a valid fixing date"
        );
        let d = self.value_calendar.advance(
            fixing_date,
            self.fixing_days() as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        Ok(self
            .maturity_calendar
            .adjust(d, BusinessDayConvention::Following))
    }

    /// The three-calendar maturity date (`custom.cpp:38-41`): the value date
    /// advanced by the tenor on the maturity calendar under the index's stored
    /// convention and end-of-month flag.
    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        Ok(self.maturity_calendar.advance_by_period(
            value_date,
            self.tenor(),
            self.ibor.business_day_convention(),
            self.ibor.end_of_month(),
        ))
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let d1 = self.value_date(fixing_date)?;
        let d2 = self.maturity_date(d1)?;
        let t = self.day_counter().year_fraction(d1, d2);
        let positive_time = t > 0.0;
        require!(
            positive_time,
            "cannot calculate forward rate between {d1:?} and {d2:?}: non positive time ({t}) using {} daycounter",
            self.day_counter().name()
        );
        self.ibor.forecast_fixing_between(d1, d2, t)
    }
}

#[cfg(test)]
mod tests {
    //! Oracle: `indexes.cpp testCustomIborIndex` (:152-204), the hand-placed
    //! bespoke-holiday date table exercised over the original index and its
    //! clone. With no weekends on a [`BespokeCalendar`], the calendar choice is
    //! the only variable, so each date assertion discriminates the
    //! three-calendar logic from a single-calendar mis-port.

    use super::*;
    use crate::time::calendars::bespokecalendar::BespokeCalendar;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    /// The `testCustomIborIndex` fixture (indexes.cpp:155-170): three bespoke
    /// calendars with hand-placed holidays and the "Custom Ibor" index over
    /// them.
    fn custom_ibor() -> (CustomIborIndex, Calendar, Calendar, Calendar) {
        let fix_cal = BespokeCalendar::new("Fixings").calendar();
        fix_cal.add_holiday(Date::new(8, Month::January, 2025));

        let val_cal = BespokeCalendar::new("Value").calendar();
        val_cal.add_holiday(Date::new(21, Month::January, 2025));

        let mat_cal = BespokeCalendar::new("Maturity").calendar();
        mat_cal.add_holiday(Date::new(7, Month::January, 2025));
        mat_cal.add_holiday(Date::new(15, Month::January, 2025));
        mat_cal.add_holiday(Date::new(23, Month::April, 2025));
        mat_cal.add_holiday(Date::new(30, Month::April, 2025));

        let settings = shared(Settings::<Date>::new());
        let index = CustomIborIndex::new(
            "Custom Ibor".into(),
            Period::new(3, TimeUnit::Months),
            2,
            Currency::new("", "", 0, "", "", 0),
            fix_cal.clone(),
            val_cal.clone(),
            mat_cal.clone(),
            BusinessDayConvention::ModifiedFollowing,
            true,
            Actual360::new(),
            Handle::empty(),
            settings,
        );
        (index, fix_cal, val_cal, mat_cal)
    }

    /// The calendar accessors (indexes.cpp:175-177): each of the three
    /// calendars survives construction and the clone.
    #[test]
    fn the_three_calendars_survive_construction_and_clone() {
        let (index, fix_cal, val_cal, mat_cal) = custom_ibor();
        let clone = index.clone_with(Handle::empty());
        for index in [&index, &clone] {
            assert_eq!(index.fixing_calendar(), fix_cal);
            assert_eq!(index.value_calendar(), val_cal);
            assert_eq!(index.maturity_calendar(), mat_cal);
        }
    }

    /// The invalid-fixing guard (indexes.cpp:179-181): 8 Jan 2025 is a
    /// fixing-calendar holiday, so it is not a valid fixing date and
    /// `value_date` errors (the Rust wording differs from the C++ message).
    #[test]
    fn value_date_rejects_a_fixing_calendar_holiday() {
        let (index, _, _, _) = custom_ibor();
        let clone = index.clone_with(Handle::empty());
        let holiday = Date::new(8, Month::January, 2025);
        for index in [&index, &clone] {
            assert!(!index.is_valid_fixing_date(holiday));
            assert!(index.value_date(holiday).is_err());
        }
    }

    /// The value-date table (indexes.cpp:183-188): advance on the value
    /// calendar, adjust on the maturity calendar.
    #[test]
    fn value_date_advances_on_value_and_adjusts_on_maturity() {
        let (index, _, _, _) = custom_ibor();
        let clone = index.clone_with(Handle::empty());
        for index in [&index, &clone] {
            assert_eq!(
                index
                    .value_date(Date::new(7, Month::January, 2025))
                    .unwrap(),
                Date::new(9, Month::January, 2025)
            );
            assert_eq!(
                index
                    .value_date(Date::new(13, Month::January, 2025))
                    .unwrap(),
                Date::new(16, Month::January, 2025)
            );
            assert_eq!(
                index
                    .value_date(Date::new(20, Month::January, 2025))
                    .unwrap(),
                Date::new(23, Month::January, 2025)
            );
        }
    }

    /// The fixing-date table (indexes.cpp:190-195): back on the value
    /// calendar, then a `Preceding` adjust on the fixing calendar.
    #[test]
    fn fixing_date_goes_back_on_value_and_adjusts_preceding_on_fixing() {
        let (index, _, _, _) = custom_ibor();
        let clone = index.clone_with(Handle::empty());
        for index in [&index, &clone] {
            assert_eq!(
                index.fixing_date(Date::new(23, Month::January, 2025)),
                Date::new(20, Month::January, 2025)
            );
            assert_eq!(
                index.fixing_date(Date::new(16, Month::January, 2025)),
                Date::new(14, Month::January, 2025)
            );
            assert_eq!(
                index.fixing_date(Date::new(10, Month::January, 2025)),
                Date::new(7, Month::January, 2025)
            );
        }
    }

    /// The maturity-date table (indexes.cpp:197-202): advance by the tenor on
    /// the maturity calendar under `ModifiedFollowing` on month-end.
    #[test]
    fn maturity_date_advances_on_the_maturity_calendar() {
        let (index, _, _, _) = custom_ibor();
        let clone = index.clone_with(Handle::empty());
        for index in [&index, &clone] {
            assert_eq!(
                index
                    .maturity_date(Date::new(23, Month::January, 2025))
                    .unwrap(),
                Date::new(24, Month::April, 2025)
            );
            assert_eq!(
                index
                    .maturity_date(Date::new(30, Month::January, 2025))
                    .unwrap(),
                Date::new(29, Month::April, 2025)
            );
            assert_eq!(
                index
                    .maturity_date(Date::new(28, Month::February, 2025))
                    .unwrap(),
                Date::new(31, Month::May, 2025)
            );
        }
    }
}
