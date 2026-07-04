//! Israeli calendars.
//!
//! Port of `ql/time/calendars/israel.{hpp,cpp}`.
//!
//! Jewish holidays follow the lunisolar calendar and are tabulated rather than
//! computed. All three markets treat Saturday/Sunday as the weekend
//! (`isWeekend`); the Tel Aviv (TASE) schedule additionally observed a
//! Friday/Saturday weekend for business-day purposes until 5th January 2026.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Israel's Jewish holidays are tabulated (matching
/// QuantLib's data). Queries beyond this year cannot be answered reliably and
/// panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2050;

/// Market handled by the Israeli calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar (deprecated in QuantLib; use [`Market::Tase`]).
    Settlement,
    /// Tel-Aviv stock exchange calendar.
    Tase,
    /// SHIR fixing calendar.
    Shir,
    /// Telbor fixing calendar.
    Telbor,
}

/// The Israeli calendar. The default market is [`Market::Tase`].
///
/// # Accuracy
///
/// Israel's Jewish holidays are tabulated (from QuantLib) only through 2050.
/// Querying a date after 2050 panics rather than silently returning an
/// unreliable business-day result.
pub struct Israel;

impl Israel {
    /// Builds an Israeli calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement | Market::Tase => shared(TelAvivImpl),
            Market::Telbor => shared(TelborImpl),
            Market::Shir => shared(ShirImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// The date `delta` days from `date`, or `None` if it leaves the supported range.
///
/// `Date`'s `Add`/`Sub` panic when the resulting serial number leaves the
/// supported range `[367, 109574]`. All tabulated Israel holidays fall well
/// inside that range, so a shift that escapes it can never match a holiday and
/// is safely reported as absent.
fn shift(date: Date, delta: i32) -> Option<Date> {
    let serial = date.serial_number() + delta;
    (Date::min_date().serial_number()..=Date::max_date().serial_number())
        .contains(&serial)
        .then(|| Date::from_serial(serial))
}

fn is_purim(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (21, Month::March, 2000),
        (9, Month::March, 2001),
        (26, Month::February, 2002),
        (18, Month::March, 2003),
        (7, Month::March, 2004),
        (25, Month::March, 2005),
        (14, Month::March, 2006),
        (4, Month::March, 2007),
        (21, Month::March, 2008),
        (10, Month::March, 2009),
        (28, Month::February, 2010),
        (20, Month::March, 2011),
        (8, Month::March, 2012),
        (24, Month::February, 2013),
        (16, Month::March, 2014),
        (5, Month::March, 2015),
        (24, Month::March, 2016),
        (12, Month::March, 2017),
        (1, Month::March, 2018),
        (21, Month::March, 2019),
        (10, Month::March, 2020),
        (26, Month::February, 2021),
        (17, Month::March, 2022),
        (7, Month::March, 2023),
        (24, Month::March, 2024),
        (14, Month::March, 2025),
        (3, Month::March, 2026),
        (23, Month::March, 2027),
        (12, Month::March, 2028),
        (1, Month::March, 2029),
        (19, Month::March, 2030),
        (9, Month::March, 2031),
        (26, Month::February, 2032),
        (15, Month::March, 2033),
        (5, Month::March, 2034),
        (25, Month::March, 2035),
        (13, Month::March, 2036),
        (1, Month::March, 2037),
        (21, Month::March, 2038),
        (10, Month::March, 2039),
        (28, Month::February, 2040),
        (17, Month::March, 2041),
        (6, Month::March, 2042),
        (26, Month::March, 2043),
        (13, Month::March, 2044),
        (3, Month::March, 2045),
        (22, Month::March, 2046),
        (12, Month::March, 2047),
        (28, Month::February, 2048),
        (18, Month::March, 2049),
        (8, Month::March, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_passover1st(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (20, Month::April, 2000),
        (8, Month::April, 2001),
        (28, Month::March, 2002),
        (17, Month::April, 2003),
        (6, Month::April, 2004),
        (24, Month::April, 2005),
        (13, Month::April, 2006),
        (3, Month::April, 2007),
        (20, Month::April, 2008),
        (9, Month::April, 2009),
        (30, Month::March, 2010),
        (19, Month::April, 2011),
        (7, Month::April, 2012),
        (26, Month::March, 2013),
        (15, Month::April, 2014),
        (4, Month::April, 2015),
        (23, Month::April, 2016),
        (11, Month::April, 2017),
        (31, Month::March, 2018),
        (20, Month::April, 2019),
        (9, Month::April, 2020),
        (28, Month::March, 2021),
        (16, Month::April, 2022),
        (6, Month::April, 2023),
        (23, Month::April, 2024),
        (13, Month::April, 2025),
        (2, Month::April, 2026),
        (22, Month::April, 2027),
        (11, Month::April, 2028),
        (31, Month::March, 2029),
        (18, Month::April, 2030),
        (8, Month::April, 2031),
        (27, Month::March, 2032),
        (14, Month::April, 2033),
        (4, Month::April, 2034),
        (24, Month::April, 2035),
        (12, Month::April, 2036),
        (31, Month::March, 2037),
        (20, Month::April, 2038),
        (9, Month::April, 2039),
        (29, Month::March, 2040),
        (16, Month::April, 2041),
        (5, Month::April, 2042),
        (25, Month::April, 2043),
        (12, Month::April, 2044),
        (2, Month::April, 2045),
        (21, Month::April, 2046),
        (11, Month::April, 2047),
        (29, Month::March, 2048),
        (17, Month::April, 2049),
        (7, Month::April, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_independence_day(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (10, Month::May, 2000),
        (26, Month::April, 2001),
        (17, Month::April, 2002),
        (7, Month::May, 2003),
        (27, Month::April, 2004),
        (12, Month::May, 2005),
        (3, Month::May, 2006),
        (24, Month::April, 2007),
        (8, Month::May, 2008),
        (29, Month::April, 2009),
        (20, Month::April, 2010),
        (10, Month::May, 2011),
        (26, Month::April, 2012),
        (16, Month::April, 2013),
        (6, Month::May, 2014),
        (23, Month::April, 2015),
        (12, Month::May, 2016),
        (2, Month::May, 2017),
        (19, Month::April, 2018),
        (9, Month::May, 2019),
        (29, Month::April, 2020),
        (15, Month::April, 2021),
        (5, Month::May, 2022),
        (26, Month::April, 2023),
        (14, Month::May, 2024),
        (1, Month::May, 2025),
        (22, Month::April, 2026),
        (12, Month::May, 2027),
        (2, Month::May, 2028),
        (19, Month::April, 2029),
        (8, Month::May, 2030),
        (29, Month::April, 2031),
        (15, Month::April, 2032),
        (4, Month::May, 2033),
        (25, Month::April, 2034),
        (15, Month::May, 2035),
        (1, Month::May, 2036),
        (21, Month::April, 2037),
        (10, Month::May, 2038),
        (28, Month::April, 2039),
        (18, Month::April, 2040),
        (7, Month::May, 2041),
        (24, Month::April, 2042),
        (14, Month::May, 2043),
        (3, Month::May, 2044),
        (20, Month::April, 2045),
        (10, Month::May, 2046),
        (1, Month::May, 2047),
        (16, Month::April, 2048),
        (6, Month::May, 2049),
        (27, Month::April, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_memorial_day(d: Date) -> bool {
    shift(d, 1).is_some_and(is_independence_day)
}

fn is_shavuot(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (9, Month::June, 2000),
        (28, Month::May, 2001),
        (17, Month::May, 2002),
        (6, Month::June, 2003),
        (26, Month::May, 2004),
        (13, Month::June, 2005),
        (2, Month::June, 2006),
        (23, Month::May, 2007),
        (9, Month::June, 2008),
        (29, Month::May, 2009),
        (19, Month::May, 2010),
        (8, Month::June, 2011),
        (27, Month::May, 2012),
        (15, Month::May, 2013),
        (4, Month::June, 2014),
        (24, Month::May, 2015),
        (12, Month::June, 2016),
        (31, Month::May, 2017),
        (20, Month::May, 2018),
        (9, Month::June, 2019),
        (29, Month::May, 2020),
        (17, Month::May, 2021),
        (5, Month::June, 2022),
        (26, Month::May, 2023),
        (12, Month::June, 2024),
        (2, Month::June, 2025),
        (22, Month::May, 2026),
        (11, Month::June, 2027),
        (31, Month::May, 2028),
        (20, Month::May, 2029),
        (7, Month::June, 2030),
        (28, Month::May, 2031),
        (16, Month::May, 2032),
        (3, Month::June, 2033),
        (24, Month::May, 2034),
        (13, Month::June, 2035),
        (1, Month::June, 2036),
        (20, Month::May, 2037),
        (9, Month::June, 2038),
        (29, Month::May, 2039),
        (18, Month::May, 2040),
        (5, Month::June, 2041),
        (25, Month::May, 2042),
        (14, Month::June, 2043),
        (1, Month::June, 2044),
        (22, Month::May, 2045),
        (10, Month::June, 2046),
        (31, Month::May, 2047),
        (18, Month::May, 2048),
        (6, Month::June, 2049),
        (27, Month::May, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_fast_day(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (10, Month::August, 2000),
        (29, Month::July, 2001),
        (18, Month::July, 2002),
        (7, Month::August, 2003),
        (27, Month::July, 2004),
        (14, Month::August, 2005),
        (3, Month::August, 2006),
        (24, Month::July, 2007),
        (10, Month::August, 2008),
        (30, Month::July, 2009),
        (20, Month::July, 2010),
        (9, Month::August, 2011),
        (29, Month::July, 2012),
        (16, Month::July, 2013),
        (5, Month::August, 2014),
        (26, Month::July, 2015),
        (14, Month::August, 2016),
        (1, Month::August, 2017),
        (22, Month::July, 2018),
        (11, Month::August, 2019),
        (30, Month::July, 2020),
        (18, Month::July, 2021),
        (7, Month::August, 2022),
        (27, Month::July, 2023),
        (13, Month::August, 2024),
        (3, Month::August, 2025),
        (23, Month::July, 2026),
        (12, Month::August, 2027),
        (1, Month::August, 2028),
        (22, Month::July, 2029),
        (8, Month::August, 2030),
        (29, Month::July, 2031),
        (18, Month::July, 2032),
        (4, Month::August, 2033),
        (25, Month::July, 2034),
        (14, Month::August, 2035),
        (3, Month::August, 2036),
        (21, Month::July, 2037),
        (10, Month::August, 2038),
        (31, Month::July, 2039),
        (19, Month::July, 2040),
        (6, Month::August, 2041),
        (27, Month::July, 2042),
        (16, Month::August, 2043),
        (2, Month::August, 2044),
        (23, Month::July, 2045),
        (12, Month::August, 2046),
        (1, Month::August, 2047),
        (19, Month::July, 2048),
        (8, Month::August, 2049),
        (28, Month::July, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_new_years_day(d: Date) -> bool {
    const DATES: &[(i32, Month, i32)] = &[
        (30, Month::September, 2000),
        (17, Month::September, 2001),
        (7, Month::September, 2002),
        (27, Month::September, 2003),
        (16, Month::September, 2004),
        (4, Month::October, 2005),
        (23, Month::September, 2006),
        (13, Month::September, 2007),
        (30, Month::September, 2008),
        (19, Month::September, 2009),
        (9, Month::September, 2010),
        (29, Month::September, 2011),
        (17, Month::September, 2012),
        (5, Month::September, 2013),
        (25, Month::September, 2014),
        (14, Month::September, 2015),
        (3, Month::October, 2016),
        (21, Month::September, 2017),
        (10, Month::September, 2018),
        (30, Month::September, 2019),
        (19, Month::September, 2020),
        (7, Month::September, 2021),
        (26, Month::September, 2022),
        (16, Month::September, 2023),
        (3, Month::October, 2024),
        (23, Month::September, 2025),
        (12, Month::September, 2026),
        (2, Month::October, 2027),
        (21, Month::September, 2028),
        (10, Month::September, 2029),
        (28, Month::September, 2030),
        (18, Month::September, 2031),
        (6, Month::September, 2032),
        (24, Month::September, 2033),
        (14, Month::September, 2034),
        (4, Month::October, 2035),
        (22, Month::September, 2036),
        (10, Month::September, 2037),
        (30, Month::September, 2038),
        (19, Month::September, 2039),
        (8, Month::September, 2040),
        (26, Month::September, 2041),
        (15, Month::September, 2042),
        (5, Month::October, 2043),
        (22, Month::September, 2044),
        (12, Month::September, 2045),
        (1, Month::October, 2046),
        (21, Month::September, 2047),
        (8, Month::September, 2048),
        (27, Month::September, 2049),
        (17, Month::September, 2050),
    ];
    DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
}

fn is_yom_kippur(d: Date) -> bool {
    shift(d, -9).is_some_and(is_new_years_day)
}

fn is_sukkot(d: Date) -> bool {
    shift(d, -5).is_some_and(is_yom_kippur)
}

fn is_simchat_torah(d: Date) -> bool {
    shift(d, -7).is_some_and(is_sukkot)
}

struct TelAvivImpl;

impl CalendarImpl for TelAvivImpl {
    fn name(&self) -> String {
        "Tel Aviv stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        w == Weekday::Saturday || w == Weekday::Sunday
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "Israel Jewish holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let switch_date = Date::new(5, Month::January, 2026);

        let weekend = if date >= switch_date {
            w == Weekday::Saturday || w == Weekday::Sunday
        } else {
            w == Weekday::Friday || w == Weekday::Saturday
        };

        !(weekend
            || is_purim(date)
            || (y <= 2020 && shift(date, 1).is_some_and(is_passover1st)) // Eve of Passover, until 2020
            || is_passover1st(date)
            || shift(date, -5).is_some_and(is_passover1st) // Eve of Passover VII, until 2020
            || shift(date, -6).is_some_and(is_passover1st) // Passover VII
            || is_memorial_day(date)
            || is_independence_day(date)
            || (y <= 2020 && shift(date, 1).is_some_and(is_shavuot)) // Eve of Shavuot, until 2020
            || is_shavuot(date)
            || is_fast_day(date)
            || (y <= 2019 && shift(date, 1).is_some_and(is_new_years_day)) // Eve of new year, until 2019
            || is_new_years_day(date)
            || shift(date, -1).is_some_and(is_new_years_day) // 2nd day of new year
            || shift(date, 1).is_some_and(is_yom_kippur) // Eve of Yom Kippur
            || is_yom_kippur(date)
            || shift(date, 1).is_some_and(is_sukkot) // Eve of Sukkot
            || is_sukkot(date)
            || shift(date, 1).is_some_and(is_simchat_torah) // Eve of Simchat Torah
            || is_simchat_torah(date))
    }
}

struct TelborImpl;

impl CalendarImpl for TelborImpl {
    fn name(&self) -> String {
        "Telbor fixing calendar".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        w == Weekday::Saturday || w == Weekday::Sunday
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "Israel Jewish holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // General Elections
            || (((d == 9 && m == Month::April) || (d == 17 && m == Month::September)) && y == 2019)
            || (d == 2 && m == Month::March && y == 2020)
            // Holiday abroad
            || (((d == 22 && m == Month::April) || (d == 27 && m == Month::May)) && y == 2019)
            || ((((d == 10 || d == 13) && m == Month::April)
                || ((d == 8 || d == 25) && m == Month::May))
                && y == 2020)
            // Purim
            || is_purim(date)
            || shift(date, -1).is_some_and(is_purim) // Shushan Purim
            // Passover I and Passover VII
            || shift(date, 1).is_some_and(is_passover1st) // Eve of Passover
            || is_passover1st(date)
            || shift(date, -6).is_some_and(is_passover1st) // Passover VII
            // Israel Independence Day
            || is_independence_day(date)
            // Feast of Shavuot (Pentecost)
            || is_shavuot(date)
            // Fast of Ninth of Av
            || is_fast_day(date)
            // Jewish New Year (Rosh Hashanah)
            || is_new_years_day(date)
            || shift(date, -1).is_some_and(is_new_years_day) // 2nd day of new year
            // Day of Atonement (Yom Kippur)
            || is_yom_kippur(date)
            // First Day of Sukkot (Tabernacles)
            || is_sukkot(date)
            // Rejoicing of the Law Festival (Simchat Torah)
            || is_simchat_torah(date)
            // last Monday of May (Spring Bank Holiday)
            || (d >= 25 && w == Weekday::Monday && m == Month::May && y != 2002 && y != 2012)
            // Christmas
            || (d == 25 && m == Month::December)
            // Day of Goodwill (Boxing Day)
            || (d == 26 && m == Month::December && y >= 2000 && y != 2020))
    }
}

struct ShirImpl;

impl CalendarImpl for ShirImpl {
    fn name(&self) -> String {
        "SHIR fixing calendar".to_string()
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
            "Israel Jewish holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        !(is_weekend_sat_sun(w)
            || is_purim(date)
            || shift(date, -1).is_some_and(is_purim) // Purim (Jerusalem)
            || shift(date, 1).is_some_and(is_passover1st) // Eve of Passover
            || is_passover1st(date)
            || shift(date, -6).is_some_and(is_passover1st) // Last day of Passover
            || is_independence_day(date)
            || is_shavuot(date)
            || is_fast_day(date)
            || shift(date, 1).is_some_and(is_new_years_day) // Eve of new year, until 2019
            || is_new_years_day(date)
            || shift(date, -1).is_some_and(is_new_years_day) // 2nd day of new year
            || shift(date, 1).is_some_and(is_yom_kippur) // Eve of Yom Kippur
            || is_yom_kippur(date)
            || is_sukkot(date)
            || is_simchat_torah(date)
            // one-off closings
            || (d == 27 && m == Month::February && y == 2024) // Municipal elections
            // holidays abroad
            || (d == 1 && m == Month::January) // Western New Year's day
            || dd == western_easter_monday(y) - 3 // Good Friday
            || (d >= 25 && w == Weekday::Monday && m == Month::May && y != 2022) // Spring Bank Holiday
            || (d == 3 && m == Month::June && y == 2022)
            || (d == 25 && m == Month::December) // Christmas
            || (d == 26 && m == Month::December) // Boxing day
            // other days when fixings were not published
            || (d == 1 && m == Month::November && y == 2022) // no idea why
            || (d == 2 && m == Month::January && y == 2023) // Maybe New Year's Day is adjusted to Monday?
            || (d == 10 && m == Month::April && y == 2023)) // Easter Monday, not a holiday in 2024 and 2025
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Israel::new(Market::Tase).name(), "Tel Aviv stock exchange");
        assert_eq!(
            Israel::new(Market::Settlement).name(),
            "Tel Aviv stock exchange"
        );
        assert_eq!(Israel::new(Market::Telbor).name(), "Telbor fixing calendar");
        assert_eq!(Israel::new(Market::Shir).name(), "SHIR fixing calendar");
    }

    #[test]
    fn telbor_new_years_day_is_holiday() {
        let c = Israel::new(Market::Telbor);
        // New Year's Day, January 1st, unconditional in the Telbor schedule.
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        // Christmas, December 25th.
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn purim_is_a_holiday_across_markets() {
        // 1 March 2018 was Purim.
        let purim = Date::new(1, Month::March, 2018);
        assert!(Israel::new(Market::Tase).is_holiday(purim));
        assert!(Israel::new(Market::Telbor).is_holiday(purim));
        assert!(Israel::new(Market::Shir).is_holiday(purim));
    }

    #[test]
    fn boundary_dates_do_not_panic() {
        // Shifted-date predicates must not panic near the lower supported range
        // boundary, where `date - N` would otherwise leave
        // `[Date::min_date(), Date::max_date()]`. The upper boundary
        // (`Date::max_date()`, year 2199) is beyond `HOLIDAY_HORIZON`, so it is
        // covered by `beyond_horizon_panics` rather than exercised here.
        for market in [
            Market::Settlement,
            Market::Tase,
            Market::Shir,
            Market::Telbor,
        ] {
            let c = Israel::new(market);
            let _ = c.is_holiday(Date::min_date());
            let _ = c.is_business_day(Date::min_date());
        }
    }

    #[test]
    fn weekend_rule() {
        let c = Israel::new(Market::Tase);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. New Year's Day is
        // an unconditional holiday in the Telbor schedule.
        let c = Israel::new(Market::Telbor);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Israel::new(Market::Tase);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
