//! Hong Kong calendars.
//!
//! Port of `ql/time/calendars/hongkong.{hpp,cpp}`.
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

/// Last year for which Hong Kong's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2025;

/// Hong Kong markets.
///
/// QuantLib defaults to [`Market::HKEx`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Hong Kong stock exchange.
    HKEx,
}

/// Hong Kong calendars.
///
/// # Accuracy
///
/// Hong Kong's public/lunar holidays are tabulated (from QuantLib) only through
/// 2025. Querying a date after 2025 panics rather than silently returning an
/// unreliable business-day result.
pub struct HongKong;

impl HongKong {
    /// Builds a Hong Kong calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::HKEx => shared(HkexImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct HkexImpl;

impl CalendarImpl for HkexImpl {
    fn name(&self) -> String {
        "Hong Kong stock exchange".to_string()
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
            "Hong Kong public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let em = western_easter_monday(y);

        if is_weekend_sat_sun(w)
            // New Year's Day
            || ((d == 1 || ((d == 2) && w == Weekday::Monday))
                && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Labor Day
            || ((d == 1 || ((d == 2) && w == Weekday::Monday)) && m == Month::May)
            // SAR Establishment Day
            || ((d == 1 || ((d == 2) && w == Weekday::Monday)) && m == Month::July)
            // National Day
            || ((d == 1 || ((d == 2) && w == Weekday::Monday))
                && m == Month::October)
            // Christmas Day
            || (d == 25 && m == Month::December)
            // Boxing Day
            || (d == 26 && m == Month::December)
        {
            return false;
        }

        if y == 2004 {
            if
            // Lunar New Year
            ((d == 22 || d == 23 || d == 24) && m == Month::January)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 26 && m == Month::May)
                // Tuen Ng festival
                || (d == 22 && m == Month::June)
                // Mid-autumn festival
                || (d == 29 && m == Month::September)
                // Chung Yeung
                || (d == 22 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2005 {
            if
            // Lunar New Year
            ((d == 9 || d == 10 || d == 11) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 16 && m == Month::May)
                // Tuen Ng festival
                || (d == 11 && m == Month::June)
                // Mid-autumn festival
                || (d == 19 && m == Month::September)
                // Chung Yeung festival
                || (d == 11 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2006 {
            if
            // Lunar New Year
            ((d >= 28 && d <= 31) && m == Month::January)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 5 && m == Month::May)
                // Tuen Ng festival
                || (d == 31 && m == Month::May)
                // Mid-autumn festival
                || (d == 7 && m == Month::October)
                // Chung Yeung festival
                || (d == 30 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2007 {
            if
            // Lunar New Year
            ((d >= 17 && d <= 20) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 24 && m == Month::May)
                // Tuen Ng festival
                || (d == 19 && m == Month::June)
                // Mid-autumn festival
                || (d == 26 && m == Month::September)
                // Chung Yeung festival
                || (d == 19 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2008 {
            if
            // Lunar New Year
            ((d >= 7 && d <= 9) && m == Month::February)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 12 && m == Month::May)
                // Tuen Ng festival
                || (d == 9 && m == Month::June)
                // Mid-autumn festival
                || (d == 15 && m == Month::September)
                // Chung Yeung festival
                || (d == 7 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2009 {
            if
            // Lunar New Year
            ((d >= 26 && d <= 28) && m == Month::January)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 2 && m == Month::May)
                // Tuen Ng festival
                || (d == 28 && m == Month::May)
                // Mid-autumn festival
                || (d == 3 && m == Month::October)
                // Chung Yeung festival
                || (d == 26 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2010 {
            if
            // Lunar New Year
            ((d == 15 || d == 16) && m == Month::February)
                // Ching Ming Festival
                || (d == 6 && m == Month::April)
                // Buddha's birthday
                || (d == 21 && m == Month::May)
                // Tuen Ng festival
                || (d == 16 && m == Month::June)
                // Mid-autumn festival
                || (d == 23 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2011 {
            if
            // Lunar New Year
            ((d == 3 || d == 4) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 10 && m == Month::May)
                // Tuen Ng festival
                || (d == 6 && m == Month::June)
                // Mid-autumn festival
                || (d == 13 && m == Month::September)
                // Chung Yeung festival
                || (d == 5 && m == Month::October)
                // Second day after Christmas
                || (d == 27 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2012 {
            if
            // Lunar New Year
            (d >= 23 && d <= 25 && m == Month::January)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 10 && m == Month::May)
                // Mid-autumn festival
                || (d == 1 && m == Month::October)
                // Chung Yeung festival
                || (d == 23 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2013 {
            if
            // Lunar New Year
            (d >= 11 && d <= 13 && m == Month::February)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 17 && m == Month::May)
                // Tuen Ng festival
                || (d == 12 && m == Month::June)
                // Mid-autumn festival
                || (d == 20 && m == Month::September)
                // Chung Yeung festival
                || (d == 14 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2014 {
            if
            // Lunar New Year
            ((d == 31 && m == Month::January) || (d <= 3 && m == Month::February))
                // Buddha's birthday
                || (d == 6 && m == Month::May)
                // Tuen Ng festival
                || (d == 2 && m == Month::June)
                // Mid-autumn festival
                || (d == 9 && m == Month::September)
                // Chung Yeung festival
                || (d == 2 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2015 {
            if
            // Lunar New Year
            ((d == 19 && m == Month::February) || (d == 20 && m == Month::February))
                // The day following Easter Monday
                || (d == 7 && m == Month::April)
                // Buddha's birthday
                || (d == 25 && m == Month::May)
                // Tuen Ng festival
                || (d == 20 && m == Month::June)
                // The 70th anniversary day of the victory of the Chinese
                // people's war of resistance against Japanese aggression
                || (d == 3 && m == Month::September)
                // Mid-autumn festival
                || (d == 28 && m == Month::September)
                // Chung Yeung festival
                || (d == 21 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2016 {
            if
            // Lunar New Year
            ((d >= 8 && d <= 10) && m == Month::February)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Tuen Ng festival
                || (d == 9 && m == Month::June)
                // Mid-autumn festival
                || (d == 16 && m == Month::September)
                // Chung Yeung festival
                || (d == 10 && m == Month::October)
                // Second day after Christmas
                || (d == 27 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2017 {
            if
            // Lunar New Year
            ((d == 30 || d == 31) && m == Month::January)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 3 && m == Month::May)
                // Tuen Ng festival
                || (d == 30 && m == Month::May)
                // Mid-autumn festival
                || (d == 5 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2018 {
            if
            // Lunar New Year
            ((d == 16 && m == Month::February) || (d == 19 && m == Month::February))
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 22 && m == Month::May)
                // Tuen Ng festival
                || (d == 18 && m == Month::June)
                // Mid-autumn festival
                || (d == 25 && m == Month::September)
                // Chung Yeung festival
                || (d == 17 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2019 {
            if
            // Lunar New Year
            ((d >= 5 && d <= 7) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Tuen Ng festival
                || (d == 7 && m == Month::June)
                // Chung Yeung festival
                || (d == 7 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2020 {
            if
            // Lunar New Year
            ((d == 27 || d == 28) && m == Month::January)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 30 && m == Month::April)
                // Tuen Ng festival
                || (d == 25 && m == Month::June)
                // Mid-autumn festival
                || (d == 2 && m == Month::October)
                // Chung Yeung festival
                || (d == 26 && m == Month::October)
            {
                return false;
            }
        }

        // data from https://www.hkex.com.hk/-/media/hkex-market/services/circulars-and-notices/participant-and-members-circulars/sehk/2020/ce_sehk_ct_038_2020.pdf
        if y == 2021 {
            if
            // Lunar New Year
            ((d == 12 || d == 15) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 19 && m == Month::May)
                // Tuen Ng festival
                || (d == 14 && m == Month::June)
                // Mid-autumn festival
                || (d == 22 && m == Month::September)
                // Chung Yeung festival
                || (d == 14 && m == Month::October)
            {
                return false;
            }
        }

        // data from https://www.hkex.com.hk/-/media/HKEX-Market/Services/Circulars-and-Notices/Participant-and-Members-Circulars/SEHK/2021/ce_SEHK_CT_082_2021.pdf
        if y == 2022 {
            if
            // Lunar New Year
            ((d >= 1 && d <= 3) && m == Month::February)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 9 && m == Month::May)
                // Tuen Ng festival
                || (d == 3 && m == Month::June)
                // Mid-autumn festival
                || (d == 12 && m == Month::September)
                // Chung Yeung festival
                || (d == 4 && m == Month::October)
            {
                return false;
            }
        }

        // data from https://www.hkex.com.hk/-/media/HKEX-Market/Services/Circulars-and-Notices/Participant-and-Members-Circulars/SEHK/2022/ce_SEHK_CT_058_2022.pdf
        if y == 2023 {
            if
            // Lunar New Year
            ((d >= 23 && d <= 25) && m == Month::January)
                // Ching Ming Festival
                || (d == 5 && m == Month::April)
                // Buddha's birthday
                || (d == 26 && m == Month::May)
                // Tuen Ng festival
                || (d == 22 && m == Month::June)
                // Chung Yeung festival
                || (d == 23 && m == Month::October)
            {
                return false;
            }
        }

        // data from https://www.hkex.com.hk/-/media/HKEX-Market/Services/Circulars-and-Notices/Participant-and-Members-Circulars/SEHK/2023/ce_SEHK_CT_079_2023.pdf
        if y == 2024 {
            if
            // Lunar New Year
            ((d == 12 || d == 13) && m == Month::February)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 15 && m == Month::May)
                // Tuen Ng festival
                || (d == 10 && m == Month::June)
                // Mid-autumn festival
                || (d == 18 && m == Month::September)
                // Chung Yeung festival
                || (d == 11 && m == Month::October)
            {
                return false;
            }
        }

        // data from https://www.hkex.com.hk/-/media/HKEX-Market/Services/Circulars-and-Notices/Participant-and-Members-Circulars/SEHK/2024/ce_SEHK_CT_063_2024.pdf
        if y == 2025 {
            if
            // Lunar New Year
            ((d >= 29 && d <= 31) && m == Month::January)
                // Ching Ming Festival
                || (d == 4 && m == Month::April)
                // Buddha's birthday
                || (d == 5 && m == Month::May)
                // Mid-autumn festival
                || (d == 7 && m == Month::October)
                // Chung Yeung festival
                || (d == 29 && m == Month::October)
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
            HongKong::new(Market::HKEx).name(),
            "Hong Kong stock exchange"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = HongKong::new(Market::HKEx);
        // New Year's Day (day 1)
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        // Christmas Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        // Boxing Day
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = HongKong::new(Market::HKEx);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = HongKong::new(Market::HKEx);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = HongKong::new(Market::HKEx);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
