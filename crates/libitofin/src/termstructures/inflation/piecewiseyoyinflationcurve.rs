//! Piecewise-bootstrapped year-on-year inflation term structure.
//!
//! Port of `ql/termstructures/inflation/piecewiseyoyinflationcurve.hpp:37-154`.
//! A [`PiecewiseYoYInflationCurve`] is built from [`YoYInflationHelper`]s - in
//! practice year-on-year inflation swaps - whose observed fixing periods mark
//! the segment boundaries; each node is solved so the helper reprices its
//! quoted rate off the curve, by the same [`IterativeBootstrap`] the zero,
//! yield and credit sides run.
//!
//! It mirrors
//! [`PiecewiseZeroInflationCurve`](super::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve)
//! field for field. C++ derives from `InterpolatedYoYInflationCurve<Interpolator>`
//! *and* `LazyObject` (`:40-42`); Rust has no inheritance, so the node storage
//! lives here and [`yoy_rate_impl`](YoYInflationTermStructure::yoy_rate_impl)
//! reads it the way [`InterpolatedYoYInflationCurve`] reads its own
//! (`interpolatedyoyinflationcurve.hpp:147-150`).
//!
//! [`InterpolatedYoYInflationCurve`]: super::interpolatedyoyinflationcurve::InterpolatedYoYInflationCurve
//!
//! ## Node zero carries the base rate, and keeps it
//!
//! This is the difference from the zero curve that the whole
//! [`YoYInflationTraits`] transcription exists for, and it takes two halves
//! that only work together. C++'s `YoYInflationTraits::initialValue` reads the
//! curve's *own* base rate off the term-structure pointer it is handed
//! (`inflationtraits.hpp:129-131`) where the zero convention returns a dummy
//! constant (`:50-53`), so this curve overrides
//! [`PiecewiseCurve::initial_value`] with
//! [`base_rate`](InflationTermStructure::base_rate) - the hook existing for
//! exactly this case. And C++'s `YoYInflationTraits::updateGuess` writes only
//! node `i` (`:175-179`) where the zero convention also mirrors the first
//! solved pillar onto node 0 (`:100-102`), so the seeded base rate is never
//! overwritten. Seed the node from the traits constant instead, or mirror
//! pillar 1 onto it, and the curve publishes an invented figure at its base
//! date in place of a quoted one.
//!
//! The base *date* anchoring node zero is the zero curve's behaviour unchanged:
//! `initialDate` is the curve's `baseDate()` on both conventions (`:124-126`
//! against `:46-48`), so the first node precedes the reference date and its
//! time is negative.
//!
//! ## Divergences from QuantLib
//!
//! - The seasonality argument (`:61`) is ported, and so is C++'s virtual
//!   `update()` behind
//!   [`set_seasonality`](InflationTermStructure::set_seasonality): installing
//!   one invalidates the bootstrap, so the next read re-solves every node
//!   against the corrected rates. The consistency gate runs from this
//!   constructor rather than from the base one; see the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergences.
//! - The `accuracy` constructor argument (`:62`) is not exposed, matching the
//!   zero curve; the field carries the C++ default, which is `1.0e-12` here
//!   against the zero curve's `1.0e-14`.
//! - Only [`Linear`] is constructible ([`new`](PiecewiseYoYInflationCurve::new)
//!   builds the interpolator itself), so the C++ `Interpolator` argument
//!   (`:63`) has no counterpart. The impls below are generic, so a second
//!   constructor is all another local interpolator needs.

use std::cell::RefCell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraptraits::CurveData;
use crate::termstructures::inflation::inflationhelpers::YoYInflationHelper;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, YoYInflationTermStructure,
};
use crate::termstructures::inflation::inflationtraits::YoYInflationTraits;
use crate::termstructures::inflation::seasonality::Seasonality;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's own
/// observers (the port of `update()`, `:145-149`).
struct CurveUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for CurveUpdater {
    fn update(&mut self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }
}

/// Year-on-year inflation term structure bootstrapped from inflation helpers.
///
/// `I` is the interpolation factory ([`Linear`]); the curve shape traits are
/// always [`YoYInflationTraits`], the nodes being the year-on-year inflation
/// rates themselves. The node data lives in a `RefCell` the bootstrap mutates
/// and the rate lookups read back.
pub struct PiecewiseYoYInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    instruments: Vec<Shared<dyn YoYInflationHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn YoYInflationTermStructure>,
}

impl PiecewiseYoYInflationCurve<Linear> {
    /// Builds a linearly interpolated curve over `instruments` with a fixed
    /// `reference_date` (`:54-73`). Construction is cheap; the bootstrap runs on
    /// first use.
    ///
    /// `base_date` is the last date for which a fixing is known and precedes
    /// the reference date; `base_yoy_rate` is the year-on-year rate observed
    /// over the period ending there, and is what node zero is seeded with and
    /// keeps.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference_date: Date,
        base_date: Date,
        base_yoy_rate: Rate,
        frequency: Frequency,
        day_counter: DayCounter,
        instruments: Vec<Shared<dyn YoYInflationHelper>>,
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> QlResult<Shared<PiecewiseYoYInflationCurve<Linear>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseYoYInflationCurve<Linear>>| {
            let self_weak: Weak<dyn YoYInflationTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseYoYInflationCurve {
                inflation: InflationTermStructureBase::with_reference_date(
                    reference_date,
                    base_date,
                    frequency,
                    Some(day_counter),
                    Some(base_yoy_rate),
                    seasonality,
                ),
                instruments,
                interpolator: Linear,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap: IterativeBootstrap::new(),
                accuracy: 1.0e-12,
                self_weak,
            }
        });

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        curve.check_seasonality()?;
        Ok(curve)
    }
}

impl<I: Interpolator + 'static> PiecewiseYoYInflationCurve<I> {
    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `:139-142`). The lazy core is not borrowed while
    /// the bootstrap runs, so a helper reading the curve mid-bootstrap - which
    /// every one of them does, through the index it forecasts with - re-enters
    /// here and returns on the pre-set flag, answering off the partially solved
    /// node prefix.
    pub fn calculate(&self) -> QlResult<()> {
        if self.lazy.borrow().is_calculated() {
            return Ok(());
        }
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.bootstrap.calculate(self);
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    /// The node times, after bootstrapping (`:114-118`). The first is negative,
    /// the base date preceding the reference date.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping (`:120-124`). The first is the base
    /// date.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node year-on-year inflation rates, after bootstrapping (`:126-130`).
    /// The first is the base rate the curve was built with.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, year-on-year rate) nodes, after bootstrapping (`:132-137`).
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseYoYInflationCurve<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseYoYInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve
        // (`:109-112`); a bootstrap failure is surfaced by the reads, so fall
        // back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.inflation.term_structure_base().reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> InflationTermStructure for PiecewiseYoYInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }

    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
        self
    }

    /// Invalidates the bootstrap before broadcasting: a seasonality change
    /// moves the rates the helpers reprice against, so the solved nodes are
    /// stale and every one of them has to be solved again. This is the curve's
    /// own `update()` (`:145-149`), which the base default cannot reach.
    fn update_after_seasonality_change(&self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }
}

impl<I: Interpolator + 'static> YoYInflationTermStructure for PiecewiseYoYInflationCurve<I> {
    fn yoy_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        data.interpolation()?.value(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseYoYInflationCurve<I> {
    type Traits = YoYInflationTraits;
    type Interp = I;
    type TS = dyn YoYInflationTermStructure;
    type Helper = dyn YoYInflationHelper;

    fn instruments(&self) -> &[Shared<dyn YoYInflationHelper>] {
        &self.instruments
    }

    fn interpolator(&self) -> &I {
        &self.interpolator
    }

    fn curve_data(&self) -> &RefCell<CurveData<I>> {
        &self.data
    }

    fn accuracy(&self) -> Real {
        self.accuracy
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.inflation.term_structure_base().reference_date()
    }

    /// The base date, where every other piecewise curve answers its reference
    /// date (`YoYInflationTraits::initialDate`, `inflationtraits.hpp:124-126`).
    fn initial_date(&self) -> QlResult<Date> {
        Ok(InflationTermStructure::base_date(self))
    }

    /// The curve's own base rate, where every other piecewise curve - the
    /// zero-inflation one included - takes the traits' constant
    /// (`YoYInflationTraits::initialValue`, `inflationtraits.hpp:129-131`).
    /// Node zero holds this figure for the life of the bootstrap, the
    /// year-on-year `update_guess` never writing to it.
    fn initial_value(&self) -> QlResult<Real> {
        InflationTermStructure::base_rate(self)
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn YoYInflationTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}
