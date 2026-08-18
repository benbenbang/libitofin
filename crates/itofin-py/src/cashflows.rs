//! Facades for the cash-flow slice: the [`PyYoYInflationCoupon`] a year-on-year
//! inflation leg pays and the [`PyYoYInflationLeg`] builder producing it
//! (`cashflows::yoyinflationcoupon`, `cashflows::yoyinflationleg`).
//!
//! This is the first coupon facade in the crate (#848). Until now the coupons
//! were reachable only *through* the instruments built over them - a
//! [`YearOnYearInflationSwap`](crate::inflation::PyYearOnYearInflationSwap) or a
//! [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor) - which
//! covers the pricing path but not a caller who wants the leg itself.

use crate::PyQlError;
use crate::inflation::{PyCpiInterpolationType, PyYoYInflationIndex};
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use libitofin::cashflows::{Coupon, YoYInflationCoupon, YoYInflationLeg};
use libitofin::event::Event;
use libitofin::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use libitofin::shared::Shared;
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
/// Deferred (visible): `with_caps` / `with_floors` and the
/// `capped_floored_coupons` they select, which need a pricer carrying an
/// optionlet volatility that has no facade here, and the erased `build()` whose
/// `Leg` of `CashFlow`s Python has no wrapper for. The plain swaplet path is
/// what #848 asks for.
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
    fn coupons(&self) -> PyResult<Vec<PyYoYInflationCoupon>> {
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
        Ok(leg
            .coupons()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyYoYInflationCoupon::from_shared)
            .collect())
    }
}
