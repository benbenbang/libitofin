//! Douglas operator splitting.
//!
//! Port of `ql/methods/finitedifferences/schemes/douglasscheme.hpp:35` and its
//! `.cpp:26-52`. C++ derives the sibling schemes from `MixedScheme`; this one
//! is standalone there too, so nothing of that base class is needed here.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::operators::FdmLinearOpComposite;
use crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet;
use crate::shared::SharedMut;
use crate::types::{Real, Time};
use crate::{fail, require};

use super::boundaryconditionschemehelper::BoundaryConditionSchemeHelper;
use super::scheme::Scheme;

/// One explicit update followed by an implicit correction per direction.
///
/// The operator is held mutably and shared, because the scheme rebuilds it at
/// every step through [`FdmLinearOpComposite::set_time`] while the caller -
/// the solver of #658, which hands the same operator to a damping scheme -
/// keeps its own handle on it.
pub struct DouglasScheme {
    dt: Option<Time>,
    theta: Real,
    map: SharedMut<dyn FdmLinearOpComposite>,
    bc_set: BoundaryConditionSchemeHelper,
}

impl DouglasScheme {
    /// The scheme splitting `map` with weight `theta` under `bc_set`
    /// (`douglasscheme.cpp:26-29`).
    ///
    /// The timestep is unset until [`set_step`](Scheme::set_step), where C++
    /// starts at `Null<Real>()`.
    pub fn new(
        theta: Real,
        map: SharedMut<dyn FdmLinearOpComposite>,
        bc_set: FdmBoundaryConditionSet,
    ) -> Self {
        DouglasScheme {
            dt: None,
            theta,
            map,
            bc_set: BoundaryConditionSchemeHelper::new(bc_set),
        }
    }
}

impl Scheme for DouglasScheme {
    /// `douglasscheme.cpp:49`.
    fn set_step(&mut self, dt: Time) {
        self.dt = Some(dt);
    }

    /// `douglasscheme.cpp:32-47`.
    ///
    /// The right-hand side of each implicit correction applies the operator to
    /// the step's input values, not to the explicit update `y` that the same
    /// expression subtracts them from (`cpp:41`). C++ gets that from `a` still
    /// holding the input at that point, since it only assigns `a = y` at
    /// `cpp:46`; here the input stays in `a` and the update lives in a local
    /// for the same reason.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn step(&mut self, a: &mut Array, t: Time) -> QlResult<()> {
        let Some(dt) = self.dt else {
            fail!("the timestep is not set: call set_step before stepping");
        };
        require!(t - dt > -1e-8, "a step towards negative time given");
        let start = (t - dt).max(0.0);

        let mut y = {
            let mut map = self.map.borrow_mut();
            map.set_time(start, t)?;
            self.bc_set.set_time(start);

            self.bc_set.apply_before_applying(&mut *map);
            let mut y = &*a + &(dt * &map.apply(a));
            self.bc_set.apply_after_applying(&mut y);

            for i in 0..map.size() {
                let rhs = &y - &((self.theta * dt) * &map.apply_direction(i, a));
                y = map.solve_splitting(i, &rhs, -self.theta * dt)?;
            }

            y
        };
        self.bc_set.apply_after_solving(&mut y);

        *a = y;

        Ok(())
    }
}
