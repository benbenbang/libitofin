//! The solver that rolls a grid backwards in time.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdmbackwardsolver.hpp:61` and
//! its `.cpp:80-199`.

use crate::errors::QlResult;
use crate::fail;
use crate::math::array::Array;
use crate::methods::finitedifferences::FiniteDifferenceModel;
use crate::methods::finitedifferences::operators::FdmLinearOpComposite;
use crate::methods::finitedifferences::schemes::{DouglasScheme, ImplicitEulerScheme};
use crate::methods::finitedifferences::stepconditions::FdmStepConditionComposite;
use crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet;
use crate::shared::{Shared, SharedMut, shared};
use crate::types::{Real, Size, Time};

use super::{FdmSchemeDesc, FdmSchemeType};

/// Rolls a grid back over an operator, damping the start of the roll.
///
/// The damping steps run fully implicit Euler before the scheme the descriptor
/// asks for takes over, which smooths a discontinuous payoff that a
/// second-order scheme would otherwise oscillate on.
pub struct FdmBackwardSolver {
    map: SharedMut<dyn FdmLinearOpComposite>,
    bc_set: FdmBoundaryConditionSet,
    condition: Shared<FdmStepConditionComposite>,
    scheme_desc: FdmSchemeDesc,
}

impl FdmBackwardSolver {
    /// The solver stepping `map` under `bc_set` with `scheme_desc`
    /// (`fdmbackwardsolver.cpp:80-90`).
    ///
    /// A missing `condition` becomes an empty composite (`cpp:86-89`), so the
    /// rollback always has one to apply and carries no stopping times.
    pub fn new(
        map: SharedMut<dyn FdmLinearOpComposite>,
        bc_set: FdmBoundaryConditionSet,
        condition: Option<Shared<FdmStepConditionComposite>>,
        scheme_desc: FdmSchemeDesc,
    ) -> Self {
        let condition =
            condition.unwrap_or_else(|| shared(FdmStepConditionComposite::new(&[], Vec::new())));

        FdmBackwardSolver {
            map,
            bc_set,
            condition,
            scheme_desc,
        }
    }

    /// Rolls `a` back from `from` to `to` (`cpp:92-199`).
    ///
    /// The interval is split at `dampingTo` (`cpp:96-98`): `damping_steps`
    /// implicit-Euler steps run above it and `steps` of the descriptor's scheme
    /// below. Both segments carry the same timestep - `dampingTo` is placed so
    /// that `(from - dampingTo) / damping_steps` and `(dampingTo - to) / steps`
    /// are both `(from - to) / (steps + damping_steps)` - so it is the schemes
    /// used, not the step sizes, that tell a split roll from an unsplit one.
    ///
    /// Implicit Euler asks for no damping, being the damping scheme itself, so
    /// that type skips the split and runs the whole interval over all the steps
    /// (`cpp:100` and `cpp:154-161`).
    ///
    /// # Errors
    ///
    /// Returns an error if the descriptor names one of the seven scheme
    /// families that are not ported, or if a step fails.
    pub fn rollback(
        &mut self,
        a: &mut Array,
        from: Time,
        to: Time,
        steps: Size,
        damping_steps: Size,
    ) -> QlResult<()> {
        let delta_t = from - to;
        let all_steps = steps + damping_steps;
        let damping_to = from - (delta_t * damping_steps as Real) / all_steps as Real;

        if damping_steps != 0 && self.scheme_desc.scheme_type != FdmSchemeType::ImplicitEuler {
            let mut damping_model = FiniteDifferenceModel::new(
                ImplicitEulerScheme::new(self.map.clone(), self.bc_set.clone()),
                self.condition.stopping_times(),
            );
            damping_model.rollback(a, from, damping_to, damping_steps, Some(&*self.condition))?;
        }

        match self.scheme_desc.scheme_type {
            FdmSchemeType::Douglas => {
                let mut model = FiniteDifferenceModel::new(
                    DouglasScheme::new(
                        self.scheme_desc.theta,
                        self.map.clone(),
                        self.bc_set.clone(),
                    ),
                    self.condition.stopping_times(),
                );
                model.rollback(a, damping_to, to, steps, Some(&*self.condition))
            }
            FdmSchemeType::ImplicitEuler => {
                let mut model = FiniteDifferenceModel::new(
                    ImplicitEulerScheme::new(self.map.clone(), self.bc_set.clone()),
                    self.condition.stopping_times(),
                );
                model.rollback(a, from, to, all_steps, Some(&*self.condition))
            }
            unported => fail!(
                "the {unported:?} scheme is not ported: the backward solver has Douglas and \
                 implicit Euler only, the rest wait on #636"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    use crate::methods::finitedifferences::operators::FdmLinearOp;
    use crate::methods::finitedifferences::schemes::testops::{
        WHOLE, assert_close, probe, scaled_composite,
    };
    use crate::shared::shared_mut;

    const COEFFICIENT: Real = 0.4;
    const SIZE: Size = 4;
    const FROM: Time = 0.75;
    const STEPS: Size = 25;
    const DAMPING_STEPS: Size = 3;

    /// A one-direction operator that records the calls it takes, so a rollback
    /// over it shows which scheme ran over which interval: Douglas reaches for
    /// `apply` and implicit Euler never does.
    struct LogComposite {
        failing: bool,
        log: Shared<RefCell<Vec<String>>>,
    }

    impl FdmLinearOp for LogComposite {
        fn apply(&self, r: &Array) -> Array {
            self.log.borrow_mut().push("apply".to_string());
            COEFFICIENT * r
        }
    }

    impl FdmLinearOpComposite for LogComposite {
        fn size(&self) -> Size {
            1
        }

        fn set_time(&mut self, t1: Time, t2: Time) -> QlResult<()> {
            self.log
                .borrow_mut()
                .push(format!("set_time {t1:.6} {t2:.6}"));
            if self.failing {
                fail!("the operator was asked to fail");
            }

            Ok(())
        }

        fn apply_mixed(&self, r: &Array) -> Array {
            Array::with_size(r.size())
        }

        fn apply_direction(&self, _direction: Size, r: &Array) -> Array {
            COEFFICIENT * r
        }

        fn solve_splitting(&self, _direction: Size, r: &Array, s: Real) -> QlResult<Array> {
            self.log.borrow_mut().push("solve".to_string());
            Ok(r / (1.0 + s * COEFFICIENT))
        }

        fn preconditioner(&self, r: &Array, s: Real) -> QlResult<Array> {
            self.solve_splitting(0, r, s)
        }
    }

    fn log_solver(
        failing: bool,
        scheme_desc: FdmSchemeDesc,
    ) -> (Shared<RefCell<Vec<String>>>, FdmBackwardSolver) {
        let log = shared(RefCell::new(Vec::new()));
        let map = shared_mut(LogComposite {
            failing,
            log: Shared::clone(&log),
        });

        (
            Shared::clone(&log),
            FdmBackwardSolver::new(map, Vec::new(), None, scheme_desc),
        )
    }

    fn solver(scheme_desc: FdmSchemeDesc) -> FdmBackwardSolver {
        FdmBackwardSolver::new(
            scaled_composite(&[COEFFICIENT]),
            Vec::new(),
            None,
            scheme_desc,
        )
    }

    fn set_times(log: &Shared<RefCell<Vec<String>>>) -> Vec<String> {
        log.borrow()
            .iter()
            .filter(|entry| entry.starts_with("set_time"))
            .cloned()
            .collect()
    }

    fn tally(log: &Shared<RefCell<Vec<String>>>, tag: &str) -> usize {
        log.borrow().iter().filter(|entry| *entry == tag).count()
    }

    /// `cpp:96-106` and `cpp:118-125`: three implicit-Euler steps down to
    /// `dampingTo`, then twenty-five Douglas steps from there to zero.
    ///
    /// The times are hand-computed from `dampingTo = from - deltaT damping /
    /// (steps + damping)`; computing it over `steps` instead moves the seam
    /// from `0.669643` to `0.660000`. The counts are what separate the split
    /// from a single twenty-eight-step Douglas roll, which would lay down the
    /// very same times.
    #[test]
    fn damping_steps_run_implicit_euler_down_to_the_split_time() {
        let (log, mut solver) = log_solver(false, FdmSchemeDesc::douglas());

        solver
            .rollback(&mut probe(SIZE), FROM, 0.0, STEPS, DAMPING_STEPS)
            .unwrap();

        let times = set_times(&log);
        assert_eq!(times.len(), STEPS + DAMPING_STEPS);
        assert_eq!(
            times[..4],
            [
                "set_time 0.723214 0.750000",
                "set_time 0.696429 0.723214",
                "set_time 0.669643 0.696429",
                "set_time 0.642857 0.669643",
            ]
        );
        assert_eq!(times[27], "set_time 0.000000 0.026786");

        assert_eq!(tally(&log, "apply"), STEPS);
        assert_eq!(tally(&log, "solve"), STEPS + DAMPING_STEPS);
    }

    /// `cpp:100` and `cpp:154-161`: the implicit-Euler descriptor takes no
    /// damping segment - the whole interval runs as one model over
    /// `steps + dampingSteps` steps, so no step ever reaches `apply`.
    #[test]
    fn the_implicit_euler_type_skips_the_damping_split() {
        let (log, mut solver) = log_solver(false, FdmSchemeDesc::implicit_euler());

        solver
            .rollback(&mut probe(SIZE), FROM, 0.0, STEPS, DAMPING_STEPS)
            .unwrap();

        let times = set_times(&log);
        assert_eq!(times.len(), STEPS + DAMPING_STEPS);
        assert_eq!(times[0], "set_time 0.723214 0.750000");
        assert_eq!(times[3], "set_time 0.642857 0.669643");
        assert_eq!(times[27], "set_time 0.000000 0.026786");
        assert_eq!(tally(&log, "apply"), 0);
    }

    /// No damping steps leaves `dampingTo` at `from`, so the descriptor's
    /// scheme runs the whole interval on its own.
    #[test]
    fn a_roll_without_damping_steps_is_all_douglas() {
        let (log, mut solver) = log_solver(false, FdmSchemeDesc::douglas());

        solver
            .rollback(&mut probe(SIZE), FROM, 0.0, STEPS, 0)
            .unwrap();

        let times = set_times(&log);
        assert_eq!(times.len(), STEPS);
        assert_eq!(times[0], "set_time 0.720000 0.750000");
        assert_eq!(tally(&log, "apply"), STEPS);
    }

    /// The split segments carry the numbers, not just the call sequence:
    /// implicit Euler divides by `1 - dt c` twice down to `dampingTo`, then
    /// Douglas maps the result twice down to zero.
    #[test]
    fn the_two_segments_compose_into_the_closed_form() {
        let mut solver = solver(FdmSchemeDesc::douglas());
        let theta = FdmSchemeDesc::douglas().theta;
        let dt = 0.25;

        let u = probe(SIZE);
        let mut a = u.clone();
        solver.rollback(&mut a, 1.0, 0.0, 2, 2).unwrap();

        let mut expected = &u / (1.0 - dt * COEFFICIENT).powi(2);
        for _ in 0..2 {
            let input = expected.clone();
            expected = &input * (1.0 + dt * WHOLE);
            expected = &(&expected - &((theta * dt * COEFFICIENT) * &input))
                / (1.0 - theta * dt * COEFFICIENT);
        }

        assert_close(&a, &expected);
    }

    /// `cpp:154-161` again, numerically: the implicit-Euler arm rolls all
    /// `steps + dampingSteps` steps, not `steps` of them.
    #[test]
    fn the_implicit_euler_arm_runs_every_step() {
        let mut solver = solver(FdmSchemeDesc::implicit_euler());

        let u = probe(SIZE);
        let mut a = u.clone();
        solver.rollback(&mut a, 1.0, 0.0, 2, 2).unwrap();

        let expected = &u / (1.0 - 0.25 * COEFFICIENT).powi(4);
        assert_close(&a, &expected);
    }

    /// `cpp:86-89`: a solver built without a condition rolls back on its own
    /// empty composite, matching one handed that composite explicitly.
    #[test]
    fn a_missing_condition_becomes_an_empty_composite() {
        let mut implicit = solver(FdmSchemeDesc::douglas());
        let mut explicit = FdmBackwardSolver::new(
            scaled_composite(&[COEFFICIENT]),
            Vec::new(),
            Some(shared(FdmStepConditionComposite::new(&[], Vec::new()))),
            FdmSchemeDesc::douglas(),
        );

        let mut a = probe(SIZE);
        let mut b = probe(SIZE);
        implicit.rollback(&mut a, 1.0, 0.0, 4, 2).unwrap();
        explicit.rollback(&mut b, 1.0, 0.0, 4, 2).unwrap();

        assert_close(&a, &b);
    }

    /// `cpp:197` fails on an unknown type; the seven families with no scheme
    /// behind them are named rather than reached, so a caller that asks for one
    /// is told which one it asked for instead of silently getting another.
    #[test]
    fn every_unported_scheme_type_is_rejected_by_name() {
        let unported = [
            FdmSchemeType::Hundsdorfer,
            FdmSchemeType::CraigSneyd,
            FdmSchemeType::ModifiedCraigSneyd,
            FdmSchemeType::ExplicitEuler,
            FdmSchemeType::MethodOfLines,
            FdmSchemeType::TrBDF2,
            FdmSchemeType::CrankNicolson,
        ];

        for scheme_type in unported {
            let mut solver = solver(FdmSchemeDesc::new(scheme_type, 0.5, 0.5));

            let error = solver
                .rollback(&mut probe(SIZE), 1.0, 0.0, 4, 0)
                .expect_err("an unported scheme type must not roll back");
            assert!(
                error.to_string().contains(&format!("{scheme_type:?}")),
                "{scheme_type:?} is not named in {error}"
            );
        }
    }

    /// The operator's failure is carried out of both segments rather than
    /// swallowed: the damping model is the first to touch it, so the roll stops
    /// there.
    #[test]
    fn an_operator_failure_stops_the_rollback() {
        let (log, mut solver) = log_solver(true, FdmSchemeDesc::douglas());

        assert!(
            solver
                .rollback(&mut probe(SIZE), FROM, 0.0, STEPS, DAMPING_STEPS)
                .is_err()
        );
        assert_eq!(set_times(&log).len(), 1);
    }
}
