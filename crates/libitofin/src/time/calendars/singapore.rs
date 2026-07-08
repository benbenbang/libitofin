//! Singapore calendars.
//!
//! Port of `ql/time/calendars/singapore.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Singapore's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2026;

/// Singapore markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Singapore exchange.
    Sgx,
}

/// The Singapore calendars.
///
/// # Accuracy
///
/// Singapore's public/lunar holidays are tabulated (from QuantLib) only through
/// 2026. Querying a date after 2026 panics rather than silently returning an
/// unreliable business-day result.
pub struct Singapore;

impl Singapore {
    /// Builds a Singapore calendar for the given `market`.
    ///
    /// QuantLib defaults the market to `SGX`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Sgx => shared(SgxImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SgxImpl;

impl CalendarImpl for SgxImpl {
    fn name(&self) -> String {
        "Singapore exchange".to_string()
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
            "Singapore public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let em = western_easter_monday(y);

        if is_weekend_sat_sun(w)
            // New Year's Day
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Labor Day
            || (d == 1 && m == Month::May)
            // National Day
            || ((d == 9 || (d == 10 && w == Weekday::Monday)) && m == Month::August)
            // Christmas Day
            || (d == 25 && m == Month::December)

            // Chinese New Year
            || ((d == 22 || d == 23) && m == Month::January && y == 2004)
            || ((d == 9 || d == 10) && m == Month::February && y == 2005)
            || ((d == 30 || d == 31) && m == Month::January && y == 2006)
            || ((d == 19 || d == 20) && m == Month::February && y == 2007)
            || ((d == 7 || d == 8) && m == Month::February && y == 2008)
            || ((d == 26 || d == 27) && m == Month::January && y == 2009)
            || ((d == 15 || d == 16) && m == Month::January && y == 2010)
            || ((d == 23 || d == 24) && m == Month::January && y == 2012)
            || ((d == 11 || d == 12) && m == Month::February && y == 2013)
            || (d == 31 && m == Month::January && y == 2014)
            || (d == 1 && m == Month::February && y == 2014)

            // Hari Raya Haji
            || ((d == 1 || d == 2) && m == Month::February && y == 2004)
            || (d == 21 && m == Month::January && y == 2005)
            || (d == 10 && m == Month::January && y == 2006)
            || (d == 2 && m == Month::January && y == 2007)
            || (d == 20 && m == Month::December && y == 2007)
            || (d == 8 && m == Month::December && y == 2008)
            || (d == 27 && m == Month::November && y == 2009)
            || (d == 17 && m == Month::November && y == 2010)
            || (d == 26 && m == Month::October && y == 2012)
            || (d == 15 && m == Month::October && y == 2013)
            || (d == 6 && m == Month::October && y == 2014)

            // Vesak Poya Day
            || (d == 2 && m == Month::June && y == 2004)
            || (d == 22 && m == Month::May && y == 2005)
            || (d == 12 && m == Month::May && y == 2006)
            || (d == 31 && m == Month::May && y == 2007)
            || (d == 18 && m == Month::May && y == 2008)
            || (d == 9 && m == Month::May && y == 2009)
            || (d == 28 && m == Month::May && y == 2010)
            || (d == 5 && m == Month::May && y == 2012)
            || (d == 24 && m == Month::May && y == 2013)
            || (d == 13 && m == Month::May && y == 2014)

            // Deepavali
            || (d == 11 && m == Month::November && y == 2004)
            || (d == 8 && m == Month::November && y == 2007)
            || (d == 28 && m == Month::October && y == 2008)
            || (d == 16 && m == Month::November && y == 2009)
            || (d == 5 && m == Month::November && y == 2010)
            || (d == 13 && m == Month::November && y == 2012)
            || (d == 2 && m == Month::November && y == 2013)
            || (d == 23 && m == Month::October && y == 2014)

            // Diwali
            || (d == 1 && m == Month::November && y == 2005)

            // Hari Raya Puasa
            || ((d == 14 || d == 15) && m == Month::November && y == 2004)
            || (d == 3 && m == Month::November && y == 2005)
            || (d == 24 && m == Month::October && y == 2006)
            || (d == 13 && m == Month::October && y == 2007)
            || (d == 1 && m == Month::October && y == 2008)
            || (d == 21 && m == Month::September && y == 2009)
            || (d == 10 && m == Month::September && y == 2010)
            || (d == 20 && m == Month::August && y == 2012)
            || (d == 8 && m == Month::August && y == 2013)
            || (d == 28 && m == Month::July && y == 2014)
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2019-01/2019%20DT%20Calendar.pdf
        if y == 2019
            && (
                // Chinese New Year
                ((d == 5 || d == 6) && m == Month::February)
                // Vesak Poya Day
                || (d == 20 && m == Month::May)
                // Hari Raya Puasa
                || (d == 5 && m == Month::June)
                // Hari Raya Haji
                || (d == 12 && m == Month::August)
                // Deepavali
                || (d == 28 && m == Month::October)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2020-11/SGX%20Derivatives%20Trading%20Calendar%202020_Dec%20Update_D3.pdf
        if y == 2020
            && (
                // Chinese New Year
                (d == 27 && m == Month::January)
                // Vesak Poya Day
                || (d == 7 && m == Month::May)
                // Hari Raya Puasa
                || (d == 25 && m == Month::May)
                // Hari Raya Haji
                || (d == 31 && m == Month::July)
                // Deepavali
                || (d == 14 && m == Month::November)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2021-07/SGX_Derivatives%20Trading%20Calendar%202021%20%28Final%20-%20Jul%29.pdf
        if y == 2021
            && (
                // Chinese New Year
                (d == 12 && m == Month::February)
                // Hari Raya Puasa
                || (d == 13 && m == Month::May)
                // Vesak Poya Day
                || (d == 26 && m == Month::May)
                // Hari Raya Haji
                || (d == 20 && m == Month::July)
                // Deepavali
                || (d == 4 && m == Month::November)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2022-06/DT%20Trading%20Calendar%202022%20%28Final%29.pdf
        if y == 2022
            && (
                // Chinese New Year
                ((d == 1 || d == 2) && m == Month::February)
                // Labour Day
                || (d == 2 && m == Month::May)
                // Hari Raya Puasa
                || (d == 3 && m == Month::May)
                // Vesak Poya Day
                || (d == 16 && m == Month::May)
                // Hari Raya Haji
                || (d == 11 && m == Month::July)
                // Deepavali
                || (d == 24 && m == Month::October)
                // Christmas Day
                || (d == 26 && m == Month::December)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2023-01/SGX%20Calendar%202023_0.pdf
        if y == 2023
            && (
                // Chinese New Year
                ((d == 23 || d == 24) && m == Month::January)
                // Hari Raya Puasa
                || (d == 22 && m == Month::April)
                // Vesak Poya Day
                || (d == 2 && m == Month::June)
                // Hari Raya Haji
                || (d == 29 && m == Month::June)
                // Public holiday on polling day
                || (d == 1 && m == Month::September)
                // Deepavali
                || (d == 13 && m == Month::November)
            )
        {
            return false;
        }
        // https://api2.sgx.com/sites/default/files/2024-01/SGX%20Calendar%202024_2.pdf
        if y == 2024
            && (
                // Chinese New Year
                (d == 12 && m == Month::February)
                // Hari Raya Puasa
                || (d == 10 && m == Month::April)
                // Vesak Poya Day
                || (d == 22 && m == Month::May)
                // Hari Raya Haji
                || (d == 17 && m == Month::June)
                // Deepavali
                || (d == 31 && m == Month::October)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2025-07/DT%20Trading%20Calendar%202025%20%28updated%2031%20Jul%202025%29.pdf
        if y == 2025
            && (
                // Chinese New Year
                ((d == 29 || d == 30) && m == Month::January)
                // Hari Raya Puasa
                || (d == 31 && m == Month::March)
                // Vesak Poya Day
                || (d == 12 && m == Month::May)
                // Deepavali
                || (d == 20 && m == Month::October)
            )
        {
            return false;
        }

        // https://api2.sgx.com/sites/default/files/2026-01/SGX%20Calendar%202026_2.pdf
        if y == 2026
            && (
                // Chinese New Year
                ((d == 17 || d == 18) && m == Month::February)
                // Hari Raya Puasa
                || (d == 20 && m == Month::March)
                // Hari Raya Haji
                || (d == 27 && m == Month::May)
                // Vesak Day (Sunday May 31st, observed Monday June 1st)
                || (d == 1 && m == Month::June)
                // Deepavali (Sunday Nov 8th, observed Monday Nov 9th)
                || (d == 9 && m == Month::November)
            )
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against known Singapore holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Singapore::new(Market::Sgx).name(), "Singapore exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Singapore::new(Market::Sgx);
        // New Year's Day (unconditional on the 1st)
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        // Labour Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        // Christmas Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Singapore::new(Market::Sgx);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = Singapore::new(Market::Sgx);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Singapore::new(Market::Sgx);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
