//! North Macedonia calendars.
//!
//! Port of `ql/time/calendars/northmacedonia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, orthodox_easter_monday};
use crate::time::calendars::islamicholidays::moon_sighting::{is_eid_al_adha, is_eid_al_fitr};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which North Macedonia's Islamic (Eid) holidays are tabulated
/// via `moon_sighting` (matching QuantLib's data). Queries beyond this year
/// cannot be answered reliably and panic rather than silently omitting
/// holidays.
const HOLIDAY_HORIZON: Year = 2040;

/// Market handled by the North Macedonia calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Market {
    /// Macedonian Stock Exchange.
    MSE,
}

/// The North Macedonia calendar. The default market is [`Market::MSE`].
///
/// # Accuracy
///
/// North Macedonia's Islamic (Eid) holidays are tabulated (from QuantLib, via
/// the `moon_sighting` tables) only through 2040. Querying a date after 2040
/// panics rather than silently returning an unreliable business-day result.
pub struct NorthMacedonia;

impl NorthMacedonia {
    /// Builds a North Macedonia calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::MSE => shared(MseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct MseImpl;

impl CalendarImpl for MseImpl {
    fn name(&self) -> String {
        "Macedonian Stock Exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let dd = date.day_of_year();
        let m = date.month();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "North Macedonia Islamic (Eid) holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let em = orthodox_easter_monday(y);
        !(is_weekend_sat_sun(w)
            || is_eid_al_fitr(date)
            || is_eid_al_adha(date)
            // New Year
            || (d == 1 && m == Month::January)
            // Orthodox Christmas
            || (d == 7 && m == Month::January)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Saints Cyril and Methodius Day
            || (d == 24 && m == Month::May)
            // Republic Day
            || (d == 2 && m == Month::August)
            // Independence Day
            || (d == 8 && m == Month::September)
            // Day of People's Uprising
            || (d == 11 && m == Month::October)
            // Day of the Macedonian Revolutionary Struggle
            || (d == 23 && m == Month::October)
            // Saint Clement of Ohrid Day,
            || (d == 8 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            NorthMacedonia::new(Market::MSE).name(),
            "Macedonian Stock Exchange"
        );
    }

    #[test]
    fn fixed_holidays() {
        let c = NorthMacedonia::new(Market::MSE);
        for (d, m) in [
            (1, Month::January),   // New Year
            (7, Month::January),   // Orthodox Christmas
            (1, Month::May),       // Labour Day
            (24, Month::May),      // Saints Cyril and Methodius Day
            (2, Month::August),    // Republic Day
            (8, Month::September), // Independence Day
            (11, Month::October),  // Day of People's Uprising
            (23, Month::October),  // Day of the Macedonian Revolutionary Struggle
            (8, Month::December),  // Saint Clement of Ohrid Day
        ] {
            assert!(c.is_holiday(Date::new(d, m, 2020)), "{d} {m} 2020");
        }
    }

    #[test]
    fn weekend_rule() {
        let c = NorthMacedonia::new(Market::MSE);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. New Year is
        // unconditional.
        let c = NorthMacedonia::new(Market::MSE);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = NorthMacedonia::new(Market::MSE);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
