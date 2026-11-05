//! A step condition that fans out over a list of step conditions.
//!
//! Port of `ql/methods/finitedifferences/stepconditions/fdmstepconditioncomposite.hpp:43`
//! and its `.cpp`.

use super::{FdmAmericanStepCondition, FdmBermudanStepCondition, FdmSnapshotCondition};
use crate::cashflows::Dividend;
use crate::errors::QlResult;
use crate::exercise::{Exercise, ExerciseType};
use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::methods::finitedifferences::utilities::{FdmDividendHandler, FdmInnerValueCalculator};
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Time};

/// How far past each dividend time the second stopping time sits, for smoother
/// convergence with the number of time steps (`cpp:116-118`).
const DIVIDEND_STOP_OFFSET: Time = 1e-5;

/// The step conditions of one solver, applied in order at every step.
pub struct FdmStepConditionComposite {
    stopping_times: Vec<Time>,
    conditions: Vec<Shared<dyn StepCondition>>,
}

impl FdmStepConditionComposite {
    /// Merges the per-condition stopping times into one sorted, unique grid
    /// (`cpp:36-46`).
    ///
    /// C++ funnels the lists through a `std::set<Real>`, which orders and
    /// deduplicates on exact equality; the Rust sort-and-dedup is exact for the
    /// same reason, unlike the `close_enough` dedup a
    /// [`TimeGrid`](crate::math::timegrid::TimeGrid) applies.
    pub fn new(stopping_times: &[Vec<Time>], conditions: Vec<Shared<dyn StepCondition>>) -> Self {
        let mut all_stopping_times: Vec<Time> = stopping_times.iter().flatten().copied().collect();
        all_stopping_times.sort_by(Real::total_cmp);
        all_stopping_times.dedup();

        Self {
            stopping_times: all_stopping_times,
            conditions,
        }
    }

    /// The merged stopping times (`cpp:53-55`).
    pub fn stopping_times(&self) -> &[Time] {
        &self.stopping_times
    }

    /// The conditions, in application order (`cpp:48-51`).
    pub fn conditions(&self) -> &[Shared<dyn StepCondition>] {
        &self.conditions
    }

    /// Wraps a snapshot around an existing composite (`cpp:63-78`).
    ///
    /// The stopping times are `c2`'s plus `c1`'s capture time, and `c2` enters
    /// the condition list whole and first, `c1` second: every step applies the
    /// wrapped composite before the snapshot records the grid.
    pub fn join_conditions(
        c1: &Shared<FdmSnapshotCondition>,
        c2: &Shared<FdmStepConditionComposite>,
    ) -> Shared<FdmStepConditionComposite> {
        let stopping_times = [c2.stopping_times().to_vec(), vec![c1.time()]];

        let composite: Shared<dyn StepCondition> = c2.clone();
        let snapshot: Shared<dyn StepCondition> = c1.clone();

        shared(FdmStepConditionComposite::new(
            &stopping_times,
            vec![composite, snapshot],
        ))
    }

    /// The conditions a vanilla option is rolled back under (`cpp:80-145`).
    ///
    /// A European exercise contributes nothing: the terminal payoff alone
    /// prices it. An American exercise contributes an
    /// [`FdmAmericanStepCondition`] opening at `exercise.dates()[0]` and no
    /// stopping time, because it exercises continuously rather than on a set of
    /// dates (`cpp:126-132`). A Bermudan exercise contributes an
    /// [`FdmBermudanStepCondition`] and, unlike the American one, its exercise
    /// times as stopping times (`cpp:133-140`): the condition only fires when a
    /// step lands on an exercise time, so the solver has to be told to put one
    /// there.
    ///
    /// `ref_date` and `day_counter` place `exercise_start` and the Bermudan
    /// exercise times on the same clock as the times the solver steps through,
    /// which is the risk-free curve's reference date and day counter at the
    /// only call site (`fdblackscholesvanillaengine.cpp:190-191`).
    ///
    /// A non-empty `cash_flow` contributes an [`FdmDividendHandler`] over the
    /// dividends dated from `ref_date` to the last exercise date, both
    /// inclusive, ahead of any exercise condition (`cpp:92-106`): each step
    /// shifts the grid across the dividend before the exercise decision. Every
    /// kept dividend time `t` becomes the stopping times `min(T, t)` and
    /// `min(T, t + 1e-5)`, with `T` the last exercise time on the same clock
    /// (`cpp:108-119`). The test at `cpp:92` reads the unfiltered schedule, so
    /// a schedule that is non-empty but entirely out of range still contributes
    /// a handler, which never fires, and no stopping time.
    ///
    /// # Errors
    ///
    /// Returns `Err` for an exercise type without a branch (`cpp:122-125`).
    /// With all three types now branched the guard is total and cannot fire;
    /// it is kept as the port of the C++ `QL_REQUIRE`, and the exhaustive match
    /// below is the stronger, compile-time form of the same check.
    ///
    /// Propagates the failure of a dividend whose amount cannot be computed.
    pub fn vanilla_composite(
        cash_flow: &[Shared<dyn Dividend>],
        exercise: &Shared<dyn Exercise>,
        mesher: Shared<dyn FdmMesher>,
        calculator: Shared<dyn FdmInnerValueCalculator>,
        ref_date: Date,
        day_counter: &DayCounter,
    ) -> QlResult<Shared<FdmStepConditionComposite>> {
        require!(
            matches!(
                exercise.exercise_type(),
                ExerciseType::European | ExerciseType::American | ExerciseType::Bermudan
            ),
            "exercise type is not supported"
        );

        let mut conditions: Vec<Shared<dyn StepCondition>> = Vec::new();
        let mut stopping_times: Vec<Vec<Time>> = Vec::new();

        if !cash_flow.is_empty() {
            let maturity_date = exercise.last_date();
            let dividends: Vec<Shared<dyn Dividend>> = cash_flow
                .iter()
                .filter(|dividend| (ref_date..=maturity_date).contains(&dividend.date()))
                .cloned()
                .collect();

            let dividend_condition = shared(FdmDividendHandler::new(
                &dividends,
                Shared::clone(&mesher),
                ref_date,
                day_counter,
                0,
            )?);

            let maturity_time = day_counter.year_fraction(ref_date, maturity_date);
            let dividend_times: Vec<Time> = dividend_condition
                .dividend_times()
                .iter()
                .map(|&t| maturity_time.min(t))
                .collect();
            let offset_times = dividend_times
                .iter()
                .map(|&t| maturity_time.min(t + DIVIDEND_STOP_OFFSET))
                .collect();
            stopping_times.push(dividend_times);
            stopping_times.push(offset_times);
            conditions.push(dividend_condition);
        }

        match exercise.exercise_type() {
            ExerciseType::American => {
                let exercise_start = day_counter.year_fraction(ref_date, exercise.dates()[0]);
                conditions.push(shared(FdmAmericanStepCondition::new(
                    mesher,
                    calculator,
                    exercise_start,
                )));
            }
            ExerciseType::Bermudan => {
                let bermudan = shared(FdmBermudanStepCondition::new(
                    exercise.dates(),
                    ref_date,
                    day_counter,
                    mesher,
                    calculator,
                ));
                stopping_times.push(bermudan.exercise_times().to_vec());
                conditions.push(bermudan);
            }
            ExerciseType::European => {}
        }

        Ok(shared(FdmStepConditionComposite::new(
            &stopping_times,
            conditions,
        )))
    }
}

impl StepCondition for FdmStepConditionComposite {
    /// `cpp:57-61`: applies every condition in turn.
    fn apply_to(&self, a: &mut Array, t: Time) {
        for condition in &self.conditions {
            condition.apply_to(a, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    use crate::cashflows::dividend_vector;
    use crate::exercise::{AmericanExercise, BermudanExercise, EuropeanExercise};
    use crate::methods::finitedifferences::FiniteDifferenceModel;
    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::{FdmLinearOpIterator, FdmLinearOpLayout};
    use crate::methods::finitedifferences::schemes::Scheme;
    use crate::time::daycounters::actual360::Actual360;
    use crate::types::Size;

    struct Recorder {
        tag: &'static str,
        scale: Real,
        offset: Real,
        log: Shared<RefCell<Vec<String>>>,
    }

    impl StepCondition for Recorder {
        fn apply_to(&self, a: &mut Array, t: Time) {
            self.log.borrow_mut().push(format!("{}:{t}", self.tag));
            a[0] = a[0] * self.scale + self.offset;
        }
    }

    fn recorder(
        tag: &'static str,
        scale: Real,
        offset: Real,
        log: &Shared<RefCell<Vec<String>>>,
    ) -> Shared<dyn StepCondition> {
        shared(Recorder {
            tag,
            scale,
            offset,
            log: Shared::clone(log),
        })
    }

    #[test]
    fn conditions_are_applied_in_order() {
        let log = shared(RefCell::new(Vec::new()));
        let composite = FdmStepConditionComposite::new(
            &[],
            vec![
                recorder("first", 2.0, 1.0, &log),
                recorder("second", 3.0, 0.0, &log),
            ],
        );
        let mut values = Array::from([1.0]);

        composite.apply_to(&mut values, 0.25);

        assert_eq!(values[0], 9.0);
        assert_eq!(
            *log.borrow(),
            vec!["first:0.25".to_string(), "second:0.25".to_string()]
        );
    }

    #[test]
    fn stopping_times_are_sorted_and_deduplicated_across_the_lists() {
        let composite = FdmStepConditionComposite::new(
            &[vec![2.0, 0.5, 1.0], vec![1.0, 0.25], vec![0.5]],
            Vec::new(),
        );

        assert_eq!(composite.stopping_times(), &[0.25, 0.5, 1.0, 2.0]);
    }

    #[test]
    fn joined_conditions_merge_the_snapshot_time_into_the_stopping_times() {
        let inner = shared(FdmStepConditionComposite::new(
            &[vec![0.25, 1.0, 2.0]],
            Vec::new(),
        ));
        let snapshot = shared(FdmSnapshotCondition::new(0.75));

        let joined = FdmStepConditionComposite::join_conditions(&snapshot, &inner);

        assert_eq!(joined.stopping_times(), &[0.25, 0.75, 1.0, 2.0]);
    }

    #[test]
    fn joined_conditions_hold_the_composite_first_and_the_snapshot_second() {
        let log = shared(RefCell::new(Vec::new()));
        let inner = shared(FdmStepConditionComposite::new(
            &[vec![1.0]],
            vec![recorder("inner", 2.0, 1.0, &log)],
        ));
        let snapshot = shared(FdmSnapshotCondition::new(1.0));

        let joined = FdmStepConditionComposite::join_conditions(&snapshot, &inner);

        assert_eq!(joined.conditions().len(), 2);
        let inner_dyn: Shared<dyn StepCondition> = inner.clone();
        let snapshot_dyn: Shared<dyn StepCondition> = snapshot.clone();
        assert!(Shared::ptr_eq(&joined.conditions()[0], &inner_dyn));
        assert!(Shared::ptr_eq(&joined.conditions()[1], &snapshot_dyn));

        let mut values = Array::from([1.0]);
        joined.apply_to(&mut values, 1.0);

        assert_eq!(*log.borrow(), vec!["inner:1".to_string()]);
        assert_eq!(snapshot.values(), Array::from([3.0]));
    }

    /// A payoff of one everywhere, so a lifted grid point is visible without
    /// building a payoff and a log-grid calculator around it.
    struct UnitInnerValue;

    impl FdmInnerValueCalculator for UnitInnerValue {
        fn inner_value(&self, _iter: &FdmLinearOpIterator, _t: Time) -> Real {
            1.0
        }

        fn avg_inner_value(&self, iter: &FdmLinearOpIterator, t: Time) -> Real {
            self.inner_value(iter, t)
        }
    }

    fn vanilla_composite(
        exercise: Shared<dyn Exercise>,
    ) -> QlResult<Shared<FdmStepConditionComposite>> {
        let layout = shared(FdmLinearOpLayout::new(vec![2]));
        let mesher = shared(UniformGridMesher::new(layout, &[(0.0, 1.0)]).unwrap());
        FdmStepConditionComposite::vanilla_composite(
            &[],
            &exercise,
            mesher as Shared<dyn FdmMesher>,
            shared(UnitInnerValue) as Shared<dyn FdmInnerValueCalculator>,
            reference(),
            &Actual360::new(),
        )
    }

    fn reference() -> Date {
        Date::new(15, crate::time::date::Month::June, 2026)
    }

    #[test]
    fn a_european_exercise_contributes_no_condition_and_no_stopping_time() {
        let exercise = shared(EuropeanExercise::new(reference() + 360)) as Shared<dyn Exercise>;

        let composite = vanilla_composite(exercise).unwrap();

        assert!(composite.conditions().is_empty());
        assert!(composite.stopping_times().is_empty());
    }

    #[test]
    fn an_american_exercise_contributes_one_condition_and_no_stopping_time() {
        let exercise = shared(AmericanExercise::over(reference(), reference() + 360).unwrap())
            as Shared<dyn Exercise>;

        let composite = vanilla_composite(exercise).unwrap();

        assert_eq!(composite.conditions().len(), 1);
        assert!(composite.stopping_times().is_empty());
    }

    /// The window opens at the first exercise date, not the last: reading
    /// `last_date()` here would put `exercise_start` at 1.0 and leave the grid
    /// untouched at 0.3.
    #[test]
    fn the_american_window_opens_at_the_first_exercise_date() {
        let exercise = shared(AmericanExercise::over(reference() + 90, reference() + 360).unwrap())
            as Shared<dyn Exercise>;
        let composite = vanilla_composite(exercise).unwrap();

        let mut before = Array::from([0.0, 0.0]);
        composite.apply_to(&mut before, 0.2);
        let mut after = Array::from([0.0, 0.0]);
        composite.apply_to(&mut after, 0.3);

        assert_eq!(before, Array::from([0.0, 0.0]));
        assert_eq!(after, Array::from([1.0, 1.0]));
    }

    /// The discriminator between the two early-exercise branches: American
    /// contributes no stopping time (`cpp:126-132`), Bermudan contributes one
    /// per exercise date (`cpp:139`). Without that push the solver would step
    /// straight past every exercise time and the condition, which only fires on
    /// an exact match, would never lift the grid.
    #[test]
    fn a_bermudan_exercise_contributes_its_exercise_times_as_stopping_times() {
        let exercise = shared(
            BermudanExercise::new(vec![reference() + 180, reference() + 360], false).unwrap(),
        ) as Shared<dyn Exercise>;

        let composite = vanilla_composite(exercise).unwrap();

        assert_eq!(composite.conditions().len(), 1);
        assert_eq!(composite.stopping_times(), &[0.5, 1.0]);
    }

    const DIVIDEND_GRID: Size = 5;

    fn spot_mesher() -> Shared<dyn FdmMesher> {
        let layout = shared(FdmLinearOpLayout::new(vec![DIVIDEND_GRID]));
        shared(UniformGridMesher::new(layout, &[(50.0_f64.ln(), 150.0_f64.ln())]).unwrap())
            as Shared<dyn FdmMesher>
    }

    fn spots() -> Vec<Real> {
        let mesher = spot_mesher();
        let locations = mesher.locations(0);
        (0..DIVIDEND_GRID).map(|i| locations[i].exp()).collect()
    }

    fn dividend_composite(
        days: &[i32],
        amounts: &[Real],
        exercise: Shared<dyn Exercise>,
    ) -> Shared<FdmStepConditionComposite> {
        let dates: Vec<Date> = days.iter().map(|&day| reference() + day).collect();
        let schedule = dividend_vector(&dates, amounts).unwrap();
        FdmStepConditionComposite::vanilla_composite(
            &schedule,
            &exercise,
            spot_mesher(),
            shared(UnitInnerValue) as Shared<dyn FdmInnerValueCalculator>,
            reference(),
            &Actual360::new(),
        )
        .unwrap()
    }

    fn european() -> Shared<dyn Exercise> {
        shared(EuropeanExercise::new(reference() + 360)) as Shared<dyn Exercise>
    }

    fn assert_grid(actual: &Array, expected: &[Real]) {
        assert_eq!(actual.size(), expected.len());
        for (k, &value) in expected.iter().enumerate() {
            assert!(
                (actual[k] - value).abs() <= 1e-12,
                "{k}: {} vs {value}",
                actual[k]
            );
        }
    }

    fn shifted(x: &[Real], dividend: Real) -> Vec<Real> {
        x.iter().map(|&s| x[0].max(s - dividend)).collect()
    }

    /// The window of `cpp:99` is inclusive at both ends: the reference date and
    /// the maturity date stay, the day either side of them goes. A dividend on
    /// maturity shows the clip of `cpp:118`: its offset time would otherwise
    /// sit past the rollback's start.
    #[test]
    fn stopping_times_hold_each_kept_dividend_time_and_its_clipped_offset() {
        let composite = dividend_composite(
            &[-1, 0, 90, 360, 361],
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            european(),
        );

        assert_eq!(composite.conditions().len(), 1);
        assert_eq!(
            composite.stopping_times(),
            &[0.0, 1e-5, 0.25, 0.25 + 1e-5, 1.0]
        );
    }

    #[test]
    fn only_a_dividend_inside_the_window_moves_the_grid() {
        let composite = dividend_composite(
            &[-1, 0, 90, 360, 361],
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            european(),
        );
        let x = spots();
        let day = 1.0 / 360.0;

        for (t, dividend) in [(0.0, 2.0), (0.25, 3.0), (1.0, 4.0)] {
            let mut values = Array::from(x.clone());
            composite.apply_to(&mut values, t);
            assert_grid(&values, &shifted(&x, dividend));
        }
        for t in [-day, 1.0 + day] {
            let mut values = Array::from(x.clone());
            composite.apply_to(&mut values, t);
            assert_eq!(values, Array::from(x.clone()));
        }
    }

    /// `cpp:92` tests the schedule before the filter, so the handler is pushed
    /// even with nothing left in it, and contributes no stopping time.
    #[test]
    fn a_schedule_entirely_out_of_range_keeps_a_handler_that_never_fires() {
        let composite = dividend_composite(&[-1, 361], &[1.0, 5.0], european());
        let x = Array::from(spots());

        assert_eq!(composite.conditions().len(), 1);
        assert!(composite.stopping_times().is_empty());

        let mut values = x.clone();
        composite.apply_to(&mut values, 0.25);
        assert_eq!(values, x);
    }

    /// The offset stopping time is a grid refinement, not a second payment: the
    /// handler moves the grid at `t` and leaves it alone at `t + 1e-5`.
    #[test]
    fn the_offset_stopping_time_does_not_pay_the_dividend_again() {
        let composite = dividend_composite(&[90], &[20.0], european());
        let x = Array::from(spots());

        let mut values = x.clone();
        composite.apply_to(&mut values, 0.25 + 1e-5);

        assert_eq!(values, x);
    }

    /// The dividend condition runs first (`cpp:106`, ahead of `cpp:129` and
    /// `cpp:138`). With a payoff of `S - 100` and an intrinsic value of one, the
    /// shift then the lift leaves `[1, 1, 1, 1, 30]`; the lift then the shift
    /// reads the floored grid between 86.6 and 114 at the fourth point and
    /// gives about 4.5 there.
    #[test]
    fn the_dividend_condition_runs_before_the_exercise_condition() {
        let exercises: [Shared<dyn Exercise>; 2] = [
            shared(AmericanExercise::over(reference(), reference() + 360).unwrap()),
            shared(
                BermudanExercise::new(vec![reference() + 90, reference() + 360], false).unwrap(),
            ),
        ];

        for exercise in exercises {
            let composite = dividend_composite(&[90], &[20.0], exercise);
            assert_eq!(composite.conditions().len(), 2);

            let mut values: Array = spots()
                .iter()
                .map(|&s| s - 100.0)
                .collect::<Vec<_>>()
                .into();
            composite.apply_to(&mut values, 0.25);

            assert_grid(&values, &[1.0, 1.0, 1.0, 1.0, 30.0]);
        }
    }

    #[test]
    fn dividend_and_bermudan_stopping_times_merge() {
        let exercise = shared(
            BermudanExercise::new(vec![reference() + 180, reference() + 360], false).unwrap(),
        ) as Shared<dyn Exercise>;

        let composite = dividend_composite(&[90], &[20.0], exercise);

        assert_eq!(composite.stopping_times(), &[0.25, 0.25 + 1e-5, 0.5, 1.0]);
    }

    struct IdentityScheme;

    impl Scheme for IdentityScheme {
        fn set_step(&mut self, _dt: Time) {}

        fn step(&mut self, _a: &mut Array, _t: Time) -> QlResult<()> {
            Ok(())
        }
    }

    /// The handler matches times by exact equality, so this is what its
    /// stopping times are for. A grid of four steps never lands on 0.3 by
    /// itself; the model has to stop there, and the maturity dividend has to
    /// fire from the rollback's start. Each pays exactly once.
    #[test]
    fn a_rollback_lands_on_the_dividend_time_and_pays_it_once() {
        let x = spots();

        for (day, dividend) in [(108, 20.0), (360, 5.0)] {
            let composite = dividend_composite(&[day], &[dividend], european());
            let mut model = FiniteDifferenceModel::new(IdentityScheme, composite.stopping_times());
            let mut values = Array::from(x.clone());

            model
                .rollback(&mut values, 1.0, 0.0, 4, Some(&*composite))
                .unwrap();

            assert_grid(&values, &shifted(&x, dividend));
        }
    }

    #[test]
    fn a_composite_is_itself_a_step_condition() {
        let log = shared(RefCell::new(Vec::new()));
        let composite: Shared<dyn StepCondition> = shared(FdmStepConditionComposite::new(
            &[vec![0.5]],
            vec![recorder("only", 2.0, 1.0, &log)],
        ));
        let mut values = Array::from([1.0]);

        composite.apply_to(&mut values, 0.5);

        assert_eq!(values[0], 3.0);
        assert_eq!(*log.borrow(), vec!["only:0.5".to_string()]);
    }
}
