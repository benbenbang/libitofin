//! Douglas operator splitting.
//!
//! Port of `ql/methods/finitedifferences/schemes/douglasscheme.hpp:35` and its
//! `.cpp:26-51`. C++ derives the sibling schemes from `MixedScheme`; this one
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

    /// `douglasscheme.cpp:31-47`.
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

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::testops::{
        GRID, WHOLE, assert_close, black_scholes_op, call_log, mesher, probe, scaled_composite,
    };
    use crate::methods::finitedifferences::operators::FdmLinearOp;
    use crate::shared::shared_mut;

    const THETA: Real = 0.5;
    const DT: Time = 0.1;
    const T: Time = 0.25;
    const COEFFICIENTS: [Real; 2] = [0.3, -0.45];

    fn douglas(
        map: SharedMut<dyn FdmLinearOpComposite>,
        bc_set: FdmBoundaryConditionSet,
    ) -> DouglasScheme {
        let mut scheme = DouglasScheme::new(THETA, map, bc_set);
        scheme.set_step(DT);
        scheme
    }

    /// The step replayed call by call against the real operator: the explicit
    /// update, then one implicit correction per direction whose right-hand
    /// side reads the input values. This pins that the scheme composes with a
    /// genuine time-dependent operator and that it rebuilds it over
    /// `[t - dt, t]`.
    #[test]
    fn a_step_replays_the_cpp_sequence_on_the_black_scholes_operator() {
        let mesher = mesher();
        let map: SharedMut<dyn FdmLinearOpComposite> = shared_mut(black_scholes_op(&mesher));
        let mut scheme = douglas(map, Vec::new());

        let u = probe(GRID);
        let mut a = u.clone();
        scheme.step(&mut a, T).unwrap();

        let mut oracle = black_scholes_op(&mesher);
        oracle.set_time(T - DT, T).unwrap();
        let mut y = &u + &(DT * &oracle.apply(&u));
        for i in 0..oracle.size() {
            let rhs = &y - &((THETA * DT) * &oracle.apply_direction(i, &u));
            y = oracle.solve_splitting(i, &rhs, -THETA * DT).unwrap();
        }

        assert_close(&a, &y);
    }

    /// The non-degeneracy guard behind the arm above, and the confirm-by-stub
    /// for `cpp:41`: substituting the explicit update `y` for the input `a` in
    /// the split right-hand side shifts it by `theta dt A (y - a)`, and this
    /// fixture makes that shift large. Against a linear probe it would be
    /// small enough to pass under either reading.
    #[test]
    fn the_split_right_hand_side_can_tell_the_input_from_the_update() {
        let mesher = mesher();
        let mut oracle = black_scholes_op(&mesher);
        oracle.set_time(T - DT, T).unwrap();

        let u = probe(GRID);
        let y = &u + &(DT * &oracle.apply(&u));
        let from_input = oracle.apply_direction(0, &u);
        let from_update = oracle.apply_direction(0, &y);

        let gap = (0..u.size())
            .map(|i| (THETA * DT * (from_input[i] - from_update[i])).abs())
            .fold(0.0, Real::max);
        assert!(gap > 1e-6, "the two right-hand sides are the same: {gap}");
    }

    /// Douglas over a diagonal operator has a closed form: the explicit update
    /// is `(1 + dt w) a`, and direction `i` carries it to
    /// `(y - theta dt c_i a) / (1 - theta dt c_i)`.
    ///
    /// `w` and the two `c_i` are pairwise distinct, so this is the arm that
    /// tells `apply` apart from `apply_direction` - on the Black-Scholes
    /// operator they are the same function. Running two directions also puts
    /// the second correction's right-hand side against a `y` that is no longer
    /// the explicit update, which is where the `cpp:41` reading bites hardest.
    ///
    /// The closed form carries no boundary term, so this is equally the
    /// empty-set arm: over no conditions the step is exactly the splitting.
    #[test]
    fn a_step_matches_the_closed_form_on_a_diagonal_operator() {
        let mut scheme = douglas(scaled_composite(&COEFFICIENTS), Vec::new());

        let u = probe(4);
        let mut a = u.clone();
        scheme.step(&mut a, T).unwrap();

        let mut expected = &u * (1.0 + DT * WHOLE);
        for c in COEFFICIENTS {
            expected = &(&expected - &((THETA * DT * c) * &u)) / (1.0 - THETA * DT * c);
        }

        assert_close(&a, &expected);
    }

    /// `cpp:33-34`: the operator and the conditions are both set at
    /// `max(0, t - dt)`. The clamp only shows on a step whose start is
    /// negative but inside the guard's tolerance, so that is the `t` here.
    #[test]
    fn a_step_sets_the_operator_and_the_conditions_at_the_clamped_start() {
        let raw = scaled_composite(&COEFFICIENTS[..1]);
        let map: SharedMut<dyn FdmLinearOpComposite> = raw.clone();
        let (log, bc_set) = call_log();
        let mut scheme = douglas(map, bc_set);

        let t = DT - 5e-9;
        scheme.step(&mut probe(4), t).unwrap();

        assert_eq!(raw.borrow().last_set_time, Some((0.0, t)));
        assert_eq!(
            *log.borrow(),
            vec![
                "set_time:0".to_string(),
                "before_applying".to_string(),
                "after_applying".to_string(),
                "after_solving".to_string(),
            ]
        );
    }

    /// C++ leaves `dt_` at `Null<Real>()`, where `t - dt_` trips the guard by
    /// accident; the unset step is named here instead.
    #[test]
    fn stepping_before_the_timestep_is_set_fails() {
        let mut scheme = DouglasScheme::new(THETA, scaled_composite(&COEFFICIENTS), Vec::new());

        assert!(scheme.step(&mut probe(4), T).is_err());
    }

    /// `cpp:32`.
    #[test]
    fn a_step_towards_negative_time_fails() {
        let mut scheme = douglas(scaled_composite(&COEFFICIENTS), Vec::new());

        assert!(scheme.step(&mut probe(4), DT / 2.0).is_err());
    }
}
