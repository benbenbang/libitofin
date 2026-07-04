//! Date and calendar machinery ported from `ql/time/`.
//!
//! This module provides the foundation the calendar layer sits on - the
//! [`Date`](date::Date) serial-number type and its component enums
//! ([`Weekday`](weekday::Weekday), [`Month`](date::Month),
//! [`TimeUnit`](timeunit::TimeUnit)) - plus the [`Calendar`](calendar::Calendar)
//! base and the per-market calendars under [`calendars`]. Only what the calendar
//! port requires is covered here; the fuller `Period`, `DayCounter` and
//! `Schedule` machinery from EPIC-2 is out of scope for this branch.

pub mod businessdayconvention;
pub mod calendar;
pub mod date;
pub mod period;
pub mod timeunit;
pub mod weekday;
