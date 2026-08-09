//! The concrete step conditions a solver applies between steps.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/`. C++ keeps the
//! `StepCondition` trait one level up in `stepcondition.hpp` and its
//! implementations here, so
//! [`stepcondition`](super::StepCondition) and this directory coexist by
//! design.
//!
//! [`FdmStepConditionComposite::vanilla_composite`] (`cpp:80-145`) carries the
//! European, American and Bermudan branches. Its one other site is deferred and
//! omitted visibly: `FdmDividendHandler` (`cpp:104`) to #828, so an exercise
//! type without a branch is an error rather than an empty condition list.

mod fdmamericanstepcondition;
mod fdmbermudanstepcondition;
mod fdmsnapshotcondition;
mod fdmstepconditioncomposite;

pub use fdmamericanstepcondition::FdmAmericanStepCondition;
pub use fdmbermudanstepcondition::FdmBermudanStepCondition;
pub use fdmsnapshotcondition::FdmSnapshotCondition;
pub use fdmstepconditioncomposite::FdmStepConditionComposite;
