//! Facades for the cash-flow slice: the [`PyYoYInflationCoupon`] a year-on-year
//! inflation leg pays, the [`PyYoYInflationLeg`] builder producing it
//! (`cashflows::yoyinflationcoupon`, `cashflows::yoyinflationleg`) and the
//! floating [`PyIborLeg`] (`cashflows::iborleg`).
//!
//! This is the first coupon facade in the crate (#848). Until now the coupons
//! were reachable only *through* the instruments built over them - a
//! [`YearOnYearInflationSwap`](crate::inflation::PyYearOnYearInflationSwap) or a
//! [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor) - which
//! covers the pricing path but not a caller who wants the leg itself.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::inflation::{
    PyConstantYoYOptionletVolatility, PyCpiInterpolationType, PyYoYInflationIndex,
};
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use libitofin::cashflows::{
    CappedFlooredYoYInflationCoupon, Coupon, IborCoupon, IborLeg, YoYInflationCoupon,
    YoYInflationCouponPricer, YoYInflationLeg, YoYInflationOptionletCouponPricer,
    set_yoy_coupon_pricer,
};
use libitofin::event::Event;
use libitofin::handle::Handle;
use libitofin::indexes::iborindex::IborIndex;
use libitofin::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::period::Period;
use libitofin::time::schedule::Schedule;
use pyo3::prelude::*;

/// Python `YoYInflationCoupon`: one coupon of a year-on-year inflation leg
/// (`cashflows::yoyinflationcoupon::YoYInflationCoupon`).
///
/// Built only through [`YoYInflationLeg`](PyYoYInflationLeg), which is where the
/// coupon's pricer comes from: a coupon holds no rate of its own and answers
/// [`rate`](Self::rate) and [`amount`](Self::amount) only once one is attached,
/// so a standalone constructor here would hand Python an object whose two
/// headline methods report `"pricer not set"`. This mirrors
/// [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor), which is
/// likewise factory-only.
///
/// Deferred (visible): a direct constructor and the
/// `SwapletYoYInflationCouponPricer` binding it would need, plus the
/// `accrued_amount` and reference-period accessors no fixture reads yet.
#[pyclass(name = "YoYInflationCoupon", unsendable)]
pub struct PyYoYInflationCoupon {
    inner: Shared<YoYInflationCoupon>,
}

#[pymethods]
impl PyYoYInflationCoupon {
    /// The rate the coupon accrues at: its pricer's swaplet rate, the geared
    /// index fixing plus the spread.
    ///
    /// # Errors
    ///
    /// Reports a coupon with no pricer attached, and whatever resolving the
    /// fixing reports - a missing history entry, or a forecast off an index with
    /// no curve linked.
    fn rate(&self) -> PyResult<f64> {
        Ok(Coupon::rate(&*self.inner).map_err(PyQlError::from)?)
    }

    /// What the coupon pays on its [`date`](Self::date), undiscounted: the
    /// [`rate`](Self::rate) over the [`accrual_period`](Self::accrual_period) on
    /// the [`nominal`](Self::nominal). Fallible as [`rate`](Self::rate).
    fn amount(&self) -> PyResult<f64> {
        Ok(Coupon::amount(&*self.inner).map_err(PyQlError::from)?)
    }

    /// The date the observation is published on: the reference-period end moved
    /// back by the observation lag, then back the fixing days.
    ///
    /// This is *not* the date the rate resolves at. An inflation index has no
    /// fixing calendar, so with no fixing days the roll is inert.
    fn fixing_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.fixing_date())
    }

    /// The year-on-year rate the coupon observes, before gearing and spread.
    ///
    /// It lags off the [`accrual_end_date`](Self::accrual_end_date), not off
    /// [`fixing_date`](Self::fixing_date): a year-on-year coupon overrides the
    /// base rule that reads the index at its fixing date.
    ///
    /// # Errors
    ///
    /// As [`rate`](Self::rate), bar the missing pricer.
    fn index_fixing(&self) -> PyResult<f64> {
        Ok(self.inner.index_fixing().map_err(PyQlError::from)?)
    }

    /// The nominal the coupon accrues on.
    fn nominal(&self) -> f64 {
        Coupon::nominal(&*self.inner)
    }

    /// The start of the accrual period.
    fn accrual_start_date(&self) -> PyDate {
        PyDate::from_inner(Coupon::accrual_start_date(&*self.inner))
    }

    /// The end of the accrual period, which is also where the observation lags
    /// from.
    fn accrual_end_date(&self) -> PyDate {
        PyDate::from_inner(Coupon::accrual_end_date(&*self.inner))
    }

    /// The whole accrual period as a fraction of a year, measured with the
    /// [`day_counter`](Self::day_counter) over the reference period.
    fn accrual_period(&self) -> f64 {
        Coupon::accrual_period(&*self.inner)
    }

    /// The payment date, which is the accrual end rolled on the leg's payment
    /// calendar.
    fn date(&self) -> PyDate {
        PyDate::from_inner(Event::date(&*self.inner))
    }

    /// The day counter the accrual is measured with.
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(Coupon::day_counter(&*self.inner))
    }

    /// The multiplicative coefficient applied to the index fixing.
    fn gearing(&self) -> f64 {
        self.inner.gearing()
    }

    /// The spread paid over the geared fixing.
    fn spread(&self) -> f64 {
        self.inner.spread()
    }

    /// How far back the coupon observes the index.
    fn observation_lag(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.observation_lag())
    }

    /// How the observation interpolates between index fixings.
    fn interpolation(&self) -> PyCpiInterpolationType {
        PyCpiInterpolationType::from_inner(self.inner.interpolation())
    }

    /// The number of business days the fixing date rolls back by.
    fn fixing_days(&self) -> u32 {
        self.inner.fixing_days()
    }

    fn __repr__(&self) -> String {
        format!(
            "YoYInflationCoupon({}, {})",
            Coupon::accrual_start_date(&*self.inner),
            Coupon::accrual_end_date(&*self.inner)
        )
    }
}

impl PyYoYInflationCoupon {
    /// Wraps a coupon the leg builder produced.
    pub(crate) fn from_shared(inner: Shared<YoYInflationCoupon>) -> Self {
        PyYoYInflationCoupon { inner }
    }

    /// The wrapped coupon, which is what the raw
    /// [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor)
    /// constructors take a vector of.
    pub(crate) fn shared(&self) -> Shared<YoYInflationCoupon> {
        Shared::clone(&self.inner)
    }
}

/// Python `YoYInflationOptionletCouponPricer`: the coupon pricer that values a
/// capped or floored year-on-year coupon's optionlets off a volatility surface
/// (`cashflows::yoyinflationoptionletpricer`).
///
/// The distribution is chosen by the constructor rather than passed as an
/// argument, mirroring the core's three constructors and C++'s three pricer
/// classes, as
/// [`YoYInflationCapFloorEngine`](crate::inflation::PyYoYInflationCapFloorEngine)
/// does: [`black`](Self::black) is lognormal, [`unit_displaced`](Self::unit_displaced)
/// lognormal in `1 + rate` and [`bachelier`](Self::bachelier) normal.
///
/// The `settings` behind `volatility` and behind the index the priced coupons
/// observe must be the same object, or the two resolve their dates against
/// different evaluation dates and the rate is silently wrong.
///
/// `nominal_ts` is optional because only the discounted `swaplet_price` path
/// reads it: a pricer built without one still answers every rate a
/// [`CappedFlooredYoYInflationCoupon`](PyCappedFlooredYoYInflationCoupon) asks
/// of it.
#[pyclass(name = "YoYInflationOptionletCouponPricer", unsendable)]
pub struct PyYoYInflationOptionletCouponPricer {
    inner: SharedMut<YoYInflationOptionletCouponPricer>,
}

#[pymethods]
impl PyYoYInflationOptionletCouponPricer {
    /// Optionlets under the lognormal model (C++
    /// `BlackYoYInflationCouponPricer`).
    #[staticmethod]
    #[pyo3(signature = (volatility, nominal_ts = None))]
    fn black(
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: Option<&PyYieldTermStructure>,
    ) -> Self {
        PyYoYInflationOptionletCouponPricer {
            inner: shared_mut(YoYInflationOptionletCouponPricer::black(
                volatility.handle(),
                nominal_handle(nominal_ts),
            )),
        }
    }

    /// Optionlets under the unit-displaced lognormal model (C++
    /// `UnitDisplacedBlackYoYInflationCouponPricer`), the usual quoting
    /// convention for an inflation rate that may go negative.
    #[staticmethod]
    #[pyo3(signature = (volatility, nominal_ts = None))]
    fn unit_displaced(
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: Option<&PyYieldTermStructure>,
    ) -> Self {
        PyYoYInflationOptionletCouponPricer {
            inner: shared_mut(YoYInflationOptionletCouponPricer::unit_displaced(
                volatility.handle(),
                nominal_handle(nominal_ts),
            )),
        }
    }

    /// Optionlets under the normal model (C++
    /// `BachelierYoYInflationCouponPricer`).
    #[staticmethod]
    #[pyo3(signature = (volatility, nominal_ts = None))]
    fn bachelier(
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: Option<&PyYieldTermStructure>,
    ) -> Self {
        PyYoYInflationOptionletCouponPricer {
            inner: shared_mut(YoYInflationOptionletCouponPricer::bachelier(
                volatility.handle(),
                nominal_handle(nominal_ts),
            )),
        }
    }
}

impl PyYoYInflationOptionletCouponPricer {
    /// The pricer the leg installs across its capped coupons.
    pub(crate) fn inner(&self) -> SharedMut<YoYInflationOptionletCouponPricer> {
        SharedMut::clone(&self.inner)
    }
}

/// The discount curve handle a pricer constructor takes, empty when Python
/// passed none.
fn nominal_handle(nominal_ts: Option<&PyYieldTermStructure>) -> Handle<dyn YieldTermStructure> {
    nominal_ts.map_or_else(Handle::empty, PyYieldTermStructure::handle)
}

/// Python `CappedFlooredYoYInflationCoupon`: a year-on-year coupon with a cap
/// and/or floor layered on its rate
/// (`cashflows::capflooredyoyinflationcoupon`).
///
/// Built only through
/// [`YoYInflationLeg.capped_floored_coupons`](PyYoYInflationLeg::capped_floored_coupons),
/// which is also where the optionlet pricer comes from, as
/// [`YoYInflationCoupon`](PyYoYInflationCoupon) is built only through
/// [`coupons`](PyYoYInflationLeg::coupons).
///
/// A negative gearing swaps the two roles: the cap the leg was given becomes
/// this coupon's floor and vice versa, which is why
/// [`is_capped`](Self::is_capped) and [`effective_cap`](Self::effective_cap)
/// answer off the stored level rather than off what was passed.
///
/// Deferred (visible): the delegated [`Coupon`] face - the nominal, the accrual
/// dates and the payment date - which is reachable on the plain coupons of the
/// same leg, and the `underlying` accessor no fixture reads.
#[pyclass(name = "CappedFlooredYoYInflationCoupon", unsendable)]
pub struct PyCappedFlooredYoYInflationCoupon {
    inner: Shared<CappedFlooredYoYInflationCoupon>,
}

#[pymethods]
impl PyCappedFlooredYoYInflationCoupon {
    /// The rate the coupon accrues at: the underlying's swaplet rate plus the
    /// floorlet, less the caplet.
    ///
    /// # Errors
    ///
    /// Reports a coupon with no pricer attached, whatever resolving the fixing
    /// reports, and a volatility the surface refuses - a strike outside its
    /// domain, or an observation before its base date.
    fn rate(&self) -> PyResult<f64> {
        Ok(Coupon::rate(&*self.inner).map_err(PyQlError::from)?)
    }

    /// What the coupon pays on its payment date, undiscounted: the
    /// [`rate`](Self::rate) over the accrual period on the nominal. Fallible as
    /// [`rate`](Self::rate).
    fn amount(&self) -> PyResult<f64> {
        Ok(Coupon::amount(&*self.inner).map_err(PyQlError::from)?)
    }

    /// Whether a cap applies.
    fn is_capped(&self) -> bool {
        self.inner.is_capped()
    }

    /// Whether a floor applies.
    fn is_floored(&self) -> bool {
        self.inner.is_floored()
    }

    /// The de-spread, de-geared cap the caplet is struck at,
    /// `(cap - spread) / gearing`, off the stored level.
    fn effective_cap(&self) -> f64 {
        self.inner.effective_cap()
    }

    /// The de-spread, de-geared floor the floorlet is struck at,
    /// `(floor - spread) / gearing`, off the stored level.
    fn effective_floor(&self) -> f64 {
        self.inner.effective_floor()
    }

    fn __repr__(&self) -> String {
        format!(
            "CappedFlooredYoYInflationCoupon({}, {})",
            Coupon::accrual_start_date(&*self.inner),
            Coupon::accrual_end_date(&*self.inner)
        )
    }
}

impl PyCappedFlooredYoYInflationCoupon {
    /// Wraps a coupon the leg builder produced.
    pub(crate) fn from_shared(inner: Shared<CappedFlooredYoYInflationCoupon>) -> Self {
        PyCappedFlooredYoYInflationCoupon { inner }
    }
}

/// Python `YoYInflationLeg`: the builder turning a schedule into a sequence of
/// [`YoYInflationCoupon`](PyYoYInflationCoupon)s (`cashflows::yoyinflationleg`).
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the whole configuration up front and
/// assembles the chain inside [`coupons`](Self::coupons), as
/// [`MakeYoYInflationCapFloor`](crate::inflation::PyMakeYoYInflationCapFloor)
/// does. An unset optional leaves the core default in place: a
/// `ModifiedFollowing` payment roll, no fixing days, a unit gearing and no
/// spread.
///
/// `payment_day_counter` is required here although the core takes it through a
/// setter: a leg built without one reports `"no payment daycounter given"` from
/// [`coupons`](Self::coupons) rather than at construction, which is a build-time
/// miss this facade need not reproduce. A notional is just as required by the
/// core (`"no notional given"`) but stays optional, since the per-coupon
/// `notionals` list is the other way to supply it; giving neither surfaces that
/// error from [`coupons`](Self::coupons).
///
/// The `caps` and `floors` lists select which of the two coupon types the leg
/// produces: given either, [`coupons`](Self::coupons) hands back coupons the
/// core deliberately leaves unpriced and
/// [`capped_floored_coupons`](Self::capped_floored_coupons) is the intended
/// entry (#863).
///
/// Deferred (visible): the erased `build()`, whose `Leg` of `CashFlow`s Python
/// has no wrapper for, and the `CashFlows::npv` leg-summing it gates (#878); a
/// standalone coupon constructor stays with #859's raw-constructor territory.
#[pyclass(name = "YoYInflationLeg", unsendable)]
pub struct PyYoYInflationLeg {
    schedule: Schedule,
    payment_calendar: Calendar,
    index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    payment_day_counter: DayCounter,
    notional: Option<f64>,
    notionals: Option<Vec<f64>>,
    payment_adjustment: Option<BusinessDayConvention>,
    fixing_days: Option<u32>,
    gearing: Option<f64>,
    gearings: Option<Vec<f64>>,
    spread: Option<f64>,
    spreads: Option<Vec<f64>>,
    caps: Option<Vec<f64>>,
    floors: Option<Vec<f64>>,
}

#[pymethods]
impl PyYoYInflationLeg {
    /// A leg over `schedule` paying `index` observed `observation_lag` back
    /// under `interpolation`, paying on `payment_calendar` and accruing with
    /// `payment_day_counter`.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        schedule,
        payment_calendar,
        index,
        observation_lag,
        interpolation,
        payment_day_counter,
        notional = None,
        notionals = None,
        payment_adjustment = None,
        fixing_days = None,
        gearing = None,
        gearings = None,
        spread = None,
        spreads = None,
        caps = None,
        floors = None,
    ))]
    fn new(
        schedule: &PySchedule,
        payment_calendar: &PyCalendar,
        index: &PyYoYInflationIndex,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        payment_day_counter: &PyDayCounter,
        notional: Option<f64>,
        notionals: Option<Vec<f64>>,
        payment_adjustment: Option<&PyBusinessDayConvention>,
        fixing_days: Option<u32>,
        gearing: Option<f64>,
        gearings: Option<Vec<f64>>,
        spread: Option<f64>,
        spreads: Option<Vec<f64>>,
        caps: Option<Vec<f64>>,
        floors: Option<Vec<f64>>,
    ) -> Self {
        PyYoYInflationLeg {
            schedule: schedule.inner(),
            payment_calendar: payment_calendar.inner(),
            index: index.shared(),
            observation_lag: observation_lag.inner(),
            interpolation: interpolation.inner(),
            payment_day_counter: payment_day_counter.inner(),
            notional,
            notionals,
            payment_adjustment: payment_adjustment.map(PyBusinessDayConvention::inner),
            fixing_days,
            gearing,
            gearings,
            spread,
            spreads,
            caps,
            floors,
        }
    }

    /// The coupons the leg is made of, each already carrying the default
    /// swaplet pricer, which is why no pricer facade is needed to read a
    /// [`rate`](PyYoYInflationCoupon::rate).
    ///
    /// Every call rebuilds the leg, so the coupons handed back are fresh objects
    /// each time: bind the list once rather than calling this per read, or two
    /// reads compare different objects.
    ///
    /// # Errors
    ///
    /// Reports a leg with no notional, a schedule holding fewer than two dates,
    /// and more notionals, gearings or spreads than the schedule has periods.
    ///
    /// With a `caps` or `floors` list given the coupons come back *unpriced*,
    /// the core withholding the default pricer, so
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) is the entry
    /// there and the coupons handed back here answer only what needs no pricer.
    fn coupons(&self) -> PyResult<Vec<PyYoYInflationCoupon>> {
        Ok(self
            .leg()
            .coupons()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyYoYInflationCoupon::from_shared)
            .collect())
    }

    /// The coupons wrapped in the leg's per-coupon `caps` and `floors`, each
    /// carrying `pricer`.
    ///
    /// The pricer is required rather than optional: the core withholds its
    /// default swaplet pricer from a capped leg, and a swaplet pricer could not
    /// value the optionlets anyway, so a coupon handed back without one would
    /// report `"pricer not set"` from
    /// [`rate`](PyCappedFlooredYoYInflationCoupon::rate). One pricer is
    /// installed across every coupon, as the core's own `set_yoy_coupon_pricer`
    /// does.
    ///
    /// Rebuilt on every call, as [`coupons`](Self::coupons) is.
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons), plus more caps or floors than the schedule
    /// has periods, and a cap sitting below its floor.
    fn capped_floored_coupons(
        &self,
        pricer: &PyYoYInflationOptionletCouponPricer,
    ) -> PyResult<Vec<PyCappedFlooredYoYInflationCoupon>> {
        let coupons = self
            .leg()
            .capped_floored_coupons()
            .map_err(PyQlError::from)?;
        set_yoy_coupon_pricer(
            &coupons,
            pricer.inner() as SharedMut<dyn YoYInflationCouponPricer>,
        );
        Ok(coupons
            .into_iter()
            .map(PyCappedFlooredYoYInflationCoupon::from_shared)
            .collect())
    }
}

impl PyYoYInflationLeg {
    /// The core builder the stored configuration assembles into, which both
    /// coupon paths start from.
    fn leg(&self) -> YoYInflationLeg {
        let mut leg = YoYInflationLeg::new(
            self.schedule.clone(),
            self.payment_calendar.clone(),
            Shared::clone(&self.index),
            self.observation_lag,
            self.interpolation,
        )
        .with_payment_day_counter(self.payment_day_counter.clone());
        if let Some(notional) = self.notional {
            leg = leg.with_notional(notional);
        }
        if let Some(notionals) = &self.notionals {
            leg = leg.with_notionals(notionals.clone());
        }
        if let Some(convention) = self.payment_adjustment {
            leg = leg.with_payment_adjustment(convention);
        }
        if let Some(fixing_days) = self.fixing_days {
            leg = leg.with_fixing_days(fixing_days);
        }
        if let Some(gearing) = self.gearing {
            leg = leg.with_gearing(gearing);
        }
        if let Some(gearings) = &self.gearings {
            leg = leg.with_gearings(gearings.clone());
        }
        if let Some(spread) = self.spread {
            leg = leg.with_spread(spread);
        }
        if let Some(spreads) = &self.spreads {
            leg = leg.with_spreads(spreads.clone());
        }
        if let Some(caps) = &self.caps {
            leg = leg.with_caps_per_coupon(caps.clone());
        }
        if let Some(floors) = &self.floors {
            leg = leg.with_floors_per_coupon(floors.clone());
        }
        leg
    }
}

/// Python `IborLeg`: the builder turning a schedule into a sequence of
/// floating `IborCoupon`s over an ibor index (`cashflows::iborleg`).
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade stores the configuration and assembles the chain
/// afresh on every read, as [`YoYInflationLeg`](PyYoYInflationLeg) does. The
/// setters keep the core's fluent shape rather than the constructor-kwargs one:
/// each returns a new leg carrying the extra setting, so a builder bound to a
/// name never changes under a later call. An unset optional leaves the core
/// default in place: a `Following` payment roll and the index's own fixing days
/// and day counter.
///
/// The coupons are reachable only as the leg a
/// [`CapFloor`](crate::capfloor::PyCapFloor) is built over, which is why no
/// `IborCoupon` facade comes with this one: the coupons the core hands back are
/// consumed by the raw cap/floor constructors, not read from Python. Only
/// [`coupon_count`](Self::coupon_count) reads them here.
///
/// Deferred (visible): the per-coupon `notionals`, `fixing_days`, `gearings`
/// and `spreads` lists, the payment lag and calendar, the fixing convention and
/// the ex-coupon period, none of which the cap/floor fixtures set; the erased
/// `build() -> Leg`, whose `CashFlow`s Python has no wrapper for; and the
/// `caps`/`floors` setters, deliberately - see [`coupon_count`](Self::coupon_count).
#[pyclass(name = "IborLeg", unsendable)]
pub struct PyIborLeg {
    schedule: Schedule,
    index: Shared<IborIndex>,
    notional: Option<f64>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: Option<BusinessDayConvention>,
    fixing_days: Option<u32>,
}

#[pymethods]
impl PyIborLeg {
    /// A leg over `schedule` paying `index`, on the schedule's own calendar.
    #[new]
    fn new(schedule: &PySchedule, index: &PyIborIndex) -> Self {
        PyIborLeg {
            schedule: schedule.inner(),
            index: index.inner(),
            notional: None,
            payment_day_counter: None,
            payment_adjustment: None,
            fixing_days: None,
        }
    }

    /// The leg with `notional` on every coupon. Required: a leg built without
    /// one reports `"no notional given"` from [`coupon_count`](Self::coupon_count).
    fn with_notional(&self, notional: f64) -> Self {
        PyIborLeg {
            notional: Some(notional),
            ..self.copied()
        }
    }

    /// The leg accruing with `day_counter`, overriding the index's.
    fn with_payment_day_counter(&self, day_counter: &PyDayCounter) -> Self {
        PyIborLeg {
            payment_day_counter: Some(day_counter.inner()),
            ..self.copied()
        }
    }

    /// The leg rolling its payment dates with `convention`.
    fn with_payment_adjustment(&self, convention: &PyBusinessDayConvention) -> Self {
        PyIborLeg {
            payment_adjustment: Some(convention.inner()),
            ..self.copied()
        }
    }

    /// The leg fixing `fixing_days` business days before each accrual start,
    /// overriding the index's own count.
    fn with_fixing_days(&self, fixing_days: u32) -> Self {
        PyIborLeg {
            fixing_days: Some(fixing_days),
            ..self.copied()
        }
    }

    /// The number of coupons the leg builds, one per schedule period.
    ///
    /// The leg is rebuilt on every call, here and in the cap/floor
    /// constructors, so this counts the coupons a construction would produce
    /// rather than a stored list.
    ///
    /// # Errors
    ///
    /// Reports a leg with no notional, a schedule holding fewer than two dates,
    /// and whatever a coupon's own preconditions reject.
    fn coupon_count(&self) -> PyResult<usize> {
        Ok(self.coupons()?.len())
    }
}

impl PyIborLeg {
    /// A copy of the stored configuration, which each setter overrides one
    /// field of. Not a [`Clone`] implementation: deriving one on a `pyclass`
    /// changes how the type is extracted from Python.
    fn copied(&self) -> Self {
        PyIborLeg {
            schedule: self.schedule.clone(),
            index: Shared::clone(&self.index),
            notional: self.notional,
            payment_day_counter: self.payment_day_counter.clone(),
            payment_adjustment: self.payment_adjustment,
            fixing_days: self.fixing_days,
        }
    }

    /// The core builder the stored configuration assembles into.
    fn leg(&self) -> IborLeg {
        let mut leg = IborLeg::new(self.schedule.clone(), Shared::clone(&self.index));
        if let Some(notional) = self.notional {
            leg = leg.with_notional(notional);
        }
        if let Some(day_counter) = &self.payment_day_counter {
            leg = leg.with_payment_day_counter(day_counter.clone());
        }
        if let Some(convention) = self.payment_adjustment {
            leg = leg.with_payment_adjustment(convention);
        }
        if let Some(fixing_days) = self.fixing_days {
            leg = leg.with_fixing_days(fixing_days);
        }
        leg
    }

    /// The coupons a [`CapFloor`](crate::capfloor::PyCapFloor) is built over,
    /// each already carrying the core's default `BlackIborCouponPricer`.
    ///
    /// This is the plain path deliberately. Setting a cap or a floor on the leg
    /// would switch the core to `capped_floored_coupons`, which withholds that
    /// pricer (`iborleg.rs:340-348`), so the strikes belong on the cap/floor
    /// constructors and no `caps`/`floors` setter is exposed here.
    pub(crate) fn coupons(&self) -> PyResult<Vec<Shared<IborCoupon>>> {
        Ok(self.leg().coupons().map_err(PyQlError::from)?)
    }
}
