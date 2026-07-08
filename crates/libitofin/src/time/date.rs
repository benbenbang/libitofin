//! Concrete date class with serial-number arithmetic.
//!
//! Port of `ql/time/date.{hpp,cpp}` (the default `!QL_HIGH_RESOLUTION_DATE`
//! configuration). A [`Date`] is a thin wrapper over an integer serial number
//! counting days since December 31st 1899 (serial 1 == January 1st 1900), the
//! same origin Excel/Applix use. All calendar arithmetic is expressed on that
//! serial number via the precomputed leap-year, month-offset and year-offset
//! tables copied verbatim from QuantLib, so results match the C++ oracle
//! exactly over the supported range `[Jan 1st 1901, Dec 31st 2199]`.
//!
//! Only the pieces the calendar layer relies on are ported: construction,
//! inspectors, month/year arithmetic via the `Add<Period>`/`Sub<Period>` impls
//! and the day/period operators. Formatting manipulators, hashing helpers and
//! the high-resolution
//! (sub-day) variant from QuantLib are out of scope for this branch.

use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::time::weekday::Weekday;
use crate::types::Integer;

/// Day-of-month number (`ql/time/date.hpp`'s `Day` typedef).
pub type Day = Integer;

/// Year number (`ql/time/date.hpp`'s `Year` typedef).
pub type Year = Integer;

/// A date's serial number (`Date::serial_type`).
pub type SerialNumber = Integer;

/// Month of the year, numbered `January = 1 .. December = 12`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Month {
    /// January (1).
    January = 1,
    /// February (2).
    February = 2,
    /// March (3).
    March = 3,
    /// April (4).
    April = 4,
    /// May (5).
    May = 5,
    /// June (6).
    June = 6,
    /// July (7).
    July = 7,
    /// August (8).
    August = 8,
    /// September (9).
    September = 9,
    /// October (10).
    October = 10,
    /// November (11).
    November = 11,
    /// December (12).
    December = 12,
}

impl Month {
    /// Builds a month from its 1-based ordinal (`January = 1 .. December = 12`).
    ///
    /// # Panics
    ///
    /// Panics if `n` is outside `1..=12`.
    pub fn from_ordinal(n: Integer) -> Month {
        match n {
            1 => Month::January,
            2 => Month::February,
            3 => Month::March,
            4 => Month::April,
            5 => Month::May,
            6 => Month::June,
            7 => Month::July,
            8 => Month::August,
            9 => Month::September,
            10 => Month::October,
            11 => Month::November,
            12 => Month::December,
            _ => panic!("month {n} outside January-December range [1,12]"),
        }
    }

    /// The 1-based ordinal of the month (`January = 1 .. December = 12`).
    pub fn ordinal(self) -> Integer {
        self as Integer
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Month::January => "January",
            Month::February => "February",
            Month::March => "March",
            Month::April => "April",
            Month::May => "May",
            Month::June => "June",
            Month::July => "July",
            Month::August => "August",
            Month::September => "September",
            Month::October => "October",
            Month::November => "November",
            Month::December => "December",
        };
        f.write_str(name)
    }
}

/// A calendar date, represented by its serial number.
///
/// The default value is the *null date* (serial number 0), used as a
/// placeholder; arithmetic and most inspectors on it are meaningless, matching
/// QuantLib's default-constructed `Date`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    serial_number: SerialNumber,
}

impl Date {
    /// The null date (serial number 0), a placeholder equal to
    /// QuantLib's default-constructed `Date`.
    pub fn null() -> Date {
        Date { serial_number: 0 }
    }

    /// Builds a date from a serial number as given by Excel/Applix.
    ///
    /// # Panics
    ///
    /// Panics if the serial number falls outside the supported range
    /// `[367, 109574]` (i.e. `[Jan 1st 1901, Dec 31st 2199]`).
    pub fn from_serial(serial_number: SerialNumber) -> Date {
        check_serial_number(serial_number);
        Date { serial_number }
    }

    /// Builds a date from a day, month and year.
    ///
    /// # Panics
    ///
    /// Panics if the year is outside `[1901, 2199]`, or if the day is outside
    /// the valid range for the given month and year.
    pub fn new(d: Day, m: Month, y: Year) -> Date {
        assert!(
            y > 1900 && y < 2200,
            "year {y} out of bound. It must be in [1901,2199]"
        );

        let leap = is_leap(y);
        let len = month_length(m, leap);
        let offset = month_offset(m.ordinal(), leap);
        assert!(
            d <= len && d > 0,
            "day {d} outside month ({}) day-range [1,{len}]",
            m.ordinal()
        );

        Date {
            serial_number: d + offset + year_offset(y),
        }
    }

    /// The serial number of the date.
    pub fn serial_number(self) -> SerialNumber {
        self.serial_number
    }

    /// The day of the week.
    pub fn weekday(self) -> Weekday {
        let w = self.serial_number % 7;
        Weekday::from_ordinal(if w == 0 { 7 } else { w })
    }

    /// The one-based day of the year (January 1st == 1).
    pub fn day_of_year(self) -> Day {
        self.serial_number - year_offset(self.year())
    }

    /// The day of the month.
    pub fn day_of_month(self) -> Day {
        self.day_of_year() - month_offset(self.month().ordinal(), is_leap(self.year()))
    }

    /// The month of the year.
    pub fn month(self) -> Month {
        let d = self.day_of_year(); // one-based
        let mut m = d / 30 + 1;
        let leap = is_leap(self.year());
        // `month_offset` accepts the trailing 13th ordinal used to bracket the
        // day, so `m + 1` may transiently reach 13 here.
        while d <= month_offset(m, leap) {
            m -= 1;
        }
        while d > month_offset(m + 1, leap) {
            m += 1;
        }
        Month::from_ordinal(m)
    }

    /// The year.
    pub fn year(self) -> Year {
        let mut y = self.serial_number / 365 + 1900;
        // year_offset(y) is December 31st of the preceding year
        if self.serial_number <= year_offset(y) {
            y -= 1;
        }
        y
    }

    /// The earliest supported date (January 1st 1901).
    pub fn min_date() -> Date {
        Date {
            serial_number: MINIMUM_SERIAL_NUMBER,
        }
    }

    /// The latest supported date (December 31st 2199).
    pub fn max_date() -> Date {
        Date {
            serial_number: MAXIMUM_SERIAL_NUMBER,
        }
    }

    /// Whether the given year is a leap year.
    pub fn is_leap(y: Year) -> bool {
        is_leap(y)
    }

    /// The first day of the month `d` belongs to.
    pub fn start_of_month(d: Date) -> Date {
        Date::new(1, d.month(), d.year())
    }

    /// Whether `d` is the first day of its month.
    pub fn is_start_of_month(d: Date) -> bool {
        d.day_of_month() == 1
    }

    /// The last day of the month `d` belongs to.
    pub fn end_of_month(d: Date) -> Date {
        let m = d.month();
        let y = d.year();
        Date::new(month_length(m, is_leap(y)), m, y)
    }

    /// Whether `d` is the last day of its month.
    pub fn is_end_of_month(d: Date) -> bool {
        d.day_of_month() == month_length(d.month(), is_leap(d.year()))
    }

    /// The next given weekday following or equal to `d`.
    ///
    /// E.g. the Friday following Tuesday, January 15th 2002 was January 18th.
    pub fn next_weekday(d: Date, w: Weekday) -> Date {
        let wd = d.weekday().ordinal();
        let target = w.ordinal();
        d + ((if wd > target { 7 } else { 0 }) - wd + target)
    }

    /// The `n`-th given weekday in the given month and year.
    ///
    /// E.g. the 4th Thursday of March 1998 was March 26th.
    ///
    /// # Panics
    ///
    /// Panics if `n` is 0 or greater than 5 (there is no zeroth weekday, and no
    /// month contains six of any weekday).
    pub fn nth_weekday(n: Integer, w: Weekday, m: Month, y: Year) -> Date {
        assert!(
            n > 0,
            "zeroth day of week in a given (month, year) is undefined"
        );
        assert!(n < 6, "no more than 5 weekday in a given (month, year)");
        let first = Date::new(1, m, y).weekday().ordinal();
        let target = w.ordinal();
        let skip = n - if target >= first { 1 } else { 0 };
        Date::new((1 + target + skip * 7) - first, m, y)
    }

    /// Advances the date by `n` of the given [`TimeUnit`], as `Date + Period`
    /// does in QuantLib.
    ///
    /// # Panics
    ///
    /// Panics on `Months`/`Years` shifts that leave the supported year range,
    /// or for the sub-day units, which have no meaning without the
    /// high-resolution date variant.
    fn advance(date: Date, n: Integer, units: TimeUnit) -> Date {
        match units {
            TimeUnit::Days => date + n,
            TimeUnit::Weeks => date + 7 * n,
            TimeUnit::Months => {
                let mut d = date.day_of_month();
                let mut m = date.month().ordinal() + n;
                let mut y = date.year();
                while m > 12 {
                    m -= 12;
                    y += 1;
                }
                while m < 1 {
                    m += 12;
                    y -= 1;
                }
                assert!(
                    (1900..=2199).contains(&y),
                    "year {y} out of bounds. It must be in [1901,2199]"
                );
                let length = month_length(Month::from_ordinal(m), is_leap(y));
                if d > length {
                    d = length;
                }
                Date::new(d, Month::from_ordinal(m), y)
            }
            TimeUnit::Years => {
                let mut d = date.day_of_month();
                let m = date.month();
                let y = date.year() + n;
                assert!(
                    (1900..=2199).contains(&y),
                    "year {y} out of bounds. It must be in [1901,2199]"
                );
                if d == 29 && m == Month::February && !is_leap(y) {
                    d = 28;
                }
                Date::new(d, m, y)
            }
            _ => panic!("undefined time units for serial-number date arithmetic"),
        }
    }
}

impl Default for Date {
    fn default() -> Date {
        Date::null()
    }
}

impl fmt::Display for Date {
    /// ISO format (`yyyy-mm-dd`), or `"null date"` for the null date.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Date::null() {
            return f.write_str("null date");
        }
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.year(),
            self.month().ordinal(),
            self.day_of_month()
        )
    }
}

impl Add<SerialNumber> for Date {
    type Output = Date;
    fn add(self, days: SerialNumber) -> Date {
        Date::from_serial(self.serial_number + days)
    }
}

impl Sub<SerialNumber> for Date {
    type Output = Date;
    fn sub(self, days: SerialNumber) -> Date {
        Date::from_serial(self.serial_number - days)
    }
}

impl AddAssign<SerialNumber> for Date {
    fn add_assign(&mut self, days: SerialNumber) {
        *self = *self + days;
    }
}

impl SubAssign<SerialNumber> for Date {
    fn sub_assign(&mut self, days: SerialNumber) {
        *self = *self - days;
    }
}

impl Sub<Date> for Date {
    type Output = SerialNumber;
    /// The number of days between the two dates.
    fn sub(self, other: Date) -> SerialNumber {
        self.serial_number - other.serial_number
    }
}

impl Add<Period> for Date {
    type Output = Date;
    fn add(self, p: Period) -> Date {
        Date::advance(self, p.length(), p.units())
    }
}

impl Sub<Period> for Date {
    type Output = Date;
    fn sub(self, p: Period) -> Date {
        Date::advance(self, -p.length(), p.units())
    }
}

// --- serial-number range -------------------------------------------------

const MINIMUM_SERIAL_NUMBER: SerialNumber = 367; // Jan 1st, 1901
const MAXIMUM_SERIAL_NUMBER: SerialNumber = 109574; // Dec 31st, 2199

fn check_serial_number(serial_number: SerialNumber) {
    assert!(
        (MINIMUM_SERIAL_NUMBER..=MAXIMUM_SERIAL_NUMBER).contains(&serial_number),
        "Date's serial number ({serial_number}) outside allowed range \
         [{MINIMUM_SERIAL_NUMBER}-{MAXIMUM_SERIAL_NUMBER}], i.e. [1901-01-01, 2199-12-31]"
    );
}

// --- lookup tables (verbatim from ql/time/date.cpp) ----------------------

fn is_leap(y: Year) -> bool {
    // 1900 is (incorrectly) leap in agreement with Excel's bug; it is out of
    // the valid date range anyway.
    const YEAR_IS_LEAP: [bool; 301] = [
        // 1900-1909
        true, false, false, false, true, false, false, false, true, false, // 1910-1919
        false, false, true, false, false, false, true, false, false, false, // 1920-1929
        true, false, false, false, true, false, false, false, true, false, // 1930-1939
        false, false, true, false, false, false, true, false, false, false, // 1940-1949
        true, false, false, false, true, false, false, false, true, false, // 1950-1959
        false, false, true, false, false, false, true, false, false, false, // 1960-1969
        true, false, false, false, true, false, false, false, true, false, // 1970-1979
        false, false, true, false, false, false, true, false, false, false, // 1980-1989
        true, false, false, false, true, false, false, false, true, false, // 1990-1999
        false, false, true, false, false, false, true, false, false, false, // 2000-2009
        true, false, false, false, true, false, false, false, true, false, // 2010-2019
        false, false, true, false, false, false, true, false, false, false, // 2020-2029
        true, false, false, false, true, false, false, false, true, false, // 2030-2039
        false, false, true, false, false, false, true, false, false, false, // 2040-2049
        true, false, false, false, true, false, false, false, true, false, // 2050-2059
        false, false, true, false, false, false, true, false, false, false, // 2060-2069
        true, false, false, false, true, false, false, false, true, false, // 2070-2079
        false, false, true, false, false, false, true, false, false, false, // 2080-2089
        true, false, false, false, true, false, false, false, true, false, // 2090-2099
        false, false, true, false, false, false, true, false, false, false, // 2100-2109
        false, false, false, false, true, false, false, false, true, false, // 2110-2119
        false, false, true, false, false, false, true, false, false, false, // 2120-2129
        true, false, false, false, true, false, false, false, true, false, // 2130-2139
        false, false, true, false, false, false, true, false, false, false, // 2140-2149
        true, false, false, false, true, false, false, false, true, false, // 2150-2159
        false, false, true, false, false, false, true, false, false, false, // 2160-2169
        true, false, false, false, true, false, false, false, true, false, // 2170-2179
        false, false, true, false, false, false, true, false, false, false, // 2180-2189
        true, false, false, false, true, false, false, false, true, false, // 2190-2199
        false, false, true, false, false, false, true, false, false, false, // 2200
        false,
    ];
    assert!((1900..=2200).contains(&y), "year {y} outside valid range");
    YEAR_IS_LEAP[(y - 1900) as usize]
}

fn month_length(m: Month, leap_year: bool) -> Integer {
    const MONTH_LENGTH: [Integer; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    const MONTH_LEAP_LENGTH: [Integer; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let i = (m.ordinal() - 1) as usize;
    if leap_year {
        MONTH_LEAP_LENGTH[i]
    } else {
        MONTH_LENGTH[i]
    }
}

/// Days before the start of month `m` (1-based ordinal). A 13th ordinal is
/// accepted and returns the year length, which [`Date::month`] uses to bracket
/// the day - mirroring QuantLib's `Month(m+1)` indexing.
fn month_offset(m: Integer, leap_year: bool) -> Integer {
    // 13 entries: the trailing 365/366 is used to bracket the day in `month()`.
    const MONTH_OFFSET: [Integer; 13] = [
        0, 31, 59, 90, 120, 151, // Jan - Jun
        181, 212, 243, 273, 304, 334, // Jul - Dec
        365,
    ];
    const MONTH_LEAP_OFFSET: [Integer; 13] = [
        0, 31, 60, 91, 121, 152, // Jan - Jun
        182, 213, 244, 274, 305, 335, // Jul - Dec
        366,
    ];
    let i = (m - 1) as usize;
    if leap_year {
        MONTH_LEAP_OFFSET[i]
    } else {
        MONTH_OFFSET[i]
    }
}

fn year_offset(y: Year) -> SerialNumber {
    // The serial number of December 31st of the preceding year, e.g. for 1901
    // YEAR_OFFSET[1] is 366, that is, December 31st 1900.
    const YEAR_OFFSET: [SerialNumber; 301] = [
        // 1900-1909
        0, 366, 731, 1096, 1461, 1827, 2192, 2557, 2922, 3288, // 1910-1919
        3653, 4018, 4383, 4749, 5114, 5479, 5844, 6210, 6575, 6940, // 1920-1929
        7305, 7671, 8036, 8401, 8766, 9132, 9497, 9862, 10227, 10593, // 1930-1939
        10958, 11323, 11688, 12054, 12419, 12784, 13149, 13515, 13880, 14245,
        // 1940-1949
        14610, 14976, 15341, 15706, 16071, 16437, 16802, 17167, 17532, 17898,
        // 1950-1959
        18263, 18628, 18993, 19359, 19724, 20089, 20454, 20820, 21185, 21550,
        // 1960-1969
        21915, 22281, 22646, 23011, 23376, 23742, 24107, 24472, 24837, 25203,
        // 1970-1979
        25568, 25933, 26298, 26664, 27029, 27394, 27759, 28125, 28490, 28855,
        // 1980-1989
        29220, 29586, 29951, 30316, 30681, 31047, 31412, 31777, 32142, 32508,
        // 1990-1999
        32873, 33238, 33603, 33969, 34334, 34699, 35064, 35430, 35795, 36160,
        // 2000-2009
        36525, 36891, 37256, 37621, 37986, 38352, 38717, 39082, 39447, 39813,
        // 2010-2019
        40178, 40543, 40908, 41274, 41639, 42004, 42369, 42735, 43100, 43465,
        // 2020-2029
        43830, 44196, 44561, 44926, 45291, 45657, 46022, 46387, 46752, 47118,
        // 2030-2039
        47483, 47848, 48213, 48579, 48944, 49309, 49674, 50040, 50405, 50770,
        // 2040-2049
        51135, 51501, 51866, 52231, 52596, 52962, 53327, 53692, 54057, 54423,
        // 2050-2059
        54788, 55153, 55518, 55884, 56249, 56614, 56979, 57345, 57710, 58075,
        // 2060-2069
        58440, 58806, 59171, 59536, 59901, 60267, 60632, 60997, 61362, 61728,
        // 2070-2079
        62093, 62458, 62823, 63189, 63554, 63919, 64284, 64650, 65015, 65380,
        // 2080-2089
        65745, 66111, 66476, 66841, 67206, 67572, 67937, 68302, 68667, 69033,
        // 2090-2099
        69398, 69763, 70128, 70494, 70859, 71224, 71589, 71955, 72320, 72685,
        // 2100-2109
        73050, 73415, 73780, 74145, 74510, 74876, 75241, 75606, 75971, 76337,
        // 2110-2119
        76702, 77067, 77432, 77798, 78163, 78528, 78893, 79259, 79624, 79989,
        // 2120-2129
        80354, 80720, 81085, 81450, 81815, 82181, 82546, 82911, 83276, 83642,
        // 2130-2139
        84007, 84372, 84737, 85103, 85468, 85833, 86198, 86564, 86929, 87294,
        // 2140-2149
        87659, 88025, 88390, 88755, 89120, 89486, 89851, 90216, 90581, 90947,
        // 2150-2159
        91312, 91677, 92042, 92408, 92773, 93138, 93503, 93869, 94234, 94599,
        // 2160-2169
        94964, 95330, 95695, 96060, 96425, 96791, 97156, 97521, 97886, 98252,
        // 2170-2179
        98617, 98982, 99347, 99713, 100078, 100443, 100808, 101174, 101539, 101904,
        // 2180-2189
        102269, 102635, 103000, 103365, 103730, 104096, 104461, 104826, 105191, 105557,
        // 2190-2199
        105922, 106287, 106652, 107018, 107383, 107748, 108113, 108479, 108844, 109209,
        // 2200
        109574,
    ];
    YEAR_OFFSET[(y - 1900) as usize]
}

/// Output manipulators for [`Date`], porting QuantLib's `io` namespace.
///
/// Each function returns a lightweight `Display` wrapper, so they compose with
/// `format!`/`write!` without allocating: `format!("{}", io::long_date(d))`.
/// The null date renders as `"null date"` in every format. The general
/// `formatted_date` manipulator (arbitrary strftime-style patterns) is not
/// ported: QuantLib delegates it to Boost's date facet, out of scope here.
pub mod io {
    use super::{Date, Day};
    use std::fmt;

    /// `Display` wrapper for the short (`mm/dd/yyyy`) format.
    pub struct ShortDate(Date);
    /// `Display` wrapper for the long (`Month ordinal, yyyy`) format.
    pub struct LongDate(Date);
    /// `Display` wrapper for the ISO (`yyyy-mm-dd`) format.
    pub struct IsoDate(Date);

    /// Formats `d` in short format, e.g. `05/01/2023` (US `mm/dd/yyyy`).
    pub fn short_date(d: Date) -> ShortDate {
        ShortDate(d)
    }

    /// Formats `d` in long format, e.g. `May 1st, 2023`.
    pub fn long_date(d: Date) -> LongDate {
        LongDate(d)
    }

    /// Formats `d` in ISO format, e.g. `2023-05-01` (`yyyy-mm-dd`).
    pub fn iso_date(d: Date) -> IsoDate {
        IsoDate(d)
    }

    /// The English ordinal suffix of a day-of-month, e.g. `1 -> "st"`,
    /// `2 -> "nd"`, `11 -> "th"`. Matches QuantLib's `io::ordinal`.
    fn ordinal_suffix(n: Day) -> &'static str {
        match n {
            11..=13 => "th",
            _ => match n % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            },
        }
    }

    impl fmt::Display for ShortDate {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == Date::null() {
                return f.write_str("null date");
            }
            write!(
                f,
                "{:02}/{:02}/{}",
                self.0.month().ordinal(),
                self.0.day_of_month(),
                self.0.year()
            )
        }
    }

    impl fmt::Display for LongDate {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == Date::null() {
                return f.write_str("null date");
            }
            let day = self.0.day_of_month();
            write!(
                f,
                "{} {}{}, {}",
                self.0.month(),
                day,
                ordinal_suffix(day),
                self.0.year()
            )
        }
    }

    impl fmt::Display for IsoDate {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == Date::null() {
                return f.write_str("null date");
            }
            write!(
                f,
                "{:04}-{:02}-{:02}",
                self.0.year(),
                self.0.month().ordinal(),
                self.0.day_of_month()
            )
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::{Date, Month};
        use super::{iso_date, long_date, short_date};

        #[test]
        fn formats_match_quantlib() {
            let d = Date::new(1, Month::May, 2023);
            assert_eq!(short_date(d).to_string(), "05/01/2023");
            assert_eq!(long_date(d).to_string(), "May 1st, 2023");
            assert_eq!(iso_date(d).to_string(), "2023-05-01");
        }

        #[test]
        fn ordinal_suffixes() {
            let y = 2023;
            assert_eq!(
                long_date(Date::new(2, Month::May, y)).to_string(),
                "May 2nd, 2023"
            );
            assert_eq!(
                long_date(Date::new(3, Month::May, y)).to_string(),
                "May 3rd, 2023"
            );
            assert_eq!(
                long_date(Date::new(4, Month::May, y)).to_string(),
                "May 4th, 2023"
            );
            // 11th/12th/13th are "th", not "st/nd/rd"
            assert_eq!(
                long_date(Date::new(11, Month::May, y)).to_string(),
                "May 11th, 2023"
            );
            assert_eq!(
                long_date(Date::new(12, Month::May, y)).to_string(),
                "May 12th, 2023"
            );
            assert_eq!(
                long_date(Date::new(13, Month::May, y)).to_string(),
                "May 13th, 2023"
            );
            assert_eq!(
                long_date(Date::new(21, Month::May, y)).to_string(),
                "May 21st, 2023"
            );
        }

        #[test]
        fn null_date_renders_uniformly() {
            let n = Date::null();
            assert_eq!(short_date(n).to_string(), "null date");
            assert_eq!(long_date(n).to_string(), "null date");
            assert_eq!(iso_date(n).to_string(), "null date");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_date_serial_and_inspectors() {
        // Tuesday, January 15th 2002.
        let d = Date::new(15, Month::January, 2002);
        assert_eq!(d.weekday(), Weekday::Tuesday);
        assert_eq!(d.day_of_month(), 15);
        assert_eq!(d.month(), Month::January);
        assert_eq!(d.year(), 2002);
        assert_eq!(d.day_of_year(), 15);
    }

    #[test]
    fn serial_round_trip_over_full_range() {
        // Self-consistency: serial -> (d,m,y) -> serial across the whole range.
        let mut d = Date::min_date();
        let last = Date::max_date();
        while d <= last {
            let rebuilt = Date::new(d.day_of_month(), d.month(), d.year());
            assert_eq!(rebuilt.serial_number(), d.serial_number());
            if d == last {
                break;
            }
            d += 1;
        }
    }

    #[test]
    fn weekday_increments_by_one_each_day() {
        let mut d = Date::new(1, Month::January, 2000); // Saturday
        assert_eq!(d.weekday(), Weekday::Saturday);
        d += 1;
        assert_eq!(d.weekday(), Weekday::Sunday);
    }

    #[test]
    fn leap_year_february_has_29_days() {
        assert!(Date::is_leap(2000));
        assert!(!Date::is_leap(2001));
        // 1900 is (wrongly) treated as leap, in agreement with Excel's bug.
        assert!(Date::is_leap(1900));
        let end = Date::end_of_month(Date::new(1, Month::February, 2000));
        assert_eq!(end.day_of_month(), 29);
        let end = Date::end_of_month(Date::new(1, Month::February, 2001));
        assert_eq!(end.day_of_month(), 28);
    }

    #[test]
    fn nth_weekday_matches_known_value() {
        // 4th Thursday of March 1998 was March 26th.
        let d = Date::nth_weekday(4, Weekday::Thursday, Month::March, 1998);
        assert_eq!(d, Date::new(26, Month::March, 1998));
    }

    #[test]
    fn next_weekday_matches_known_value() {
        // The Friday following Tuesday, January 15th 2002 was January 18th.
        let start = Date::new(15, Month::January, 2002);
        assert_eq!(
            Date::next_weekday(start, Weekday::Friday),
            Date::new(18, Month::January, 2002)
        );
    }

    #[test]
    fn advance_by_months_clamps_day() {
        // Jan 31st + 1 month -> Feb 28th (non-leap).
        let d = Date::new(31, Month::January, 2001) + Period::new(1, TimeUnit::Months);
        assert_eq!(d, Date::new(28, Month::February, 2001));
    }

    #[test]
    fn advance_by_years_handles_leap_day() {
        // Feb 29th 2000 + 1 year -> Feb 28th 2001.
        let d = Date::new(29, Month::February, 2000) + Period::new(1, TimeUnit::Years);
        assert_eq!(d, Date::new(28, Month::February, 2001));
    }

    #[test]
    fn difference_in_days() {
        let a = Date::new(1, Month::January, 2020);
        let b = Date::new(31, Month::December, 2020);
        assert_eq!(b - a, 365); // 2020 is a leap year
    }

    #[test]
    fn display_is_iso() {
        assert_eq!(Date::new(5, Month::March, 2002).to_string(), "2002-03-05");
        assert_eq!(Date::null().to_string(), "null date");
    }
}
