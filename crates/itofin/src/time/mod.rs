//! Date and calendar machinery ported from `ql/time/`.
//!
//! This module provides the foundation the calendar layer sits on - the
//! [`Date`](date::Date) serial-number type and its component enums
//! ([`Weekday`](weekday::Weekday), [`Month`](date::Month),
//! [`TimeUnit`](timeunit::TimeUnit)) - plus the [`Calendar`](calendar::Calendar)
//! base and the per-market calendars under [`calendars`], plus the
//! [`Period`](period::Period), [`Frequency`](frequency::Frequency) and
//! [`DayCounter`](daycounter::DayCounter) machinery, plus the
//! [`Schedule`](schedule::Schedule) payment-date generation layer.

pub mod asx;
pub mod businessdayconvention;
pub mod calendar;
pub mod calendars;
pub mod date;
pub mod dategenerationrule;
pub mod daycounter;
pub mod daycounters;
pub mod ecb;
pub mod frequency;
pub mod imm;
pub mod period;
pub mod schedule;
pub mod timeunit;
pub mod weekday;
