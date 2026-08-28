//! Localised piecewise-curve bootstrap.
//!
//! Port of `ql/termstructures/localbootstrap.hpp`. Where [`IterativeBootstrap`]
//! solves one node at a time against the curve built so far, `LocalBootstrap`
//! grows the curve one node per step and least-squares-fits a *window* of the
//! trailing `localisation` nodes to the trailing `localisation` helpers at each
//! step, so a non-local interpolation method keeps a localised IR risk profile.
//! It only works with an interpolator that can approximate the still-unsolved
//! span - the [`LocalInterpolator`] bound, which in practice means the
//! convex-monotone spline.
//!
//! [`IterativeBootstrap`]: crate::termstructures::iterativebootstrap::IterativeBootstrap
//!
//! ## What is ported, and what is not
//!
//! - The driver transcribes `LocalBootstrap::calculate`
//!   (`localbootstrap.hpp:133-258`): sort helpers, reject duplicate pillars and
//!   invalid quotes, size the node vectors `n_insts + 1` up front (there is
//!   **no** first-alive expiry scan - every instrument participates), then the
//!   grow loop with a Levenberg-Marquardt solve per step.
//! - The `setup()` guards (`:119-126`) are checked at the start of `calculate`
//!   as explicit `Err`s (D4): the Rust [`Bootstrap`] trait has no setup hook.
//!   Its other half, registering the curve as an observer of every instrument
//!   (`:128-130`), is performed - for any bootstrap - by the curve constructor
//!   (`PiecewiseYieldCurve::with_bootstrap`), which registers all instruments
//!   unconditionally, so the D1 observability contract holds without a
//!   bootstrap-side hook.
//! - `PenaltyFunction` (`:41-66`, `:262-306`) is **not** ported: deprecated
//!   upstream since 1.40 and unused - `calculate` builds its cost from a plain
//!   closure (`SimpleCostFunction`, `:234-246`), transcribed here as
//!   [`LocalCost`].
//! - The `validCurve_` warm-restart field is **not** ported: `calculate` sets
//!   it `false` as its first statement (`:136`), so the `if (validCurve_)`
//!   reuse branch (`:167-170`) is dead and every pass resets the nodes to the
//!   curve's initial value.
//!
//! ## The per-evaluation rebuild and the step-entry `prev`
//!
//! C++ mutates `ts_->data_` in place and calls `interpolation_.update()` on
//! every cost evaluation; the interpolation object keeps the sections it
//! copied from the *previous step's* interpolation frozen and recomputes only
//! the trailing window. The Rust [`ConvexMonotoneInterpolation`] has no
//! in-place update, so every evaluation rebuilds through
//! [`LocalInterpolator::local_interpolate`] - and it must pass the interpolation
//! as it stood at the **start of the step** (`step_prev`) as `prev`, never the
//! previous evaluation's output: the frozen-section seam is derived from
//! `prev`, and feeding each evaluation's output back in would grow the seam
//! once per solver iteration and silently diverge from the C++ fit.
//!
//! [`ConvexMonotoneInterpolation`]: crate::math::interpolations::convexmonotone::ConvexMonotoneInterpolation
//!
//! Because the driver, not the curve, must hold that step-entry interpolation
//! while the solver replaces the curve's own via
//! [`CurveData::set_interpolation`], the end of each step builds the final
//! interpolation twice from identical inputs - one copy installed on the
//! curve, one kept as the next step's `prev`. `local_interpolate` is
//! deterministic, so the two are bit-identical.
//!
//! ## Divergence: sections are recomputed, not shared
//!
//! Where C++ *shares* the frozen section objects - whose coefficients embed
//! node values as they stood when the section was frozen, values a later
//! window may since have revised - the Rust seam recomputes every section from
//! the final node values, carrying only the seam boundary forward (see
//! `convexmonotone.rs`). Pillar discounts agree regardless: the convex-monotone
//! method conserves each section's integral (`primitive(pillar)` is a sum of
//! `y[j] * dt_j` over nodes, all final at that pillar), so only intra-interval
//! sampling can differ, and the `testLocalBootstrapConsistency` oracle at
//! 1e-6 adjudicates that residual.

use std::cell::RefCell;

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::interpolations::{Interpolator, LocalInterpolator};
use crate::math::optimization::constraint::{Constraint, NoConstraint, PositiveConstraint};
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::EndCriteria;
use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{BootstrapHelperShared, sort_by_pillar_date};
use crate::termstructures::bootstraptraits::BootstrapTraits;
use crate::termstructures::iterativebootstrap::{Bootstrap, PiecewiseCurve};
use crate::types::{Real, Size};

/// The localised bootstrap (`LocalBootstrap`).
///
/// Carries the window size, the positivity switch and the stopping-accuracy
/// override; defaults mirror the C++ constructor
/// (`localisation = 2`, `forcePositive = true`, accuracy from the curve).
#[derive(Clone, Copy, Debug)]
pub struct LocalBootstrap {
    localisation: Size,
    force_positive: bool,
    accuracy: Option<Real>,
}

impl LocalBootstrap {
    /// A localised bootstrap fitting `localisation` nodes per step, optionally
    /// constraining them positive, with an optional accuracy override.
    pub fn new(localisation: Size, force_positive: bool, accuracy: Option<Real>) -> LocalBootstrap {
        LocalBootstrap {
            localisation,
            force_positive,
            accuracy,
        }
    }
}

impl Default for LocalBootstrap {
    /// The C++ defaults: `localisation = 2`, `forcePositive = true`, accuracy
    /// taken from the curve.
    fn default() -> LocalBootstrap {
        LocalBootstrap::new(2, true, None)
    }
}

/// The per-step cost (`SimpleCostFunction`, `localbootstrap.hpp:234-246`):
/// writes the trial window into the node vector, rebuilds the interpolation
/// from the step-entry `prev`, and returns the window helpers' quote errors as
/// the residual vector.
///
/// The [`CostFunction`] trait is infallible, so a failed rebuild or reprice
/// parks its error in `error` and returns NaN residuals, which the solver
/// adapter treats as an infeasible penalty; the driver surfaces the parked
/// error after the solve (D4), matching the C++ exception propagating out of
/// the cost closure.
struct LocalCost<'a, C: PiecewiseCurve>
where
    C::Interp: LocalInterpolator,
{
    curve: &'a C,
    /// The trailing `localisation` helpers of this step's window,
    /// `[helpers_end - localisation, helpers_end)` with
    /// `helpers_end = i_inst + 1`.
    window: &'a [Shared<C::Helper>],
    /// The first node index the trial vector writes
    /// (`iInst + 1 - localisation + dataAdjust`, `:206`).
    initial_data_pt: Size,
    i_inst: Size,
    localisation: Size,
    n_insts: Size,
    /// The interpolation as at the START of this step - the frozen-prefix
    /// source for every evaluation's rebuild. Never the previous evaluation's
    /// output; see the module docs.
    step_prev: Option<&'a <C::Interp as Interpolator>::Output>,
    error: RefCell<Option<QlError>>,
}

impl<C: PiecewiseCurve> LocalCost<'_, C>
where
    C::Interp: LocalInterpolator,
{
    fn try_values(&self, x: &Array) -> QlResult<Array> {
        {
            let mut cd = self.curve.curve_data().borrow_mut();
            for k in 0..x.size() {
                C::Traits::update_guess(cd.data_mut(), x[k], self.initial_data_pt + k);
            }
            let interpolation = self.curve.interpolator().local_interpolate(
                &cd.times()[..self.i_inst + 2],
                &cd.data()[..self.i_inst + 2],
                self.localisation,
                self.step_prev,
                self.n_insts + 1,
            )?;
            cd.set_interpolation(interpolation);
        }
        let mut penalties = Array::with_size(self.localisation);
        for (k, helper) in self.window.iter().enumerate() {
            penalties[k] = helper.quote_error()?;
        }
        Ok(penalties)
    }
}

impl<C: PiecewiseCurve> CostFunction for LocalCost<'_, C>
where
    C::Interp: LocalInterpolator,
{
    fn values(&self, x: &Array) -> Array {
        match self.try_values(x) {
            Ok(values) => values,
            Err(err) => {
                let mut slot = self.error.borrow_mut();
                if slot.is_none() {
                    *slot = Some(err);
                }
                std::iter::repeat_n(Real::NAN, self.localisation).collect()
            }
        }
    }
}

impl<C: PiecewiseCurve> Bootstrap<C> for LocalBootstrap
where
    C::Interp: LocalInterpolator,
{
    fn calculate(&self, curve: &C) -> QlResult<()> {
        let mut helpers: Vec<Shared<C::Helper>> = curve.instruments().to_vec();
        let n_insts = helpers.len();

        // The C++ setup() guards (`:119-126`), surfaced here because the
        // trait has no setup hook.
        let required = curve.interpolator().required_points();
        require!(
            n_insts >= required,
            "not enough instruments: {n_insts} provided, {required} required"
        );
        require!(
            n_insts > self.localisation,
            "not enough instruments: {n_insts} provided, {} required.",
            self.localisation
        );

        sort_by_pillar_date(&mut helpers);

        for i in 1..n_insts {
            let m1 = helpers[i - 1].pillar_date();
            let m2 = helpers[i].pillar_date();
            require!(m1 != m2, "two instruments have the same pillar date ({m1})");
        }

        for (i, helper) in helpers.iter().enumerate() {
            if helper.quote_value().is_err() {
                crate::fail!(
                    "instrument {} (maturity: {}, pillar: {}) has an invalid quote",
                    i + 1,
                    helper.maturity_date(),
                    helper.pillar_date()
                );
            }
        }

        let term_structure = curve.term_structure_shared()?;
        for helper in &helpers {
            helper.set_term_structure(&term_structure);
        }

        // Lay out ALL n_insts + 1 nodes up front (`:171-186`); the reuse
        // branch is dead (see the module docs), so every node is seeded with
        // the curve's initial value.
        let first_date = curve.initial_date()?;
        let initial_value = curve.initial_value()?;
        let mut dates = Vec::with_capacity(n_insts + 1);
        let mut times = Vec::with_capacity(n_insts + 1);
        dates.push(first_date);
        times.push(curve.time_from_reference(first_date)?);
        let mut max_date = first_date;
        for helper in &helpers {
            let pillar = helper.pillar_date();
            dates.push(pillar);
            times.push(curve.time_from_reference(pillar)?);
            max_date = max_date.max(pillar.max(helper.latest_relevant_date()));
        }
        {
            let mut cd = curve.curve_data().borrow_mut();
            cd.set_pillars(dates, times);
            cd.reset_data(initial_value, n_insts + 1);
            cd.set_max_date(max_date);
        }

        // Solver configuration (`:188-198`). The EndCriteria zeros are the
        // C++ literals: only the function accuracy is active.
        let accuracy = self.accuracy.unwrap_or_else(|| curve.accuracy());
        let mut solver = LevenbergMarquardt::new(accuracy, accuracy, accuracy, false);
        let end_criteria = EndCriteria::new(100, Some(10), 0.0, accuracy, Some(0.0))?;
        let positive = PositiveConstraint;
        let unconstrained = NoConstraint;
        let constraint: &dyn Constraint = if self.force_positive {
            &positive
        } else {
            &unconstrained
        };

        let data_adjust = <C::Interp as LocalInterpolator>::DATA_SIZE_ADJUSTMENT;
        let mut step_prev: Option<<C::Interp as Interpolator>::Output> = None;
        let mut i_inst = self.localisation - 1;

        // The grow loop (`:200-256`): each step extends the curve to node
        // i_inst + 1 and least-squares-fits the trailing window.
        loop {
            let initial_data_pt = i_inst + 1 - self.localisation + data_adjust;
            let mut start_array = Array::with_size(self.localisation + 1 - data_adjust);
            {
                let cd = curve.curve_data().borrow();
                for j in 0..start_array.size() - 1 {
                    start_array[j] = cd.data()[initial_data_pt + j];
                }

                // The step-entry interpolation over the extended node range,
                // grown from the previous step's (`:218-225`).
                let entry = curve.interpolator().local_interpolate(
                    &cd.times()[..i_inst + 2],
                    &cd.data()[..i_inst + 2],
                    self.localisation,
                    step_prev.as_ref(),
                    n_insts + 1,
                )?;

                // The solver start point's final element (`:227-231`; the
                // C++ carries a literal `// ?` doubting the `iInst` index,
                // ported as-is).
                start_array[self.localisation - data_adjust] = if i_inst >= self.localisation {
                    C::Traits::guess(i_inst, cd.times(), cd.data(), false)
                } else {
                    cd.data()[0]
                };
                drop(cd);
                curve.curve_data().borrow_mut().set_interpolation(entry);
            }

            let window = &helpers[i_inst + 1 - self.localisation..i_inst + 1];
            let cost = LocalCost::<C> {
                curve,
                window,
                initial_data_pt,
                i_inst,
                localisation: self.localisation,
                n_insts,
                step_prev: step_prev.as_ref(),
                error: RefCell::new(None),
            };

            let (end_type, solution) = {
                let mut problem = Problem::new(&cost, constraint, start_array);
                let outcome = solver.minimize(&mut problem, &end_criteria);
                (outcome, problem.current_value().clone())
            };
            if let Some(inner) = cost.error.into_inner() {
                return Err(inner);
            }
            let end_type = end_type?;
            require!(
                end_type.succeeded(),
                "Unable to strip yieldcurve to required accuracy: {end_type}"
            );

            // Pin the returned solution: rewrite the window and rebuild, so
            // the curve holds the solver's answer rather than its last trial
            // point. The second, bit-identical build becomes the next step's
            // frozen-prefix `prev` (see the module docs).
            {
                let mut cd = curve.curve_data().borrow_mut();
                for k in 0..solution.size() {
                    C::Traits::update_guess(cd.data_mut(), solution[k], initial_data_pt + k);
                }
                let for_curve = curve.interpolator().local_interpolate(
                    &cd.times()[..i_inst + 2],
                    &cd.data()[..i_inst + 2],
                    self.localisation,
                    step_prev.as_ref(),
                    n_insts + 1,
                )?;
                let next_prev = curve.interpolator().local_interpolate(
                    &cd.times()[..i_inst + 2],
                    &cd.data()[..i_inst + 2],
                    self.localisation,
                    step_prev.as_ref(),
                    n_insts + 1,
                )?;
                cd.set_interpolation(for_curve);
                step_prev = Some(next_prev);
            }

            i_inst += 1;
            if i_inst >= n_insts {
                break;
            }
        }
        Ok(())
    }
}
