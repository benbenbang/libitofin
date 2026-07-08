//! Chilean calendars.
//!
//! Port of `ql/time/calendars/chile.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Chilean markets.
///
/// QuantLib defaults this to [`Market::Sse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Santiago Stock Exchange.
    Sse,
}

/// Chilean calendar.
pub struct Chile;

impl Chile {
    /// Builds a Chilean calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Sse => shared(SseImpl),
        };
        Calendar::from_impl(imp)
    }
}

// Celebrated on the Winter Solstice day, except in 2021, when it was the day
// after. The table is indexed by `y - 2021` and only covers 2021..=2199 (as in
// QuantLib); beyond that range we return `false` to avoid an out-of-bounds
// panic (QuantLib reads out of bounds there).
fn is_aboriginal_people_day(d: i32, m: Month, y: i32) -> bool {
    const ABORIGINAL_PEOPLE_DAY: [u8; 179] = [
        21, 21, 21, 20, 20, 21, 21, 20, 20, // 2021-2029
        21, 21, 20, 20, 21, 21, 20, 20, 21, 21, // 2030-2039
        20, 20, 21, 21, 20, 20, 21, 21, 20, 20, // 2040-2049
        20, 21, 20, 20, 20, 21, 20, 20, 20, 21, // 2050-2059
        20, 20, 20, 21, 20, 20, 20, 21, 20, 20, // 2060-2069
        20, 21, 20, 20, 20, 21, 20, 20, 20, 20, // 2070-2079
        20, 20, 20, 20, 20, 20, 20, 20, 20, 20, // 2080-2089
        20, 20, 20, 20, 20, 20, 20, 20, 20, 20, // 2090-2099
        21, 21, 21, 21, 21, 21, 21, 21, 20, 21, // 2100-2109
        21, 21, 20, 21, 21, 21, 20, 21, 21, 21, // 2110-2119
        20, 21, 21, 21, 20, 21, 21, 21, 20, 21, // 2120-2129
        21, 21, 20, 21, 21, 21, 20, 20, 21, 21, // 2130-2139
        20, 20, 21, 21, 20, 20, 21, 21, 20, 20, // 2140-2149
        21, 21, 20, 20, 21, 21, 20, 20, 21, 21, // 2150-2159
        20, 20, 21, 21, 20, 20, 21, 21, 20, 20, // 2160-2169
        20, 21, 20, 20, 20, 21, 20, 20, 20, 21, // 2170-2179
        20, 20, 20, 21, 20, 20, 20, 21, 20, 20, // 2180-2189
        20, 21, 20, 20, 20, 21, 20, 20, 20, 20, // 2190-2199
    ];
    if m != Month::June || y < 2021 {
        return false;
    }
    let idx = (y - 2021) as usize;
    idx < ABORIGINAL_PEOPLE_DAY.len() && d == ABORIGINAL_PEOPLE_DAY[idx] as i32
}

struct SseImpl;

impl CalendarImpl for SseImpl {
    fn name(&self) -> String {
        "Santiago Stock Exchange".to_string()
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
        let em = western_easter_monday(y);

        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            || (d == 2 && m == Month::January && w == Weekday::Monday && y > 2016)
            // Papal visit in 2018
            || (d == 16 && m == Month::January && y == 2018)
            // Good Friday
            || (dd == em - 3)
            // Easter Saturday
            || (dd == em - 2)
            // Census Day in 2017
            || (d == 19 && m == Month::April && y == 2017)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Navy Day
            || (d == 21 && m == Month::May)
            // Day of Aboriginal People
            || is_aboriginal_people_day(d, m, y)
            // St. Peter and St. Paul
            || (d >= 26 && d <= 29 && m == Month::June && w == Weekday::Monday)
            || (d == 2 && m == Month::July && w == Weekday::Monday)
            // Our Lady of Mount Carmel
            || (d == 16 && m == Month::July)
            // Assumption Day
            || (d == 15 && m == Month::August)
            // Independence Day
            || (d == 16 && m == Month::September && y == 2022)
            || (d == 17
                && m == Month::September
                && ((w == Weekday::Monday && y >= 2007) || (w == Weekday::Friday && y > 2016)))
            || (d == 18 && m == Month::September)
            // Army Day
            || (d == 19 && m == Month::September)
            || (d == 20 && m == Month::September && w == Weekday::Friday && y >= 2007)
            // Discovery of Two Worlds
            || (d >= 9 && d <= 12 && m == Month::October && w == Weekday::Monday)
            || (d == 15 && m == Month::October && w == Weekday::Monday)
            // Reformation Day
            || (((d == 27 && m == Month::October && w == Weekday::Friday)
                || (d == 31
                    && m == Month::October
                    && w != Weekday::Tuesday
                    && w != Weekday::Wednesday)
                || (d == 2 && m == Month::November && w == Weekday::Friday))
                && y >= 2008)
            // All Saints' Day
            || (d == 1 && m == Month::November)
            // Immaculate Conception
            || (d == 8 && m == Month::December)
            // Christmas Day
            || (d == 25 && m == Month::December)
            // New Year's Eve
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Chile::new(Market::Sse).name(), "Santiago Stock Exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Chile::new(Market::Sse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(21, Month::May, 2019))); // Navy Day
        assert!(c.is_holiday(Date::new(16, Month::July, 2019))); // Our Lady of Mount Carmel
        assert!(c.is_holiday(Date::new(15, Month::August, 2019))); // Assumption Day
        assert!(c.is_holiday(Date::new(18, Month::September, 2019))); // Independence Day
        assert!(c.is_holiday(Date::new(19, Month::September, 2019))); // Army Day
        assert!(c.is_holiday(Date::new(1, Month::November, 2019))); // All Saints' Day
        assert!(c.is_holiday(Date::new(8, Month::December, 2019))); // Immaculate Conception
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas Day
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // New Year's Eve
    }

    #[test]
    fn weekend_rule() {
        let c = Chile::new(Market::Sse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
