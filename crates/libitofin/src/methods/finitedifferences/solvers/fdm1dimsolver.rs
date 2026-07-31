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
