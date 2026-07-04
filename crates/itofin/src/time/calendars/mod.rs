//! Per-market calendars ported from `ql/time/calendars/`.
//!
//! Each concrete calendar is a zero-sized type whose constructor returns a
//! ready-to-use [`Calendar`](crate::time::calendar::Calendar). Calendars with
//! several holiday schedules (e.g. settlement vs exchange) take a `Market`
//! enum; the rest expose a nullary `new`.
//!
//! Two style lints are relaxed for the whole module tree:
//! - `clippy::new_ret_no_self`: constructors deliberately return `Calendar`,
//!   not `Self`.
//! - `clippy::manual_range_contains`: the holiday predicates mirror QuantLib's
//!   `d >= a && d <= b` clause style verbatim for line-by-line faithfulness.
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::manual_range_contains)]

pub mod bespokecalendar;
pub mod islamicholidays;
pub mod jointcalendar;
pub mod nullcalendar;
pub mod target;
pub mod weekendsonly;

pub use bespokecalendar::BespokeCalendar;
pub use jointcalendar::{JointCalendar, JointCalendarRule};
pub use nullcalendar::NullCalendar;
pub use target::Target;
pub use weekendsonly::WeekendsOnly;
