//! Piecewise-bootstrapped zero-coupon inflation term structure.
//!
//! Port of `ql/termstructures/inflation/piecewisezeroinflationcurve.hpp:55-72`.
//! A [`PiecewiseZeroInflationCurve`] is built from
//! [`ZeroInflationHelper`]s - in practice zero-coupon inflation swaps - whose
//! observed fixing periods mark the segment boundaries; each node is solved so
//! the helper reprices its quoted rate off the curve, by the same
//! [`IterativeBootstrap`] the yield and credit sides run.
//!
//! It mirrors
//! [`PiecewiseDefaultCurve`](crate::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve)
//! field for field: the laziness contract, the pre-set `calculated` flag, the
//! observer registration on every helper and the [`PiecewiseCurve`] surface the
//! bootstrap drives are all the same. C++ derives from
//! `InterpolatedZeroInflationCurve<Interpolator>` *and* `LazyObject`
//! (`:56-57`); Rust has no inheritance, so the node storage lives here and
//! [`zero_rate_impl`](ZeroInflationTermStructure::zero_rate_impl) reads it the
//! way [`InterpolatedZeroInflationCurve`] reads its own
//! (`interpolatedzeroinflationcurve.hpp:137`).
//!
//! ## The base date, not the reference date, anchors node zero
//!
//! This is the one structural difference from every other piecewise curve.
//! C++'s `ZeroInflationTraits::initialDate` returns the curve's `baseDate()`
//! (`inflationtraits.hpp:46-48`), the last date for which a fixing is known,
//! which *precedes* the reference date. The port carries that decision on the
//! curve as [`PiecewiseCurve::initial_date`], overridden below, so the first
//! bootstrap node lands on the base date while `time_from_reference` stays
//! anchored at the reference date - leaving `times()[0]` negative.
//!
//! [`InterpolatedZeroInflationCurve`]: super::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve
//!
//! ## Divergences from QuantLib
//!
//! - Seasonality (`:61`) is omitted rather than accepted and ignored, following
//!   the [`inflationtermstructure`](super::inflationtermstructure) contract: a
//!   caller needing it fails to compile. It follows with the seasonality
//!   classes in EPIC Inflation (#705).
//! - The `BaseDateFunc` constructor overload (`:73-90`), whose base date is
//!   resolved lazily inside `performCalculations` (`:167-169`), is deferred
//!   with it; the base date here is always the caller's.
//! - Only [`Linear`] is constructible ([`new`](PiecewiseZeroInflationCurve::new)
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
use crate::termstructures::inflation::inflationhelpers::ZeroInflationHelper;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, ZeroInflationTermStructure,
};
use crate::termstructures::inflation::inflationtraits::ZeroInflationTraits;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's own
/// observers (the port of `update()`, `:180-183`).
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

/// Zero-coupon inflation term structure bootstrapped from inflation helpers.
///
/// `I` is the interpolation factory ([`Linear`]); the curve shape traits are
/// always [`ZeroInflationTraits`], the nodes being the zero-coupon inflation
/// rates themselves. The node data lives in a `RefCell` the bootstrap mutates
/// and the zero-rate lookups read back.
pub struct PiecewiseZeroInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    instruments: Vec<Shared<dyn ZeroInflationHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn ZeroInflationTermStructure>,
}

impl PiecewiseZeroInflationCurve<Linear> {
    /// Builds a linearly interpolated curve over `instruments` with a fixed
    /// `reference_date` (`:69-84`). Construction is cheap; the bootstrap runs on
    /// first use.
    ///
    /// `base_date` is the last date for which a fixing is known - in practice
    /// [`ZeroInflationIndex::last_fixing_date`] - and precedes the reference
    /// date.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    ///
    /// [`ZeroInflationIndex::last_fixing_date`]: crate::indexes::inflationindex::ZeroInflationIndex::last_fixing_date
    pub fn new(
        reference_date: Date,
        base_date: Date,
        frequency: Frequency,
        day_counter: DayCounter,
        instruments: Vec<Shared<dyn ZeroInflationHelper>>,
    ) -> QlResult<Shared<PiecewiseZeroInflationCurve<Linear>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseZeroInflationCurve<Linear>>| {
            let self_weak: Weak<dyn ZeroInflationTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseZeroInflationCurve {
                inflation: InflationTermStructureBase::with_reference_date(
                    reference_date,
                    base_date,
                    frequency,
                    Some(day_counter),
                    None,
                ),
                instruments,
                interpolator: Linear,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap: IterativeBootstrap::new(),
                accuracy: 1.0e-14,
                self_weak,
            }
        });

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        Ok(curve)
    }
}

impl<I: Interpolator + 'static> PiecewiseZeroInflationCurve<I> {
    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `:165-171`). The lazy core is not borrowed while
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

    /// The node times, after bootstrapping (`:143-147`). The first is negative,
    /// the base date preceding the reference date.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping (`:149-153`). The first is the base
    /// date.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node zero-coupon inflation rates, after bootstrapping (`:155-159`).
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, zero rate) nodes, after bootstrapping (`:161-166`).
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseZeroInflationCurve<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseZeroInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve
        // (`:135-139`); a bootstrap failure is surfaced by the reads, so fall
        // back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.inflation.term_structure_base().reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> InflationTermStructure for PiecewiseZeroInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }
}

impl<I: Interpolator + 'static> ZeroInflationTermStructure for PiecewiseZeroInflationCurve<I> {
    fn zero_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        data.interpolation()?.value(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseZeroInflationCurve<I> {
    type Traits = ZeroInflationTraits;
    type Interp = I;
    type TS = dyn ZeroInflationTermStructure;
    type Helper = dyn ZeroInflationHelper;

    fn instruments(&self) -> &[Shared<dyn ZeroInflationHelper>] {
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
    /// date (`ZeroInflationTraits::initialDate`, `inflationtraits.hpp:46-48`).
    fn initial_date(&self) -> QlResult<Date> {
        Ok(InflationTermStructure::base_date(self))
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn ZeroInflationTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}
