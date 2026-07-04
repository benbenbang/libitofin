//! Russian calendars.
//!
//! Port of `ql/time/calendars/russia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Day, Month, Year};
use crate::time::weekday::Weekday;

/// Market handled by the Russian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Moscow Exchange calendar.
    MOEX,
}

/// The Russian calendar. The default market is [`Market::Settlement`].
pub struct Russia;

impl Russia {
    /// Builds a Russian calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::MOEX => shared(ExchangeImpl),
        };
        Calendar::from_impl(imp)
    }
}

fn is_extra_holiday_settlement_impl(d: Day, month: Month, year: Year) -> bool {
    match year {
        2017 => match month {
            Month::February => d == 24,
            Month::May => d == 8,
            Month::November => d == 6,
            _ => false,
        },
        2018 => match month {
            Month::March => d == 9,
            Month::April => d == 30,
            Month::May => d == 2,
            Month::June => d == 11,
            Month::December => d == 31,
            _ => false,
        },
        2019 => match month {
            Month::May => d == 2 || d == 3 || d == 10,
            _ => false,
        },
        2020 => match month {
            Month::March => d == 30 || d == 31,
            Month::April => d == 1 || d == 2 || d == 3,
            Month::May => d == 4 || d == 5,
            _ => false,
        },
        _ => false,
    }
}

struct SettlementImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Russian settlement".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        if is_weekend_sat_sun(w)
            // New Year's holidays
            || (y <= 2005 && d <= 2 && m == Month::January)
            || (y >= 2005 && d <= 5 && m == Month::January)
            // in 2012, the 6th was also a holiday
            || (y == 2012 && d == 6 && m == Month::January)
            // Christmas (possibly moved to Monday)
            || ((d == 7 || ((d == 8 || d == 9) && w == Weekday::Monday))
                && m == Month::January)
            // Defender of the Fatherland Day (possibly moved to Monday)
            || ((d == 23 || ((d == 24 || d == 25) && w == Weekday::Monday))
                && m == Month::February)
            // International Women's Day (possibly moved to Monday)
            || ((d == 8 || ((d == 9 || d == 10) && w == Weekday::Monday))
                && m == Month::March)
            // Labour Day (possibly moved to Monday)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday))
                && m == Month::May)
            // Victory Day (possibly moved to Monday)
            || ((d == 9 || ((d == 10 || d == 11) && w == Weekday::Monday))
                && m == Month::May)
            // Russia Day (possibly moved to Monday)
            || ((d == 12 || ((d == 13 || d == 14) && w == Weekday::Monday))
                && m == Month::June)
            // Unity Day (possibly moved to Monday)
            || ((d == 4 || ((d == 5 || d == 6) && w == Weekday::Monday))
                && m == Month::November)
        {
            return false;
        }

        if is_extra_holiday_settlement_impl(d, m, y) {
            return false;
        }

        true
    }
}

fn is_working_weekend(d: Day, month: Month, year: Year) -> bool {
    match year {
        2012 => match month {
            Month::March => d == 11,
            Month::April => d == 28,
            Month::May => d == 5 || d == 12,
            Month::June => d == 9,
            _ => false,
        },
        2016 => match month {
            Month::February => d == 20,
            _ => false,
        },
        2018 => match month {
            Month::April => d == 28,
            Month::June => d == 9,
            Month::December => d == 29,
            _ => false,
        },
        _ => false,
    }
}

fn is_extra_holiday_exchange_impl(d: Day, month: Month, year: Year) -> bool {
    match year {
        2012 => match month {
            Month::January => d == 2,
            Month::March => d == 9,
            Month::April => d == 30,
            Month::June => d == 11,
            _ => false,
        },
        2013 => match month {
            Month::January => d == 1 || d == 2 || d == 3 || d == 4 || d == 7,
            _ => false,
        },
        2014 => match month {
            Month::January => d == 1 || d == 2 || d == 3 || d == 7,
            _ => false,
        },
        2015 => match month {
            Month::January => d == 1 || d == 2 || d == 7,
            _ => false,
        },
        2016 => match month {
            Month::January => d == 1 || d == 7 || d == 8,
            Month::May => d == 2 || d == 3,
            Month::June => d == 13,
            Month::December => d == 30,
            _ => false,
        },
        2017 => match month {
            Month::January => d == 2,
            Month::May => d == 8,
            _ => false,
        },
        2018 => match month {
            Month::January => d == 1 || d == 2 || d == 8,
            Month::December => d == 31,
            _ => false,
        },
        2019 => match month {
            Month::January => d == 1 || d == 2 || d == 7,
            Month::December => d == 31,
            _ => false,
        },
        2020 => match month {
            Month::January => d == 1 || d == 2 || d == 7,
            Month::February => d == 24,
            Month::June => d == 24,
            Month::July => d == 1,
            _ => false,
        },
        _ => false,
    }
}

struct ExchangeImpl;

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "Moscow exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();

        // the exchange was formally established in 2011, so data are only
        // available from 2012 to present
        if y < 2012 {
            panic!("MOEX calendar for the year {y} does not exist.");
        }

        if is_working_weekend(d, m, y) {
            return true;
        }

        // Known holidays
        if is_weekend_sat_sun(w)
            // Defender of the Fatherland Day
            || (d == 23 && m == Month::February)
            // International Women's Day (possibly moved to Monday)
            || ((d == 8 || ((d == 9 || d == 10) && w == Weekday::Monday)) && m == Month::March)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Victory Day (possibly moved to Monday)
            || ((d == 9 || ((d == 10 || d == 11) && w == Weekday::Monday)) && m == Month::May)
            // Russia Day
            || (d == 12 && m == Month::June)
            // Unity Day (possibly moved to Monday)
            || ((d == 4 || ((d == 5 || d == 6) && w == Weekday::Monday))
                && m == Month::November)
            // New Years Eve
            || (d == 31 && m == Month::December)
        {
            return false;
        }

        if is_extra_holiday_exchange_impl(d, m, y) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Russia::new(Market::Settlement).name(), "Russian settlement");
        assert_eq!(Russia::new(Market::MOEX).name(), "Moscow exchange");
    }

    #[test]
    fn settlement_fixed_holidays() {
        let c = Russia::new(Market::Settlement);
        // Defender of the Fatherland Day, Feb 23rd (unconditional in the base clause).
        assert!(c.is_holiday(Date::new(23, Month::February, 2019)));
        // Christmas, Jan 7th.
        assert!(c.is_holiday(Date::new(7, Month::January, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Russia::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }
}
