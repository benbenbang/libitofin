//! Piecewise-bootstrapped year-on-year optionlet volatility curve.
//!
//! Port of `ql/experimental/inflation/piecewiseyoyoptionletvolatility.hpp`:
//! [`YoYInflationVolatilityTraits`] (`hpp:36-96`) drives the bootstrap's seed,
//! guesses and brackets, and [`PiecewiseYoYOptionletVolatilityCurve`]
//! (`hpp:105-175`) is the curve, one per strike of the stripper, fitted to
//! [`YoYOptionletVolatilityHelper`]s by the same [`IterativeBootstrap`] every
//! other piecewise curve runs. "We use a flat smile for bootstrapping at
//! constant K" (`hpp:100-103`): the smile is flat, the strike bounds a thin
//! band around the quoted K.
//!
//! It mirrors [`PiecewiseYoYInflationCurve`] field for field; C++ derives from
//! `InterpolatedYoYOptionletVolatilityCurve<Interpolator>` *and* `LazyObject`
//! (`hpp:108-110`), and Rust has no inheritance, so the node storage lives
//! here and the volatility lookup reads it directly.
//!
//! ## Node zero carries the base level
//!
//! `YoYInflationVolatilityTraits::initialDate` is the curve's own base date
//! and `initialValue` its own base level (`hpp:41-51` - "REALLLYYYY important
//! because generally don't have a clue what this should be"), both read off
//! the curve being bootstrapped. They land here as the
//! [`PiecewiseCurve::initial_date`]/[`PiecewiseCurve::initial_value`] hook
//! overrides, the same seam [`PiecewiseYoYInflationCurve`] uses for its base
//! rate; `updateGuess` writes only node `i` (`hpp:89-93`), so the seeded level
//! survives the whole solve.
//!
//! ## Divergences from QuantLib
//!
//! - The moving reference date takes the shared [`Settings`] handle (D5).
//! - The `accuracy` constructor argument (`hpp:133`) is not exposed, matching
//!   the sibling curves; the field carries the C++ default `1.0e-12`.
//! - Only [`Linear`] is constructible, the impls staying generic, exactly as
//!   on [`PiecewiseYoYInflationCurve`].
//!
//! [`PiecewiseYoYInflationCurve`]: crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve
//! [`IterativeBootstrap`]: crate::termstructures::iterativebootstrap::IterativeBootstrap
//! [`PiecewiseCurve::initial_date`]: crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_date
//! [`PiecewiseCurve::initial_value`]: crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_value

use std::cell::RefCell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraptraits::{BootstrapTraits, CurveData};
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Size, Time, Volatility};

use super::yoyoptionlethelpers::YoYOptionletVolatilityHelper;
use super::{YoYOptionletVolatilitySurface, YoYOptionletVolatilitySurfaceBase};

/// Traits for the inflation-volatility bootstrap
/// (`YoYInflationVolatilityTraits`, `hpp:36-96`).
pub struct YoYInflationVolatilityTraits;

impl BootstrapTraits for YoYInflationVolatilityTraits {
    /// C++ has no constant here: `initialValue` reads the bootstrapped curve's
    /// own `baseLevel()` (`hpp:45-51`), which lands as
    /// [`PiecewiseYoYOptionletVolatilityCurve`]'s `initial_value` hook
    /// override, so this static is never the seeding path. A zero volatility
    /// stands in for the total function the trait requires.
    fn initial_value() -> Real {
        0.0
    }

    /// The per-node guess (`hpp:54-68`): the stored node on a seeded pass,
    /// `0.005` at the first pillar, `0.002` after it.
    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        if i == 1 {
            return 0.005;
        }
        0.002
    }

    /// The lower bracket (`hpp:71-78`): two vol points under the previous
    /// node, floored at zero - "vol cannot be negative".
    fn min_value_after(i: Size, _times: &[Time], data: &[Real], _valid_data: bool) -> Real {
        (data[i - 1] - 0.02).max(0.0)
    }

    /// The upper bracket (`hpp:79-86`): two vol points over the previous node.
    fn max_value_after(i: Size, _times: &[Time], data: &[Real], _valid_data: bool) -> Real {
        data[i - 1] + 0.02
    }

    /// Writes a solved level back (`hpp:89-93`): node `i` takes it and nothing
    /// else moves, so the seeded base level is never overwritten.
    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
    }

    /// The convergence-loop cap (`hpp:95`).
    fn max_iterations() -> Size {
        25
    }
}

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's
/// own observers (the port of `update()`, `hpp:226-230`).
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

/// Piecewise year-on-year optionlet volatility curve
/// (`PiecewiseYoYOptionletVolatilityCurve`, `hpp:105-175`).
///
/// `I` is the interpolation factory ([`Linear`]); the shape traits are always
/// [`YoYInflationVolatilityTraits`], the nodes being the optionlet
/// volatilities themselves.
pub struct PiecewiseYoYOptionletVolatilityCurve<I: Interpolator> {
    vol_base: YoYOptionletVolatilitySurfaceBase,
    min_strike: Rate,
    max_strike: Rate,
    instruments: Vec<Shared<dyn YoYOptionletVolatilityHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn YoYOptionletVolatilitySurface>,
}

impl PiecewiseYoYOptionletVolatilityCurve<Linear> {
    /// Builds a linearly interpolated curve over `instruments`
    /// (`hpp:121-148`). Construction is cheap; the bootstrap runs on first
    /// use. `base_yoy_volatility` is the artificial volatility at the base
    /// date - the C++ constructor forwards it into the protected
    /// interpolated-curve constructor, whose whole job is `setBaseLevel`
    /// (`yoyinflationoptionletvolatilitystructure2.hpp:156-176`) - and it is
    /// what node zero is seeded with and keeps.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: Rate,
        max_strike: Rate,
        base_yoy_volatility: Volatility,
        instruments: Vec<Shared<dyn YoYOptionletVolatilityHelper>>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<PiecewiseYoYOptionletVolatilityCurve<Linear>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(
            |weak: &Weak<PiecewiseYoYOptionletVolatilityCurve<Linear>>| {
                let self_weak: Weak<dyn YoYOptionletVolatilitySurface> = weak.clone();
                let lazy = shared_mut(LazyObject::new(true));
                let observable = lazy.borrow().observable_handle();
                let updater = shared_mut(CurveUpdater {
                    lazy: SharedMut::clone(&lazy),
                });
                let vol_base = YoYOptionletVolatilitySurfaceBase::new(
                    settlement_days,
                    calendar,
                    business_day_convention,
                    day_counter,
                    observation_lag,
                    frequency,
                    index_is_interpolated,
                    settings,
                );
                vol_base.set_base_level(base_yoy_volatility);
                PiecewiseYoYOptionletVolatilityCurve {
                    vol_base,
                    min_strike,
                    max_strike,
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
            },
        );

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        // The reference date moves with the evaluation date, whose change must
        // invalidate the bootstrap; the term base broadcasts it and the lazy
        // core turns it into a re-solve on the next read.
        curve
            .vol_base
            .term_structure_base()
            .observable()
            .register_observer(&observer);
        Ok(curve)
    }
}

impl<I: Interpolator + 'static> PiecewiseYoYOptionletVolatilityCurve<I> {
    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `hpp:220-224`; the stripper calls C++'s
    /// `recalculate()` on a fresh curve, which this equals). Re-entrant: a
    /// helper repricing mid-bootstrap - every one does, through the engine
    /// reading this same surface - returns on the pre-set flag and answers off
    /// the solved prefix.
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

    /// The node times, after bootstrapping (`hpp:192-197`). The first is
    /// negative, the base date preceding the reference date.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping (`hpp:199-204`). The first is the
    /// base date.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node volatilities, after bootstrapping (`hpp:206-211`). The first
    /// is the base level the curve was built with.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, volatility) nodes, after bootstrapping (`hpp:213-218`).
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }

    /// For the curve the strike is ignored, the smile being flat; C++
    /// evaluates without extrapolation
    /// (`yoyinflationoptionletvolatilitystructure2.hpp:180-186`), and every
    /// read of the bootstrap - the last coupon of pillar `i`'s helper fixes
    /// *on* node `i` - lands inside the solved prefix.
    fn volatility_impl(&self, t: Time) -> QlResult<Volatility> {
        self.calculate()?;
        let data = self.data.borrow();
        data.interpolation()?.value(t)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseYoYOptionletVolatilityCurve<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseYoYOptionletVolatilityCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.vol_base.term_structure_base()
    }

    /// `maxDate` triggers the bootstrap (`hpp:186-190`) and then runs the base
    /// curve's approximation: the reference date advanced by the last node
    /// time rounded up to whole years
    /// (`yoyinflationoptionletvolatilitystructure2.hpp:73-76`). Fallbacks
    /// follow the sibling piecewise curves: the reference date, then the null
    /// date.
    fn max_date(&self) -> Date {
        let _ = self.calculate();
        let t_max = self.data.borrow().times().last().copied();
        match t_max {
            Some(t_max) => self
                .option_date_from_tenor(Period::new(t_max.ceil() as i32, TimeUnit::Years))
                .unwrap_or_else(|_| Date::null()),
            None => self
                .vol_base
                .term_structure_base()
                .reference_date()
                .unwrap_or_else(|_| Date::null()),
        }
    }
}

impl<I: Interpolator + 'static> VolatilityTermStructure
    for PiecewiseYoYOptionletVolatilityCurve<I>
{
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.vol_base.business_day_convention()
    }

    fn min_strike(&self) -> Rate {
        self.min_strike
    }

    fn max_strike(&self) -> Rate {
        self.max_strike
    }
}

impl<I: Interpolator + 'static> YoYOptionletVolatilitySurface
    for PiecewiseYoYOptionletVolatilityCurve<I>
{
    /// `baseDate` triggers the bootstrap first (`hpp:180-184`), then runs the
    /// shared date arithmetic.
    fn base_date(&self) -> QlResult<Date> {
        self.calculate()?;
        self.vol_base.base_date()
    }

    fn volatility(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Volatility> {
        let observed = self.vol_base.observed(date - obs_lag)?;
        self.vol_base.check_range(
            observed,
            strike,
            self.min_strike,
            self.max_strike,
            TermStructure::max_date(self),
        )?;
        self.volatility_impl(TermStructure::time_from_reference(self, observed)?)
    }

    fn total_variance(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Real> {
        let volatility = self.volatility(date, strike, obs_lag)?;
        Ok(volatility * volatility * self.vol_base.time_from_base(date, obs_lag)?)
    }

    fn base_level(&self) -> QlResult<Volatility> {
        self.vol_base.base_level()
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseYoYOptionletVolatilityCurve<I> {
    type Traits = YoYInflationVolatilityTraits;
    type Interp = I;
    type TS = dyn YoYOptionletVolatilitySurface;
    type Helper = dyn YoYOptionletVolatilityHelper;

    fn instruments(&self) -> &[Shared<dyn YoYOptionletVolatilityHelper>] {
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
        self.vol_base.term_structure_base().reference_date()
    }

    /// The curve's own base date (`Traits::initialDate`, `hpp:41-43`), which
    /// precedes the reference date, so node zero sits at a negative time.
    fn initial_date(&self) -> QlResult<Date> {
        self.vol_base.base_date()
    }

    /// The curve's own base level (`Traits::initialValue`, `hpp:45-51`),
    /// where the yield conventions take a traits constant; an `Err` if it was
    /// never set, though the public constructor always sets it.
    fn initial_value(&self) -> QlResult<Real> {
        self.vol_base.base_level()
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn YoYOptionletVolatilitySurface>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}
