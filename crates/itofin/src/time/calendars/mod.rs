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
pub mod botswana;
pub mod brazil;
pub mod canada;
pub mod chile;
pub mod china;
pub mod croatia;
pub mod czechrepublic;
pub mod denmark;
pub mod finland;
pub mod france;
pub mod germany;
pub mod hungary;
pub mod iceland;
pub mod islamicholidays;
pub mod italy;
pub mod jointcalendar;
pub mod malta;
pub mod mexico;
pub mod montenegro;
pub mod newzealand;
pub mod northmacedonia;
pub mod norway;
pub mod nullcalendar;
pub mod poland;
pub mod romania;
pub mod russia;
pub mod serbia;
pub mod slovakia;
pub mod slovenia;
pub mod southafrica;
pub mod sweden;
pub mod switzerland;
pub mod target;
pub mod ukraine;
pub mod unitedkingdom;
pub mod unitedstates;
pub mod uzbekistan;
pub mod weekendsonly;

pub use argentina::Argentina;
pub use australia::Australia;
pub use austria::Austria;
pub use bespokecalendar::BespokeCalendar;
pub use botswana::Botswana;
pub use brazil::Brazil;
pub use canada::Canada;
pub use chile::Chile;
pub use china::China;
pub use croatia::Croatia;
pub use czechrepublic::CzechRepublic;
pub use denmark::Denmark;
pub use finland::Finland;
pub use france::France;
pub use germany::Germany;
pub use hungary::Hungary;
pub use iceland::Iceland;
pub use italy::Italy;
pub use jointcalendar::{JointCalendar, JointCalendarRule};
pub use malta::Malta;
pub use mexico::Mexico;
pub use montenegro::Montenegro;
pub use newzealand::NewZealand;
pub use northmacedonia::NorthMacedonia;
pub use norway::Norway;
pub use nullcalendar::NullCalendar;
pub use poland::Poland;
pub use romania::Romania;
pub use russia::Russia;
pub use serbia::Serbia;
pub use slovakia::Slovakia;
pub use slovenia::Slovenia;
pub use southafrica::SouthAfrica;
pub use sweden::Sweden;
pub use switzerland::Switzerland;
pub use target::Target;
pub use ukraine::Ukraine;
pub use unitedkingdom::UnitedKingdom;
pub use unitedstates::UnitedStates;
pub use uzbekistan::Uzbekistan;
pub use weekendsonly::WeekendsOnly;
