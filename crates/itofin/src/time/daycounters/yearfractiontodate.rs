//! "Inverse" of a day counter: mapping a year fraction back to a date.
//!
//! Port of `ql/time/daycounters/yearfractiontodate.{hpp,cpp}`. Starting from a
//! calendar-time guess, the search refines by whole years, then months, then
//! days, and finally rounds to whichever neighbouring date reproduces the
//! target fraction more closely.

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::require;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Time};

fn day_offset(x: Time) -> QlResult<Integer> {
    let days = x.round();
    require!(
        days >= Time::from(Integer::MIN) && days <= Time::from(Integer::MAX),
        "time {x} is out of range for a day offset"
    );
    Ok(days as Integer)
}

/// The date `d` such that `day_counter.year_fraction(reference_date, d)` is
/// closest to `t`, QuantLib's `yearFractionToDate`.
///
/// # Errors
///
/// Returns an error if `t` is not finite or lies outside the representable
/// day-offset range, where QuantLib's `boost::numeric_cast` throws.
pub fn year_fraction_to_date(
    day_counter: &DayCounter,
    reference_date: Date,
    t: Time,
) -> QlResult<Date> {
    let mut guess_date = reference_date + Period::new(day_offset(t * 365.25)?, TimeUnit::Days);
    let mut guess_time = day_counter.year_fraction(reference_date, guess_date);

    guess_date = guess_date + Period::new(day_offset((t - guess_time) * 365.25)?, TimeUnit::Days);
    guess_time = day_counter.year_fraction(reference_date, guess_date);

    if close_enough(guess_time, t) {
        return Ok(guess_date);
    }

    let search_direction = 1.0f64.copysign(t - guess_time) as Integer;

    let t = t + Time::from(search_direction) * 100.0 * f64::EPSILON;

    for u in [TimeUnit::Years, TimeUnit::Months, TimeUnit::Days] {
        loop {
            let next_date = guess_date + Period::new(search_direction, u);
            if Time::from(search_direction)
                * (day_counter.year_fraction(reference_date, next_date) - t)
                < 0.0
            {
                guess_date = next_date;
            } else {
                break;
            }
        }
    }

    let guess_time = day_counter.year_fraction(reference_date, guess_date);
    let next_date = guess_date + Period::new(search_direction, TimeUnit::Days);
    if close_enough(guess_time, t)
        || (day_counter.year_fraction(reference_date, next_date) - t).abs() > (guess_time - t).abs()
    {
        Ok(guess_date)
    } else {
        Ok(next_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual364::Actual364;
    use crate::time::daycounters::actual365fixed::{
        Actual365Fixed, Convention as Act365Convention,
    };
    use crate::time::daycounters::actual366::Actual366;
    use crate::time::daycounters::actual36525::Actual36525;
    use crate::time::daycounters::actualactual::{ActualActual, Convention as ActActConvention};
    use crate::time::daycounters::business252::Business252;
    use crate::time::daycounters::simpledaycounter::SimpleDayCounter;
    use crate::time::daycounters::thirty360::{Convention as Thirty360Convention, Thirty360};
    use crate::time::daycounters::thirty365::Thirty365;

    #[test]
    fn round_trips_bulk_dates_for_every_day_counter() {
        let day_counters = [
            Actual365Fixed::new(),
            Actual365Fixed::with_convention(Act365Convention::NoLeap),
            Actual360::new(),
            Actual360::with_last_day(true),
            Actual36525::new(),
            Actual36525::with_last_day(true),
            Actual364::new(),
            Actual366::new(),
            Actual366::with_last_day(true),
            ActualActual::with_convention(ActActConvention::ISDA),
            ActualActual::with_convention(ActActConvention::ISMA),
            ActualActual::with_convention(ActActConvention::Bond),
            ActualActual::with_convention(ActActConvention::Historical),
            ActualActual::with_convention(ActActConvention::Actual365),
            ActualActual::with_convention(ActActConvention::AFB),
            ActualActual::with_convention(ActActConvention::Euro),
            Business252::new(),
            Thirty360::with_convention(Thirty360Convention::USA),
            Thirty360::with_convention(Thirty360Convention::BondBasis),
            Thirty360::with_convention(Thirty360Convention::European),
            Thirty360::with_convention(Thirty360Convention::EurobondBasis),
            Thirty360::with_convention(Thirty360Convention::Italian),
            Thirty360::with_convention(Thirty360Convention::German),
            Thirty360::with_convention(Thirty360Convention::ISMA),
            Thirty360::with_convention(Thirty360Convention::ISDA),
            Thirty360::with_convention(Thirty360Convention::NASD),
            Thirty365::new(),
            SimpleDayCounter::new(),
        ];

        for dc in &day_counters {
            for i in -360..730 {
                let today = Date::new(1, Month::January, 2020) + Period::new(i, TimeUnit::Days);
                let target = today + Period::new(i, TimeUnit::Days);

                let t = dc.year_fraction(today, target);
                let time_to_date = year_fraction_to_date(dc, today, t).unwrap();
                let t_new = dc.year_fraction(today, time_to_date);

                assert!(
                    close_enough(t, t_new),
                    "today {today}, target {target}, inverse {time_to_date}, \
                     time diff {}, day counter {}",
                    t - t_new,
                    dc.name()
                );
            }
        }
    }

    #[test]
    fn rounds_to_the_closer_date() {
        let day_counters = [
            Thirty360::with_convention(Thirty360Convention::USA),
            Actual360::new(),
        ];
        let d1 = Date::new(1, Month::February, 2023);
        let d2 = Date::new(17, Month::February, 2124);

        for dc in &day_counters {
            let t = dc.year_fraction(d1, d2);
            let mut offset: Time = 0.0;
            while offset < 1.0 + 1e-10 {
                let inv = year_fraction_to_date(dc, d1, t + offset / 360.0).unwrap();
                let expected = if offset < 0.4999 { d2 } else { d2 + 1 };
                assert_eq!(inv, expected, "offset {offset}, day counter {}", dc.name());
                offset += 0.05;
            }
        }
    }

    #[test]
    fn rejects_non_finite_or_out_of_range_times() {
        let dc = Actual360::new();
        let reference = Date::new(1, Month::January, 2020);

        assert!(year_fraction_to_date(&dc, reference, Time::NAN).is_err());
        assert!(year_fraction_to_date(&dc, reference, Time::INFINITY).is_err());
        assert!(year_fraction_to_date(&dc, reference, Time::NEG_INFINITY).is_err());
        assert!(year_fraction_to_date(&dc, reference, 1e10).is_err());
        assert!(year_fraction_to_date(&dc, reference, -1e10).is_err());
    }
}
