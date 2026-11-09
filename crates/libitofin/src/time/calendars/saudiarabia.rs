//! Saudi Arabian calendar.
//!
//! Port of `ql/time/calendars/saudiarabia.{hpp,cpp}`.
//!
//! The Tadawul weekend rule changed on 29th June 2013 (Thursday/Friday before,
//! Friday/Saturday afterwards). `is_weekend` mirrors QuantLib's `isWeekend`
//! override (Friday/Saturday), while `is_business_day` uses a date-dependent
//! `is_true_weekend` helper.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Saudi Arabia's Eid al-Adha and Eid al-Fitr windows are
/// tabulated (matching QuantLib's data: both tables end in 2029). Past this
/// year the calendar falls through to its weekend and recurring rules, exactly
/// like QuantLib (which never checks the year).
pub const HOLIDAY_HORIZON: Year = 2029;

/// Market handled by the Saudi Arabian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Tadawul financial market.
    Tadawul,
}

/// The Saudi Arabian calendar. The default market is [`Market::Tadawul`].
///
/// # Accuracy
///
/// Saudi Arabia's Islamic (Eid) holiday windows are tabulated (from QuantLib)
/// only through 2029. After 2029 they are not observed and the calendar falls
/// through to its weekend and recurring rules (National Day, Founding Day),
/// exactly like QuantLib (which never checks the year).
pub struct SaudiArabia;

impl SaudiArabia {
    /// Builds a Saudi Arabian calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Tadawul => shared(TadawulImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// The Saudi weekend rule, which changed from 29th June 2013.
fn is_true_weekend(d: Date) -> bool {
    let w = d.weekday();
    if d < Date::new(29, Month::June, 2013) {
        w == Weekday::Thursday || w == Weekday::Friday
    } else {
        w == Weekday::Friday || w == Weekday::Saturday
    }
}

// Tadawul Eid holiday windows: through 2022 they are Eid-1 to Eid+4 around the
// published Gregorian dates, with exchange-documented ranges for 2012-2015;
// from 2023 through 2029 they follow the Saudi Exchange holiday calendar.

type Window = ((i32, Month, i32), (i32, Month, i32));

fn in_any_window(d: Date, windows: &[Window]) -> bool {
    windows.iter().any(|&((d1, m1, y1), (d2, m2, y2))| {
        d >= Date::new(d1, m1, y1) && d <= Date::new(d2, m2, y2)
    })
}

/// Whether `d` falls within a Tadawul Eid al-Adha holiday window.
fn is_eid_al_adha(d: Date) -> bool {
    const EID_AL_ADHA: &[Window] = &[
        ((6, Month::April, 1998), (11, Month::April, 1998)),
        ((26, Month::March, 1999), (31, Month::March, 1999)),
        ((15, Month::March, 2000), (20, Month::March, 2000)),
        ((4, Month::March, 2001), (9, Month::March, 2001)),
        ((22, Month::February, 2002), (27, Month::February, 2002)),
        ((11, Month::February, 2003), (16, Month::February, 2003)),
        ((31, Month::January, 2004), (5, Month::February, 2004)),
        ((20, Month::January, 2005), (25, Month::January, 2005)),
        ((9, Month::January, 2006), (14, Month::January, 2006)),
        ((30, Month::December, 2006), (4, Month::January, 2007)),
        ((19, Month::December, 2007), (24, Month::December, 2007)),
        ((7, Month::December, 2008), (12, Month::December, 2008)),
        ((26, Month::November, 2009), (1, Month::December, 2009)),
        ((15, Month::November, 2010), (20, Month::November, 2010)),
        ((5, Month::November, 2011), (10, Month::November, 2011)),
        ((25, Month::October, 2012), (3, Month::November, 2012)),
        ((11, Month::October, 2013), (19, Month::October, 2013)),
        ((3, Month::October, 2014), (11, Month::October, 2014)),
        ((22, Month::September, 2015), (27, Month::September, 2015)),
        ((10, Month::September, 2016), (15, Month::September, 2016)),
        ((31, Month::August, 2017), (5, Month::September, 2017)),
        ((22, Month::August, 2018), (27, Month::August, 2018)),
        ((11, Month::August, 2019), (16, Month::August, 2019)),
        ((30, Month::July, 2020), (4, Month::August, 2020)),
        ((19, Month::July, 2021), (24, Month::July, 2021)),
        ((9, Month::July, 2022), (14, Month::July, 2022)),
        ((23, Month::June, 2023), (1, Month::July, 2023)),
        ((14, Month::June, 2024), (22, Month::June, 2024)),
        ((5, Month::June, 2025), (10, Month::June, 2025)),
        ((22, Month::May, 2026), (30, Month::May, 2026)),
        ((16, Month::May, 2027), (20, Month::May, 2027)),
        ((3, Month::May, 2028), (9, Month::May, 2028)),
        ((22, Month::April, 2029), (26, Month::April, 2029)),
    ];
    in_any_window(d, EID_AL_ADHA)
}

/// Whether `d` falls within a Tadawul Eid al-Fitr holiday window.
fn is_eid_al_fitr(d: Date) -> bool {
    const EID_AL_FITR: &[Window] = &[
        ((15, Month::December, 2001), (20, Month::December, 2001)),
        ((4, Month::December, 2002), (9, Month::December, 2002)),
        ((24, Month::November, 2003), (29, Month::November, 2003)),
        ((12, Month::November, 2004), (17, Month::November, 2004)),
        ((2, Month::November, 2005), (7, Month::November, 2005)),
        ((22, Month::October, 2006), (27, Month::October, 2006)),
        ((11, Month::October, 2007), (16, Month::October, 2007)),
        ((29, Month::September, 2008), (4, Month::October, 2008)),
        ((19, Month::September, 2009), (24, Month::September, 2009)),
        ((9, Month::September, 2010), (14, Month::September, 2010)),
        ((29, Month::August, 2011), (3, Month::September, 2011)),
        ((16, Month::August, 2012), (24, Month::August, 2012)),
        ((6, Month::August, 2013), (12, Month::August, 2013)),
        ((25, Month::July, 2014), (3, Month::August, 2014)),
        ((18, Month::July, 2015), (21, Month::July, 2015)),
        ((5, Month::July, 2016), (10, Month::July, 2016)),
        ((24, Month::June, 2017), (29, Month::June, 2017)),
        ((14, Month::June, 2018), (19, Month::June, 2018)),
        ((3, Month::June, 2019), (8, Month::June, 2019)),
        ((23, Month::May, 2020), (28, Month::May, 2020)),
        ((12, Month::May, 2021), (17, Month::May, 2021)),
        ((1, Month::May, 2022), (6, Month::May, 2022)),
        ((17, Month::April, 2023), (25, Month::April, 2023)),
        ((4, Month::April, 2024), (14, Month::April, 2024)),
        ((27, Month::March, 2025), (2, Month::April, 2025)),
        ((17, Month::March, 2026), (23, Month::March, 2026)),
        ((7, Month::March, 2027), (11, Month::March, 2027)),
        ((27, Month::February, 2028), (2, Month::March, 2028)),
        ((12, Month::February, 2029), (18, Month::February, 2029)),
    ];
    in_any_window(d, EID_AL_FITR)
}

struct TadawulImpl;

impl CalendarImpl for TadawulImpl {
    fn holiday_horizon(&self) -> Option<Year> {
        Some(HOLIDAY_HORIZON)
    }

    fn name(&self) -> String {
        "Tadawul".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        w == Weekday::Friday || w == Weekday::Saturday
    }

    /// Date-dependent weekend (Thu/Fri before 29 June 2013, Fri/Sat after), so
    /// `holiday_list` filters weekends correctly across the switch.
    fn is_weekend_on(&self, date: Date) -> bool {
        is_true_weekend(date)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let w = date.weekday();

        !(is_true_weekend(date)
            || is_eid_al_adha(date)
            || is_eid_al_fitr(date)
            // National Day
            || (d == 23 && m == Month::September)
            // Founding Day (from 2022; rolls to the following Sunday when on Fri/Sat)
            || ((d == 22 || ((d == 23 || d == 24) && w == Weekday::Sunday))
                && m == Month::February
                && y >= 2022)
            // other one-shot holidays
            || (d == 26 && m == Month::February && y == 2011)
            || (d == 19 && m == Month::March && y == 2011))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(SaudiArabia::new(Market::Tadawul).name(), "Tadawul");
    }

    #[test]
    fn national_day_is_holiday() {
        let c = SaudiArabia::new(Market::Tadawul);
        // National Day, September 23rd, unconditional.
        assert!(c.is_holiday(Date::new(23, Month::September, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = SaudiArabia::new(Market::Tadawul);
        // isWeekend override is Friday/Saturday.
        assert!(c.is_weekend(Weekday::Friday));
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(!c.is_weekend(Weekday::Sunday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. National Day
        // (September 23rd) is unconditional.
        let c = SaudiArabia::new(Market::Tadawul);
        assert!(c.is_holiday(Date::new(23, Month::September, HOLIDAY_HORIZON)));
    }

    #[test]
    fn beyond_horizon_falls_through() {
        // QuantLib never checks the year: past the tables only weekends and
        // recurring rules apply, and tabulated dates do not recur.
        let c = SaudiArabia::new(Market::Tadawul);
        assert!(c.is_holiday(Date::new(23, Month::September, 2030)));
        assert!(c.is_holiday(Date::new(24, Month::February, 2030)));
        assert!(c.is_business_day(Date::new(5, Month::February, 2030)));
    }

    #[test]
    fn eid_windows_follow_quantlib_v1_44_tables() {
        let c = SaudiArabia::new(Market::Tadawul);
        // Eid al-Adha 2013 window 11-19 Oct (saudiarabia.cpp, EidAlAdha table).
        assert!(c.is_holiday(Date::new(13, Month::October, 2013)));
        // Eid al-Adha 2023 window 23 Jun-1 Jul.
        assert!(c.is_holiday(Date::new(28, Month::June, 2023)));
        assert!(c.is_business_day(Date::new(2, Month::July, 2023)));
    }

    #[test]
    fn founding_day_starts_in_2022() {
        let c = SaudiArabia::new(Market::Tadawul);
        assert!(c.is_business_day(Date::new(22, Month::February, 2021)));
        assert!(c.is_holiday(Date::new(22, Month::February, 2022)));
        assert!(c.is_holiday(Date::new(22, Month::February, 2027)));
    }

    /// Port of `testSaudiArabiaFoundingDayAndEidHolidays`
    /// (`test-suite/calendars.cpp:4304`, QuantLib v1.44).
    #[test]
    fn saudi_arabia_founding_day_and_eid_holidays() {
        let saudi = SaudiArabia::new(Market::Tadawul);
        let hol = |d, m, y| assert!(saudi.is_holiday(Date::new(d, m, y)), "{d} {m:?} {y}");
        let bus = |d, m, y| assert!(saudi.is_business_day(Date::new(d, m, y)), "{d} {m:?} {y}");

        hol(23, Month::February, 2025);

        hol(22, Month::February, 2026);
        bus(19, Month::February, 2026);

        hol(21, Month::July, 2015);
        bus(16, Month::July, 2015);
        bus(22, Month::July, 2015);

        hol(22, Month::September, 2015);
        bus(21, Month::September, 2015);
        bus(28, Month::September, 2015);

        hol(5, Month::June, 2025);
        hol(10, Month::June, 2025);
        bus(4, Month::June, 2025);
        bus(11, Month::June, 2025);

        hol(20, Month::March, 2026);
        bus(16, Month::March, 2026);
        bus(24, Month::March, 2026);

        hol(25, Month::May, 2026);
        bus(21, Month::May, 2026);
        bus(31, Month::May, 2026);
    }

    /// Every non-weekend holiday per year, generated from the v1.44 C++ source
    /// (`ql/time/calendars/saudiarabia.cpp:54-171`: Eid windows and Founding Day), not from this port.
    #[rustfmt::skip]
    const SOURCE_HOLIDAYS: &[(Year, &[(i32, i32)])] = &[
        (2022, &[(22, 2), (1, 5), (2, 5), (3, 5), (4, 5), (5, 5), (10, 7), (11, 7), (12, 7),
            (13, 7), (14, 7)]),
        (2023, &[(22, 2), (17, 4), (18, 4), (19, 4), (20, 4), (23, 4), (24, 4), (25, 4),
            (25, 6), (26, 6), (27, 6), (28, 6), (29, 6)]),
        (2024, &[(22, 2), (4, 4), (7, 4), (8, 4), (9, 4), (10, 4), (11, 4), (14, 4), (16, 6),
            (17, 6), (18, 6), (19, 6), (20, 6), (23, 9)]),
        (2025, &[(23, 2), (27, 3), (30, 3), (31, 3), (1, 4), (2, 4), (5, 6), (8, 6), (9, 6),
            (10, 6), (23, 9)]),
        (2026, &[(22, 2), (17, 3), (18, 3), (19, 3), (22, 3), (23, 3), (24, 5), (25, 5),
            (26, 5), (27, 5), (28, 5), (23, 9)]),
        (2027, &[(22, 2), (7, 3), (8, 3), (9, 3), (10, 3), (11, 3), (16, 5), (17, 5), (18, 5),
            (19, 5), (20, 5), (23, 9)]),
        (2028, &[(22, 2), (27, 2), (28, 2), (29, 2), (1, 3), (2, 3), (3, 5), (4, 5), (7, 5),
            (8, 5), (9, 5)]),
        (2029, &[(12, 2), (13, 2), (14, 2), (15, 2), (18, 2), (22, 2), (22, 4), (23, 4),
            (24, 4), (25, 4), (26, 4), (23, 9)]),
    ];

    #[test]
    fn holiday_lists_match_quantlib_v1_44_source() {
        let c = SaudiArabia::new(Market::Tadawul);
        for &(y, expected) in SOURCE_HOLIDAYS {
            let expected: Vec<Date> = expected
                .iter()
                .map(|&(d, m)| Date::new(d, Month::from_ordinal(m), y))
                .collect();
            let got = c.holiday_list(
                Date::new(1, Month::January, y),
                Date::new(31, Month::December, y),
                false,
            );
            assert_eq!(got, expected, "{y}");
        }
    }
}
