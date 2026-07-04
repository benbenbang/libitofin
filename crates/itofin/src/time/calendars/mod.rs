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

pub mod argentina;
pub mod australia;
pub mod austria;
pub mod bespokecalendar;
pub mod brazil;
pub mod canada;
pub mod chile;
pub mod croatia;
pub mod czechrepublic;
pub mod denmark;
pub mod finland;
pub mod france;
pub mod germany;
pub mod iceland;
pub mod islamicholidays;
pub mod italy;
pub mod jointcalendar;
pub mod malta;
pub mod mexico;
pub mod montenegro;
pub mod newzealand;
pub mod norway;
pub mod nullcalendar;
pub mod serbia;
pub mod slovakia;
pub mod slovenia;
pub mod sweden;
pub mod switzerland;
pub mod target;
pub mod unitedkingdom;
pub mod unitedstates;
pub mod weekendsonly;

pub use argentina::Argentina;
pub use australia::Australia;
pub use austria::Austria;
pub use bespokecalendar::BespokeCalendar;
pub use brazil::Brazil;
pub use canada::Canada;
pub use chile::Chile;
pub use croatia::Croatia;
pub use czechrepublic::CzechRepublic;
pub use denmark::Denmark;
pub use finland::Finland;
pub use france::France;
pub use germany::Germany;
pub use iceland::Iceland;
pub use italy::Italy;
pub use jointcalendar::{JointCalendar, JointCalendarRule};
pub use malta::Malta;
pub use mexico::Mexico;
pub use montenegro::Montenegro;
pub use newzealand::NewZealand;
pub use norway::Norway;
pub use nullcalendar::NullCalendar;
pub use serbia::Serbia;
pub use slovakia::Slovakia;
pub use slovenia::Slovenia;
pub use sweden::Sweden;
pub use switzerland::Switzerland;
pub use target::Target;
pub use unitedkingdom::UnitedKingdom;
pub use unitedstates::UnitedStates;
pub use weekendsonly::WeekendsOnly;
