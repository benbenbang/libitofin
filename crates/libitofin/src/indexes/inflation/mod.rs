//! Named concrete inflation indexes.
//!
//! Port of `ql/indexes/inflation/`. The concrete named [`ZeroInflationIndex`]
//! family lands here, one file per C++ header as in
//! [`ibor`](crate::indexes::ibor), starting with [`UkRpi`], [`UkHicp`] and
//! [`EuHicp`]. Items are re-exported flat, so the index is
//! `indexes::inflation::UkRpi` rather than `indexes::inflation::ukrpi::UkRpi`.
//!
//! Each is pure configuration over [`ZeroInflationIndex`], so - like
//! [`Sofr`](crate::indexes::ibor::Sofr) - each is a zero-sized namespace whose
//! `new` returns a plain [`ZeroInflationIndex`] rather than a newtype. Nothing
//! is gained by wrapping: [`Cpi::lagged_fixing`](crate::indexes::Cpi) and every
//! other consumer take a [`ZeroInflationIndex`] by reference, and a newtype
//! would owe them an unwrapping accessor for no behaviour of its own.
//!
//! Every constructor takes its [`Settings`](crate::settings::Settings) handle
//! explicitly (D5): the index reads today's date and its fixing history off
//! that handle, and there is no global to default it to.
//!
//! [`ZeroInflationIndex`]: crate::indexes::inflationindex::ZeroInflationIndex

pub mod euhicp;
pub mod ukhicp;
pub mod ukrpi;

pub use euhicp::EuHicp;
pub use ukhicp::UkHicp;
pub use ukrpi::UkRpi;
