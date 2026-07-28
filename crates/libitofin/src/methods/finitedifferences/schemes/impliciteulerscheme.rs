//! Fully implicit Euler stepping.
//!
//! Port of `ql/methods/finitedifferences/schemes/impliciteulerscheme.hpp:35`
//! and the one-direction branch of its `.cpp:41-63`. This is the scheme the
//! backward solver of #658 damps with before handing over to the scheme the
//! caller asked for (`fdmbackwardsolver.cpp:100-106`).
//!
//! Four things in the C++ class belong to features that are not ported, and
//! are omitted here rather than accepted and left wrong:
//!
//! - the iterative branch taken when the operator splits into more than one
//!   direction (`cpp:59-73`), which needs `BiCGstab` and `GMRES`; those are
//!   deferred with the rest of the sparse-matrix work in #636, so [`step`]
//!   answers such an operator with an error instead of silently running the
//!   one-direction arm on it;
//! - the `relTol` and `solverType` constructor parameters (`hpp:47-50`), which
//!   only reach that branch. The solver of #658 constructs the scheme with
//!   both defaulted (`fdmbackwardsolver.cpp:101` and `:155`), so dropping them
//!   costs it nothing;
//! - `numberOfIterations` and the `iterations_` counter it reports
//!   (`hpp:54`), which count iterations of that branch and stay at zero
//!   without it;
//! - the protected `step(a, t, theta)` overload and the `apply(r, theta)` it
//!   uses (`hpp:57-59`), which exist for `CrankNicolsonScheme` - a `friend`
//!   (`hpp:56`) that is not ported. The public step fixes `theta` at `1.0`
//!   (`cpp:46`), which is what this one does.
//!
//! [`step`]: Scheme::step

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::operators::FdmLinearOpComposite;
use crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet;
use crate::shared::SharedMut;
use crate::types::Time;
use crate::{fail, require};

use super::boundaryconditionschemehelper::BoundaryConditionSchemeHelper;
use super::scheme::Scheme;

/// One implicit solve per step, with no explicit stage.
///
/// Unconditionally stable and only first-order accurate, which is what makes
/// it the damping scheme: a few of its steps smooth a discontinuous payoff
/// before a higher-order scheme takes over.
pub struct ImplicitEulerScheme {
    dt: Option<Time>,
    map: SharedMut<dyn FdmLinearOpComposite>,
    bc_set: BoundaryConditionSchemeHelper,
}

impl ImplicitEulerScheme {
    /// The scheme solving `map` under `bc_set` (`impliciteulerscheme.cpp:30-35`).
    ///
    /// The timestep is unset until [`set_step`](Scheme::set_step), where C++
    /// starts at `Null<Real>()`.
    pub fn new(map: SharedMut<dyn FdmLinearOpComposite>, bc_set: FdmBoundaryConditionSet) -> Self {
        ImplicitEulerScheme {
            dt: None,
            map,
            bc_set: BoundaryConditionSchemeHelper::new(bc_set),
        }
    }
}

impl Scheme for ImplicitEulerScheme {
    /// `impliciteulerscheme.cpp:78`.
    fn set_step(&mut self, dt: Time) {
        self.dt = Some(dt);
    }

    /// `impliciteulerscheme.cpp:45-63`, the one-direction branch, with `theta`
    /// at the `1.0` the public C++ step passes (`cpp:46`).
    ///
    /// Unlike Douglas there is no explicit stage, so the conditions are
    /// reached through `apply_before_solving` and `apply_after_solving` only.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn step(&mut self, a: &mut Array, t: Time) -> QlResult<()> {
        let Some(dt) = self.dt else {
            fail!("the timestep is not set: call set_step before stepping");
        };
        require!(t - dt > -1e-8, "a step towards negative time given");
        let start = (t - dt).max(0.0);

        {
            let mut map = self.map.borrow_mut();
            map.set_time(start, t)?;
            self.bc_set.set_time(start);

            self.bc_set.apply_before_solving(&mut *map, a);

            let size = map.size();
            if size != 1 {
                fail!(
                    "implicit Euler over an operator splitting into {size} directions needs the \
                     iterative solvers deferred to #636"
                );
            }
            *a = map.solve_splitting(0, a, -dt)?;
        }
        self.bc_set.apply_after_solving(a);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::testops::{
        GRID, assert_close, black_scholes_op, call_log, mesher, probe, scaled_composite,
    };
    use crate::shared::shared_mut;
    use crate::types::Real;

    const DT: Time = 0.1;
    const T: Time = 0.25;
    const COEFFICIENTS: [Real; 2] = [0.3, -0.45];

    fn implicit_euler(
        map: SharedMut<dyn FdmLinearOpComposite>,
        bc_set: FdmBoundaryConditionSet,
    ) -> ImplicitEulerScheme {
        let mut scheme = ImplicitEulerScheme::new(map, bc_set);
        scheme.set_step(DT);
        scheme
    }

    /// The whole step is the splitting solve over `[t - dt, t]`, at the full
    /// `-dt` the public C++ step's `theta = 1.0` produces.
    ///
    /// The solve has to move the values for this to say anything, so that is
    /// asserted before they are compared: the quadratic probe is what keeps
    /// the second-derivative term of the operator from vanishing.
    #[test]
    fn a_step_is_the_implicit_solve_on_the_black_scholes_operator() {
        let mesher = mesher();
        let map: SharedMut<dyn FdmLinearOpComposite> = shared_mut(black_scholes_op(&mesher));
        let mut scheme = implicit_euler(map, Vec::new());

        let u = probe(GRID);
        let mut a = u.clone();
        scheme.step(&mut a, T).unwrap();

        let mut oracle = black_scholes_op(&mesher);
        oracle.set_time(T - DT, T).unwrap();
        let expected = oracle.solve_splitting(0, &u, -DT).unwrap();

        let moved = (0..u.size())
            .map(|i| (expected[i] - u[i]).abs())
            .fold(0.0, Real::max);
        assert!(moved > 1e-6, "the solve leaves the values alone: {moved}");
        assert_close(&a, &expected);
    }

    /// Over a diagonal operator the step is `a / (1 - dt c)`. That pins the
    /// weight on `dt`, which no Black-Scholes arm can: there `apply` and
    /// `apply_direction(0, .)` are the same function, and the solve is the only
    /// method the scheme calls, so only a closed form fixes the scalar it is
    /// given.
    #[test]
    fn a_step_matches_the_closed_form_on_a_diagonal_operator() {
        let mut scheme = implicit_euler(scaled_composite(&COEFFICIENTS[..1]), Vec::new());

        let u = probe(4);
        let mut a = u.clone();
        scheme.step(&mut a, T).unwrap();

        assert_close(&a, &(&u / (1.0 - DT * COEFFICIENTS[0])));
    }

    /// `cpp:34-35` again, plus the call set: without an explicit stage the
    /// conditions never see `apply_before_applying` or `apply_after_applying`,
    /// which is what tells this scheme's boundary handling from Douglas's. The
    /// step's start is negative but inside the guard's tolerance, so the
    /// logged time also pins the `max(0, t - dt)` clamp.
    #[test]
    fn the_conditions_see_only_the_solving_calls_at_the_clamped_start() {
        let raw = scaled_composite(&COEFFICIENTS[..1]);
        let map: SharedMut<dyn FdmLinearOpComposite> = raw.clone();
        let (log, bc_set) = call_log();
        let mut scheme = implicit_euler(map, bc_set);

        let t = DT - 5e-9;
        scheme.step(&mut probe(4), t).unwrap();

        assert_eq!(raw.borrow().last_set_time, Some((0.0, t)));
        assert_eq!(
            *log.borrow(),
            vec![
                "set_time:0".to_string(),
                "before_solving".to_string(),
                "after_solving".to_string(),
            ]
        );
    }

    /// The deferral of `cpp:59-73` is visible, not silent: an operator with
    /// more than one direction is refused rather than run through the
    /// one-direction arm.
    #[test]
    fn a_multi_direction_operator_reports_the_deferred_iterative_solvers() {
        let mut scheme = implicit_euler(scaled_composite(&COEFFICIENTS), Vec::new());

        let error = scheme.step(&mut probe(4), T).unwrap_err();

        assert!(error.message().contains("#636"), "{error}");
    }

    #[test]
    fn stepping_before_the_timestep_is_set_fails() {
        let mut scheme = ImplicitEulerScheme::new(scaled_composite(&COEFFICIENTS[..1]), Vec::new());

        assert!(scheme.step(&mut probe(4), T).is_err());
    }

    /// `cpp:45`.
    #[test]
    fn a_step_towards_negative_time_fails() {
        let mut scheme = implicit_euler(scaled_composite(&COEFFICIENTS[..1]), Vec::new());

        assert!(scheme.step(&mut probe(4), DT / 2.0).is_err());
    }
}
