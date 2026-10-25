use super::*;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::calendars::weekendsonly::WeekendsOnly;

#[test]
fn extreme_periods_fail_without_entering_panicking_date_arithmetic() {
    let calendar = NullCalendar::new();
    for unit in [TimeUnit::Weeks, TimeUnit::Months, TimeUnit::Years] {
        for length in [i32::MIN, i32::MAX] {
            assert!(
                advance(
                    &calendar,
                    Date::min_date(),
                    Period::new(length, unit),
                    Bdc::Unadjusted,
                    false
                )
                .is_err()
            );
        }
    }
    assert!(
        advance(
            &calendar,
            Date::max_date(),
            Period::new(1, TimeUnit::Days),
            Bdc::Unadjusted,
            false
        )
        .is_err()
    );
    assert!(
        advance(
            &calendar,
            Date::min_date(),
            Period::new(-1, TimeUnit::Days),
            Bdc::Unadjusted,
            false
        )
        .is_err()
    );
}
#[test]
fn checked_month_end_matches_native_calendar() {
    let calendar = WeekendsOnly::new();
    for date in [
        Date::new(31, crate::time::date::Month::January, 2025),
        Date::new(30, crate::time::date::Month::May, 2025),
    ] {
        for roll in [Bdc::Following, Bdc::Unadjusted] {
            for length in [-2, -1, 0, 1, 2] {
                assert_eq!(
                    advance(
                        &calendar,
                        date,
                        Period::new(length, TimeUnit::Months),
                        roll,
                        true
                    )
                    .unwrap(),
                    calendar.advance(date, length, TimeUnit::Months, roll, true)
                );
            }
        }
    }
}

#[test]
fn ordinary_checked_date_operations_match_calendar() {
    let calendar = WeekendsOnly::new();
    let date = Date::from_serial(45823);
    for units in [
        TimeUnit::Days,
        TimeUnit::Weeks,
        TimeUnit::Months,
        TimeUnit::Years,
    ] {
        for amount in [-2, -1, 0, 1, 2] {
            for roll in [
                Bdc::Following,
                Bdc::Preceding,
                Bdc::Unadjusted,
                Bdc::ModifiedFollowing,
            ] {
                assert_eq!(
                    advance(&calendar, date, Period::new(amount, units), roll, false).unwrap(),
                    calendar.advance(date, amount, units, roll, false)
                );
            }
        }
    }
}
