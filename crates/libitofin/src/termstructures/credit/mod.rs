//! Credit term structures.
//!
//! Port of `ql/termstructures/defaulttermstructure.{hpp,cpp}` and the curves
//! under `ql/termstructures/credit/`. The
//! [`DefaultProbabilityTermStructure`](defaulttermstructure::DefaultProbabilityTermStructure)
//! trait is the contract every credit curve plugs into; the adapters and
//! concrete curves that build on it follow within EPIC Credit (#676).

pub mod defaulttermstructure;
pub mod hazardratestructure;
