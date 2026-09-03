//! Facades for the cash-flow slice: the YoYInflationCoupon a year-on-year
//! inflation leg pays, the YoYInflationLeg builder producing it, the floating
//! IborLeg, and the erased CashFlow/Leg pair with the leg-summing npv() (#878).
//!
//! This is the first coupon facade in the crate (#848). Until now the coupons
//! were reachable only *through* the instruments built over them - a
//! YearOnYearInflationSwap or a YoYInflationCapFloor - which covers the pricing
//! path but not a caller who wants the leg itself.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::inflation::{
    PyConstantYoYOptionletVolatility, PyCpiInterpolationType, PyYoYInflationIndex,
};
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::{
    CappedFlooredYoYInflationCoupon, CashFlows, Coupon, IborCoupon, IborLeg, YoYInflationCoupon,
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

/// One coupon of a year-on-year inflation leg.
///
/// Built only through YoYInflationLeg, which attaches the pricer rate() and
/// amount() need.
#[pyclass(name = "YoYInflationCoupon", unsendable)]
pub struct PyYoYInflationCoupon {
    inner: Shared<YoYInflationCoupon>,
}

#[pymethods]
impl PyYoYInflationCoupon {
    /// Return the rate the coupon accrues at: the geared index fixing plus the spread.
    ///
    /// Returns:
    ///     float: The pricer's swaplet rate.
    ///
    /// Raises:
    ///     ItofinError: If no pricer is attached, or resolving the fixing
    ///         fails - a missing history entry, or a forecast off an index with
    ///         no curve linked.
    fn rate(&self) -> PyResult<f64> {
        Ok(Coupon::rate(&*self.inner).map_err(PyQlError::from)?)
    }

    /// Return what the coupon pays on its payment date, undiscounted.
    ///
    /// Returns:
    ///     float: rate() * accrual_period() * nominal().
    ///
    /// Raises:
    ///     ItofinError: As rate().
    fn amount(&self) -> PyResult<f64> {
        Ok(Coupon::amount(&*self.inner).map_err(PyQlError::from)?)
    }

    /// Return the date the observation is published on.
    ///
    /// The reference-period end moved back by the observation lag, then back
    /// the fixing days. This is not the date the rate resolves at: an inflation
    /// index has no fixing calendar, so with no fixing days the roll is inert.
    ///
    /// Returns:
    ///     Date: The publication date.
    fn fixing_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.fixing_date())
    }

    /// Return the year-on-year rate observed, before gearing and spread.
    ///
    /// It lags off accrual_end_date(), not off fixing_date(): a year-on-year
    /// coupon overrides the base rule that reads the index at its fixing date.
    ///
    /// Returns:
    ///     float: The observed rate.
    ///
    /// Raises:
    ///     ItofinError: As rate(), bar the missing pricer.
    fn index_fixing(&self) -> PyResult<f64> {
        Ok(self.inner.index_fixing().map_err(PyQlError::from)?)
    }

    /// Return the nominal the coupon accrues on.
    ///
    /// Returns:
    ///     float: The nominal.
    fn nominal(&self) -> f64 {
        Coupon::nominal(&*self.inner)
    }

    /// Return the start of the accrual period.
    ///
    /// Returns:
    ///     Date: The accrual start.
    fn accrual_start_date(&self) -> PyDate {
        PyDate::from_inner(Coupon::accrual_start_date(&*self.inner))
    }

    /// Return the end of the accrual period, which is also where the observation lags from.
    ///
    /// Returns:
    ///     Date: The accrual end.
    fn accrual_end_date(&self) -> PyDate {
        PyDate::from_inner(Coupon::accrual_end_date(&*self.inner))
    }

    /// Return the whole accrual period as a fraction of a year.
    ///
    /// Measured with day_counter() over the reference period.
    ///
    /// Returns:
    ///     float: The year fraction.
    fn accrual_period(&self) -> f64 {
        Coupon::accrual_period(&*self.inner)
    }

    /// Return the payment date: the accrual end rolled on the leg's payment calendar.
    ///
    /// Returns:
    ///     Date: The payment date.
    fn date(&self) -> PyDate {
        PyDate::from_inner(Event::date(&*self.inner))
    }

    /// Return the day counter the accrual is measured with.
    ///
    /// Returns:
    ///     DayCounter: The coupon day count.
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(Coupon::day_counter(&*self.inner))
    }

    /// Return the multiplicative coefficient applied to the index fixing.
    ///
    /// Returns:
    ///     float: The gearing.
    fn gearing(&self) -> f64 {
        self.inner.gearing()
    }

    /// Return the spread paid over the geared fixing.
    ///
    /// Returns:
    ///     float: The spread.
    fn spread(&self) -> f64 {
        self.inner.spread()
    }

    /// Return how far back the coupon observes the index.
    ///
    /// Returns:
    ///     Period: The observation lag.
    fn observation_lag(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.observation_lag())
    }

    /// Return how the observation interpolates between index fixings.
    ///
    /// Returns:
    ///     CpiInterpolationType: Flat or Linear.
    fn interpolation(&self) -> PyCpiInterpolationType {
        PyCpiInterpolationType::from_inner(self.inner.interpolation())
    }

    /// Return the number of business days the fixing date rolls back by.
    ///
    /// Returns:
    ///     int: The fixing days.
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

    /// The wrapped coupon, which is what the raw YoYInflationCapFloor
    /// constructors take a vector of.
    pub(crate) fn shared(&self) -> Shared<YoYInflationCoupon> {
        Shared::clone(&self.inner)
    }
}

/// Values a capped or floored year-on-year coupon's optionlets off a
/// volatility surface.
///
/// The distribution is chosen by the constructor: black is lognormal,
/// unit_displaced lognormal in 1 + rate and bachelier normal. The settings
/// behind volatility and behind the priced coupons' index must be the same
/// object. nominal_ts is optional: only the discounted price path reads it.
#[pyclass(name = "YoYInflationOptionletCouponPricer", unsendable)]
pub struct PyYoYInflationOptionletCouponPricer {
    inner: SharedMut<YoYInflationOptionletCouponPricer>,
}

#[pymethods]
impl PyYoYInflationOptionletCouponPricer {
    /// Build a pricer valuing optionlets under the lognormal model.
    ///
    /// Args:
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure | None): The discount curve; only the
    ///         discounted price path reads it.
    ///
    /// Returns:
    ///     YoYInflationOptionletCouponPricer: The lognormal pricer.
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

    /// Build a pricer valuing optionlets under the unit-displaced lognormal model.
    ///
    /// Lognormal in 1 + rate, the usual quoting convention for an inflation
    /// rate that may go negative.
    ///
    /// Args:
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure | None): The discount curve; only the
    ///         discounted price path reads it.
    ///
    /// Returns:
    ///     YoYInflationOptionletCouponPricer: The unit-displaced pricer.
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

    /// Build a pricer valuing optionlets under the normal model.
    ///
    /// Args:
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure | None): The discount curve; only the
    ///         discounted price path reads it.
    ///
    /// Returns:
    ///     YoYInflationOptionletCouponPricer: The normal pricer.
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

/// A year-on-year inflation coupon with a cap and/or floor on its rate.
///
/// Built only through YoYInflationLeg.capped_floored_coupons. A negative
/// gearing swaps the two roles, so is_capped and effective_cap answer off the
/// stored level rather than off what the leg was given.
#[pyclass(name = "CappedFlooredYoYInflationCoupon", unsendable)]
pub struct PyCappedFlooredYoYInflationCoupon {
    inner: Shared<CappedFlooredYoYInflationCoupon>,
}

#[pymethods]
impl PyCappedFlooredYoYInflationCoupon {
    /// Return the rate the coupon accrues at.
    ///
    /// The underlying's swaplet rate plus the floorlet, less the caplet.
    ///
    /// Returns:
    ///     float: The capped and floored rate.
    ///
    /// Raises:
    ///     ItofinError: If no pricer is attached, if resolving the fixing
    ///         fails, or if the surface refuses the volatility - a strike
    ///         outside its domain, or an observation before its base date.
    fn rate(&self) -> PyResult<f64> {
        Ok(Coupon::rate(&*self.inner).map_err(PyQlError::from)?)
    }

    /// Return what the coupon pays on its payment date, undiscounted.
    ///
    /// Returns:
    ///     float: rate() * accrual_period() * nominal().
    ///
    /// Raises:
    ///     ItofinError: As rate().
    fn amount(&self) -> PyResult<f64> {
        Ok(Coupon::amount(&*self.inner).map_err(PyQlError::from)?)
    }

    /// Return whether a cap applies.
    ///
    /// Returns:
    ///     bool: True if the stored cap level is set.
    fn is_capped(&self) -> bool {
        self.inner.is_capped()
    }

    /// Return whether a floor applies.
    ///
    /// Returns:
    ///     bool: True if the stored floor level is set.
    fn is_floored(&self) -> bool {
        self.inner.is_floored()
    }

    /// Return the de-spread, de-geared cap the caplet is struck at.
    ///
    /// Read off the stored level, so a negative gearing has already swapped the
    /// two roles.
    ///
    /// Returns:
    ///     float: (cap - spread) / gearing.
    fn effective_cap(&self) -> f64 {
        self.inner.effective_cap()
    }

    /// Return the de-spread, de-geared floor the floorlet is struck at.
    ///
    /// Read off the stored level, so a negative gearing has already swapped the
    /// two roles.
    ///
    /// Returns:
    ///     float: (floor - spread) / gearing.
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

/// Builds a sequence of year-on-year inflation coupons from a schedule.
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the whole configuration up front and
/// assembles the chain inside coupons(). An unset optional leaves the core
/// default in place: a ModifiedFollowing payment roll, no fixing days, a unit
/// gearing and no spread.
///
/// The caps and floors lists select which of the two coupon types the leg
/// produces: given either, coupons() hands back coupons the core deliberately
/// leaves unpriced and capped_floored_coupons is the intended entry.
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
    /// Configure a leg over schedule paying index, observed observation_lag back.
    ///
    /// payment_day_counter is required here although the core takes it through
    /// a setter, so a missing one is a build-time error rather than one raised
    /// from coupons(). A notional is just as required by the core but stays
    /// optional, since the per-coupon notionals list is the other way to supply
    /// it; giving neither surfaces that error from coupons().
    ///
    /// Args:
    ///     schedule (Schedule): The accrual schedule, one coupon per period.
    ///     payment_calendar (Calendar): The calendar payment dates roll on.
    ///     index (YoYInflationIndex): The index the coupons observe.
    ///     observation_lag (Period): How far back each coupon observes it.
    ///     interpolation (CpiInterpolationType): How the observation
    ///         interpolates between index fixings.
    ///     payment_day_counter (DayCounter): The day count the accruals are
    ///         measured with.
    ///     notional (float | None): One nominal for every coupon.
    ///     notionals (list[float] | None): A per-coupon nominal, the
    ///         alternative to notional.
    ///     payment_adjustment (BusinessDayConvention | None): The payment roll;
    ///         the core default is ModifiedFollowing.
    ///     fixing_days (int | None): The business days the fixing date rolls
    ///         back by; the core default is none.
    ///     gearing (float | None): One gearing for every coupon; the core
    ///         default is unit.
    ///     gearings (list[float] | None): A per-coupon gearing.
    ///     spread (float | None): One spread for every coupon; the core default
    ///         is none.
    ///     spreads (list[float] | None): A per-coupon spread.
    ///     caps (list[float] | None): A per-coupon cap level; given either
    ///         list, capped_floored_coupons is the intended entry.
    ///     floors (list[float] | None): A per-coupon floor level.
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

    /// Return the coupons, each carrying the default swaplet pricer.
    ///
    /// Every call rebuilds the leg, so the coupons handed back are fresh
    /// objects each time: bind the list once rather than calling this per read,
    /// or two reads compare different objects. Given caps or floors the coupons
    /// come back unpriced, and capped_floored_coupons is the intended entry.
    ///
    /// Returns:
    ///     list[YoYInflationCoupon]: The freshly built coupons.
    ///
    /// Raises:
    ///     ItofinError: If the leg has no notional, the schedule holds fewer
    ///         than two dates, or there are more notionals, gearings or spreads
    ///         than the schedule has periods.
    fn coupons(&self) -> PyResult<Vec<PyYoYInflationCoupon>> {
        Ok(self
            .leg()
            .coupons()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyYoYInflationCoupon::from_shared)
            .collect())
    }

    /// Return the coupons wrapped in the leg's caps and floors, each carrying pricer.
    ///
    /// The pricer is required rather than optional: the core withholds its
    /// default swaplet pricer from a capped leg, and a swaplet pricer could not
    /// value the optionlets anyway. One pricer is installed across every
    /// coupon. Rebuilt on every call, as coupons() is.
    ///
    /// Args:
    ///     pricer (YoYInflationOptionletCouponPricer): The pricer installed
    ///         across every coupon.
    ///
    /// Returns:
    ///     list[CappedFlooredYoYInflationCoupon]: The freshly built coupons.
    ///
    /// Raises:
    ///     ItofinError: As coupons(), plus more caps or floors than the
    ///         schedule has periods, and a cap sitting below its floor.
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

    /// Return the leg with its coupon type erased, the form npv() sums.
    ///
    /// The plain path erases coupons already carrying the default swaplet
    /// pricer. With a caps or floors list the erased coupons carry NO pricer,
    /// and because every call rebuilds the leg a pricer installed through
    /// capped_floored_coupons() does not reach them: a capped erased leg
    /// reports "pricer not set" from CashFlow.amount(), and the priced capped
    /// path stays capped_floored_coupons(). Rebuilt on every call.
    ///
    /// Returns:
    ///     Leg: The freshly built erased leg.
    ///
    /// Raises:
    ///     ItofinError: As coupons().
    fn build(&self) -> PyResult<PyLeg> {
        Ok(PyLeg {
            inner: self.leg().build().map_err(PyQlError::from)?,
        })
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

/// Builds a sequence of floating ibor coupons from a schedule.
///
/// The setters keep the core's fluent shape: each returns a NEW leg carrying
/// the extra setting, so a leg bound to a name never changes under a later
/// call. An unset optional leaves the core default in place: a Following
/// payment roll and the index's own fixing days and day counter.
///
/// The coupons themselves are not exposed: they are consumed by the raw
/// CapFloor.cap / floor / collar constructors, which is the reason this leg
/// exists. No caps/floors setter is offered either - a capped leg withholds the
/// default coupon pricer in the core, so the strikes belong on the cap/floor
/// constructor.
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
    /// Configure a leg over schedule paying index, on the schedule's own calendar.
    ///
    /// Args:
    ///     schedule (Schedule): The accrual schedule, one coupon per period.
    ///     index (IborIndex): The index the floating coupons fix off.
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

    /// Return the leg with notional on every coupon.
    ///
    /// Required: a leg built without one reports "no notional given" from
    /// coupon_count().
    ///
    /// Args:
    ///     notional (float): The nominal every coupon accrues on.
    ///
    /// Returns:
    ///     IborLeg: A new leg carrying the notional.
    fn with_notional(&self, notional: f64) -> Self {
        PyIborLeg {
            notional: Some(notional),
            ..self.copied()
        }
    }

    /// Return the leg accruing with day_counter, overriding the index's.
    ///
    /// Args:
    ///     day_counter (DayCounter): The day count the accruals are measured
    ///         with.
    ///
    /// Returns:
    ///     IborLeg: A new leg carrying the day counter.
    fn with_payment_day_counter(&self, day_counter: &PyDayCounter) -> Self {
        PyIborLeg {
            payment_day_counter: Some(day_counter.inner()),
            ..self.copied()
        }
    }

    /// Return the leg rolling its payment dates with convention.
    ///
    /// Args:
    ///     convention (BusinessDayConvention): The payment roll, overriding the
    ///         core default of Following.
    ///
    /// Returns:
    ///     IborLeg: A new leg carrying the convention.
    fn with_payment_adjustment(&self, convention: &PyBusinessDayConvention) -> Self {
        PyIborLeg {
            payment_adjustment: Some(convention.inner()),
            ..self.copied()
        }
    }

    /// Return the leg fixing fixing_days business days before each accrual start.
    ///
    /// Overrides the index's own count.
    ///
    /// Args:
    ///     fixing_days (int): The business days each coupon fixes ahead of its
    ///         accrual start.
    ///
    /// Returns:
    ///     IborLeg: A new leg carrying the fixing days.
    fn with_fixing_days(&self, fixing_days: u32) -> Self {
        PyIborLeg {
            fixing_days: Some(fixing_days),
            ..self.copied()
        }
    }

    /// Return the number of coupons the leg builds, one per schedule period.
    ///
    /// The leg is rebuilt on every call, here and in the cap/floor
    /// constructors, so this counts the coupons a construction would produce
    /// rather than a stored list.
    ///
    /// Returns:
    ///     int: The coupon count.
    ///
    /// Raises:
    ///     ItofinError: If the leg has no notional, the schedule holds fewer
    ///         than two dates, or a coupon's own preconditions reject.
    fn coupon_count(&self) -> PyResult<usize> {
        Ok(self.coupons()?.len())
    }
}

impl PyIborLeg {
    /// A copy of the stored configuration, which each setter overrides one
    /// field of. Not a Clone implementation: deriving one on a `pyclass`
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

    /// The coupons a CapFloor is built over, each already carrying the core's
    /// default `BlackIborCouponPricer`.
    ///
    /// This is the plain path deliberately. Setting a cap or a floor on the leg
    /// would switch the core to `capped_floored_coupons`, which withholds that
    /// pricer, so the strikes belong on the cap/floor constructors and no
    /// `caps`/`floors` setter is exposed here.
    pub(crate) fn coupons(&self) -> PyResult<Vec<Shared<IborCoupon>>> {
        Ok(self.leg().coupons().map_err(PyQlError::from)?)
    }
}

/// One erased flow of a Leg, read-only.
///
/// It answers what it pays and when, which is all the leg-summing npv() needs;
/// the concrete coupon accessors stay on the typed coupon wrappers.
#[pyclass(name = "CashFlow", unsendable)]
pub struct PyCashFlow {
    inner: Shared<dyn CashFlow>,
}

#[pymethods]
impl PyCashFlow {
    /// Return what the flow pays on its date, undiscounted.
    ///
    /// Returns:
    ///     float: The undiscounted payment amount.
    ///
    /// Raises:
    ///     ItofinError: On a coupon with no pricer attached, and on whatever
    ///         resolving its fixing reports - a missing history entry, or a
    ///         forecast off an index with no curve linked.
    fn amount(&self) -> PyResult<f64> {
        Ok(self.inner.amount().map_err(PyQlError::from)?)
    }

    /// Return the date the flow pays on.
    ///
    /// Returns:
    ///     Date: The payment date.
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.inner.date())
    }
}

/// A sequence of erased cash flows, built by a leg builder's build().
///
/// Indexable and sized, which with CashFlow's two accessors is enough to
/// hand-check what npv() sums.
#[pyclass(name = "Leg", unsendable)]
pub struct PyLeg {
    inner: Leg,
}

#[pymethods]
impl PyLeg {
    /// Return the number of flows on the leg.
    ///
    /// Returns:
    ///     int: The flow count.
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Return the flow at index, counting from the end when negative.
    ///
    /// Args:
    ///     index (int): The position, negative to count from the end.
    ///
    /// Returns:
    ///     CashFlow: The flow at that position.
    ///
    /// Raises:
    ///     IndexError: If index is out of range.
    fn __getitem__(&self, index: isize) -> PyResult<PyCashFlow> {
        let len = self.inner.len() as isize;
        let resolved = if index < 0 { index + len } else { index };
        if resolved < 0 || resolved >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "leg index {index} out of range for {len} flows"
            )));
        }
        Ok(PyCashFlow {
            inner: Shared::clone(&self.inner[resolved as usize]),
        })
    }
}

/// Return the NPV of leg: every surviving flow discounted on discount_curve.
///
/// Args:
///     leg (Leg): The erased flows to sum.
///     discount_curve (YieldTermStructure): The curve the flows discount on.
///     settings (Settings): The evaluation context deciding which flows have
///         occurred.
///     include_settlement_date_flows (bool | None): Whether a flow paying
///         exactly on the settlement date counts; None defers to the settings'
///         include_todays_cash_flows policy.
///     settlement_date (Date | None): The date deciding which flows have
///         occurred; None uses the evaluation date, which must then be set.
///     npv_date (Date | None): The date the sum is discounted to; None uses
///         settlement_date.
///
/// Returns:
///     float: The discounted sum; exactly 0.0 for an empty leg.
///
/// Raises:
///     ItofinError: On a flow or curve lookup failure, and without a
///         settlement_date when the evaluation date is unset.
#[pyfunction]
#[pyo3(signature = (leg, discount_curve, settings, include_settlement_date_flows = None, settlement_date = None, npv_date = None))]
pub(crate) fn npv(
    leg: &PyLeg,
    discount_curve: &PyYieldTermStructure,
    settings: &PySettings,
    include_settlement_date_flows: Option<bool>,
    settlement_date: Option<&PyDate>,
    npv_date: Option<&PyDate>,
) -> PyResult<f64> {
    let curve = discount_curve
        .handle()
        .current_link()
        .map_err(PyQlError::from)?;
    let settings = settings.inner();
    Ok(CashFlows::npv(
        &leg.inner,
        &*curve,
        &settings,
        include_settlement_date_flows,
        settlement_date.map(PyDate::inner),
        npv_date.map(PyDate::inner),
    )
    .map_err(PyQlError::from)?)
}
