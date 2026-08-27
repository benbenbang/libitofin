//! The Libor family base constructor.
//!
//! Port of `ql/indexes/ibor/libor.{hpp,cpp}`. `Libor` is the base
//! configuration for all ICE LIBOR indexes but the EUR, O/N, and S/N ones:
//! the fixing (and value) calendar is UK Exchange, the maturity calendar is
//! the joint UK-plus-financial-center calendar under `JoinHolidays`
//! (`libor.cpp:78-84`), and the convention and end-of-month flag derive from
//! the tenor unit (`liborConvention`/`liborEOM`, `libor.cpp:31-56`).
//!
//! C++ makes `Libor` an `IborIndex` subclass overriding
//! `valueDate`/`maturityDate` (`libor.cpp:86-113`). The port folds those two
//! calendars into [`IborIndex`] as data instead, so the joint roll keeps
//! dispatching through every site that holds the concrete index (a
//! [`SwapRateHelper`] pricing through `MakeVanillaSwap`,
//! `makevanillaswap.rs:440`), where a newtype override would be bypassed.
//! [`Libor::new`] is thus a configuring constructor in the [`Euribor`] style,
//! returning a plain [`IborIndex`]; the named currencies (#306) hang off it.
//!
//! Deferred visibly: `DailyTenorLibor` (the O/N-S/N constructors need the
//! dedicated daily-tenor semantics) and the non-USD Libor currencies (#306).
//! `EurLibor` is a separate C++ subclass with its own spot lag, not a
//! branch of `libor.cpp` - it only guards against EUR (`libor.cpp:82-84`).
//!
//! [`Euribor`]: crate::indexes::ibor::Euribor
//! [`SwapRateHelper`]: crate::termstructures::yields::SwapRateHelper

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::jointcalendar::{JointCalendar, JointCalendarRule};
use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::Natural;
use crate::{fail, require};

/// The Libor base configuration (`ql/indexes/ibor/libor.hpp`).
///
/// A zero-sized namespace for the shared Libor constructor; the named
/// currency indexes ([`UsdLibor`](crate::indexes::ibor::UsdLibor), the rest
/// under #306) are thin wrappers over it.
pub struct Libor;

impl Libor {
    /// Builds a Libor index of the given `tenor` over the `forwarding` curve.
    ///
    /// Mirrors the C++ `Libor` constructor (`libor.cpp:60-85`): fixing on UK
    /// Exchange with the tenor-dependent convention and end-of-month flag,
    /// the maturity roll on the joint UK-plus-`financial_center_calendar`
    /// calendar. Daily tenors are rejected (they need the dedicated
    /// `DailyTenor` constructor, not ported yet), as is EUR (its Libor has a
    /// dedicated `EurLibor` constructor with a different spot lag, not
    /// ported yet).
    #[allow(clippy::new_ret_no_self)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        financial_center_calendar: Calendar,
        day_counter: DayCounter,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        let uk_exchange = UnitedKingdom::new(UkMarket::Exchange);
        let mut index = IborIndex::new(
            family_name,
            tenor,
            settlement_days,
            currency.clone(),
            uk_exchange.clone(),
            libor_convention(tenor)?,
            libor_eom(tenor)?,
            day_counter,
            forwarding,
            settings,
        );
        require!(
            index.tenor().units() != TimeUnit::Days,
            "for daily tenors ({}) dedicated DailyTenor constructor must be used",
            index.tenor()
        );
        require!(
            currency != Currency::eur(),
            "for EUR Libor dedicated EurLibor constructor must be used"
        );
        index.set_value_calendar(uk_exchange.clone());
        index.set_maturity_calendar(JointCalendar::of_two(
            uk_exchange,
            financial_center_calendar,
            JointCalendarRule::JoinHolidays,
        ));
        Ok(index)
    }
}

/// The tenor-dependent business-day convention (`liborConvention`).
pub(crate) fn libor_convention(tenor: Period) -> QlResult<BusinessDayConvention> {
    match tenor.units() {
        TimeUnit::Days | TimeUnit::Weeks => Ok(BusinessDayConvention::Following),
        TimeUnit::Months | TimeUnit::Years => Ok(BusinessDayConvention::ModifiedFollowing),
        _ => fail!("invalid time units"),
    }
}

/// The tenor-dependent end-of-month flag (`liborEOM`).
pub(crate) fn libor_eom(tenor: Period) -> QlResult<bool> {
    match tenor.units() {
        TimeUnit::Days | TimeUnit::Weeks => Ok(false),
        TimeUnit::Months | TimeUnit::Years => Ok(true),
        _ => fail!("invalid time units"),
    }
}

#[cfg(test)]
mod tests {
    //! Constructor-guard oracles for the Libor base (`libor.cpp:81-85`).

    use super::*;
    use crate::shared::shared;
    use crate::time::calendars::unitedstates::{Market as UsMarket, UnitedStates};
    use crate::time::daycounters::actual360::Actual360;

    fn libor(tenor: Period, currency: Currency) -> QlResult<IborIndex> {
        Libor::new(
            "TestLibor".into(),
            tenor,
            2,
            currency,
            UnitedStates::new(UsMarket::LiborImpact),
            Actual360::new(),
            Handle::empty(),
            shared(Settings::<Date>::new()),
        )
    }

    /// The daily-tenor guard: `QL_REQUIRE(tenor().units() != Days)` becomes a
    /// D4 `Err` carrying the C++ message.
    #[test]
    fn daily_tenor_is_rejected() {
        let err = libor(Period::new(3, TimeUnit::Days), Currency::usd())
            .err()
            .expect("daily tenors must be rejected");
        assert!(err.to_string().contains("dedicated DailyTenor constructor"));
    }

    /// The currency guard: `QL_REQUIRE(currency != EURCurrency())` becomes a
    /// D4 `Err` carrying the C++ message.
    #[test]
    fn eur_currency_is_rejected() {
        let err = libor(Period::new(3, TimeUnit::Months), Currency::eur())
            .err()
            .expect("EUR must be rejected");
        assert!(err.to_string().contains("dedicated EurLibor constructor"));
    }

    /// `liborConvention`/`liborEOM` (`libor.cpp:31-56`) keyed on the tenor
    /// unit: weeks roll `Following` off month-end, months roll
    /// `ModifiedFollowing` on month-end.
    #[test]
    fn convention_and_eom_derive_from_the_tenor_unit() {
        let weekly = libor(Period::new(1, TimeUnit::Weeks), Currency::usd()).unwrap();
        assert_eq!(
            weekly.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert!(!weekly.end_of_month());

        let monthly = libor(Period::new(3, TimeUnit::Months), Currency::usd()).unwrap();
        assert_eq!(
            monthly.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(monthly.end_of_month());
    }

    /// The calendar wiring (`libor.cpp:69-84`): fixing and value on UK
    /// Exchange, maturity on the joint UK-plus-financial-center calendar.
    #[test]
    fn calendars_are_uk_exchange_and_the_joint_calendar() {
        use crate::indexes::index::Index;
        let index = libor(Period::new(3, TimeUnit::Months), Currency::usd()).unwrap();
        assert_eq!(index.fixing_calendar().name(), "London stock exchange");
        assert_eq!(index.value_calendar().name(), "London stock exchange");
        assert_eq!(
            index.maturity_calendar().name(),
            "JoinHolidays(London stock exchange, US with Libor impact)"
        );
    }
}
