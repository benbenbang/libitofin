//! Serbia calendars.
//!
//! Port of `ql/time/calendars/serbia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Serbian markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Belgrade stock exchange.
    Bse,
}

/// Serbian calendars. Defaults to [`Market::Bse`] in QuantLib.
pub struct Serbia;

impl Serbia {
    /// Builds a Serbian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Bse => shared(BseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct BseImpl;

impl CalendarImpl for BseImpl {
    fn name(&self) -> String {
        "Belgrade stock exchange".to_string()
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
        let em = western_easter_monday(y);
        // Serbian holidays
        !(is_weekend_sat_sun(w)
            // New Year
            || (d == 1 && m == Month::January)
            || (d == 2 && m == Month::January)
            // Serbian Orthodox Christmas
            || (d == 7 && m == Month::January)
            // Serbian Statehood Day
            || (d == 15 && m == Month::February)
            || (d == 16 && m == Month::February)
            // Serbian Statehood Day (observed)
            // Serbia's Statehood Day (Sretenje) is officially observed on February 15 and February 16.
            // However, since these dates fall on a Saturday and Sunday, the government designates Monday,
            // February 17, as an additional non-working day. This practice ensures that citizens receive a weekday off
            // when public holidays coincide with weekends.
            || ((d == 17 && m == Month::February)
                && is_weekend_sat_sun(Date::new(15, Month::February, y).weekday())
                && is_weekend_sat_sun(Date::new(16, Month::February, y).weekday()))
            // Good Friday
            || (dd == em - 3 && y >= 2016)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Armistice Day in World War I
            || (d == 11 && m == Month::November)
            // Trading system maintenance, statistics and data migration
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Serbia::new(Market::Bse).name(), "Belgrade stock exchange");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Serbia::new(Market::Bse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year
        assert!(c.is_holiday(Date::new(2, Month::January, 2019))); // New Year
        assert!(c.is_holiday(Date::new(7, Month::January, 2019))); // Orthodox Christmas
        assert!(c.is_holiday(Date::new(15, Month::February, 2019))); // Statehood Day
        assert!(c.is_holiday(Date::new(16, Month::February, 2019))); // Statehood Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(11, Month::November, 2019))); // Armistice Day
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // Trading maintenance
    }

    #[test]
    fn weekend_rule() {
        let c = Serbia::new(Market::Bse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
