//! ASX (Australian Securities Exchange) date functions.
//!
//! Port of `ql/time/asx.{hpp,cpp}`. QuantLib resolves null-date defaults
//! through the `Settings` evaluation date; per D5 the crate has no such
//! singleton, so every date and reference-date parameter is explicit here.
//! The `nextDate`/`nextCode` overloads taking a code become the
//! `*_from_code` functions.

use crate::errors::QlResult;
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;
use crate::types::Integer;

const ALL_MONTH_CODES: &[u8] = b"FGHJKMNQUVXZ";
const MAIN_CYCLE_CODES: &[u8] = b"HMUZ";

/// Whether the given date is an ASX date (the second Friday of the month;
/// of March, June, September or December only when `main_cycle` is set).
pub fn is_asx_date(date: Date, main_cycle: bool) -> bool {
    if date.weekday() != Weekday::Friday {
        return false;
    }
    let d = date.day_of_month();
    if !(8..=14).contains(&d) {
        return false;
    }
    if !main_cycle {
        return true;
    }
    matches!(
        date.month(),
        Month::March | Month::June | Month::September | Month::December
    )
}

/// Whether the given string is an ASX code (a month letter followed by a
/// year digit, e.g. `H3`), restricted to the main cycle when `main_cycle`
/// is set.
pub fn is_asx_code(code: &str, main_cycle: bool) -> bool {
    let bytes = code.as_bytes();
    if bytes.len() != 2 {
        return false;
    }
    if !bytes[1].is_ascii_digit() {
        return false;
    }
    let valid = if main_cycle {
        MAIN_CYCLE_CODES
    } else {
        ALL_MONTH_CODES
    };
    valid.contains(&bytes[0].to_ascii_uppercase())
}

/// The ASX code for the given date (e.g. `H3` for March 8th, 2013).
///
/// Fails if the input date is not an ASX date.
pub fn code(asx_date: Date) -> QlResult<String> {
    crate::require!(
        is_asx_date(asx_date, false),
        "{asx_date} is not an ASX date"
    );
    let month_code = ALL_MONTH_CODES[(asx_date.month().ordinal() - 1) as usize] as char;
    Ok(format!("{month_code}{}", asx_date.year() % 10))
}

/// The ASX date for the given ASX code (e.g. March 8th, 2013 for `H3`),
/// resolved as the first such date on or after `reference_date`.
///
/// Fails if the input string is not an ASX code, or if the resolved date
/// would fall outside the supported date range.
pub fn date(asx_code: &str, reference_date: Date) -> QlResult<Date> {
    crate::require!(
        is_asx_code(asx_code, false),
        "{asx_code} is not a valid ASX code"
    );
    let bytes = asx_code.as_bytes();
    let m = match ALL_MONTH_CODES
        .iter()
        .position(|&c| c == bytes[0].to_ascii_uppercase())
    {
        Some(index) => Month::from_ordinal(index as Integer + 1),
        None => crate::fail!("invalid ASX month letter. code: {asx_code}"),
    };
    let mut y = (bytes[1] - b'0') as Year;
    if y == 0 && reference_date.year() <= 1909 {
        y += 10;
    }
    y += reference_date.year() - reference_date.year() % 10;
    let result = next_date(Date::new(1, m, y), false);
    if result >= reference_date {
        Ok(result)
    } else {
        crate::require!(
            y + 10 <= Date::max_date().year(),
            "no ASX date matching {asx_code} on or after {reference_date} \
             within the supported date range"
        );
        Ok(next_date(Date::new(1, m, y + 10), false))
    }
}

/// The next ASX date strictly following the given date.
pub fn next_date(date: Date, main_cycle: bool) -> Date {
    let mut y = date.year();
    let mut m = date.month().ordinal();
    let offset: Integer = if main_cycle { 3 } else { 1 };
    let mut skip_months = offset - m % offset;
    if skip_months != offset || date.day_of_month() > 14 {
        skip_months += m;
        if skip_months <= 12 {
            m = skip_months;
        } else {
            m = skip_months - 12;
            y += 1;
        }
    }
    let result = Date::nth_weekday(2, Weekday::Friday, Month::from_ordinal(m), y);
    if result <= date {
        next_date(Date::new(15, Month::from_ordinal(m), y), main_cycle)
    } else {
        result
    }
}

/// The next ASX date strictly following the date the given ASX code maps to
/// at `reference_date`.
pub fn next_date_from_code(
    asx_code: &str,
    main_cycle: bool,
    reference_date: Date,
) -> QlResult<Date> {
    let asx_date = date(asx_code, reference_date)?;
    Ok(next_date(asx_date + 1, main_cycle))
}

/// The ASX code of the next ASX date strictly following the given date.
pub fn next_code(date: Date, main_cycle: bool) -> QlResult<String> {
    code(next_date(date, main_cycle))
}

/// The ASX code of the next ASX date strictly following the date the given
/// ASX code maps to at `reference_date`.
pub fn next_code_from_code(
    asx_code: &str,
    main_cycle: bool,
    reference_date: Date,
) -> QlResult<String> {
    code(next_date_from_code(asx_code, main_cycle, reference_date)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASX_CODES: [&str; 120] = [
        "F0", "G0", "H0", "J0", "K0", "M0", "N0", "Q0", "U0", "V0", "X0", "Z0", "F1", "G1", "H1",
        "J1", "K1", "M1", "N1", "Q1", "U1", "V1", "X1", "Z1", "F2", "G2", "H2", "J2", "K2", "M2",
        "N2", "Q2", "U2", "V2", "X2", "Z2", "F3", "G3", "H3", "J3", "K3", "M3", "N3", "Q3", "U3",
        "V3", "X3", "Z3", "F4", "G4", "H4", "J4", "K4", "M4", "N4", "Q4", "U4", "V4", "X4", "Z4",
        "F5", "G5", "H5", "J5", "K5", "M5", "N5", "Q5", "U5", "V5", "X5", "Z5", "F6", "G6", "H6",
        "J6", "K6", "M6", "N6", "Q6", "U6", "V6", "X6", "Z6", "F7", "G7", "H7", "J7", "K7", "M7",
        "N7", "Q7", "U7", "V7", "X7", "Z7", "F8", "G8", "H8", "J8", "K8", "M8", "N8", "Q8", "U8",
        "V8", "X8", "Z8", "F9", "G9", "H9", "J9", "K9", "M9", "N9", "Q9", "U9", "V9", "X9", "Z9",
    ];

    #[test]
    fn asx_dates_from_2000_to_2040() {
        let last = Date::new(1, Month::January, 2040);
        let mut counter = Date::new(1, Month::January, 2000);

        while counter <= last {
            let asx = next_date(counter, false);

            assert!(asx > counter, "{asx} is not greater than {counter}",);
            assert!(
                is_asx_date(asx, false),
                "{asx} is not an ASX date (calculated from {counter})",
            );
            assert!(
                asx <= next_date(counter, true),
                "{asx} is not less than or equal to the next future in the main cycle {}",
                next_date(counter, true),
            );
            assert_eq!(
                date(&code(asx).unwrap(), counter).unwrap(),
                asx,
                "{} at calendar day {counter} is not the ASX code matching {asx}",
                code(asx).unwrap(),
            );

            for asx_code in &ASX_CODES {
                assert!(
                    date(asx_code, counter).unwrap() >= counter,
                    "{} is wrong for {asx_code} at reference date {counter}",
                    date(asx_code, counter).unwrap(),
                );
            }

            counter += 1;
        }
    }

    #[test]
    fn asx_specific_dates() {
        let date_2024 = Date::new(12, Month::January, 2024);
        assert_eq!(date_2024.weekday(), Weekday::Friday);
        assert!(is_asx_date(date_2024, false));
        assert!(!is_asx_date(date_2024, true));

        assert_eq!(
            next_date_from_code("F2", false, Date::new(1, Month::January, 2000)).unwrap(),
            Date::new(8, Month::February, 2002),
        );
        assert_eq!(
            next_date_from_code("K3", true, Date::new(1, Month::January, 2014)).unwrap(),
            Date::new(9, Month::June, 2023),
        );

        assert_eq!(
            next_code(Date::new(1, Month::January, 2024), false).unwrap(),
            "F4"
        );
        assert_eq!(
            next_code(Date::new(15, Month::January, 2024), false).unwrap(),
            "G4"
        );
        assert_eq!(
            next_code(Date::new(15, Month::January, 2024), true).unwrap(),
            "H4"
        );

        assert_eq!(
            next_code_from_code("F4", false, Date::new(1, Month::January, 2020)).unwrap(),
            "G4"
        );
        assert_eq!(
            next_code_from_code("Z4", true, Date::new(1, Month::January, 2020)).unwrap(),
            "H5"
        );
    }

    #[test]
    fn date_is_rejected_beyond_the_supported_range() {
        let reference = Date::new(31, Month::December, 2199);
        assert!(date("F9", reference).is_err());
    }
}
