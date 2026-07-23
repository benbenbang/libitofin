//! Period and ISO-date string parsing.
//!
//! Port of `ql/utilities/dataparsers.{hpp,cpp}`: the `PeriodParser` and
//! `DateParser::parseISO` entry points, exposed as fallible free functions
//! (design decision D4 - parsing untrusted strings is fallible, so QuantLib's
//! `QL_REQUIRE`/`QL_FAIL` become `Err`).
//!
//! # Deferred
//!
//! `DateParser::parseFormatted` (dataparsers.cpp:90-104) is a `boost::date_time`
//! locale/format-string parser that QuantLib itself `QL_FAIL`s under Solaris.
//! The core carries no strftime-style date parser and does not pull in a date
//! crate, so it is omitted here; a limited format parser could follow later if a
//! consumer needs one.

use std::str::FromStr;

use crate::errors::{QlError, QlResult};
use crate::time::date::{Date, Month};
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::Integer;

/// Parses a single-unit period such as `"6M"`, `"-3D"` or `"1Y"`.
///
/// The unit letter (`D`/`W`/`M`/`Y`, case-insensitive) must be the last
/// character, preceded by an optionally-signed integer. Mirrors
/// `PeriodParser::parseOnePeriod` (dataparsers.cpp:63-88).
pub fn parse_one_period(s: &str) -> QlResult<Period> {
    crate::require!(
        s.len() > 1,
        "single period require a string of at least 2 characters"
    );

    let i_pos = match s.find(['D', 'd', 'W', 'w', 'M', 'm', 'Y', 'y']) {
        Some(p) if p == s.len() - 1 => p,
        _ => {
            let last = s.chars().last().expect("s has length > 1");
            crate::fail!("unknown '{last}' unit");
        }
    };

    let units = match s.as_bytes()[i_pos].to_ascii_uppercase() {
        b'D' => TimeUnit::Days,
        b'W' => TimeUnit::Weeks,
        b'M' => TimeUnit::Months,
        _ => TimeUnit::Years,
    };

    let n_pos = match s.find(|c: char| c == '-' || c == '+' || c.is_ascii_digit()) {
        Some(p) if p < i_pos => p,
        _ => crate::fail!("no numbers of {units} provided"),
    };

    let n = s[n_pos..i_pos].parse::<Integer>().map_err(|_| {
        QlError::new(
            format!("unable to parse the number of units of {units} in '{s}'"),
            file!(),
            line!(),
        )
    })?;

    Ok(Period::new(n, units))
}

/// Parses a period string, summing consecutive single-unit spans.
///
/// Simple spans (`"6M"`) and compounds (`"2Y6M"` = `2Y + 6M`) are both
/// accepted; the string is split at each unit letter and each piece parsed by
/// [`parse_one_period`]. Mirrors `PeriodParser::parse` (dataparsers.cpp:40-61).
///
/// Where QuantLib leans on unsigned wraparound when no unit letter remains, this
/// port splits structurally instead: a trailing run with no unit letter is
/// reported as an unknown unit rather than looping to a runaway guard.
///
/// A compound that combines incompatible units (e.g. `"1D1Y"`) returns `Err`:
/// [`Period`]'s addition algebra would panic there, so summability is checked
/// before each accumulation, faithfully mapping QuantLib's throwing `operator+=`.
pub fn parse_period(s: &str) -> QlResult<Period> {
    crate::require!(s.len() > 1, "period string length must be at least 2");

    let mut segments = Vec::new();
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        if matches!(ch, 'D' | 'd' | 'W' | 'w' | 'M' | 'm' | 'Y' | 'y') {
            segments.push(&s[start..=i]);
            start = i + 1;
        }
    }
    crate::require!(start == s.len(), "unknown '{s}' unit");

    let mut result = parse_one_period(segments[0])?;
    for seg in &segments[1..] {
        let next = parse_one_period(seg)?;
        crate::require!(
            is_summable(&result, &next),
            "impossible addition between {result} and {next}"
        );
        result += next;
    }
    Ok(result)
}

/// Whether `Period + Period` is defined for these two spans, mirroring the guard
/// in `Period::add_assign` (period.rs): a zero-length side, matching units, or a
/// `{Years, Months}` / `{Weeks, Days}` inter-convertible pairing are summable;
/// any other cross-unit pairing would panic.
fn is_summable(acc: &Period, incoming: &Period) -> bool {
    use TimeUnit::{Days, Months, Weeks, Years};
    acc.length() == 0
        || incoming.length() == 0
        || acc.units() == incoming.units()
        || matches!(
            (acc.units(), incoming.units()),
            (Years, Months) | (Months, Years) | (Weeks, Days) | (Days, Weeks)
        )
}

/// Parses an ISO 8601 date of the form `YYYY-MM-DD`.
///
/// Mirrors `DateParser::parseISO` (dataparsers.cpp:106-114), but validates the
/// year, month and day ranges before building the [`Date`]: QuantLib lets its
/// `Date` constructor assert, whereas [`Month::from_ordinal`] and [`Date::new`]
/// panic out of range, so bad input is turned into `Err` up front. Non-ASCII
/// input fails the length/separator check rather than splitting a multibyte
/// character.
pub fn parse_iso_date(s: &str) -> QlResult<Date> {
    let bytes = s.as_bytes();
    crate::require!(
        s.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-',
        "invalid format"
    );

    let year = s[0..4]
        .parse::<Integer>()
        .map_err(|_| QlError::new("invalid format", file!(), line!()))?;
    let month = s[5..7]
        .parse::<Integer>()
        .map_err(|_| QlError::new("invalid format", file!(), line!()))?;
    let day = s[8..10]
        .parse::<Integer>()
        .map_err(|_| QlError::new("invalid format", file!(), line!()))?;

    crate::require!((1901..=2199).contains(&year), "invalid format");
    crate::require!((1..=12).contains(&month), "invalid format");

    let month = Month::from_ordinal(month);
    let last_day = Date::end_of_month(Date::new(1, month, year)).day_of_month();
    crate::require!((1..=last_day).contains(&day), "invalid format");

    Ok(Date::new(day, month, year))
}

impl FromStr for Period {
    type Err = QlError;

    /// Parses a period string via [`parse_period`], so `"6M".parse::<Period>()`
    /// works.
    fn from_str(s: &str) -> QlResult<Period> {
        parse_period(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // parse_iso_date: the isoDates case, ported from test-suite/dates.cpp:381-393.
    #[test]
    fn parses_iso_date() {
        assert_eq!(
            parse_iso_date("2006-01-15").unwrap(),
            Date::new(15, Month::January, 2006)
        );
    }

    // parse_iso_date fallible boundary: every bad shape must Err, never panic.
    // QuantLib skips these checks (its Date ctor asserts); the port pre-validates.
    #[test]
    fn iso_date_rejects_bad_input() {
        assert!(parse_iso_date("2006-1-15").is_err()); // wrong length
        assert!(parse_iso_date("2006/01/15").is_err()); // wrong separators
        assert!(parse_iso_date("2006-13-15").is_err()); // month out of range
        assert!(parse_iso_date("20xx-01-15").is_err()); // non-numeric
        assert!(parse_iso_date("2006-02-30").is_err()); // day out of month range
        assert!(parse_iso_date("1900-01-01").is_err()); // year below the Date floor
        assert!(parse_iso_date("2006-01-1é").is_err()); // non-ASCII, must not panic
    }

    // PeriodParser has no QuantLib test-suite oracle (grep confirms it is unused
    // there); the following cases are authored from dataparsers.cpp:40-88.
    #[test]
    fn parses_single_periods() {
        assert_eq!(
            parse_period("6M").unwrap(),
            Period::new(6, TimeUnit::Months)
        );
        assert_eq!(parse_period("1Y").unwrap(), Period::new(1, TimeUnit::Years));
        assert_eq!(parse_period("2W").unwrap(), Period::new(2, TimeUnit::Weeks));
        assert_eq!(
            parse_period("-3D").unwrap(),
            Period::new(-3, TimeUnit::Days)
        );
        assert_eq!(parse_period("+3D").unwrap(), Period::new(3, TimeUnit::Days));
    }

    #[test]
    fn parse_period_is_case_insensitive() {
        assert_eq!(
            parse_period("6m").unwrap(),
            Period::new(6, TimeUnit::Months)
        );
    }

    #[test]
    fn parses_compound_period() {
        let expected = Period::new(2, TimeUnit::Years) + Period::new(6, TimeUnit::Months);
        assert_eq!(parse_period("2Y6M").unwrap(), expected);
    }

    // A compound combining incompatible units must Err (not panic through
    // Period::add_assign); both orders, since the accumulator/incoming roles are
    // asymmetric. Inter-convertible pairings still parse.
    #[test]
    fn parse_period_rejects_incompatible_units() {
        assert!(parse_period("1D1Y").is_err());
        assert!(parse_period("1Y1D").is_err());
        assert_eq!(
            parse_period("1W2D").unwrap(),
            Period::new(1, TimeUnit::Weeks) + Period::new(2, TimeUnit::Days)
        );
        assert_eq!(
            parse_period("2Y6M").unwrap(),
            Period::new(2, TimeUnit::Years) + Period::new(6, TimeUnit::Months)
        );
    }

    #[test]
    fn parse_period_rejects_bad_input() {
        assert!(parse_period("").is_err()); // too short
        assert!(parse_period("6").is_err()); // too short
        assert!(parse_period("M").is_err()); // too short
        assert!(parse_period("6X").is_err()); // unknown unit
        assert!(parse_period("M6").is_err()); // unit not last / no number
        assert!(parse_period("MM").is_err()); // no number before unit
        assert!(parse_period("66").is_err()); // no unit letter (the wraparound trap)
    }

    // Round-trip: Display exists and PartialEq is semantic, so this is clean for
    // canonical single-unit periods.
    #[test]
    fn round_trips_through_display() {
        let periods = [
            Period::new(6, TimeUnit::Months),
            Period::new(2, TimeUnit::Weeks),
            Period::new(-3, TimeUnit::Days),
            Period::new(1, TimeUnit::Years),
            Period::new(12, TimeUnit::Months),
        ];
        for p in periods {
            assert_eq!(parse_period(&p.to_string()).unwrap(), p);
        }
    }

    #[test]
    fn from_str_delegates_to_parse_period() {
        assert_eq!(
            "6M".parse::<Period>().unwrap(),
            Period::new(6, TimeUnit::Months)
        );
        assert!("6X".parse::<Period>().is_err());
    }
}
