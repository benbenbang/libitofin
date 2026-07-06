//! Day-counting conventions ported from `ql/time/daycounters/`.
//!
//! Each concrete convention is a zero-sized type whose constructor returns a
//! ready-to-use [`DayCounter`](crate::time::daycounter::DayCounter), mirroring
//! how the per-market calendars build a
//! [`Calendar`](crate::time::calendar::Calendar).
//!
//! The constructors deliberately return `DayCounter` rather than `Self`, so the
//! `clippy::new_ret_no_self` lint is relaxed for the whole module tree.
#![allow(clippy::new_ret_no_self)]

pub mod actual360;
pub mod actual364;
pub mod actual36525;
pub mod actual365fixed;
pub mod actual366;
pub mod actualactual;
pub mod business252;
pub mod one;
pub mod thirty360;
pub mod thirty365;
