//! The JPY Libor index.
//!
//! Port of `ql/indexes/ibor/jpylibor.hpp`. `JPYLibor` is the Japanese Yen
//! LIBOR fixed by ICE: the [`Libor`] base configuration with family name
//! "JPYLibor", two settlement days, JPY, the [`Japan`] calendar as the
//! financial center, and an [`Actual360`] day counter (`jpylibor.hpp:44-53`).
//! This is the rate fixed in London by ICE, not the Tokyo TIBOR fixing. Like
//! the base, it is pure configuration: [`JpyLibor::new`] returns a plain
//! [`IborIndex`] whose maturity calendar is the joint
//! UK-Exchange-plus-Japan calendar.
//!
//! Deferred visibly: `DailyTenorJPYLibor` (needs the `DailyTenorLibor` base,
//! see the deferral note in [`libor`](crate::indexes::ibor::libor)).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::ibor::libor::Libor;
use crate::indexes::iborindex::IborIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::japan::Japan;
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;

/// The JPY Libor index (`ql/indexes/ibor/jpylibor.hpp`, C++ `JPYLibor`).
///
/// A zero-sized namespace for the JPYLibor constructor.
pub struct JpyLibor;

impl JpyLibor {
    /// Builds a JPY Libor index of the given `tenor` over the `forwarding`
    /// curve.
    ///
    /// Mirrors the C++ `JPYLibor::JPYLibor(tenor, h)` constructor: the
    /// [`Libor`] base with family name "JPYLibor", two settlement days, JPY,
    /// the [`Japan`] financial-center calendar, and an [`Actual360`] day
    /// counter. The base's guards apply: daily tenors are rejected
    /// (`DailyTenorJPYLibor` is not ported yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        Libor::new(
            "JPYLibor".into(),
            tenor,
            2,
            Currency::jpy(),
            Japan::new(),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for `JPYLibor`: the `jpylibor.hpp:44-53` configuration table
    //! plus a maturity pin over the actual joint calendar, mirroring the
    //! `usdlibor.rs` oracles.

    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::timeunit::TimeUnit;

    fn jpy_libor_6m(settings: Shared<Settings<Date>>) -> IborIndex {
        JpyLibor::new(Period::new(6, TimeUnit::Months), Handle::empty(), settings)
            .expect("a 6M JPYLibor tenor is valid")
    }

    /// The `jpylibor.hpp:44-53` configuration: name, fixing days, currency,
    /// day counter, the tenor-derived convention and end-of-month flag, and
    /// the calendar wiring (fixing on UK Exchange, NOT Japan; maturity on the
    /// joint UK-plus-Japan calendar).
    #[test]
    fn jpy_libor_carries_the_ice_configuration() {
        let index = jpy_libor_6m(shared(Settings::<Date>::new()));
        assert_eq!(index.name(), "JPYLibor6M Actual/360");
        assert_eq!(index.fixing_days(), 2);
        assert_eq!(index.currency(), &Currency::jpy());
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(index.fixing_calendar().name(), "London stock exchange");
        assert_eq!(
            index.maturity_calendar().name(),
            "JoinHolidays(London stock exchange, Japan)"
        );
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// `Libor::maturityDate` (`libor.cpp:102-113`) advances on the JOINT
    /// calendar, and this date discriminates joint-vs-UK-only:
    ///
    /// v = Monday 23 May 2022 (a business day in both calendars: not a
    /// Japanese May holiday, which end on the 5th-6th, and not a UK May bank
    /// holiday, which in 2022 were Monday 2 May and the Jubilee days 2-3
    /// June; not the last business day of May, so the end-of-month flag is
    /// inert). v + 6M lands on Wednesday 23 November 2022: Labor
    /// Thanksgiving Day, a fixed-date (d == 23, no Monday shift) Japanese
    /// holiday (`japan.rs:128`), a UK Exchange business day (the UK has no
    /// November bank holiday), and not month-end (the 30th is, so the EOM
    /// roll cannot mask the difference). The joint calendar rolls
    /// ModifiedFollowing to Thursday the 24th (a Japan business day: the
    /// d == 24 arm only fires on a Monday); UK-only would keep the 23rd.
    ///
    /// The clone round-trip pins that `clone_with` copies the maturity
    /// calendar: a `SwapRateHelper` prices off a clone (`ratehelpers.rs:834`),
    /// so a clone dropping it would silently pass a spot-date oracle.
    #[test]
    fn maturity_date_advances_on_the_joint_calendar() {
        let index = jpy_libor_6m(shared(Settings::<Date>::new()));
        let v = Date::new(23, Month::May, 2022);
        let maturity = index.maturity_date(v).unwrap();

        let uk = UnitedKingdom::new(UkMarket::Exchange);
        let joint = index.maturity_calendar();
        let expected = joint.advance_by_period(
            v,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        );
        let uk_only = uk.advance_by_period(
            v,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        );

        assert_eq!(maturity, expected);
        assert_eq!(maturity, Date::new(24, Month::November, 2022));
        assert_eq!(uk_only, Date::new(23, Month::November, 2022));
        assert_ne!(maturity, uk_only);

        let clone = index.clone_with(Handle::empty());
        assert_eq!(clone.maturity_date(v).unwrap(), maturity);
    }
}
