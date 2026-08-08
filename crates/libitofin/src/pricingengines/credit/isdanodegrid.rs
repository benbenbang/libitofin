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

#[cfg(test)]
mod tests {
    //! Oracle: the node-collection prologue of `IsdaCdsEngine::calculate`
    //! (`isdacdsengine.cpp:104-157`). There are no numbers to transcribe - the
    //! block is pure curve introspection - so the assertions are on which
    //! curves are accepted and on the exact grid each pair yields.

    use super::*;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, shared};
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::{DepositRateHelper, FlatForward};
    use crate::termstructures::{
        RateHelper, TermStructure, TermStructureBase, yields::DiscountCurve,
    };
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Month, SerialNumber};
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{DiscountFactor, Time};

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn day_counter() -> DayCounter {
        Actual360::new()
    }

    /// Yield pillars offset from the reference date, in days. Deliberately
    /// interleaved with [`CREDIT_OFFSETS`] and sharing two dates with it, so a
    /// grid built by concatenating without sorting, or by failing to collapse
    /// the shared dates, is caught on both length and order.
    const YIELD_OFFSETS: [SerialNumber; 4] = [0, 90, 270, 540];
    const CREDIT_OFFSETS: [SerialNumber; 4] = [0, 180, 270, 720];

    fn dates_at(offsets: &[SerialNumber]) -> Vec<Date> {
        offsets.iter().map(|days| reference() + *days).collect()
    }

    /// A log-linear discount curve over [`YIELD_OFFSETS`] (C++ arm `castY1`).
    fn discount_curve() -> Shared<DiscountCurve> {
        let dates = dates_at(&YIELD_OFFSETS);
        let discounts = vec![1.0, 0.995, 0.985, 0.97];
        shared(
            DiscountCurve::new(dates, discounts, day_counter(), None)
                .expect("the discount nodes are well formed"),
        )
    }

    /// A backward-flat hazard-rate curve over [`CREDIT_OFFSETS`] (arm `castC2`).
    fn hazard_rate_curve() -> Shared<InterpolatedHazardRateCurve<BackwardFlat>> {
        let dates = dates_at(&CREDIT_OFFSETS);
        let rates = vec![0.01, 0.012, 0.014, 0.016];
        shared(
            InterpolatedHazardRateCurve::new(dates, rates, day_counter(), BackwardFlat)
                .expect("the hazard-rate nodes are well formed"),
        )
    }

    fn flat_forward() -> Shared<FlatForward> {
        shared(FlatForward::with_rate(
            reference(),
            0.02,
            day_counter(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }

    fn flat_hazard_rate() -> Shared<FlatHazardRate> {
        shared(FlatHazardRate::with_rate(reference(), 0.01, day_counter()))
    }

    fn yield_handle(
        curve: Shared<impl YieldTermStructure + 'static>,
    ) -> Handle<dyn YieldTermStructure> {
        Handle::new(curve as Shared<dyn YieldTermStructure>)
    }

    fn credit_handle(
        curve: Shared<impl DefaultProbabilityTermStructure + 'static>,
    ) -> Handle<dyn DefaultProbabilityTermStructure> {
        Handle::new(curve as Shared<dyn DefaultProbabilityTermStructure>)
    }

    /// A date beyond every pillar, so any test whose grid comes from the curves
    /// can assert the fallback maturity was *not* used.
    fn maturity() -> Date {
        reference() + 1000
    }

    #[test]
    fn discount_curve_is_downcastable_to_its_pillar_dates() {
        let curve = discount_curve();
        let any = YieldTermStructure::as_any(&*curve).expect("the curve opts into the seam");
        let recovered = any
            .downcast_ref::<InterpolatedDiscountCurve<LogLinear>>()
            .expect("the curve is log-linear interpolated");
        assert_eq!(recovered.dates(), dates_at(&YIELD_OFFSETS));
    }

    #[test]
    fn forward_curve_is_downcastable_to_its_pillar_dates() {
        let dates = dates_at(&YIELD_OFFSETS);
        let curve = InterpolatedForwardCurve::new(
            dates.clone(),
            vec![0.02, 0.021, 0.022, 0.023],
            day_counter(),
            BackwardFlat,
        )
        .expect("the forward nodes are well formed");
        let any = YieldTermStructure::as_any(&curve).expect("the curve opts into the seam");
        let recovered = any
            .downcast_ref::<InterpolatedForwardCurve<BackwardFlat>>()
            .expect("the curve is backward-flat interpolated");
        assert_eq!(recovered.dates(), dates);
    }

    #[test]
    fn hazard_rate_curve_is_downcastable_to_its_pillar_dates() {
        let curve = hazard_rate_curve();
        let any =
            DefaultProbabilityTermStructure::as_any(&*curve).expect("the curve opts into the seam");
        let recovered = any
            .downcast_ref::<InterpolatedHazardRateCurve<BackwardFlat>>()
            .expect("the curve is backward-flat interpolated");
        assert_eq!(recovered.dates(), dates_at(&CREDIT_OFFSETS));
    }

    /// `isdacdsengine.cpp:150-151`: the grid is the union of the two pillar
    /// sets, sorted, with the dates they share collapsed to one.
    #[test]
    fn grid_is_the_sorted_deduplicated_union_of_both_curves() {
        let grid = isda_node_grid(
            &yield_handle(discount_curve()),
            &credit_handle(hazard_rate_curve()),
            maturity(),
        )
        .expect("both curves are supported shapes");

        let expected = dates_at(&[0, 90, 180, 270, 540, 720]);
        assert_eq!(grid.len(), expected.len(), "the two shared dates collapse");
        assert_eq!(grid, expected);
        assert!(
            !grid.contains(&maturity()),
            "the maturity is a fallback, not a grid point"
        );
    }

    /// `isdacdsengine.cpp:125-127`: a flat yield curve has no dates to extract,
    /// so the grid is the credit curve's pillars alone.
    #[test]
    fn flat_yield_curve_contributes_no_dates() {
        let grid = isda_node_grid(
            &yield_handle(flat_forward()),
            &credit_handle(hazard_rate_curve()),
            maturity(),
        )
        .expect("a flat forward curve is supported");
        assert_eq!(grid, dates_at(&CREDIT_OFFSETS));
    }

    /// `isdacdsengine.cpp:142-145`: the credit-side twin.
    #[test]
    fn flat_credit_curve_contributes_no_dates() {
        let grid = isda_node_grid(
            &yield_handle(discount_curve()),
            &credit_handle(flat_hazard_rate()),
            maturity(),
        )
        .expect("a flat hazard rate is supported");
        assert_eq!(grid, dates_at(&YIELD_OFFSETS));
    }

    /// `isdacdsengine.cpp:154-156`: with nothing to integrate over, the grid
    /// falls back to the maturity alone.
    #[test]
    fn two_flat_curves_give_the_maturity_alone() {
        let grid = isda_node_grid(
            &yield_handle(flat_forward()),
            &credit_handle(flat_hazard_rate()),
            maturity(),
        )
        .expect("both flat curves are supported");
        assert_eq!(grid, vec![maturity()]);
    }

    /// `isdacdsengine.cpp:128-129`: the interpolator is part of the C++ type
    /// being cast to, so a linearly interpolated discount curve matches no arm.
    #[test]
    fn yield_curve_under_an_unsupported_interpolator_is_refused() {
        let curve = InterpolatedDiscountCurve::<Linear>::new(
            dates_at(&YIELD_OFFSETS),
            vec![1.0, 0.995, 0.985, 0.97],
            day_counter(),
            None,
        )
        .expect("the discount nodes are well formed");
        let error = isda_node_grid(
            &yield_handle(shared(curve)),
            &credit_handle(hazard_rate_curve()),
            maturity(),
        )
        .expect_err("a linear discount curve is not an ISDA curve");
        assert!(
            error
                .to_string()
                .contains("Yield curve must be flat forward")
        );
    }

    /// `isdacdsengine.cpp:146-147`: the credit-side twin.
    #[test]
    fn credit_curve_under_an_unsupported_interpolator_is_refused() {
        let curve = InterpolatedHazardRateCurve::<Linear>::new(
            dates_at(&CREDIT_OFFSETS),
            vec![0.01, 0.012, 0.014, 0.016],
            day_counter(),
            Linear,
        )
        .expect("the hazard-rate nodes are well formed");
        let error = isda_node_grid(
            &yield_handle(discount_curve()),
            &credit_handle(shared(curve)),
            maturity(),
        )
        .expect_err("a linear hazard-rate curve is not an ISDA curve");
        assert!(
            error
                .to_string()
                .contains("Credit curve must be flat forward")
        );
    }

    /// A curve that never opts into the seam at all (`as_any` left at `None`)
    /// is refused rather than silently read as flat.
    #[test]
    fn curve_outside_the_seam_is_refused() {
        let error = isda_node_grid(
            &yield_handle(shared(SpreadedCurve::new())),
            &credit_handle(hazard_rate_curve()),
            maturity(),
        )
        .expect_err("a curve outside the seam is not an ISDA curve");
        assert!(
            error
                .to_string()
                .contains("Yield curve must be flat forward")
        );
    }

    /// The bootstrapped discount curve the ISDA engine is actually handed.
    ///
    /// C++ reaches this through inheritance - a
    /// `PiecewiseYieldCurve<Discount, LogLinear>` *is* the
    /// `InterpolatedDiscountCurve<LogLinear>` of arm `castY1` - so without the
    /// piecewise arm this port would reject the main case outright.
    #[test]
    fn bootstrapped_discount_curve_contributes_its_solved_pillars() {
        let settings = shared(Settings::<Date>::new());
        let today = Target::new().adjust(reference(), BusinessDayConvention::Following);
        settings.set_evaluation_date(today);

        let tenors = [(3, TimeUnit::Months), (6, TimeUnit::Months)];
        let helpers: Vec<Shared<dyn RateHelper>> = tenors
            .iter()
            .map(|(n, units)| {
                let quote = Handle::new(shared(SimpleQuote::new(0.04)) as Shared<dyn Quote>);
                let index =
                    Euribor::new(Period::new(*n, *units), Handle::empty(), settings.clone())
                        .expect("the deposit tenor is valid");
                DepositRateHelper::new(quote, &index) as Shared<dyn RateHelper>
            })
            .collect();

        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            today,
            helpers,
            day_counter(),
            LogLinear,
        )
        .expect("the deposit helpers bootstrap");
        let pillars = curve.dates().expect("the bootstrap succeeds");
        assert_eq!(pillars.len(), 3, "the reference date plus the two deposits");

        let grid = isda_node_grid(
            &yield_handle(curve),
            &credit_handle(flat_hazard_rate()),
            maturity(),
        )
        .expect("a bootstrapped log-linear discount curve is supported");
        assert_eq!(grid, pillars);
    }

    /// A yield curve that declines the seam, standing in for the spreaded and
    /// implied wrappers.
    struct SpreadedCurve {
        base: TermStructureBase,
    }

    impl SpreadedCurve {
        fn new() -> SpreadedCurve {
            SpreadedCurve {
                base: TermStructureBase::with_reference_date(
                    reference(),
                    None,
                    Some(day_counter()),
                ),
            }
        }
    }

    impl AsObservable for SpreadedCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for SpreadedCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl YieldTermStructure for SpreadedCurve {
        fn discount_impl(&self, _t: Time) -> QlResult<DiscountFactor> {
            Ok(1.0)
        }
    }
}
