//! ECB (European Central Bank) reserve-maintenance date functions.
//!
//! Port of `ql/time/ecb.{hpp,cpp}`. QuantLib keeps the published maintenance
//! period start dates in a mutable global set and exposes static functions;
//! per D5 the crate avoids global mutable state, so [`Ecb`] is a value object
//! owning its date set (seeded with the published table, extendable through
//! [`add_date`](Ecb::add_date)/[`remove_date`](Ecb::remove_date)). The
//! stateless code helpers stay module-level functions, and null-date
//! defaults become explicit parameters.

use std::collections::BTreeSet;
use std::ops::Bound;

use crate::errors::QlResult;
use crate::time::date::{Date, Month, SerialNumber, Year};
use crate::types::Integer;

const MONTH_NAMES: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

const KNOWN_DATE_SERIALS: [SerialNumber; 200] = [
    38371, 38391, 38420, 38455, 38483, 38511, 38546, 38574, 38602, 38637, 38665, 38692, 38735,
    38756, 38784, 38819, 38847, 38883, 38910, 38938, 38966, 39001, 39029, 39064, 39099, 39127,
    39155, 39190, 39217, 39246, 39274, 39302, 39337, 39365, 39400, 39428, 39463, 39491, 39519,
    39554, 39582, 39610, 39638, 39673, 39701, 39729, 39764, 39792, 39834, 39855, 39883, 39911,
    39946, 39974, 40002, 40037, 40065, 40100, 40128, 40155, 40198, 40219, 40247, 40282, 40310,
    40345, 40373, 40401, 40429, 40464, 40492, 40520, 40562, 40583, 40611, 40646, 40674, 40709,
    40737, 40765, 40800, 40828, 40856, 40891, 40926, 40954, 40982, 41010, 41038, 41073, 41101,
    41129, 41164, 41192, 41227, 41255, 41290, 41318, 41346, 41374, 41402, 41437, 41465, 41493,
    41528, 41556, 41591, 41619, 41654, 41682, 41710, 41738, 41773, 41801, 41829, 41864, 41892,
    41920, 41955, 41983, 42032, 42074, 42116, 42165, 42207, 42256, 42305, 42347, 42396, 42445,
    42487, 42529, 42578, 42627, 42669, 42718, 42760, 42809, 42858, 42900, 42942, 42991, 43040,
    43089, 43131, 43167, 43216, 43265, 43307, 43356, 43398, 43447, 43495, 43537, 43572, 43628,
    43677, 43726, 43768, 43817, 43859, 43908, 43957, 43992, 44034, 44090, 44139, 44181, 44223,
    44272, 44314, 44363, 44405, 44454, 44503, 44552, 44601, 44636, 44671, 44727, 44769, 44818,
    44867, 44916, 44965, 45007, 45056, 45098, 45140, 45189, 45231, 45280, 45322, 45364, 45399,
    45455, 45497, 45553, 45588, 45644,
];

fn month_from_code(code: &[u8]) -> Option<Month> {
    let upper = [
        code[0].to_ascii_uppercase(),
        code[1].to_ascii_uppercase(),
        code[2].to_ascii_uppercase(),
    ];
    MONTH_NAMES
        .iter()
        .position(|name| name.as_bytes() == upper)
        .map(|index| Month::from_ordinal(index as Integer + 1))
}

/// Whether the given string is an ECB code (a three-letter month followed by
/// a two-digit year, e.g. `MAR10`), case-insensitively.
pub fn is_ecb_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 5
        && month_from_code(&bytes[..3]).is_some()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
}

/// The ECB code following the given ECB code (e.g. `FEB06` for `JAN06`).
///
/// Fails if the input string is not an ECB code.
pub fn next_code_from_code(ecb_code: &str) -> QlResult<String> {
    crate::require!(is_ecb_code(ecb_code), "{ecb_code} is not a valid ECB code");
    let bytes = ecb_code.as_bytes();
    let m = match month_from_code(&bytes[..3]) {
        Some(m) => m,
        None => crate::fail!("invalid ECB month. code: {ecb_code}"),
    };
    if m != Month::December {
        Ok(format!(
            "{}{}{}",
            MONTH_NAMES[m.ordinal() as usize],
            bytes[3] as char,
            bytes[4] as char,
        ))
    } else {
        let y = (bytes[3] - b'0') as Year * 10 + (bytes[4] - b'0') as Year;
        Ok(format!("JAN{:02}", (y + 1) % 100))
    }
}

/// The published ECB reserve-maintenance period start dates.
///
/// Seeded with the table published by the ECB (2005 through 2024); dates
/// beyond the table can be added with [`add_date`](Ecb::add_date).
pub struct Ecb {
    known_dates: BTreeSet<Date>,
}

impl Default for Ecb {
    fn default() -> Self {
        Ecb {
            known_dates: KNOWN_DATE_SERIALS
                .iter()
                .map(|&serial| Date::from_serial(serial))
                .collect(),
        }
    }
}

impl Ecb {
    /// Creates the date set holding the published maintenance period start dates.
    pub fn new() -> Self {
        Ecb::default()
    }

    /// The known maintenance period start dates, in ascending order.
    pub fn known_dates(&self) -> &BTreeSet<Date> {
        &self.known_dates
    }

    /// Adds a maintenance period start date to the set.
    pub fn add_date(&mut self, date: Date) {
        self.known_dates.insert(date);
    }

    /// Removes a maintenance period start date from the set.
    pub fn remove_date(&mut self, date: Date) {
        self.known_dates.remove(&date);
    }

    /// The maintenance period start date in the given month and year.
    pub fn date_from_month_year(&self, month: Month, year: Year) -> QlResult<Date> {
        self.next_date(Date::new(1, month, year) - 1)
    }

    /// The ECB date for the given ECB code (e.g. March 14th, 2007 for
    /// `MAR07`), with the century resolved against `reference_date`.
    ///
    /// Fails if the input string is not an ECB code.
    pub fn date(&self, ecb_code: &str, reference_date: Date) -> QlResult<Date> {
        crate::require!(is_ecb_code(ecb_code), "{ecb_code} is not a valid ECB code");
        let bytes = ecb_code.as_bytes();
        let m = match month_from_code(&bytes[..3]) {
            Some(m) => m,
            None => crate::fail!("invalid ECB month. code: {ecb_code}"),
        };
        let mut y = (bytes[3] - b'0') as Year * 10 + (bytes[4] - b'0') as Year;
        y += reference_date.year() - reference_date.year() % 100;
        if y < Date::min_date().year() {
            return self.next_date(Date::min_date());
        }
        self.next_date(Date::new(1, m, y) - 1)
    }

    /// The ECB code for the given date (e.g. `MAR10` for March 10th, 2010).
    ///
    /// Fails if the input date is not an ECB date.
    pub fn code(&self, ecb_date: Date) -> QlResult<String> {
        crate::require!(
            self.is_ecb_date(ecb_date),
            "{ecb_date} is not a valid ECB date"
        );
        Ok(format!(
            "{}{:02}",
            MONTH_NAMES[(ecb_date.month().ordinal() - 1) as usize],
            ecb_date.year() % 100,
        ))
    }

    /// The next maintenance period start date strictly following the given date.
    ///
    /// Fails if the given date falls at or beyond the last known date.
    pub fn next_date(&self, date: Date) -> QlResult<Date> {
        match self
            .known_dates
            .range((Bound::Excluded(date), Bound::Unbounded))
            .next()
        {
            Some(next) => Ok(*next),
            None => match self.known_dates.iter().next_back() {
                Some(last) => crate::fail!("ECB dates after {last} are unknown"),
                None => crate::fail!("no known ECB dates"),
            },
        }
    }

    /// The next maintenance period start date strictly following the date the
    /// given ECB code maps to at `reference_date`.
    pub fn next_date_from_code(&self, ecb_code: &str, reference_date: Date) -> QlResult<Date> {
        self.next_date(self.date(ecb_code, reference_date)?)
    }

    /// The maintenance period start dates strictly following the given date.
    ///
    /// Fails if the given date falls at or beyond the last known date.
    pub fn next_dates(&self, date: Date) -> QlResult<Vec<Date>> {
        let dates: Vec<Date> = self
            .known_dates
            .range((Bound::Excluded(date), Bound::Unbounded))
            .copied()
            .collect();
        if dates.is_empty() {
            match self.known_dates.iter().next_back() {
                Some(last) => crate::fail!("ECB dates after {last} are unknown"),
                None => crate::fail!("no known ECB dates"),
            }
        }
        Ok(dates)
    }

    /// The maintenance period start dates strictly following the date the
    /// given ECB code maps to at `reference_date`.
    pub fn next_dates_from_code(
        &self,
        ecb_code: &str,
        reference_date: Date,
    ) -> QlResult<Vec<Date>> {
        self.next_dates(self.date(ecb_code, reference_date)?)
    }

    /// Whether the given date is a known maintenance period start date.
    pub fn is_ecb_date(&self, date: Date) -> bool {
        self.known_dates.contains(&date)
    }

    /// The ECB code of the next maintenance period start date strictly
    /// following the given date.
    pub fn next_code(&self, date: Date) -> QlResult<String> {
        self.code(self.next_date(date)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecb_code_validity() {
        assert!(is_ecb_code("JAN00"));
        assert!(is_ecb_code("FEB78"));
        assert!(is_ecb_code("mar58"));
        assert!(is_ecb_code("aPr99"));

        assert!(!is_ecb_code(""));
        assert!(!is_ecb_code("JUNE99"));
        assert!(!is_ecb_code("JUN1999"));
        assert!(!is_ecb_code("JUNE"));
        assert!(!is_ecb_code("JUNE1999"));
        assert!(!is_ecb_code("1999"));
    }

    #[test]
    fn ecb_dates() {
        let mut ecb = Ecb::new();
        assert!(!ecb.known_dates().is_empty(), "empty ECB date vector");

        assert_eq!(
            ecb.next_dates(Date::min_date()).unwrap().len(),
            ecb.known_dates().len(),
            "nextDates(minDate) does not return all known dates",
        );

        let known_dates: Vec<Date> = ecb.known_dates().iter().copied().collect();
        let mut previous_ecb_date = Date::min_date();
        for &current_ecb_date in &known_dates {
            assert!(
                ecb.is_ecb_date(current_ecb_date),
                "{current_ecb_date} fails isECBdate check",
            );

            let ecb_date_minus_one = current_ecb_date - 1;
            assert!(
                !ecb.is_ecb_date(ecb_date_minus_one),
                "{ecb_date_minus_one} fails isECBdate check",
            );

            assert_eq!(
                ecb.next_date(ecb_date_minus_one).unwrap(),
                current_ecb_date,
                "next ECB date following {ecb_date_minus_one} must be {current_ecb_date}",
            );
            assert_eq!(
                ecb.next_date(previous_ecb_date).unwrap(),
                current_ecb_date,
                "next ECB date following {previous_ecb_date} must be {current_ecb_date}",
            );

            previous_ecb_date = current_ecb_date;
        }

        let known_date = known_dates[0];
        ecb.remove_date(known_date);
        assert!(!ecb.is_ecb_date(known_date), "unable to remove an ECB date");
        ecb.add_date(known_date);
        assert!(ecb.is_ecb_date(known_date), "unable to add an ECB date");
    }

    #[test]
    fn ecb_date_from_code() {
        let ecb = Ecb::new();
        let ref2000 = Date::new(1, Month::January, 2000);

        assert_eq!(
            ecb.date("JAN05", ref2000).unwrap(),
            Date::new(19, Month::January, 2005)
        );
        assert_eq!(
            ecb.date("FEB06", ref2000).unwrap(),
            Date::new(8, Month::February, 2006)
        );
        assert_eq!(
            ecb.date("MAR07", ref2000).unwrap(),
            Date::new(14, Month::March, 2007)
        );
        assert_eq!(
            ecb.date("APR08", ref2000).unwrap(),
            Date::new(16, Month::April, 2008)
        );
        assert_eq!(
            ecb.date("JUN09", ref2000).unwrap(),
            Date::new(10, Month::June, 2009)
        );
        assert_eq!(
            ecb.date("JUL10", ref2000).unwrap(),
            Date::new(14, Month::July, 2010)
        );
        assert_eq!(
            ecb.date("AUG11", ref2000).unwrap(),
            Date::new(10, Month::August, 2011)
        );
        assert_eq!(
            ecb.date("SEP12", ref2000).unwrap(),
            Date::new(12, Month::September, 2012)
        );
        assert_eq!(
            ecb.date("OCT13", ref2000).unwrap(),
            Date::new(9, Month::October, 2013)
        );
        assert_eq!(
            ecb.date("NOV14", ref2000).unwrap(),
            Date::new(12, Month::November, 2014)
        );
        assert_eq!(
            ecb.date("DEC15", ref2000).unwrap(),
            Date::new(9, Month::December, 2015)
        );
    }

    #[test]
    fn ecb_code_from_date() {
        let ecb = Ecb::new();
        assert_eq!(
            ecb.code(Date::new(18, Month::January, 2006)).unwrap(),
            "JAN06"
        );
        assert_eq!(
            ecb.code(Date::new(10, Month::March, 2010)).unwrap(),
            "MAR10"
        );
        assert_eq!(
            ecb.code(Date::new(1, Month::November, 2017)).unwrap(),
            "NOV17"
        );
    }

    #[test]
    fn ecb_next_code() {
        assert_eq!(next_code_from_code("JAN06").unwrap(), "FEB06");
        assert_eq!(next_code_from_code("FeB10").unwrap(), "MAR10");
        assert_eq!(next_code_from_code("OCT17").unwrap(), "NOV17");
        assert_eq!(next_code_from_code("dEC17").unwrap(), "JAN18");
        assert_eq!(next_code_from_code("dec99").unwrap(), "JAN00");
    }
}
