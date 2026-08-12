//! Named concrete inflation indexes.
//!
//! Port of `ql/indexes/inflation/`. The concrete named index family lands here,
//! one file per C++ header as in [`ibor`](crate::indexes::ibor): the zero
//! indexes [`UkRpi`], [`UkHicp`] and [`EuHicp`], and the quoted year-on-year
//! siblings [`YyUkRpi`], [`YyEuHicp`] and [`YyEuHicpXt`], each beside the zero
//! index sharing its header. Items are re-exported flat, so the index is
//! `indexes::inflation::UkRpi` rather than `indexes::inflation::ukrpi::UkRpi`.
//!
//! Each is pure configuration over [`ZeroInflationIndex`] or
//! [`YoYInflationIndex`], so - like [`Sofr`](crate::indexes::ibor::Sofr) - each
//! is a zero-sized namespace whose `new` returns a plain one of those rather
//! than a newtype. Nothing is gained by wrapping:
//! [`Cpi::lagged_fixing`](crate::indexes::Cpi) and every other consumer take
//! the base index by reference, and a newtype would owe them an unwrapping
//! accessor for no behaviour of its own.
//!
//! Every constructor takes its [`Settings`](crate::settings::Settings) handle
//! explicitly (D5): the index reads today's date and its fixing history off
//! that handle, and there is no global to default it to. The year-on-year ones
//! leave the curve handle empty, for
//! [`with_term_structure`](crate::indexes::inflationindex::YoYInflationIndex::with_term_structure)
//! to link.
//!
//! [`ZeroInflationIndex`]: crate::indexes::inflationindex::ZeroInflationIndex
//! [`YoYInflationIndex`]: crate::indexes::inflationindex::YoYInflationIndex

pub mod euhicp;
pub mod ukhicp;
pub mod ukrpi;

pub use euhicp::{EuHicp, YyEuHicp, YyEuHicpXt};
pub use ukhicp::UkHicp;
pub use ukrpi::{UkRpi, YyUkRpi};
