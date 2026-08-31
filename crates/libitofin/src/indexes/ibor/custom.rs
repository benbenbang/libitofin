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
use crate::indexes::interestrateindex::{InterestRateIndex, InterestRateIndexBase};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Natural, Rate};

/// An [`IborIndex`] with separate fixing, value, and maturity calendars
/// (`ql/indexes/ibor/custom.hpp:28`).
///
/// The port keeps the C++ "is-an-`IborIndex`" relation as a newtype embedding
/// the configured [`IborIndex`] (the [`OvernightIndex`] precedent); the base's
/// single calendar plays the fixing-calendar role. The inner index carries the
/// other two calendars and the separate-calendar roll rule as data
/// ([`with_separate_calendars`](IborIndex::with_separate_calendars)), so the
/// three date methods C++ overrides (`custom.cpp:23-41`) are pure delegations
/// here and [`upcast`](Self::upcast) hands out an `IborIndex` that still rolls
/// on all three calendars.
///
/// [`forecast_fixing`](InterestRateIndex::forecast_fixing) delegates for the
/// same reason. The C++ body lives on `IborIndex` but calls
/// `valueDate`/`maturityDate` virtually, resolving to this subclass's
/// three-calendar overrides; folding the roll into the inner index makes its
/// own body resolve to those same dates, closing the composition trap at the
/// data level rather than by re-deriving both dates here.
///
/// [`OvernightIndex`]: crate::indexes::iborindex::OvernightIndex
pub struct CustomIborIndex {
    ibor: Shared<IborIndex>,
}

impl CustomIborIndex {
    /// Builds the index over `forwarding`, mirroring the C++ constructor
    /// (`custom.cpp:8-21`): the base [`IborIndex`] takes `fixing_calendar` as
    /// its single calendar, then takes the value and maturity calendars and
    /// the separate-calendar roll rule, which together reproduce the C++
    /// subclass's three date overrides.
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
            ibor: shared(
                IborIndex::new(
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
                )
                .with_separate_calendars(value_calendar, maturity_calendar),
            ),
        }
    }

    /// The inner [`IborIndex`] this index configures, sharing its identity (the
    /// C++ upcast of the same `shared_ptr`).
    ///
    /// The upcast is safe because the roll lives on the inner index as data:
    /// the returned index reproduces the three-calendar fixing, value, and
    /// maturity dates rather than the base's single-calendar ones, so a
    /// consumer taking a plain `IborIndex` (a rate helper, a coupon) prices
    /// this index the way C++ does.
    pub fn upcast(&self) -> Shared<IborIndex> {
        self.ibor.clone()
    }

    /// The calendar value dates are advanced on (`valueCalendar`).
    pub fn value_calendar(&self) -> Calendar {
        self.ibor.value_calendar()
    }

    /// The calendar maturity dates are advanced on (`maturityCalendar`).
    pub fn maturity_calendar(&self) -> Calendar {
        self.ibor.maturity_calendar()
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
    /// `clone` override, `custom.cpp:43-49`). The calendars and the roll rule
    /// ride along inside [`IborIndex::clone_with`].
    pub fn clone_with(&self, forwarding: Handle<dyn YieldTermStructure>) -> CustomIborIndex {
        CustomIborIndex {
            ibor: shared(self.ibor.clone_with(forwarding)),
        }
    }
}

impl InterestRateIndex for CustomIborIndex {
    fn base(&self) -> &InterestRateIndexBase {
        self.ibor.base()
    }

    /// The three-calendar fixing date (`custom.cpp:23-27`), answered by the
    /// inner index's separate-calendar roll: back `fixing_days` business days
    /// on the value calendar, then a `Preceding` adjust on the fixing calendar.
    fn fixing_date(&self, value_date: Date) -> Date {
        self.ibor.fixing_date(value_date)
    }

    /// The three-calendar value date (`custom.cpp:29-36`), answered by the
    /// inner index's separate-calendar roll: a valid fixing date on the fixing
    /// calendar, `fixing_days` business days forward on the value calendar,
    /// then a `Following` adjust on the maturity calendar.
    fn value_date(&self, fixing_date: Date) -> QlResult<Date> {
        self.ibor.value_date(fixing_date)
    }

    /// The three-calendar maturity date (`custom.cpp:38-41`): the value date
    /// advanced by the tenor on the maturity calendar under the index's stored
    /// convention and end-of-month flag.
    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        self.ibor.maturity_date(value_date)
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        self.ibor.forecast_fixing(fixing_date)
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
    use crate::indexes::index::Index;
    use crate::time::calendars::bespokecalendar::BespokeCalendar;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::timeunit::TimeUnit;

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

    /// The safe-upcast pin (no C++ oracle: C++ upcasts a `shared_ptr` for
    /// free). The plain `IborIndex` [`upcast`](CustomIborIndex::upcast) hands
    /// out must reproduce all three date tables above, since a consumer taking
    /// a concrete `IborIndex` gets exactly this index and nothing re-routes
    /// the calls back through the newtype.
    #[test]
    fn the_upcast_ibor_index_reproduces_the_three_calendar_dates() {
        let (index, _, _, _) = custom_ibor();
        let ibor = index.upcast();

        assert_eq!(
            ibor.value_date(Date::new(20, Month::January, 2025))
                .unwrap(),
            Date::new(23, Month::January, 2025)
        );
        assert_eq!(
            ibor.fixing_date(Date::new(23, Month::January, 2025)),
            Date::new(20, Month::January, 2025)
        );
        assert_eq!(
            ibor.maturity_date(Date::new(23, Month::January, 2025))
                .unwrap(),
            Date::new(24, Month::April, 2025)
        );
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

    /// The composition-trap pin (no C++ oracle covers it: the test above has
    /// no curve): `forecastFixing` lives on `IborIndex` but calls
    /// `valueDate`/`maturityDate` virtually, so the forecast must read the
    /// curve between this subclass's three-calendar dates (23 Jan / 24 Apr for
    /// a 20 Jan fixing), not the inner index's single-calendar ones
    /// (22 Jan / 22 Apr). Over a steep zero curve the two pairs give visibly
    /// different simple forwards, so a plain delegation cannot pass.
    #[test]
    fn forecast_fixing_reads_the_curve_between_the_overridden_dates() {
        use crate::math::interpolations::linear::Linear;
        use crate::termstructures::yields::InterpolatedZeroCurve;

        let curve = shared(
            InterpolatedZeroCurve::new(
                vec![
                    Date::new(2, Month::January, 2025),
                    Date::new(1, Month::July, 2025),
                ],
                vec![0.02, 0.10],
                Actual360::new(),
                Linear,
            )
            .expect("two well-ordered nodes"),
        ) as Shared<dyn YieldTermStructure>;

        let (index, _, _, _) = custom_ibor();
        let index = index.clone_with(Handle::new(Shared::clone(&curve)));

        let fixing = Date::new(20, Month::January, 2025);
        let d1 = index.value_date(fixing).unwrap();
        let d2 = index.maturity_date(d1).unwrap();
        assert_eq!(d1, Date::new(23, Month::January, 2025));
        assert_eq!(d2, Date::new(24, Month::April, 2025));

        let simple_forward = |d1: Date, d2: Date| {
            let disc1 = curve.discount_date(d1, false).unwrap();
            let disc2 = curve.discount_date(d2, false).unwrap();
            (disc1 / disc2 - 1.0) / Actual360::new().year_fraction(d1, d2)
        };
        let expected = simple_forward(d1, d2);
        let single_calendar = simple_forward(
            Date::new(22, Month::January, 2025),
            Date::new(22, Month::April, 2025),
        );
        assert!(
            (expected - single_calendar).abs() > 1.0e-4,
            "the fixture no longer discriminates the date pairs"
        );
        assert!((index.forecast_fixing(fixing).unwrap() - expected).abs() < 1.0e-15);
    }
}
