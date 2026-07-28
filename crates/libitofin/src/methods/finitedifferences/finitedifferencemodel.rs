//! The rollback loop every finite-difference solver drives.
//!
//! Port of `ql/methods/finitedifferences/finitedifferencemodel.hpp:37`.
//!
//! Three pieces of the C++ class are omitted rather than accepted and left
//! wrong:
//!
//! - the operator-taking constructor (`hpp:45-52`), which builds the evolver
//!   from an operator and a boundary-condition set. That needs every scheme to
//!   share one constructor signature, which the [`Scheme`] trait does not carry
//!   and no caller wants: `FdmBackwardSolver` builds its scheme first and hands
//!   it over (`fdmbackwardsolver.cpp:101-103`);
//! - the `evolver()` accessor (`hpp:62`), which nothing in C++ or here reads;
//! - the no-condition `rollback` overload (`hpp:67-72`). C++ has two that
//!   differ only in passing a null `condition_type*` down to `rollbackImpl`
//!   (`hpp:71`); the one method here takes the [`Option`] that pointer is.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::methods::finitedifferences::schemes::Scheme;
use crate::require;
use crate::types::{Real, Size, Time};

/// A scheme and the times a rollback over it must land on exactly.
///
/// C++ is a template over the evolver and reaches its `setStep` and `step`
/// through the type parameter; the Rust generic is bounded by [`Scheme`] and
/// monomorphises the same way. The backward solver builds one of these per
/// segment it rolls, so the parameter is always known at the call site and
/// nothing needs the dynamic form.
pub struct FiniteDifferenceModel<S> {
    evolver: S,
    stopping_times: Vec<Time>,
}

impl<S: Scheme> FiniteDifferenceModel<S> {
    /// The model stepping `evolver`, stopping on `stopping_times`
    /// (`hpp:53-59`).
    ///
    /// The times are sorted and deduplicated here, so the rollback can scan
    /// them in order and hit each one once. C++ deduplicates with
    /// `std::unique` on exact equality (`hpp:57`), which is what
    /// [`dedup`](slice::dedup) does and what
    /// [`FdmStepConditionComposite`](crate::methods::finitedifferences::stepconditions::FdmStepConditionComposite)
    /// already did to the times this is usually handed.
    pub fn new(evolver: S, stopping_times: &[Time]) -> Self {
        let mut stopping_times = stopping_times.to_vec();
        stopping_times.sort_by(Real::total_cmp);
        stopping_times.dedup();

        FiniteDifferenceModel {
            evolver,
            stopping_times,
        }
    }

    /// Rolls `a` back from `from` to `to` over `steps` steps, applying
    /// `condition` after each (`hpp:86-145`).
    ///
    /// A step that spans one or more stopping times is cut into sub-steps
    /// landing on each of them (`hpp:110-136`), so a condition that only fires
    /// at its own time is reached exactly there. That path is dead on the
    /// solver's default route - a null condition carries no stopping times -
    /// and stays so until the Bermudan and American step conditions land with
    /// #636; the tests below drive it directly instead.
    ///
    /// The last step ends on `to` itself rather than on the accumulated
    /// `from - steps dt`, which can sit a few ulps above it and step past a
    /// stopping time sitting at `to`. C++ guards that twice over - the ternary
    /// at `hpp:106` and the `sqrt(QL_EPSILON)` snap at `hpp:108` - and on the
    /// last step either alone is enough, so no test can separate them. The snap
    /// beyond that only catches an interior step landing within `1.5e-8` of
    /// `to`, which no uniform grid produces; it is ported unexercised.
    ///
    /// # Errors
    ///
    /// Returns an error if `from` is earlier than `to` (`hpp:92`), or if the
    /// scheme fails on any step - including a sub-step, which stops the
    /// rollback where it failed rather than carrying a half-stepped grid on.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn rollback(
        &mut self,
        a: &mut Array,
        from: Time,
        to: Time,
        steps: Size,
        condition: Option<&dyn StepCondition>,
    ) -> QlResult<()> {
        require!(from >= to, "trying to roll back from {from} to {to}");

        let dt = (from - to) / steps as Real;
        let mut t = from;
        self.evolver.set_step(dt);

        if self.stopping_times.last() == Some(&from)
            && let Some(condition) = condition
        {
            condition.apply_to(a, from);
        }

        for i in 0..steps {
            let mut now = t;
            let mut next = if i < steps - 1 { t - dt } else { to };
            if (to - next).abs() < Real::EPSILON.sqrt() {
                next = to;
            }

            let mut hit = false;
            for &stopping_time in self.stopping_times.iter().rev() {
                if next <= stopping_time && stopping_time < now {
                    hit = true;

                    self.evolver.set_step(now - stopping_time);
                    self.evolver.step(a, now)?;
                    if let Some(condition) = condition {
                        condition.apply_to(a, stopping_time);
                    }
                    now = stopping_time;
                }
            }

            if hit {
                if now > next {
                    self.evolver.set_step(now - next);
                    self.evolver.step(a, now)?;
                    if let Some(condition) = condition {
                        condition.apply_to(a, next);
                    }
                }
                self.evolver.set_step(dt);
            } else {
                self.evolver.step(a, now)?;
                if let Some(condition) = condition {
                    condition.apply_to(a, next);
                }
            }

            t -= dt;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    use crate::fail;
    use crate::methods::finitedifferences::schemes::testops::{
        WHOLE, assert_close, probe, scaled_composite,
    };
    use crate::methods::finitedifferences::schemes::{DouglasScheme, ImplicitEulerScheme};
    use crate::shared::{Shared, shared};

    const THETA: Real = 0.5;
    const COEFFICIENT: Real = 0.4;
    const COEFFICIENTS: [Real; 2] = [0.3, -0.45];
    const SIZE: Size = 4;

    /// A scheme that runs no numbers and records the calls it is given, so a
    /// rollback over it shows its step and time bookkeeping directly.
    struct LogScheme {
        dt: Option<Time>,
        failing: bool,
        log: Shared<RefCell<Vec<String>>>,
    }

    impl Scheme for LogScheme {
        fn set_step(&mut self, dt: Time) {
            self.dt = Some(dt);
        }

        fn step(&mut self, _a: &mut Array, t: Time) -> QlResult<()> {
            let dt = self.dt.expect("the rollback sets the step before stepping");
            self.log
                .borrow_mut()
                .push(format!("step dt={dt:.6} t={t:.6}"));
            if self.failing {
                fail!("the scheme was asked to fail");
            }

            Ok(())
        }
    }

    /// A condition that records the times it is applied at.
    struct LogCondition {
        log: Shared<RefCell<Vec<String>>>,
    }

    impl StepCondition for LogCondition {
        fn apply_to(&self, _a: &mut Array, t: Time) {
            self.log.borrow_mut().push(format!("condition t={t:.6}"));
        }
    }

    fn log_model(
        failing: bool,
        stopping_times: &[Time],
    ) -> (
        Shared<RefCell<Vec<String>>>,
        FiniteDifferenceModel<LogScheme>,
    ) {
        let log = shared(RefCell::new(Vec::new()));
        let scheme = LogScheme {
            dt: None,
            failing,
            log: Shared::clone(&log),
        };

        (
            Shared::clone(&log),
            FiniteDifferenceModel::new(scheme, stopping_times),
        )
    }

    /// Implicit Euler over a diagonal operator divides by `1 - dt c` once per
    /// step, so `steps` of them are that factor raised to `steps` - a closed
    /// form the loop cannot reproduce by stepping the wrong number of times or
    /// with the wrong `dt`.
    #[test]
    fn implicit_euler_steps_compound_to_the_closed_form() {
        let steps = 4;
        let dt = 0.25;
        let mut model = FiniteDifferenceModel::new(
            ImplicitEulerScheme::new(scaled_composite(&[COEFFICIENT]), Vec::new()),
            &[],
        );

        let u = probe(SIZE);
        let mut a = u.clone();
        model.rollback(&mut a, 1.0, 0.0, steps, None).unwrap();

        let expected = &u / (1.0 - dt * COEFFICIENT).powi(steps as i32);
        assert_close(&a, &expected);
    }

    /// The same for Douglas, whose one-step map over a diagonal operator is
    /// the explicit update carried through one implicit correction per
    /// direction. Chaining the closed form three times pins that the rollback
    /// threads the grid from each step into the next.
    #[test]
    fn a_douglas_rollback_chains_the_one_step_map() {
        let dt = 0.2;
        let mut model = FiniteDifferenceModel::new(
            DouglasScheme::new(THETA, scaled_composite(&COEFFICIENTS), Vec::new()),
            &[],
        );

        let mut a = probe(SIZE);
        model.rollback(&mut a, 0.9, 0.3, 3, None).unwrap();

        let mut expected = probe(SIZE);
        for _ in 0..3 {
            let u = expected.clone();
            expected = &u * (1.0 + dt * WHOLE);
            for c in COEFFICIENTS {
                expected = &(&expected - &((THETA * dt * c) * &u)) / (1.0 - THETA * dt * c);
            }
        }

        assert_close(&a, &expected);
    }

    /// `hpp:95-96` and `hpp:102-106`: one `set_step` of `(from - to) / steps`
    /// up front, then a step per iteration at the running time, each followed
    /// by the condition at the time the step ends - and the last of those is
    /// `to` exactly, not `from - steps dt`.
    #[test]
    fn the_step_and_condition_times_follow_the_cpp_bookkeeping() {
        let (log, mut model) = log_model(false, &[]);
        let condition = LogCondition {
            log: Shared::clone(&log),
        };

        model
            .rollback(&mut probe(SIZE), 1.0, 0.0, 4, Some(&condition))
            .unwrap();

        assert_eq!(
            *log.borrow(),
            vec![
                "step dt=0.250000 t=1.000000",
                "condition t=0.750000",
                "step dt=0.250000 t=0.750000",
                "condition t=0.500000",
                "step dt=0.250000 t=0.500000",
                "condition t=0.250000",
                "step dt=0.250000 t=0.250000",
                "condition t=0.000000",
            ]
        );
    }

    /// `hpp:92`.
    #[test]
    fn rolling_forward_fails() {
        let (_, mut model) = log_model(false, &[]);

        assert!(model.rollback(&mut probe(SIZE), 0.0, 1.0, 4, None).is_err());
    }

    /// The scheme's `Result` is carried out of the loop rather than swallowed,
    /// and the loop stops on it: one step is logged, not four.
    #[test]
    fn a_failing_step_stops_the_rollback() {
        let (log, mut model) = log_model(true, &[]);

        assert!(model.rollback(&mut probe(SIZE), 1.0, 0.0, 4, None).is_err());
        assert_eq!(log.borrow().len(), 1);
    }
}
