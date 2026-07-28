//! The concrete step conditions a solver applies between steps.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/`. C++ keeps the
//! `StepCondition` trait one level up in `stepcondition.hpp` and its
//! implementations here, so
//! [`stepcondition`](super::StepCondition) and this directory coexist by
//! design.
//!
//! `FdmStepConditionComposite::vanillaComposite` (`cpp:80-145`) is deferred to
//! #636: it is the only site of `FdmDividendHandler` (`cpp:104`),
//! `FdmAmericanStepCondition` (`cpp:130`) and `FdmBermudanStepCondition`
//! (`cpp:134`), none of which are ported yet.

mod fdmsnapshotcondition;
mod fdmstepconditioncomposite;

pub use fdmsnapshotcondition::FdmSnapshotCondition;
pub use fdmstepconditioncomposite::FdmStepConditionComposite;
