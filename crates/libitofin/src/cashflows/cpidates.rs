use crate::errors::QlResult;
use crate::require;
use crate::time::{businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date};

pub(crate) fn valid_cpi_date(date: Date) -> QlResult<()> {
    require!(
        (367..=109574).contains(&date.serial_number()),
        "invalid CPI date"
    );
    Ok(())
}
pub(crate) fn checked_cpi_day(date: Date, n: i32) -> QlResult<Date> {
    let serial = date.serial_number().checked_add(n);
    require!(
        serial.is_some_and(|s| (367..=109574).contains(&s)),
        "CPI date outside supported range"
    );
    match serial {
        Some(s) => Ok(Date::from_serial(s)),
        None => crate::fail!("CPI date overflow"),
    }
}
pub(crate) fn checked_cpi_adjust(
    calendar: &Calendar,
    date: Date,
    convention: BusinessDayConvention,
) -> QlResult<Date> {
    use BusinessDayConvention::*;
    valid_cpi_date(date)?;
    let seek = |direction| -> QlResult<Date> {
        let mut current = date;
        while calendar.is_holiday(current) {
            current = checked_cpi_day(current, direction)?;
        }
        Ok(current)
    };
    match convention {
        Unadjusted => Ok(date),
        Following => seek(1),
        Preceding => seek(-1),
        ModifiedFollowing | HalfMonthModifiedFollowing => {
            let next = seek(1)?;
            if next.month() != date.month()
                || (convention == HalfMonthModifiedFollowing
                    && date.day_of_month() <= 15
                    && next.day_of_month() > 15)
            {
                seek(-1)
            } else {
                Ok(next)
            }
        }
        ModifiedPreceding => {
            let prior = seek(-1)?;
            if prior.month() != date.month() {
                seek(1)
            } else {
                Ok(prior)
            }
        }
        Nearest => {
            let mut after = date;
            let mut before = date;
            while calendar.is_holiday(after) && calendar.is_holiday(before) {
                after = checked_cpi_day(after, 1)?;
                before = checked_cpi_day(before, -1)?;
            }
            if calendar.is_holiday(after) {
                Ok(before)
            } else {
                Ok(after)
            }
        }
    }
}
