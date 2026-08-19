//! Facades for the overnight-indexed-swap stack: [`PyOvernightIndexedSwap`]
//! and the [`PyMakeOis`] builder.
//!
//! The OIS analogue of [`crate::swap`]: the builder derives both schedules and
//! the discounting engine from a swap tenor and an overnight index, and
//! [`PyMakeOis::build`] returns a swap that already carries its
//! [`DiscountingSwapEngine`] (`makeois.rs:508-516`), so it prices straight
//! away.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::helpers::{PyOvernightIndex, PyRateAveraging};
use crate::settings::PySettings;
use crate::time::{PyDate, PyPeriod};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::OvernightIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::{MakeOis, OvernightIndexedSwap};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::Date;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::{Integer, Real};
use pyo3::prelude::*;

/// Python `OvernightIndexedSwap`: a fixed leg versus a compounded overnight leg
/// (`instruments::overnightindexedswap::OvernightIndexedSwap`).
///
/// Held as the OIS type rather than lowered to its [`FixedVsFloatingSwap`] base
/// (unlike [`crate::swap::PyVanillaSwap`], which a swaption underlying needs in
/// the base shape): nothing here consumes the base, and keeping the OIS type
/// leaves the overnight accessors reachable for a later widening. The base's
/// rate and nominal are read through `fixed_vs_floating`
/// (`overnightindexedswap.rs:235-241`).
///
/// Only [`PyMakeOis`] builds one, so the swap always arrives priced; there is
/// no `set_engine` and no raw constructor. Both are deferred with the
/// two-schedule master ctor (`overnightindexedswap.rs:169`), which needs a
/// [`crate::time::PySchedule`] pair the OIS oracle does not build.
///
/// [`FixedVsFloatingSwap`]: libitofin::instruments::FixedVsFloatingSwap
#[pyclass(name = "OvernightIndexedSwap", unsendable)]
pub struct PyOvernightIndexedSwap {
    inner: SharedMut<OvernightIndexedSwap>,
}

#[pymethods]
impl PyOvernightIndexedSwap {
    /// The fair fixed rate that zeroes the swap NPV (`fairRate()`), read
    /// through the base (`overnightindexedswap.rs:241`).
    ///
    /// Fallible: the swap must be non-expired and its engine must price.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_vs_floating_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// The swap NPV under the engine [`PyMakeOis::build`] attached.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// The swap nominal, read through the base (`nominal()`).
    ///
    /// Fallible: the base errors on a swap whose legs carry per-coupon
    /// nominals, which has no single one to report.
    fn nominal(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow()
            .fixed_vs_floating()
            .nominal()
            .map_err(PyQlError::from)?)
    }

    /// The fixed-leg rate: the rate given to the builder, or the fair rate it
    /// filled in for a `fixed_rate=None` par swap.
    fn fixed_rate(&self) -> f64 {
        self.inner.borrow().fixed_vs_floating().fixed_rate()
    }
}

impl PyOvernightIndexedSwap {
    /// Wraps the swap [`PyMakeOis::build`] hands back.
    fn from_inner(inner: SharedMut<OvernightIndexedSwap>) -> PyOvernightIndexedSwap {
        PyOvernightIndexedSwap { inner }
    }
}

/// Python `MakeOis`: the market-convention builder for a
/// [`PyOvernightIndexedSwap`] (`instruments::makeois::MakeOis`).
///
/// Derives the start and end dates, both schedules and the discounting engine
/// from a swap tenor and an overnight index, so the caller states conventions
/// instead of hand-building two schedules. `fixed_rate=None` builds a par swap:
/// the fair rate is computed off a temporary swap and written into the fixed
/// leg (`makeois.rs:461-469`), so the result prices to a zero NPV.
///
/// The core's `with_*` chain consumes the builder by value, which a
/// `#[pyclass]` cannot hand out, so the overrides are constructor keywords
/// instead and the chain is assembled inside [`build`](Self::build). Only the
/// five the OIS reprice oracle needs are exposed - `effective_date`,
/// `nominal`, `payment_lag`, `discounting_term_structure` and
/// `averaging_method`. Every other core override (the swap type, the overnight
/// spread, the fixed-leg day count, the settlement days, the termination date,
/// the payment frequency and calendar, the schedule conventions and rule, the
/// end-of-month flag) is deferred and keeps its core default; the four the core
/// rejects outright (telescopic value dates, lookback, lockout, observation
/// shift, `makeois.rs:364-387`) are unreachable from here by construction.
#[pyclass(name = "MakeOis", unsendable)]
pub struct PyMakeOis {
    swap_tenor: Period,
    overnight_index: Shared<OvernightIndex>,
    fixed_rate: Option<Real>,
    forward_start: Period,
    settings: Shared<Settings<Date>>,
    effective_date: Option<Date>,
    nominal: Option<Real>,
    payment_lag: Option<Integer>,
    discounting_term_structure: Option<Handle<dyn YieldTermStructure>>,
    averaging_method: Option<RateAveraging>,
}

#[pymethods]
impl PyMakeOis {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        swap_tenor,
        overnight_index,
        settings,
        fixed_rate = None,
        forward_start = None,
        effective_date = None,
        nominal = None,
        payment_lag = None,
        discounting_term_structure = None,
        averaging_method = None,
    ))]
    fn new(
        swap_tenor: &PyPeriod,
        overnight_index: &PyOvernightIndex,
        settings: &PySettings,
        fixed_rate: Option<f64>,
        forward_start: Option<&PyPeriod>,
        effective_date: Option<&PyDate>,
        nominal: Option<f64>,
        payment_lag: Option<Integer>,
        discounting_term_structure: Option<&PyYieldTermStructure>,
        averaging_method: Option<PyRateAveraging>,
    ) -> Self {
        PyMakeOis {
            swap_tenor: swap_tenor.inner(),
            overnight_index: overnight_index.inner(),
            fixed_rate,
            forward_start: forward_start
                .map(PyPeriod::inner)
                .unwrap_or_else(|| Period::new(0, TimeUnit::Days)),
            settings: settings.inner(),
            effective_date: effective_date.map(PyDate::inner),
            nominal,
            payment_lag,
            discounting_term_structure: discounting_term_structure.map(|curve| curve.handle()),
            averaging_method: averaging_method.map(|method| method.inner()),
        }
    }

    /// Builds the priced swap.
    ///
    /// Fallible: without an `effective_date` the start date is derived from the
    /// evaluation date, so an unset one is an error (D10); the schedule and the
    /// overnight leg can be degenerate; and the par-rate fill propagates a
    /// pricing failure.
    fn build(&self) -> PyResult<PyOvernightIndexedSwap> {
        let mut maker = MakeOis::new(
            self.swap_tenor,
            Shared::clone(&self.overnight_index),
            self.fixed_rate,
            self.forward_start,
            Shared::clone(&self.settings),
        );
        if let Some(effective_date) = self.effective_date {
            maker = maker.with_effective_date(effective_date);
        }
        if let Some(nominal) = self.nominal {
            maker = maker.with_nominal(nominal);
        }
        if let Some(payment_lag) = self.payment_lag {
            maker = maker.with_payment_lag(payment_lag);
        }
        if let Some(curve) = &self.discounting_term_structure {
            maker = maker.with_discounting_term_structure(curve.clone());
        }
        if let Some(averaging_method) = self.averaging_method {
            maker = maker.with_averaging_method(averaging_method);
        }
        let swap = maker.build().map_err(PyQlError::from)?;
        Ok(PyOvernightIndexedSwap::from_inner(shared_mut(swap)))
    }
}
