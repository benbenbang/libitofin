//! Facades for the cash-flow slice: the [`PyYoYInflationCoupon`] a year-on-year
//! inflation leg pays (`cashflows::yoyinflationcoupon`).
//!
//! This is the first coupon facade in the crate (#848). Until now the coupons
//! were reachable only *through* the instruments built over them - a
//! [`YearOnYearInflationSwap`](crate::inflation::PyYearOnYearInflationSwap) or a
//! [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor) - which
//! covers the pricing path but not a caller who wants the leg itself.

use crate::PyQlError;
use crate::inflation::PyCpiInterpolationType;
use crate::time::{PyDate, PyDayCounter, PyPeriod};
use libitofin::cashflows::{Coupon, YoYInflationCoupon};
use libitofin::event::Event;
use libitofin::shared::Shared;
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
    #[allow(dead_code)]
    pub(crate) fn from_shared(inner: Shared<YoYInflationCoupon>) -> Self {
        PyYoYInflationCoupon { inner }
    }
}
