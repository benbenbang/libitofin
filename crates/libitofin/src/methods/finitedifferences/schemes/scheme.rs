//! The stepping contract a finite-difference model drives.
//!
//! C++ has no scheme base class: `FiniteDifferenceModel` is a template over
//! the evolver (`finitedifferencemodel.hpp:37`) and reaches `setStep` and
//! `step` through the type parameter. The model of #658 dispatches instead, so
//! the two methods every scheme already carries become a trait here.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::types::Time;

/// One timestep of an operator-splitting scheme.
pub trait Scheme {
    /// Fixes the timestep every later [`step`](Self::step) takes
    /// (`douglasscheme.cpp:49`).
    ///
    /// C++ leaves `dt_` at `Null<Real>()` until this is called; the schemes
    /// here hold an `Option` instead and answer an unset step with an error.
    /// `FiniteDifferenceModel::rollbackImpl` calls this once, before the first
    /// step (`finitedifferencemodel.hpp:97`).
    fn set_step(&mut self, dt: Time);

    /// Rolls `a` back over one timestep ending at `t`, in place.
    ///
    /// C++ returns `void` and closes with `a = y` (`douglasscheme.cpp:46`).
    /// The splitting solve is fallible here
    /// ([`FdmLinearOpComposite::solve_splitting`]), so its failure is carried
    /// out of the stepping loop rather than swallowed.
    ///
    /// [`FdmLinearOpComposite::solve_splitting`]:
    ///     crate::methods::finitedifferences::operators::FdmLinearOpComposite::solve_splitting
    ///
    /// # Errors
    ///
    /// Returns an error if [`set_step`](Self::set_step) has not been called,
    /// if the step runs towards negative time, or if the operator fails.
    fn step(&mut self, a: &mut Array, t: Time) -> QlResult<()>;
}
