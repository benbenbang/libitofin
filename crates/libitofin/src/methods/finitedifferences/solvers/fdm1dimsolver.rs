//! The driver that rolls a one-dimensional grid back and reads it off.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdm1dimsolver.hpp:38` and its
//! `.cpp`.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::cubic::{CubicInterpolation, MonotonicCubicNaturalSpline};
use crate::methods::finitedifferences::operators::FdmLinearOpComposite;
use crate::methods::finitedifferences::stepconditions::{
    FdmSnapshotCondition, FdmStepConditionComposite,
};
use crate::shared::{Shared, SharedMut, shared};
use crate::types::{Real, Time};

use super::{FdmBackwardSolver, FdmSchemeDesc, FdmSolverDesc};

/// The interval the theta capture is placed inside, one day in years
/// (`fdm1dimsolver.cpp:37`).
const ONE_DAY: Time = 1.0 / 365.0;

/// The fraction of that interval the capture sits at (`cpp:37`), which keeps
/// the snapshot strictly before the first stopping time rather than on it.
const CAPTURE_FRACTION: Real = 0.99;

/// Seeds a grid with the terminal payoff, rolls it back to today and answers
/// value, derivative and theta off a monotone natural cubic spline.
///
/// The grid is read along direction zero only (`cpp:49`): C++ takes a
/// one-dimensional layout for granted here and so does this port, which is
/// what the engines of #668 construct.
///
/// C++ is a `LazyObject` (`hpp:38`) and this is not. The distinction is not
/// observed: `FdmBlackScholesSolver` builds a fresh solver on every
/// recalculation (`fdmblackscholessolver.cpp:55`), so the laziness only ever
/// buys the compute-once behaviour of a single pricing, never a notification
/// from the market data underneath. That is a plain internal cache, and the
/// observer wiring a [`LazyObject`](crate::patterns::lazyobject::LazyObject)
/// brings would be wiring nothing ever fires.
pub struct Fdm1DimSolver {
    solver_desc: FdmSolverDesc,
    scheme_desc: FdmSchemeDesc,
    op: SharedMut<dyn FdmLinearOpComposite>,
    theta_condition: Shared<FdmSnapshotCondition>,
    conditions: Shared<FdmStepConditionComposite>,
    x: Vec<Real>,
    initial_values: Vec<Real>,
    interpolation: RefCell<Option<CubicInterpolation>>,
}

impl Fdm1DimSolver {
    /// The solver rolling `op` over the grid `solver_desc` describes, under the
    /// scheme `scheme_desc` names (`cpp:32-51`).
    ///
    /// Seeding happens here rather than on the first read, as it does in C++:
    /// the payoff at maturity is a property of the descriptor, not of the
    /// rollback.
    pub fn new(
        solver_desc: FdmSolverDesc,
        scheme_desc: FdmSchemeDesc,
        op: SharedMut<dyn FdmLinearOpComposite>,
    ) -> Self {
        let theta_condition = shared(FdmSnapshotCondition::new(theta_time(&solver_desc)));
        let conditions =
            FdmStepConditionComposite::join_conditions(&theta_condition, &solver_desc.condition);

        let layout = Shared::clone(solver_desc.mesher.layout());
        let mut x = vec![0.0; layout.size()];
        let mut initial_values = vec![0.0; layout.size()];
        for iter in layout.iter() {
            initial_values[iter.index()] = solver_desc
                .calculator
                .avg_inner_value(&iter, solver_desc.maturity);
            x[iter.index()] = solver_desc.mesher.location(&iter, 0);
        }

        Fdm1DimSolver {
            solver_desc,
            scheme_desc,
            op,
            theta_condition,
            conditions,
            x,
            initial_values,
            interpolation: RefCell::new(None),
        }
    }

    /// The rolled-back value at `x` (`cpp:67-70`).
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback or the spline fails.
    pub fn interpolate_at(&self, x: Real) -> QlResult<Real> {
        self.read_off(|spline| spline.value(x))
    }

    /// The first derivative of the rolled-back grid at `x` (`cpp:88-91`).
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback or the spline fails.
    pub fn derivative_x(&self, x: Real) -> QlResult<Real> {
        self.read_off(|spline| spline.derivative(x))
    }

    /// The second derivative of the rolled-back grid at `x` (`cpp:93-96`).
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback or the spline fails.
    pub fn derivative_xx(&self, x: Real) -> QlResult<Real> {
        self.read_off(|spline| spline.second_derivative(x))
    }

    /// The theta at `x`, from the grid captured a fraction of a day before
    /// today (`cpp:72-85`).
    ///
    /// [`None`] is C++'s `Null<Real>` (`cpp:73-74`), returned when the first
    /// stopping time is zero. That is a division guard rather than a special
    /// case: a stopping time at zero drives [`theta_time`] to zero through the
    /// `min`, which would leave the capture on today itself and the difference
    /// quotient dividing by nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback or either spline fails. Included in
    /// that is an unfired capture, which C++ instead reads as a grid of zeros
    /// (`cpp:77-80` copies an empty array into a sized one). No route the
    /// backward solver offers reaches it: the capture time is carried by the
    /// joined stopping times and sits strictly inside `(0, maturity)`, and
    /// whichever arm that solver takes, its last segment ends on today - so
    /// the model rolling that segment cuts a sub-step on the capture and the
    /// condition fires there.
    pub fn theta_at(&self, x: Real) -> QlResult<Option<Real>> {
        if self.conditions.stopping_times().first() == Some(&0.0) {
            return Ok(None);
        }

        let value = self.interpolate_at(x)?;
        let captured = self.theta_condition.values();
        let capture_spline = MonotonicCubicNaturalSpline::new(self.x.clone(), captured.to_vec())?;

        Ok(Some(
            (capture_spline.value(x)? - value) / self.theta_condition.time(),
        ))
    }

    /// Rolls the seeded grid back and splines the result, once (`cpp:54-65`).
    fn calculate(&self) -> QlResult<()> {
        let built = self.interpolation.borrow().is_some();
        if built {
            return Ok(());
        }

        let mut rhs = Array::from(self.initial_values.clone());
        FdmBackwardSolver::new(
            self.op.clone(),
            self.solver_desc.bc_set.clone(),
            Some(Shared::clone(&self.conditions)),
            self.scheme_desc,
        )
        .rollback(
            &mut rhs,
            self.solver_desc.maturity,
            0.0,
            self.solver_desc.time_steps,
            self.solver_desc.damping_steps,
        )?;

        let spline = MonotonicCubicNaturalSpline::new(self.x.clone(), rhs.to_vec())?;
        *self.interpolation.borrow_mut() = Some(spline);

        Ok(())
    }

    fn read_off<T>(&self, read: impl FnOnce(&CubicInterpolation) -> QlResult<T>) -> QlResult<T> {
        self.calculate()?;

        let cached = self.interpolation.borrow();
        let spline = cached
            .as_ref()
            .expect("calculate leaves the interpolation built");

        read(spline)
    }
}

/// The time the theta capture fires at (`cpp:36-40`): a fraction of a day
/// before today, or of the first stopping time when one falls inside that day.
fn theta_time(solver_desc: &FdmSolverDesc) -> Time {
    let first_stopping_time = solver_desc
        .condition
        .stopping_times()
        .first()
        .copied()
        .unwrap_or(solver_desc.maturity);

    CAPTURE_FRACTION * ONE_DAY.min(first_stopping_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::instruments::CashOrNothingPayoff;
    use crate::methods::finitedifferences::meshers::FdmMesher;
    use crate::methods::finitedifferences::operators::FdmLinearOp;
    use crate::methods::finitedifferences::schemes::testops;
    use crate::methods::finitedifferences::utilities::{
        FdmInnerValueCalculator, fdm_log_inner_value,
    };
    use crate::option::OptionType::Put;
    use crate::payoff::Payoff;
    use crate::shared::shared_mut;
    use crate::types::Size;

    const COEFFICIENT: Real = 0.4;
    const MATURITY: Time = 0.75;
    const STEPS: Size = 10;
    const DAMPING_STEPS: Size = 2;
    const STRIKE: Real = 100.0;
    const CASH: Real = 10.0;
    const CAPTURE_TIME: Time = 0.99 / 365.0;
    const PROBE: Real = 4.5;
    const INTERIOR: Size = 2;

    /// A diagonal composite that counts the steps taken over it, so a second
    /// read shows whether it drove a second rollback.
    struct CountingComposite {
        set_times: Shared<RefCell<Size>>,
    }

    impl FdmLinearOp for CountingComposite {
        fn apply(&self, r: &Array) -> Array {
            COEFFICIENT * r
        }
    }

    impl FdmLinearOpComposite for CountingComposite {
        fn size(&self) -> Size {
            1
        }

        fn set_time(&mut self, _t1: Time, _t2: Time) -> QlResult<()> {
            *self.set_times.borrow_mut() += 1;
            Ok(())
        }

        fn apply_mixed(&self, r: &Array) -> Array {
            Array::with_size(r.size())
        }

        fn apply_direction(&self, _direction: Size, r: &Array) -> Array {
            COEFFICIENT * r
        }

        fn solve_splitting(&self, _direction: Size, r: &Array, s: Real) -> QlResult<Array> {
            Ok(r / (1.0 + s * COEFFICIENT))
        }

        fn preconditioner(&self, r: &Array, s: Real) -> QlResult<Array> {
            self.solve_splitting(0, r, s)
        }
    }

    /// A digital put whose strike falls inside the grid, so the seed carries a
    /// jump rather than a constant a rollback could not move visibly.
    fn calculator(mesher: &Shared<dyn FdmMesher>) -> Shared<dyn FdmInnerValueCalculator> {
        let payoff = shared(CashOrNothingPayoff::new(Put, STRIKE, CASH)) as Shared<dyn Payoff>;

        shared(fdm_log_inner_value(payoff, Shared::clone(mesher), 0))
    }

    fn empty_condition() -> Shared<FdmStepConditionComposite> {
        shared(FdmStepConditionComposite::new(&[], Vec::new()))
    }

    fn desc(
        mesher: &Shared<dyn FdmMesher>,
        condition: Shared<FdmStepConditionComposite>,
        maturity: Time,
    ) -> FdmSolverDesc {
        FdmSolverDesc {
            mesher: Shared::clone(mesher),
            bc_set: Vec::new(),
            condition,
            calculator: calculator(mesher),
            maturity,
            time_steps: STEPS,
            damping_steps: DAMPING_STEPS,
        }
    }

    fn solver(
        mesher: &Shared<dyn FdmMesher>,
        condition: Shared<FdmStepConditionComposite>,
    ) -> Fdm1DimSolver {
        Fdm1DimSolver::new(
            desc(mesher, condition, MATURITY),
            FdmSchemeDesc::douglas(),
            testops::scaled_composite(&[COEFFICIENT]),
        )
    }

    fn capture_time(condition: Shared<FdmStepConditionComposite>, maturity: Time) -> Time {
        let mesher = testops::mesher();
        let solver = Fdm1DimSolver::new(
            desc(&mesher, condition, maturity),
            FdmSchemeDesc::douglas(),
            testops::scaled_composite(&[COEFFICIENT]),
        );

        solver.theta_condition.time()
    }

    /// The same roll driven by hand: seed, join a capture at the literal
    /// `0.99 / 365`, roll from maturity to today, spline. Every number the
    /// solver answers has to come off this, which is what separates a spline
    /// over the rolled grid from one over the seed, and a capture time or a
    /// step count read out of the wrong slot from the right one.
    fn rolled_by_hand(
        mesher: &Shared<dyn FdmMesher>,
        condition: &Shared<FdmStepConditionComposite>,
    ) -> CubicInterpolation {
        let calculator = calculator(mesher);
        let layout = Shared::clone(mesher.layout());
        let mut x = vec![0.0; layout.size()];
        let mut rhs = Array::with_size(layout.size());
        for iter in layout.iter() {
            rhs[iter.index()] = calculator.avg_inner_value(&iter, MATURITY);
            x[iter.index()] = mesher.location(&iter, 0);
        }

        let snapshot = shared(FdmSnapshotCondition::new(CAPTURE_TIME));
        let joined = FdmStepConditionComposite::join_conditions(&snapshot, condition);
        FdmBackwardSolver::new(
            testops::scaled_composite(&[COEFFICIENT]),
            Vec::new(),
            Some(joined),
            FdmSchemeDesc::douglas(),
        )
        .rollback(&mut rhs, MATURITY, 0.0, STEPS, DAMPING_STEPS)
        .unwrap();

        MonotonicCubicNaturalSpline::new(x, rhs.to_vec()).unwrap()
    }

    /// `cpp:45-50`: the seed is the cell-averaged payoff at maturity, indexed
    /// the way the layout indexes its points.
    #[test]
    fn the_grid_is_seeded_with_the_cell_averaged_payoff_at_maturity() {
        let mesher = testops::mesher();
        let solver = solver(&mesher, empty_condition());

        let calculator = calculator(&mesher);
        let expected: Vec<Real> = mesher
            .layout()
            .iter()
            .map(|iter| calculator.avg_inner_value(&iter, MATURITY))
            .collect();

        assert_eq!(solver.initial_values, expected);
    }

    /// `cpp:49` reads direction zero point by point; the mesher answers the
    /// same coordinates in one call, so the two paths cross-check.
    #[test]
    fn the_grid_coordinates_are_the_mesher_locations() {
        let mesher = testops::mesher();
        let solver = solver(&mesher, empty_condition());

        assert_eq!(solver.x, mesher.locations(0).to_vec());
    }

    /// `cpp:54-65` and `cpp:67-96`: value and both derivatives come off a
    /// spline through the rolled-back grid.
    #[test]
    fn the_read_offs_come_from_the_rolled_back_grid() {
        let mesher = testops::mesher();
        let condition = empty_condition();
        let solver = solver(&mesher, Shared::clone(&condition));
        let expected = rolled_by_hand(&mesher, &condition);

        assert_eq!(
            solver.interpolate_at(PROBE).unwrap(),
            expected.value(PROBE).unwrap()
        );
        assert_eq!(
            solver.derivative_x(PROBE).unwrap(),
            expected.derivative(PROBE).unwrap()
        );
        assert_eq!(
            solver.derivative_xx(PROBE).unwrap(),
            expected.second_derivative(PROBE).unwrap()
        );
    }

    /// The rollback moves the grid far enough that splining the seed instead
    /// would be visible: the digital's seed at the interior node is the full
    /// cash payment and the rolled value is not.
    #[test]
    fn the_rolled_grid_is_not_the_seed() {
        let mesher = testops::mesher();
        let solver = solver(&mesher, empty_condition());

        let rolled = solver.interpolate_at(solver.x[INTERIOR]).unwrap();

        assert!(
            (rolled - solver.initial_values[INTERIOR]).abs() > 1e-3,
            "the rollback left the interior node at its seed {}: {rolled}",
            solver.initial_values[INTERIOR]
        );
    }

    /// `cpp:36-40`: with nothing stopping inside the day before today, the
    /// capture sits a hundredth short of that whole day.
    #[test]
    fn the_capture_sits_just_inside_the_day_before_today() {
        assert_eq!(capture_time(empty_condition(), MATURITY), CAPTURE_TIME);
    }

    /// `cpp:38-39`: an empty stopping-time list falls back on the maturity,
    /// and a maturity inside that day pulls the capture in with it.
    #[test]
    fn a_maturity_inside_that_day_pulls_the_capture_in() {
        assert_eq!(capture_time(empty_condition(), 0.001), 0.99 * 0.001);
    }

    /// `cpp:40`: the first stopping time wins over the maturity, and pulls the
    /// capture in when it too falls inside the day.
    #[test]
    fn the_first_stopping_time_pulls_the_capture_in() {
        let condition = shared(FdmStepConditionComposite::new(
            &[vec![0.5, 0.002]],
            Vec::new(),
        ));

        assert_eq!(capture_time(condition, MATURITY), 0.99 * 0.002);
    }

    /// `cpp:72-85`: the theta is the captured grid less the result, over the
    /// time between them, and the capture fires because the joined conditions
    /// put its time on the rollback's grid.
    #[test]
    fn the_theta_is_the_captured_grid_less_the_result_over_the_capture_time() {
        let mesher = testops::mesher();
        let solver = solver(&mesher, empty_condition());

        let theta = solver
            .theta_at(PROBE)
            .unwrap()
            .expect("a capture away from today has a theta");

        let captured = solver.theta_condition.values();
        assert!(
            !captured.is_empty(),
            "the capture did not fire through the joined conditions"
        );
        let capture_spline =
            MonotonicCubicNaturalSpline::new(solver.x.clone(), captured.to_vec()).unwrap();
        let expected = (capture_spline.value(PROBE).unwrap()
            - solver.interpolate_at(PROBE).unwrap())
            / CAPTURE_TIME;

        assert_eq!(theta, expected);
    }

    /// `cpp:73-74`: a stopping time on today drives the capture time to zero,
    /// so there is no interval to difference over and no theta.
    #[test]
    fn a_stopping_time_on_today_has_no_theta() {
        let mesher = testops::mesher();
        let condition = shared(FdmStepConditionComposite::new(
            &[vec![0.0, 0.5]],
            Vec::new(),
        ));
        let solver = solver(&mesher, condition);

        assert_eq!(solver.theta_condition.time(), 0.0);
        assert_eq!(solver.theta_at(PROBE).unwrap(), None);
    }

    /// The cache is what C++ gets from `LazyObject::calculate` (`cpp:68`): the
    /// reads after the first roll no further steps.
    #[test]
    fn the_rollback_runs_once_however_many_reads_follow() {
        let mesher = testops::mesher();
        let set_times = shared(RefCell::new(0));
        let op = shared_mut(CountingComposite {
            set_times: Shared::clone(&set_times),
        });
        let solver = Fdm1DimSolver::new(
            desc(&mesher, empty_condition(), MATURITY),
            FdmSchemeDesc::douglas(),
            op,
        );

        solver.interpolate_at(PROBE).unwrap();
        let after_first_read = *set_times.borrow();

        solver.derivative_x(PROBE).unwrap();
        solver.derivative_xx(PROBE).unwrap();
        solver.theta_at(PROBE).unwrap();

        assert!(after_first_read >= STEPS + DAMPING_STEPS);
        assert_eq!(*set_times.borrow(), after_first_read);
    }
}
