//! The EUR Libor index.
//!
//! Port of `ql/indexes/ibor/eurlibor.{hpp,cpp}`. `EURLibor` is the Euro ICE
//! LIBOR fixed in London (use [`Euribor`] for the rate fixed by the ECB): two
//! settlement days, EUR, an [`Actual360`] day counter, fixing on the joint
//! UK-Exchange-plus-TARGET calendar, and value and maturity rolls on TARGET
//! alone through a private `target_` calendar (`eurlibor.cpp:60-105`).
//!
//! Divergence: C++ `EURLibor : IborIndex` overrides `fixingDate`, `valueDate`,
//! and `maturityDate`; those three bodies are behaviourally identical to
//! [`CustomIborIndex`] under the EUR calendar assignment (fixing calendar =
//! `JoinHolidays(UK Exchange, TARGET)`, value calendar = maturity calendar =
//! TARGET), so [`EurLibor::new`] returns a configured [`CustomIborIndex`]
//! rather than a second newtype with the same date logic. The one textual
//! difference is a proven no-op: `CustomIborIndex::value_date` ends with a
//! `maturity_calendar.adjust(_, Following)` that `EURLibor::valueDate` lacks,
//! but the TARGET advance already lands on a TARGET business day and the
//! maturity calendar is that same TARGET, so the adjust is identity here.
//! The tenor-derived convention and end-of-month flag
//! (`eurliborConvention`/`eurliborEOM`, `eurlibor.cpp:32-56`) are byte-for-byte
//! the Libor ones, so the port reuses [`libor_convention`]/[`libor_eom`].
//!
//! Deferred visibly: `DailyTenorEURLibor` and `EURLiborON`
//! (`eurlibor.hpp:64-84`) need the `DailyTenorLibor` base, see the deferral
//! note in [`libor`](crate::indexes::ibor::libor).
//!
//! [`Euribor`]: crate::indexes::ibor::Euribor

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::ibor::custom::CustomIborIndex;
use crate::indexes::ibor::libor::{libor_convention, libor_eom};
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::jointcalendar::{JointCalendar, JointCalendarRule};
use crate::time::calendars::target::Target;
use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The EUR Libor index (`ql/indexes/ibor/eurlibor.hpp`, C++ `EURLibor`).
///
/// A zero-sized namespace for the EURLibor constructors.
pub struct EurLibor;

impl EurLibor {
    /// Builds a EUR Libor index of the given `tenor` over the `forwarding`
    /// curve.
    ///
    /// Mirrors the C++ `EURLibor::EURLibor(tenor, h)` constructor
    /// (`eurlibor.cpp:60-78`): family name "EURLibor", two settlement days,
    /// EUR, fixing on `JoinHolidays(UK Exchange, TARGET)`, value and maturity
    /// on TARGET, the tenor-dependent convention and end-of-month flag, and an
    /// [`Actual360`] day counter. Daily tenors are rejected with the C++
    /// message (they need the dedicated `DailyTenorEURLibor` constructor, not
    /// ported yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CustomIborIndex> {
        let index = CustomIborIndex::new(
            "EURLibor".into(),
            tenor,
            2,
            Currency::eur(),
            JointCalendar::of_two(
                UnitedKingdom::new(UkMarket::Exchange),
                Target::new(),
                JointCalendarRule::JoinHolidays,
            ),
            Target::new(),
            Target::new(),
            libor_convention(tenor)?,
            libor_eom(tenor)?,
            Actual360::new(),
            forwarding,
            settings,
        );
        require!(
            index.tenor().units() != TimeUnit::Days,
            "for daily tenors ({}) dedicated DailyTenor constructor must be used",
            index.tenor()
        );
        Ok(index)
    }

    /// The 1-month EUR Libor index (`EURLibor1M`).
    pub fn one_month(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        Self::named(Period::new(1, TimeUnit::Months), forwarding, settings)
    }

    /// The 3-month EUR Libor index (`EURLibor3M`).
    pub fn three_months(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        Self::named(Period::new(3, TimeUnit::Months), forwarding, settings)
    }

    /// The 6-month EUR Libor index (`EURLibor6M`).
    pub fn six_months(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        Self::named(Period::new(6, TimeUnit::Months), forwarding, settings)
    }

    /// The 1-year EUR Libor index (`EURLibor1Y`).
    pub fn one_year(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        Self::named(Period::new(1, TimeUnit::Years), forwarding, settings)
    }

    /// Shared body for the named month/year helpers, whose fixed tenors can
    /// never trip the daily-tenor guard or the invalid-units branch.
    fn named(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> CustomIborIndex {
        Self::new(tenor, forwarding, settings)
            .expect("a month or year EUR Libor tenor is always valid")
    }
}

#[cfg(test)]
mod tests {
    //! Hand-derived oracles for `EURLibor`: the `eurlibor.cpp:60-78`
    //! configuration table plus one calendar pin per date role. EURLibor is
    //! absent from the QuantLib test-suite, so the date fixtures are derived
    //! by hand against `unitedkingdom.rs` (Exchange) and `target.rs`: the
    //! joint and TARGET calendars differ exactly on the UK bank holidays
    //! TARGET stays open for, and each pin sits on such a day so that each of
    //! the three calendar roles is discriminated independently.

    use super::*;
    use crate::indexes::index::Index;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::date::Month;

    fn eur_libor_3m(settings: Shared<Settings<Date>>) -> CustomIborIndex {
        EurLibor::three_months(Handle::empty(), settings)
    }

    /// The `eurlibor.cpp:60-78` configuration: name, fixing days, currency,
    /// day counter, the tenor-derived convention and end-of-month flag, and
    /// the calendar wiring (fixing on the joint UK-Exchange-plus-TARGET
    /// calendar, value and maturity on TARGET).
    #[test]
    fn eur_libor_carries_the_ice_configuration() {
        let index = eur_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(index.name(), "EURLibor3M Actual/360");
        assert_eq!(index.fixing_days(), 2);
        assert_eq!(index.currency(), &Currency::eur());
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(
            index.fixing_calendar().name(),
            "JoinHolidays(London stock exchange, TARGET)"
        );
        assert_eq!(index.value_calendar().name(), "TARGET");
        assert_eq!(index.maturity_calendar().name(), "TARGET");
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// The daily-tenor guard (`eurlibor.cpp:74-77`): `QL_REQUIRE` becomes a
    /// D4 `Err` carrying the C++ message. There is no EUR-currency guard here;
    /// that one lives in `libor.cpp` to keep EUR out of the plain Libor
    /// constructor.
    #[test]
    fn daily_tenor_is_rejected() {
        let err = EurLibor::new(
            Period::new(3, TimeUnit::Days),
            Handle::empty(),
            shared(Settings::<Date>::new()),
        )
        .err()
        .expect("daily tenors must be rejected");
        assert!(err.to_string().contains("dedicated DailyTenor constructor"));
    }

    /// The fixing calendar is the JOINT calendar: 30 August 2021 is the UK
    /// Summer Bank Holiday (last Monday of August) but a TARGET business day
    /// (TARGET has no August holiday), so only the joint calendar rejects it.
    /// A `fixing = TARGET` mutant that drops the UK join returns true.
    #[test]
    fn fixing_dates_reject_uk_holidays_open_on_target() {
        let index = eur_libor_3m(shared(Settings::<Date>::new()));
        let uk_holiday = Date::new(30, Month::August, 2021);
        assert!(Target::new().is_business_day(uk_holiday));
        assert!(!index.is_valid_fixing_date(uk_holiday));
    }

    /// The value date advances on TARGET, not the joint calendar: Thursday
    /// 26 August 2021 plus two TARGET business days is Friday the 27th, then
    /// Monday the 30th - TARGET stays open on the UK Summer Bank Holiday. A
    /// `value = joint` mutant skips the 30th and lands on Tuesday the 31st.
    #[test]
    fn value_date_advances_on_target_not_the_joint_calendar() {
        let index = eur_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(
            index
                .value_date(Date::new(26, Month::August, 2021))
                .unwrap(),
            Date::new(30, Month::August, 2021)
        );
    }

    /// The maturity date advances on TARGET, not the joint calendar: Monday
    /// 1 February 2021 plus 3M is Saturday 1 May, and `ModifiedFollowing`
    /// rolls to Monday 3 May - a TARGET business day (TARGET's Labour Day is
    /// the 1st) but the UK Early May Bank Holiday (first Monday of May). A
    /// `maturity = joint` mutant rolls on to Tuesday 4 May.
    #[test]
    fn maturity_date_advances_on_target_not_the_joint_calendar() {
        let index = eur_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(
            index
                .maturity_date(Date::new(1, Month::February, 2021))
                .unwrap(),
            Date::new(3, Month::May, 2021)
        );
    }

    /// The fixing date round-trips the value date (`eurlibor.cpp:79-84`): two
    /// TARGET business days back from Monday 30 August 2021 is Thursday the
    /// 26th, already a joint business day, so the `Preceding` adjust on the
    /// joint fixing calendar is inert and the round trip returns the fixing.
    #[test]
    fn fixing_date_round_trips_the_value_date() {
        let index = eur_libor_3m(shared(Settings::<Date>::new()));
        let fixing = Date::new(26, Month::August, 2021);
        let value = index.value_date(fixing).unwrap();
        assert_eq!(index.fixing_date(value), fixing);
    }
}
