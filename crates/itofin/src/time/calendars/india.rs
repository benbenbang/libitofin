//! Indian calendars.
//!
//! Port of `ql/time/calendars/india.{hpp,cpp}`.
//!
//! The holiday clauses are transcribed verbatim from QuantLib; the style lints
//! below are allowed so the date logic can mirror the C++ clause-for-clause.
#![allow(
    clippy::collapsible_if,
    clippy::manual_range_contains,
    clippy::nonminimal_bool
)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which India's religious holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2026;

/// Indian markets.
///
/// QuantLib defaults to [`Market::Nse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// National Stock Exchange.
    Nse,
}

/// Indian calendars.
///
/// # Accuracy
///
/// India's religious holidays are tabulated (from QuantLib) only through 2026.
/// Querying a date after 2026 panics rather than silently returning an
/// unreliable business-day result.
pub struct India;

impl India {
    /// Builds an Indian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Nse => shared(NseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct NseImpl;

impl CalendarImpl for NseImpl {
    fn name(&self) -> String {
        "National Stock Exchange of India".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let dd = date.day_of_year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "India religious holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let em = western_easter_monday(y);

        if is_weekend_sat_sun(w)
            // Republic Day
            || (d == 26 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Ambedkar Jayanti
            || (d == 14 && m == Month::April)
            // May Day
            || (d == 1 && m == Month::May)
            // Independence Day
            || (d == 15 && m == Month::August)
            // Gandhi Jayanti
            || (d == 2 && m == Month::October)
            // Christmas
            || (d == 25 && m == Month::December)
        {
            return false;
        }

        if y == 2005 {
            // Moharram, Holi, Maharashtra Day, and Ramzan Id fall
            // on Saturday or Sunday in 2005
            if
            // Bakri Id
            (d == 21 && m == Month::January)
                // Ganesh Chaturthi
                || (d == 7 && m == Month::September)
                // Dasara
                || (d == 12 && m == Month::October)
                // Laxmi Puja
                || (d == 1 && m == Month::November)
                // Bhaubeej
                || (d == 3 && m == Month::November)
                // Guru Nanak Jayanti
                || (d == 15 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2006 {
            if
            // Bakri Id
            (d == 11 && m == Month::January)
                // Moharram
                || (d == 9 && m == Month::February)
                // Holi
                || (d == 15 && m == Month::March)
                // Ram Navami
                || (d == 6 && m == Month::April)
                // Mahavir Jayanti
                || (d == 11 && m == Month::April)
                // Maharashtra Day
                || (d == 1 && m == Month::May)
                // Bhaubeej
                || (d == 24 && m == Month::October)
                // Ramzan Id
                || (d == 25 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2007 {
            if
            // Bakri Id
            (d == 1 && m == Month::January)
                // Moharram
                || (d == 30 && m == Month::January)
                // Mahashivratri
                || (d == 16 && m == Month::February)
                // Ram Navami
                || (d == 27 && m == Month::March)
                // Maharashtra Day
                || (d == 1 && m == Month::May)
                // Buddha Pournima
                || (d == 2 && m == Month::May)
                // Laxmi Puja
                || (d == 9 && m == Month::November)
                // Bakri Id (again)
                || (d == 21 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2008 {
            if
            // Mahashivratri
            (d == 6 && m == Month::March)
                // Id-E-Milad
                || (d == 20 && m == Month::March)
                // Mahavir Jayanti
                || (d == 18 && m == Month::April)
                // Maharashtra Day
                || (d == 1 && m == Month::May)
                // Buddha Pournima
                || (d == 19 && m == Month::May)
                // Ganesh Chaturthi
                || (d == 3 && m == Month::September)
                // Ramzan Id
                || (d == 2 && m == Month::October)
                // Dasara
                || (d == 9 && m == Month::October)
                // Laxmi Puja
                || (d == 28 && m == Month::October)
                // Bhau bhij
                || (d == 30 && m == Month::October)
                // Gurunanak Jayanti
                || (d == 13 && m == Month::November)
                // Bakri Id
                || (d == 9 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2009 {
            if
            // Moharram
            (d == 8 && m == Month::January)
                // Mahashivratri
                || (d == 23 && m == Month::February)
                // Id-E-Milad
                || (d == 10 && m == Month::March)
                // Holi
                || (d == 11 && m == Month::March)
                // Ram Navmi
                || (d == 3 && m == Month::April)
                // Mahavir Jayanti
                || (d == 7 && m == Month::April)
                // Maharashtra Day
                || (d == 1 && m == Month::May)
                // Ramzan Id
                || (d == 21 && m == Month::September)
                // Dasara
                || (d == 28 && m == Month::September)
                // Bhau Bhij
                || (d == 19 && m == Month::October)
                // Gurunanak Jayanti
                || (d == 2 && m == Month::November)
                // Moharram (again)
                || (d == 28 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2010 {
            if
            // New Year's Day
            (d == 1 && m == Month::January)
                // Mahashivratri
                || (d == 12 && m == Month::February)
                // Holi
                || (d == 1 && m == Month::March)
                // Ram Navmi
                || (d == 24 && m == Month::March)
                // Ramzan Id
                || (d == 10 && m == Month::September)
                // Laxmi Puja
                || (d == 5 && m == Month::November)
                // Bakri Id
                || (d == 17 && m == Month::November)
                // Moharram
                || (d == 17 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2011 {
            if
            // Mahashivratri
            (d == 2 && m == Month::March)
                // Ram Navmi
                || (d == 12 && m == Month::April)
                // Ramzan Id
                || (d == 31 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 1 && m == Month::September)
                // Dasara
                || (d == 6 && m == Month::October)
                // Laxmi Puja
                || (d == 26 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 27 && m == Month::October)
                // Bakri Id
                || (d == 7 && m == Month::November)
                // Gurunanak Jayanti
                || (d == 10 && m == Month::November)
                // Moharram
                || (d == 6 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2012 {
            if
            // Mahashivratri
            (d == 20 && m == Month::February)
                // Holi
                || (d == 8 && m == Month::March)
                // Mahavir Jayanti
                || (d == 5 && m == Month::April)
                // Ramzan Id
                || (d == 20 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 19 && m == Month::September)
                // Dasara
                || (d == 24 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 14 && m == Month::November)
                // Gurunanak Jayanti
                || (d == 28 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2013 {
            if
            // Holi
            (d == 27 && m == Month::March)
                // Ram Navmi
                || (d == 19 && m == Month::April)
                // Mahavir Jayanti
                || (d == 24 && m == Month::April)
                // Ramzan Id
                || (d == 9 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 9 && m == Month::September)
                // Bakri Id
                || (d == 16 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 4 && m == Month::November)
                // Moharram
                || (d == 14 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2014 {
            if
            // Mahashivratri
            (d == 27 && m == Month::February)
                // Holi
                || (d == 17 && m == Month::March)
                // Ram Navmi
                || (d == 8 && m == Month::April)
                // Ramzan Id
                || (d == 29 && m == Month::July)
                // Ganesh Chaturthi
                || (d == 29 && m == Month::August)
                // Dasera
                || (d == 3 && m == Month::October)
                // Bakri Id
                || (d == 6 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 24 && m == Month::October)
                // Moharram
                || (d == 4 && m == Month::November)
                // Gurunank Jayanti
                || (d == 6 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2019 {
            if
            // Chatrapati Shivaji Jayanti
            (d == 19 && m == Month::February)
                // Mahashivratri
                || (d == 4 && m == Month::March)
                // Holi
                || (d == 21 && m == Month::March)
                // Annual Bank Closing
                || (d == 1 && m == Month::April)
                // Mahavir Jayanti
                || (d == 17 && m == Month::April)
                // Parliamentary Elections
                || (d == 29 && m == Month::April)
                // Ramzan Id
                || (d == 5 && m == Month::June)
                // Bakri Id
                || (d == 12 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 2 && m == Month::September)
                // Moharram
                || (d == 10 && m == Month::September)
                // Dasera
                || (d == 8 && m == Month::October)
                // General Assembly Elections in Maharashtra
                || (d == 21 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 28 && m == Month::October)
                // Gurunank Jayanti
                || (d == 12 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2020 {
            if
            // Chatrapati Shivaji Jayanti
            (d == 19 && m == Month::February)
                // Mahashivratri
                || (d == 21 && m == Month::February)
                // Holi
                || (d == 10 && m == Month::March)
                // Gudi Padwa
                || (d == 25 && m == Month::March)
                // Annual Bank Closing
                || (d == 1 && m == Month::April)
                // Ram Navami
                || (d == 2 && m == Month::April)
                // Mahavir Jayanti
                || (d == 6 && m == Month::April)
                // Buddha Pournima
                || (d == 7 && m == Month::May)
                // Ramzan Id
                || (d == 25 && m == Month::May)
                // Id-E-Milad
                || (d == 30 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 16 && m == Month::November)
                // Gurunank Jayanti
                || (d == 30 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2021 {
            if
            // Chatrapati Shivaji Jayanti
            (d == 19 && m == Month::February)
                // Mahashivratri
                || (d == 11 && m == Month::March)
                // Holi
                || (d == 29 && m == Month::March)
                // Gudi Padwa
                || (d == 13 && m == Month::April)
                // Mahavir Jayanti
                || (d == 14 && m == Month::April)
                // Ram Navami
                || (d == 21 && m == Month::April)
                // Buddha Pournima
                || (d == 26 && m == Month::May)
                // Bakri Id
                || (d == 21 && m == Month::July)
                // Ganesh Chaturthi
                || (d == 10 && m == Month::September)
                // Dasera
                || (d == 15 && m == Month::October)
                // Id-E-Milad
                || (d == 19 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 5 && m == Month::November)
                // Gurunank Jayanti
                || (d == 19 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2022 {
            if
            // Mahashivratri
            (d == 1 && m == Month::March)
                // Holi
                || (d == 18 && m == Month::March)
                // Ramzan Id
                || (d == 3 && m == Month::May)
                // Buddha Pournima
                || (d == 16 && m == Month::May)
                // Ganesh Chaturthi
                || (d == 31 && m == Month::August)
                // Dasera
                || (d == 5 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 26 && m == Month::October)
                // Gurunank Jayanti
                || (d == 8 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2023 {
            if
            // Holi
            (d == 7 && m == Month::March)
                // Gudi Padwa
                || (d == 22 && m == Month::March)
                // Ram Navami
                || (d == 30 && m == Month::March)
                // Mahavir Jayanti
                || (d == 4 && m == Month::April)
                // Buddha Pournima
                || (d == 5 && m == Month::May)
                // Bakri Id
                || (d == 29 && m == Month::June)
                // Parsi New year
                || (d == 16 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 19 && m == Month::September)
                // Id-E-Milad (was moved to Friday 29th)
                || (d == 29 && m == Month::September)
                // Dasera
                || (d == 24 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 14 && m == Month::November)
                // Gurunank Jayanti
                || (d == 27 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2024 {
            if
            // Special holiday
            (d == 22 && m == Month::January)
                // Chatrapati Shivaji Jayanti
                || (d == 19 && m == Month::February)
                // Mahashivratri
                || (d == 8 && m == Month::March)
                // Holi
                || (d == 25 && m == Month::March)
                // Annual Bank Closing
                || (d == 1 && m == Month::April)
                // Gudi Padwa
                || (d == 9 && m == Month::April)
                // Id-Ul-Fitr (Ramadan Eid)
                || (d == 11 && m == Month::April)
                // Ram Navami
                || (d == 17 && m == Month::April)
                // Mahavir Jayanti
                || (d == 21 && m == Month::April)
                // General Parliamentary Elections
                || (d == 20 && m == Month::May)
                // Buddha Pournima
                || (d == 23 && m == Month::May)
                // Bakri Eid
                || (d == 17 && m == Month::June)
                // Moharram
                || (d == 17 && m == Month::July)
                // Eid-E-Milad (estimated Sunday 15th or Monday 16th)
                || (d == 16 && m == Month::September)
                // Diwali-Laxmi Pujan
                || (d == 1 && m == Month::November)
                // Gurunank Jayanti
                || (d == 15 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2025 {
            if
            // Chatrapati Shivaji Jayanti
            (d == 19 && m == Month::February)
                // Mahashivratri
                || (d == 26 && m == Month::February)
                // Holi
                || (d == 14 && m == Month::March)
                // Ramzan Id (estimated Sunday 30th or Monday 31st)
                || (d == 31 && m == Month::March)
                // Mahavir Jayanti
                || (d == 10 && m == Month::April)
                // Buddha Pournima
                || (d == 12 && m == Month::May)
                // Id-E-Milad (estimated Thursday 4th or Friday 5th)
                || (d == 5 && m == Month::September)
                // Diwali - Balipratipada
                || (d == 22 && m == Month::October)
                // Gurunank Jayanti
                || (d == 5 && m == Month::November)
            {
                return false;
            }
        }

        if y == 2026 {
            if
            // Municipal Corporation Election - Maharashtra
            (d == 15 && m == Month::January)
                // Chatrapati Shivaji Jayanti
                || (d == 19 && m == Month::February)
                // Holi
                || (d == 3 && m == Month::March)
                // Gudi Padwa
                || (d == 19 && m == Month::March)
                // Ram Navami
                || (d == 26 && m == Month::March)
                // Mahavir Jayanti
                || (d == 31 && m == Month::March)
                // Annual Bank Closing
                || (d == 1 && m == Month::April)
                // Bakri Id
                || (d == 28 && m == Month::May)
                // Moharram
                || (d == 26 && m == Month::June)
                // Id-E-Milad
                || (d == 26 && m == Month::August)
                // Ganesh Chaturthi
                || (d == 14 && m == Month::September)
                // Dussehra
                || (d == 20 && m == Month::October)
                // Diwali - Balipratipada
                || (d == 10 && m == Month::November)
                // Gurunank Jayanti
                || (d == 24 && m == Month::November)
            {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            India::new(Market::Nse).name(),
            "National Stock Exchange of India"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = India::new(Market::Nse);
        // Republic Day
        assert!(c.is_holiday(Date::new(26, Month::January, 2019)));
        // Ambedkar Jayanti
        assert!(c.is_holiday(Date::new(14, Month::April, 2019)));
        // May Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        // Independence Day
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        // Gandhi Jayanti
        assert!(c.is_holiday(Date::new(2, Month::October, 2019)));
        // Christmas
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = India::new(Market::Nse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = India::new(Market::Nse);
        // Republic Day at the horizon year does not panic.
        assert!(c.is_holiday(Date::new(26, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = India::new(Market::Nse);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
