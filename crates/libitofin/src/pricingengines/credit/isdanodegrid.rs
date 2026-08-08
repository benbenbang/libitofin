//! The ISDA integration grid: the union of the curves' own node dates.
//!
//! Port of the node-collection prologue of `IsdaCdsEngine::calculate`
//! (`ql/pricingengines/credit/isdacdsengine.cpp:104-157`). The ISDA model
//! integrates the protection and premium legs over the pillar dates of the
//! discount and credit curves themselves, so the engine must recover each
//! curve's concrete type from the abstract handle it was given.
//!
//! ## The downcast seam
//!
//! C++ does this with `dynamic_pointer_cast` down a class hierarchy. `Rc`
//! carries no such projection, so the port routes it through
//! [`YieldTermStructure::as_any`] /
//! [`DefaultProbabilityTermStructure::as_any`]: a curve that can expose its
//! nodes returns `Some(self)`, every other curve keeps the `None` default and
//! lands in the `QL_FAIL` arm.
//!
//! One C++ cast has no Rust counterpart and so is spelled out twice here. A
//! `PiecewiseYieldCurve<Discount, LogLinear>` *is* an
//! `InterpolatedDiscountCurve<LogLinear>` in C++ (the traits pick the base
//! class), so the engine's single cast catches both the hand-built curve and
//! the bootstrapped one. This port composes rather than inherits - the
//! piecewise curve holds its own `CurveData` and implements the term-structure
//! traits itself - so each bootstrapped curve needs its own arm. Omitting them
//! would reject exactly the curves the ISDA engine exists to price.
//!
//! ## Faithful rejections
//!
//! The arms are a one-to-one transcription of the C++ cascade, so the curve
//! shapes it refuses are refused here too, by construction rather than by
//! oversight: `InterpolatedDiscountCurve<Linear>` is a different C++ type from
//! `InterpolatedDiscountCurve<LogLinear>` and matches no arm, and there is no
//! `InterpolatedZeroCurve` arm at all, so a `PiecewiseYieldCurve<ZeroYield, _>`
//! is an error on both sides.
//!
//! ## Deferred (#799)
//!
//! `InterpolatedForwardCurve<ForwardFlat>` (`isdacdsengine.cpp:120-124`) and
//! `InterpolatedSurvivalProbabilityCurve<LogLinear>` (`:130-135`) are the two
//! ISDA curve variants absent from this port: there is no `ForwardFlat`
//! interpolator factory and no survival-probability curve yet. Their arms are
//! simply missing, so such a curve reports the same error as any other
//! unsupported shape.

use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::math::interpolations::flat::BackwardFlat;
use crate::math::interpolations::loglinear::LogLinear;
use crate::termstructures::bootstraptraits::{Discount, ForwardRate};
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::flathazardrate::FlatHazardRate;
use crate::termstructures::credit::interpolatedhazardratecurve::InterpolatedHazardRateCurve;
use crate::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve;
use crate::termstructures::credit::probabilitytraits::HazardRate;
use crate::termstructures::yields::{
    FlatForward, InterpolatedDiscountCurve, InterpolatedForwardCurve, PiecewiseYieldCurve,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;

/// The dates the ISDA engine integrates over: the sorted union of the discount
/// and credit curves' node dates, or `maturity` alone when both curves are flat
/// (`isdacdsengine.cpp:149-156`).
///
/// Both curves are read once up front (`:110-111`). C++ forces the bootstrap
/// there because its `dates()` resolves to the interpolated base class and so
/// would return an un-bootstrapped node set; this port's piecewise `dates()`
/// runs the bootstrap itself, but the reads are kept because they are also what
/// surfaces a bootstrap failure as an error rather than as an empty grid.
///
/// The merge is `sort` + `dedup` rather than a two-range `set_union`
/// (`:150-151`). The two agree because every node list is strictly increasing
/// and duplicate-free - each curve constructor requires it - so the only
/// duplicates `dedup` can see are dates shared between the two curves, which is
/// exactly what `set_union` collapses.
pub fn isda_node_grid(
    rate: &Handle<dyn YieldTermStructure>,
    credit: &Handle<dyn DefaultProbabilityTermStructure>,
    maturity: Date,
) -> QlResult<Vec<Date>> {
    let rate_curve = rate.current_link()?;
    let credit_curve = credit.current_link()?;

    rate_curve.discount(0.0, false)?;
    credit_curve.default_probability(0.0, false)?;

    let mut nodes = yield_curve_dates(&*rate_curve)?;
    nodes.extend(credit_curve_dates(&*credit_curve)?);
    nodes.sort_unstable();
    nodes.dedup();

    if nodes.is_empty() {
        nodes.push(maturity);
    }
    Ok(nodes)
}

/// The discount curve's node dates (`isdacdsengine.cpp:112-130`): its pillars
/// if it is one of the supported interpolated shapes, none if it is flat, an
/// error otherwise.
fn yield_curve_dates(curve: &dyn YieldTermStructure) -> QlResult<Vec<Date>> {
    const UNSUPPORTED: &str = "Yield curve must be flat forward interpolated";

    let Some(any) = curve.as_any() else {
        fail!("{UNSUPPORTED}");
    };
    if let Some(curve) = any.downcast_ref::<InterpolatedDiscountCurve<LogLinear>>() {
        return Ok(curve.dates().to_vec());
    }
    if let Some(curve) = any.downcast_ref::<PiecewiseYieldCurve<Discount, LogLinear>>() {
        return curve.dates();
    }
    if let Some(curve) = any.downcast_ref::<InterpolatedForwardCurve<BackwardFlat>>() {
        return Ok(curve.dates().to_vec());
    }
    if let Some(curve) = any.downcast_ref::<PiecewiseYieldCurve<ForwardRate, BackwardFlat>>() {
        return curve.dates();
    }
    if any.is::<FlatForward>() {
        return Ok(Vec::new());
    }
    fail!("{UNSUPPORTED}")
}

/// The credit curve's node dates (`isdacdsengine.cpp:131-148`), on the same
/// terms as [`yield_curve_dates`].
fn credit_curve_dates(curve: &dyn DefaultProbabilityTermStructure) -> QlResult<Vec<Date>> {
    const UNSUPPORTED: &str = "Credit curve must be flat forward interpolated";

    let Some(any) = curve.as_any() else {
        fail!("{UNSUPPORTED}");
    };
    if let Some(curve) = any.downcast_ref::<InterpolatedHazardRateCurve<BackwardFlat>>() {
        return Ok(curve.dates().to_vec());
    }
    if let Some(curve) = any.downcast_ref::<PiecewiseDefaultCurve<HazardRate, BackwardFlat>>() {
        return curve.dates();
    }
    if any.is::<FlatHazardRate>() {
        return Ok(Vec::new());
    }
    fail!("{UNSUPPORTED}")
}
