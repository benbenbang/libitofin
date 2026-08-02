//! Piecewise-bootstrapped credit term structure.
//!
//! Port of `ql/termstructures/credit/piecewisedefaultcurve.hpp:53`. A
//! [`PiecewiseDefaultCurve`] is built from a set of
//! [`DefaultProbabilityHelper`]s whose maturities mark the segment boundaries;
//! each node is solved so the helper reprices its quote off the curve, by the
//! same [`IterativeBootstrap`] the yield side runs.
//!
//! This is the credit twin of
//! [`PiecewiseYieldCurve`](crate::termstructures::yields::PiecewiseYieldCurve)
//! and mirrors it field for field: the laziness contract, the pre-set
//! `calculated` flag, the observer registration on every helper and the
//! [`PiecewiseCurve`] surface the bootstrap drives are all the same.
//!
//! ## Laziness and bootstrap re-entrancy
//!
//! C++ derives from `Traits::curve<Interpolator>::type` *and* `LazyObject`
//! (`:53-60`), and its `survivalProbabilityImpl` / `defaultDensityImpl` /
//! `hazardRateImpl` each call `calculate()` before delegating to the base curve
//! (`:259-276`). Rust has no inheritance, so the node storage lives here and the
//! reads run the same conversions over it, through the free functions
//! `survival_probability_from_nodes` and `hazard_rate_from_nodes` the plain
//! [`InterpolatedHazardRateCurve`](crate::termstructures::credit::interpolatedhazardratecurve::InterpolatedHazardRateCurve)
//! reads its own nodes with.
//!
//! The pre-set `calculated` flag ([`LazyObject::new(true)`](LazyObject::new)) is
//! what breaks the bootstrap cycle, and the cycle here is tighter than on the
//! yield side: mid-bootstrap a
//! [`SpreadCdsHelper`](crate::termstructures::credit::defaultprobabilityhelpers::SpreadCdsHelper)
//! reprices its contract, whose engine reads *this* curve, which re-enters
//! [`calculate`](PiecewiseDefaultCurve::calculate). The re-entrant call finds
//! the calculation already running, returns immediately, and the read answers
//! off the partially solved node prefix - which is exactly what the bootstrap
//! needs, since a helper only ever reads up to its own pillar.
//!
//! ## Scope
//!
//! The `Traits` type parameter carries the C++ spelling
//! (`PiecewiseDefaultCurve<HazardRate, BackwardFlat>`) but only
//! [`HazardRate`] is wired: every impl below is written at that instantiation,
//! so a curve on the yield traits will not compile - the Rust counterpart of
//! C++'s `Traits::curve<I>::type` failing to be a
//! `DefaultProbabilityTermStructure`. `DefaultDensity` and
//! `SurvivalProbability` need their own base curves and follow within EPIC
//! Credit (#676); adding them means relaxing these bounds.
//!
//! Jump quotes are not ported, per the
//! [`defaulttermstructure`](crate::termstructures::credit::defaulttermstructure)
//! divergence (#676), so the four C++ constructors collapse to
//! [`new`](PiecewiseDefaultCurve::new).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::Interpolator;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::termstructures::credit::defaultprobabilityhelpers::DefaultProbabilityHelper;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::hazardratestructure::HazardRateStructure;
use crate::termstructures::credit::interpolatedhazardratecurve::{
    hazard_rate_from_nodes, survival_probability_from_nodes,
};
use crate::termstructures::credit::probabilitytraits::HazardRate;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Rate, Real, Time};

/// Feeds a helper-quote, discount-curve or evaluation-date notification into
/// the curve's lazy core: it invalidates the bootstrap cache and re-broadcasts
/// to the curve's own observers (the port of `registerWithObservables` +
/// `LazyObject::update`), as the yield curve's own updater does.
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

/// Default-probability term structure bootstrapped from credit helpers.
///
/// `T` is the curve-shape traits ([`HazardRate`]) and `I` the interpolation
/// factory (`BackwardFlat`). The node data lives in a `RefCell` the bootstrap
/// mutates and the survival/hazard lookups read back.
pub struct PiecewiseDefaultCurve<T: BootstrapTraits, I: Interpolator> {
    base: TermStructureBase,
    instruments: Vec<Shared<dyn DefaultProbabilityHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn DefaultProbabilityTermStructure>,
    _traits: PhantomData<fn() -> T>,
}

impl<I: Interpolator + 'static> PiecewiseDefaultCurve<HazardRate, I> {
    /// Builds a curve over `instruments` with a fixed `reference_date` (the C++
    /// reference-date constructor, `piecewisedefaultcurve.hpp:68-79`).
    /// Construction is cheap; the bootstrap runs on first use.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    pub fn new(
        reference_date: Date,
        instruments: Vec<Shared<dyn DefaultProbabilityHelper>>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Shared<PiecewiseDefaultCurve<HazardRate, I>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseDefaultCurve<HazardRate, I>>| {
            let self_weak: Weak<dyn DefaultProbabilityTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseDefaultCurve {
                base: TermStructureBase::with_reference_date(
                    reference_date,
                    None,
                    Some(day_counter),
                ),
                instruments,
                interpolator,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap: IterativeBootstrap::new(),
                accuracy: 1.0e-12,
                self_weak,
                _traits: PhantomData,
            }
        });

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        Ok(curve)
    }

    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `piecewisedefaultcurve.hpp:279-283`). The lazy
    /// core is not borrowed while the bootstrap runs, so a helper reading the
    /// curve mid-bootstrap re-enters here and returns on the pre-set flag.
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

    /// The node times, after bootstrapping.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node hazard rates, after bootstrapping.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, hazard rate) nodes, after bootstrapping.
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseDefaultCurve<HazardRate, I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseDefaultCurve<HazardRate, I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve; a
        // bootstrap failure is surfaced by the reads, so fall back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.base.reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> HazardRateStructure for PiecewiseDefaultCurve<HazardRate, I> {
    fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        hazard_rate_from_nodes(data.interpolation()?, t)
    }
}

impl<I: Interpolator + 'static> DefaultProbabilityTermStructure
    for PiecewiseDefaultCurve<HazardRate, I>
{
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        self.calculate()?;
        let data = self.data.borrow();
        survival_probability_from_nodes(data.interpolation()?, t)
    }

    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        self.default_density_from_hazard_rate(t)
    }

    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.hazard_rate_curve_impl(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseDefaultCurve<HazardRate, I> {
    type Traits = HazardRate;
    type Interp = I;
    type TS = dyn DefaultProbabilityTermStructure;
    type Helper = dyn DefaultProbabilityHelper;

    fn instruments(&self) -> &[Shared<dyn DefaultProbabilityHelper>] {
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
        self.base.reference_date()
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn DefaultProbabilityTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}
