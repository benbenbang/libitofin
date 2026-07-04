//! United States calendars.
//!
//! Port of `ql/time/calendars/unitedstates.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// United States markets.
///
/// QuantLib has no default; a market must be supplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// New York stock exchange calendar.
    Nyse,
    /// Government-bond calendar.
    GovernmentBond,
    /// Off-peak days for NERC.
    Nerc,
    /// Libor impact calendar.
    LiborImpact,
    /// Federal Reserve Bankwire System.
    FederalReserve,
    /// SOFR fixing calendar.
    Sofr,
}

/// United States calendar.
pub struct UnitedStates;

impl UnitedStates {
    /// Builds a US calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Nyse => shared(NyseImpl),
            Market::GovernmentBond => shared(GovernmentBondImpl),
            Market::Nerc => shared(NercImpl),
            Market::LiborImpact => shared(LiborImpactImpl),
            Market::FederalReserve => shared(FederalReserveImpl),
            Market::Sofr => shared(SofrImpl),
        };
        Calendar::from_impl(imp)
    }
}

// a few rules used by multiple calendars

fn is_washington_birthday(d: i32, m: Month, y: i32, w: Weekday) -> bool {
    if y >= 1971 {
        // third Monday in February
        (d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::February
    } else {
        // February 22nd, possibly adjusted
        (d == 22 || (d == 23 && w == Weekday::Monday) || (d == 21 && w == Weekday::Friday))
            && m == Month::February
    }
}

fn is_memorial_day(d: i32, m: Month, y: i32, w: Weekday) -> bool {
    if y >= 1971 {
        // last Monday in May
        d >= 25 && w == Weekday::Monday && m == Month::May
    } else {
        // May 30th, possibly adjusted
        (d == 30 || (d == 31 && w == Weekday::Monday) || (d == 29 && w == Weekday::Friday))
            && m == Month::May
    }
}

fn is_labor_day(d: i32, m: Month, _y: i32, w: Weekday) -> bool {
    // first Monday in September
    d <= 7 && w == Weekday::Monday && m == Month::September
}

fn is_columbus_day(d: i32, m: Month, y: i32, w: Weekday) -> bool {
    // second Monday in October
    (d >= 8 && d <= 14) && w == Weekday::Monday && m == Month::October && y >= 1971
}

fn is_veterans_day(d: i32, m: Month, y: i32, w: Weekday) -> bool {
    if y <= 1970 || y >= 1978 {
        // November 11th, adjusted
        (d == 11 || (d == 12 && w == Weekday::Monday) || (d == 10 && w == Weekday::Friday))
            && m == Month::November
    } else {
        // fourth Monday in October
        (d >= 22 && d <= 28) && w == Weekday::Monday && m == Month::October
    }
}

fn is_veterans_day_no_saturday(d: i32, m: Month, y: i32, w: Weekday) -> bool {
    if y <= 1970 || y >= 1978 {
        // November 11th, adjusted, but no Saturday to Friday
        (d == 11 || (d == 12 && w == Weekday::Monday)) && m == Month::November
    } else {
        // fourth Monday in October
        (d >= 22 && d <= 28) && w == Weekday::Monday && m == Month::October
    }
}

fn is_juneteenth(d: i32, m: Month, y: i32, w: Weekday, move_to_friday: bool) -> bool {
    // declared in 2021, but only observed by exchanges since 2022
    (d == 19
        || (d == 20 && w == Weekday::Monday)
        || ((d == 18 && w == Weekday::Friday) && move_to_friday))
        && m == Month::June
        && y >= 2022
}

fn settlement_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let m = date.month();
    let y = date.year();
    !(is_weekend_sat_sun(w)
        // New Year's Day (possibly moved to Monday if on Sunday)
        || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
        // (or to Friday if on Saturday)
        || (d == 31 && w == Weekday::Friday && m == Month::December)
        // Martin Luther King's birthday (third Monday in January)
        || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::January && y >= 1983)
        // Washington's birthday (third Monday in February)
        || is_washington_birthday(d, m, y, w)
        // Memorial Day (last Monday in May)
        || is_memorial_day(d, m, y, w)
        // Juneteenth (Monday if Sunday or Friday if Saturday)
        || is_juneteenth(d, m, y, w, true)
        // Independence Day (Monday if Sunday or Friday if Saturday)
        || ((d == 4 || (d == 5 && w == Weekday::Monday) || (d == 3 && w == Weekday::Friday))
            && m == Month::July)
        // Labor Day (first Monday in September)
        || is_labor_day(d, m, y, w)
        // Columbus Day (second Monday in October)
        || is_columbus_day(d, m, y, w)
        // Veteran's Day (Monday if Sunday or Friday if Saturday)
        || is_veterans_day(d, m, y, w)
        // Thanksgiving Day (fourth Thursday in November)
        || ((d >= 22 && d <= 28) && w == Weekday::Thursday && m == Month::November)
        // Christmas (Monday if Sunday or Friday if Saturday)
        || ((d == 25 || (d == 26 && w == Weekday::Monday) || (d == 24 && w == Weekday::Friday))
            && m == Month::December))
}

fn government_bond_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let dd = date.day_of_year();
    let m = date.month();
    let y = date.year();
    let em = western_easter_monday(y);
    if is_weekend_sat_sun(w)
        // New Year's Day (possibly moved to Monday if on Sunday)
        || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
        // Martin Luther King's birthday (third Monday in January)
        || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::January && y >= 1983)
        // Washington's birthday (third Monday in February)
        || is_washington_birthday(d, m, y, w)
        // Good Friday. Since 1996 it's an early close and not a full market
        // close when it coincides with the NFP release date, which is the
        // first Friday of the month(*).
        // See <https://www.sifma.org/resources/general/holiday-schedule/>
        //
        // (*) The full rule is "the third Friday after the conclusion of the
        // week which includes the 12th of the month". This is usually the
        // first Friday of the next month, but can be the second Friday if the
        // month has fewer than 31 days. Since Good Friday is always between
        // March 20th and April 23rd, it can only coincide with the April NFP,
        // which is always on the first Friday, because March has 31 days.
        || (dd == em - 3 && (y < 1996 || d > 7))
        // Memorial Day (last Monday in May)
        || is_memorial_day(d, m, y, w)
        // Juneteenth (Monday if Sunday or Friday if Saturday)
        || is_juneteenth(d, m, y, w, true)
        // Independence Day (Monday if Sunday or Friday if Saturday)
        || ((d == 4 || (d == 5 && w == Weekday::Monday) || (d == 3 && w == Weekday::Friday))
            && m == Month::July)
        // Labor Day (first Monday in September)
        || is_labor_day(d, m, y, w)
        // Columbus Day (second Monday in October)
        || is_columbus_day(d, m, y, w)
        // Veteran's Day (Monday if Sunday)
        || is_veterans_day_no_saturday(d, m, y, w)
        // Thanksgiving Day (fourth Thursday in November)
        || ((d >= 22 && d <= 28) && w == Weekday::Thursday && m == Month::November)
        // Christmas (Monday if Sunday or Friday if Saturday)
        || ((d == 25 || (d == 26 && w == Weekday::Monday) || (d == 24 && w == Weekday::Friday))
            && m == Month::December)
    {
        return false;
    }

    // Special closings
    if
    // President Bush's Funeral
    (y == 2018 && m == Month::December && d == 5)
        // Hurricane Sandy
        || (y == 2012 && m == Month::October && d == 30)
        // President Reagan's funeral
        || (y == 2004 && m == Month::June && d == 11)
    {
        return false;
    }

    true
}

struct SettlementImpl;
struct LiborImpactImpl;
struct NyseImpl;
struct GovernmentBondImpl;
struct SofrImpl;
struct NercImpl;
struct FederalReserveImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "US settlement".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        settlement_is_business_day(date)
    }
}

impl CalendarImpl for LiborImpactImpl {
    fn name(&self) -> String {
        "US with Libor impact".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        // Since 2015 Independence Day only impacts Libor if it falls
        // on a weekday
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        if ((d == 5 && w == Weekday::Monday) || (d == 3 && w == Weekday::Friday))
            && m == Month::July
            && y >= 2015
        {
            return true;
        }
        settlement_is_business_day(date)
    }
}

impl CalendarImpl for NyseImpl {
    fn name(&self) -> String {
        "New York stock exchange".to_string()
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
        if is_weekend_sat_sun(w)
            // New Year's Day (possibly moved to Monday if on Sunday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
            // Washington's birthday (third Monday in February)
            || is_washington_birthday(d, m, y, w)
            // Good Friday
            || (dd == em - 3)
            // Memorial Day (last Monday in May)
            || is_memorial_day(d, m, y, w)
            // Juneteenth (Monday if Sunday or Friday if Saturday)
            || is_juneteenth(d, m, y, w, true)
            // Independence Day (Monday if Sunday or Friday if Saturday)
            || ((d == 4 || (d == 5 && w == Weekday::Monday) || (d == 3 && w == Weekday::Friday))
                && m == Month::July)
            // Labor Day (first Monday in September)
            || is_labor_day(d, m, y, w)
            // Thanksgiving Day (fourth Thursday in November)
            || ((d >= 22 && d <= 28) && w == Weekday::Thursday && m == Month::November)
            // Christmas (Monday if Sunday or Friday if Saturday)
            || ((d == 25 || (d == 26 && w == Weekday::Monday) || (d == 24 && w == Weekday::Friday))
                && m == Month::December)
        {
            return false;
        }

        if y >= 1998 && (d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::January {
            // Martin Luther King's birthday (third Monday in January)
            return false;
        }

        if (y <= 1968 || (y <= 1980 && y % 4 == 0))
            && m == Month::November
            && d <= 7
            && w == Weekday::Tuesday
        {
            // Presidential election days
            return false;
        }

        // Special closings
        if
        // President Carter's Funeral
        (y == 2025 && m == Month::January && d == 9)
            // President Bush's Funeral
            || (y == 2018 && m == Month::December && d == 5)
            // Hurricane Sandy
            || (y == 2012 && m == Month::October && (d == 29 || d == 30))
            // President Ford's funeral
            || (y == 2007 && m == Month::January && d == 2)
            // President Reagan's funeral
            || (y == 2004 && m == Month::June && d == 11)
            // September 11-14, 2001
            || (y == 2001 && m == Month::September && (11..=14).contains(&d))
            // President Nixon's funeral
            || (y == 1994 && m == Month::April && d == 27)
            // Hurricane Gloria
            || (y == 1985 && m == Month::September && d == 27)
            // 1977 Blackout
            || (y == 1977 && m == Month::July && d == 14)
            // Funeral of former President Lyndon B. Johnson.
            || (y == 1973 && m == Month::January && d == 25)
            // Funeral of former President Harry S. Truman
            || (y == 1972 && m == Month::December && d == 28)
            // National Day of Participation for the lunar exploration.
            || (y == 1969 && m == Month::July && d == 21)
            // Funeral of former President Eisenhower.
            || (y == 1969 && m == Month::March && d == 31)
            // Closed all day - heavy snow.
            || (y == 1969 && m == Month::February && d == 10)
            // Day after Independence Day.
            || (y == 1968 && m == Month::July && d == 5)
            // June 12-Dec. 31, 1968
            // Four day week (closed on Wednesdays) - Paperwork Crisis
            || (y == 1968 && dd >= 163 && w == Weekday::Wednesday)
            // Day of mourning for Martin Luther King Jr.
            || (y == 1968 && m == Month::April && d == 9)
            // Funeral of President Kennedy
            || (y == 1963 && m == Month::November && d == 25)
            // Day before Decoration Day
            || (y == 1961 && m == Month::May && d == 29)
            // Day after Christmas
            || (y == 1958 && m == Month::December && d == 26)
            // Christmas Eve
            || ((y == 1954 || y == 1956 || y == 1965) && m == Month::December && d == 24)
        {
            return false;
        }

        true
    }
}

impl CalendarImpl for GovernmentBondImpl {
    fn name(&self) -> String {
        "US government bond market".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        government_bond_is_business_day(date)
    }
}

impl CalendarImpl for SofrImpl {
    fn name(&self) -> String {
        "SOFR fixing calendar".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        // so far (that is, up to 2023 at the time of this change) SOFR never
        // fixed on Good Friday. We're extrapolating that pattern. This might
        // change if a fixing on Good Friday occurs in future years.
        let dy = date.day_of_year();
        let y = date.year();

        // Good Friday
        if dy == western_easter_monday(y) - 3 {
            return false;
        }

        government_bond_is_business_day(date)
    }
}

impl CalendarImpl for NercImpl {
    fn name(&self) -> String {
        "North American Energy Reliability Council".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        !(is_weekend_sat_sun(w)
            // New Year's Day (possibly moved to Monday if on Sunday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
            // Memorial Day (last Monday in May)
            || is_memorial_day(d, m, y, w)
            // Independence Day (Monday if Sunday)
            || ((d == 4 || (d == 5 && w == Weekday::Monday)) && m == Month::July)
            // Labor Day (first Monday in September)
            || is_labor_day(d, m, y, w)
            // Thanksgiving Day (fourth Thursday in November)
            || ((d >= 22 && d <= 28) && w == Weekday::Thursday && m == Month::November)
            // Christmas (Monday if Sunday)
            || ((d == 25 || (d == 26 && w == Weekday::Monday)) && m == Month::December))
    }
}

impl CalendarImpl for FederalReserveImpl {
    fn name(&self) -> String {
        "Federal Reserve Bankwire System".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        // see https://www.frbservices.org/about/holiday-schedules for details
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        !(is_weekend_sat_sun(w)
            // New Year's Day (possibly moved to Monday if on Sunday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
            // Martin Luther King's birthday (third Monday in January)
            || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::January && y >= 1983)
            // Washington's birthday (third Monday in February)
            || is_washington_birthday(d, m, y, w)
            // Memorial Day (last Monday in May)
            || is_memorial_day(d, m, y, w)
            // Juneteenth (Monday if Sunday)
            || is_juneteenth(d, m, y, w, false)
            // Independence Day (Monday if Sunday)
            || ((d == 4 || (d == 5 && w == Weekday::Monday)) && m == Month::July)
            // Labor Day (first Monday in September)
            || is_labor_day(d, m, y, w)
            // Columbus Day (second Monday in October)
            || is_columbus_day(d, m, y, w)
            // Veteran's Day (Monday if Sunday)
            || is_veterans_day_no_saturday(d, m, y, w)
            // Thanksgiving Day (fourth Thursday in November)
            || ((d >= 22 && d <= 28) && w == Weekday::Thursday && m == Month::November)
            // Christmas (Monday if Sunday)
            || ((d == 25 || (d == 26 && w == Weekday::Monday)) && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            UnitedStates::new(Market::Settlement).name(),
            "US settlement"
        );
        assert_eq!(
            UnitedStates::new(Market::LiborImpact).name(),
            "US with Libor impact"
        );
        assert_eq!(
            UnitedStates::new(Market::Nyse).name(),
            "New York stock exchange"
        );
        assert_eq!(
            UnitedStates::new(Market::GovernmentBond).name(),
            "US government bond market"
        );
        assert_eq!(
            UnitedStates::new(Market::Sofr).name(),
            "SOFR fixing calendar"
        );
        assert_eq!(
            UnitedStates::new(Market::Nerc).name(),
            "North American Energy Reliability Council"
        );
        assert_eq!(
            UnitedStates::new(Market::FederalReserve).name(),
            "Federal Reserve Bankwire System"
        );
    }

    #[test]
    fn settlement_fixed_holidays() {
        let c = UnitedStates::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
    }

    #[test]
    fn weekend_rule() {
        let c = UnitedStates::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
