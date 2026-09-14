//! Checked calendar primitives and Python-compatible time conventions.
use crate::boundary::*;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::calendars::{NullCalendar, Target, WeekendsOnly};
use libitofin::time::date::{Date, Month};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actualactual::{ActualActual, Convention};
use libitofin::time::daycounters::thirty360::{Convention as ThirtyConvention, Thirty360};
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use libitofin::time::frequency::Frequency;
use libitofin::time::imm;
use libitofin::time::schedule::{MakeSchedule, Schedule};
use libitofin::time::timeunit::TimeUnit;

pub(crate) fn date(serial: i32) -> BindingResult<Date> {
    if !(367..=109_574).contains(&serial) {
        return Err(BindingError::invalid("date outside 1901-2199"));
    }
    Ok(Date::from_serial(serial))
}
pub(crate) fn day_counter(c: &Context, id: u64) -> BindingResult<DayCounter> {
    c.get(id)
}
pub(crate) fn calendar(c: &Context, id: u64) -> BindingResult<Calendar> {
    c.get(id)
}
pub(crate) fn frequency(value: i32) -> BindingResult<Frequency> {
    match value {
        0 => Ok(Frequency::Annual),
        1 => Ok(Frequency::Semiannual),
        2 => Ok(Frequency::Quarterly),
        3 => Ok(Frequency::Monthly),
        _ => Err(BindingError::invalid("unknown frequency")),
    }
}
pub(crate) fn convention(value: i32) -> BindingResult<BusinessDayConvention> {
    use BusinessDayConvention::*;
    match value {
        0 => Ok(ModifiedFollowing),
        1 => Ok(Following),
        2 => Ok(Unadjusted),
        3 => Ok(Preceding),
        4 => Ok(ModifiedPreceding),
        5 => Ok(HalfMonthModifiedFollowing),
        6 => Ok(Nearest),
        _ => Err(BindingError::invalid("unknown business-day convention")),
    }
}
pub(crate) fn time_unit(value: i32) -> BindingResult<TimeUnit> {
    match value {
        0 => Ok(TimeUnit::Days),
        1 => Ok(TimeUnit::Weeks),
        2 => Ok(TimeUnit::Months),
        3 => Ok(TimeUnit::Years),
        _ => Err(BindingError::invalid("unknown time unit")),
    }
}
pub(crate) fn generation(value: i32) -> BindingResult<DateGeneration> {
    use DateGeneration::*;
    match value {
        0 => Ok(Backward),
        1 => Ok(Forward),
        2 => Ok(Zero),
        3 => Ok(ThirdWednesday),
        4 => Ok(ThirdWednesdayInclusive),
        5 => Ok(Twentieth),
        6 => Ok(TwentiethIMM),
        7 => Ok(OldCDS),
        8 => Ok(CDS),
        9 => Ok(CDS2015),
        _ => Err(BindingError::invalid("unknown date generation rule")),
    }
}
fn ymd(day: i32, month: i32, year: i32) -> BindingResult<Date> {
    if !(1..=12).contains(&month) || !(1901..=2199).contains(&year) {
        return Err(BindingError::invalid("invalid month or year"));
    }
    let lengths = [
        31,
        if Date::is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=lengths[month as usize - 1]).contains(&day) {
        return Err(BindingError::invalid("invalid day of month"));
    }
    Ok(Date::new(day, Month::from_ordinal(month), year))
}
fn shifted(d: Date, n: i64) -> BindingResult<Date> {
    let serial = i64::from(d.serial_number())
        .checked_add(n)
        .ok_or_else(|| BindingError::invalid("date shift overflow"))?;
    date(i32::try_from(serial).map_err(|_| BindingError::invalid("date shift overflow"))?)
}
#[repr(C)]
pub struct ItofinDateParts {
    pub day: i32,
    pub month: i32,
    pub year: i32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_date_new(
    day: i32,
    month: i32,
    year: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            output(out, ymd(day, month, year)?.serial_number())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_date_parts(
    serial: i32,
    out: *mut ItofinDateParts,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let d = date(serial)?;
            output(
                out,
                ItofinDateParts {
                    day: d.day_of_month(),
                    month: d.month().ordinal(),
                    year: d.year(),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_date_shift(
    serial: i32,
    days: i64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            output(out, shifted(date(serial)?, days)?.serial_number())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_is_imm_date(
    serial: i32,
    main_cycle: u8,
    out: *mut u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            bool_flag(main_cycle)?;
            output(
                out,
                u8::from(imm::is_imm_date(date(serial)?, main_cycle != 0)),
            )
        })
    }
}
pub(crate) fn bool_flag(value: u8) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("boolean must be 0 or 1")),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_next_imm_date(
    serial: i32,
    main_cycle: u8,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let main = bool_flag(main_cycle)?;
            let mut d = shifted(date(serial)?, 1)?;
            while !imm::is_imm_date(d, main) {
                d = shifted(d, 1)?;
            }
            output(out, d.serial_number())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_day_counter_new(
    ctx: *mut Context,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dc = match kind {
                0 => Actual360::new(),
                1 => Actual365Fixed::new(),
                2 => ActualActual::with_convention(Convention::ISDA),
                3 => Thirty360::with_convention(ThirtyConvention::BondBasis),
                _ => return Err(BindingError::invalid("unknown day counter")),
            };
            output(out, c.insert(dc)?)
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_day_counter_year_fraction(
    ctx: *mut Context,
    id: u64,
    start: i32,
    end: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                day_counter(c, id)?.year_fraction(date(start)?, date(end)?),
            )
        })
    }
}
/// UTF-8 bytes without a trailing NUL. Query required length with capacity zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_time_name(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    out: *mut u8,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let name = match kind {
                0 => day_counter(c, id)?.name(),
                1 => calendar(c, id)?.name().to_string(),
                _ => return Err(BindingError::invalid("unknown name kind")),
            };
            output(required, name.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < name.len() {
                return Err(BindingError::invalid("name buffer too small"));
            }
            check_ptr(out)?;
            std::ptr::copy_nonoverlapping(name.as_ptr(), out, name.len());
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_calendar_new(
    ctx: *mut Context,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let cal = match kind {
                0 => Target::new(),
                1 => NullCalendar::new(),
                2 => WeekendsOnly::new(),
                3 => UnitedKingdom::new(Market::Settlement),
                _ => return Err(BindingError::invalid("unknown calendar")),
            };
            output(out, c.insert(cal)?)
        })
    }
}
