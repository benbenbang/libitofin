//! Facades for the overnight-indexed-swap stack: OvernightIndexedSwap and the
//! MakeOis builder.
//!
//! The OIS analogue of swap(): the builder derives both schedules and the
//! discounting engine from a swap tenor and an overnight index, and
//! MakeOis.build() returns a swap that already carries its
//! DiscountingSwapEngine, so it prices straight away.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::helpers::{PyOvernightIndex, PyRateAveraging};
use crate::results::Results;
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

/// A fixed leg versus a compounded overnight leg.
///
/// Only MakeOis builds one, so it always arrives priced; there is no
/// set_engine and no raw constructor (both deferred with the two-schedule
/// master ctor).
#[pyclass(name = "OvernightIndexedSwap", unsendable)]
pub struct PyOvernightIndexedSwap {
    inner: SharedMut<OvernightIndexedSwap>,
}

#[pymethods]
impl PyOvernightIndexedSwap {
    /// Return the fixed rate that zeroes the swap NPV.
    ///
    /// Returns:
    ///     float: The fair fixed rate, read through the swap's base.
    ///
    /// Raises:
    ///     ItofinError: If the swap has expired or its engine fails to price.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_vs_floating_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set or the engine refuses the
    ///         swap.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Price the swap and return the NPV.
    ///
    /// The only no-argument price(): MakeOis already attached the discounting
    /// engine, so none is left to install.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail, including
    ///         the "null pricing engine" a swap that somehow arrived without
    ///         one reports.
    fn price(&mut self) -> PyResult<f64> {
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// Return the swap NPV under the engine the builder attached.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// Return the notional both legs accrue on.
    ///
    /// Returns:
    ///     float: The single nominal, read through the swap's base.
    ///
    /// Raises:
    ///     ItofinError: If the legs carry per-coupon nominals, which leaves no
    ///         single one to report.
    fn nominal(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow()
            .fixed_vs_floating()
            .nominal()
            .map_err(PyQlError::from)?)
    }

    /// Return the fixed-leg rate.
    ///
    /// Returns:
    ///     float: The rate given to the builder, or the fair rate it filled in
    ///         for a par swap.
    fn fixed_rate(&self) -> f64 {
        self.inner.borrow().fixed_vs_floating().fixed_rate()
    }
}

impl PyOvernightIndexedSwap {
    /// Wraps the swap MakeOis.build() hands back.
    fn from_inner(inner: SharedMut<OvernightIndexedSwap>) -> PyOvernightIndexedSwap {
        PyOvernightIndexedSwap { inner }
    }
}

/// Market-convention builder for an OvernightIndexedSwap.
///
/// Derives the start and end dates, both schedules and the discounting engine
/// from a swap tenor and an overnight index, so the caller states conventions
/// instead of hand-building two schedules. ``fixed_rate=None`` builds a par
/// swap: the fair rate is computed off a temporary swap and written into the
/// fixed leg, so the result prices to a zero NPV.
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the overrides as constructor keywords and
/// assembles the chain inside build(). Only five overrides are exposed; every
/// other core one keeps its default, and the four the core rejects outright
/// (telescopic value dates, lookback, lockout and observation shift) are
/// unreachable from here by construction. The built swap already carries its
/// DiscountingSwapEngine.
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
    /// Store the configuration the chain is assembled from in build().
    ///
    /// Args:
    ///     swap_tenor (Period): The length of the swap.
    ///     overnight_index (OvernightIndex): The index the overnight leg
    ///         compounds.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     fixed_rate (float | None): The rate of the fixed leg; None builds a
    ///         par swap.
    ///     forward_start (Period | None): The delay before the swap starts;
    ///         None starts it spot, at a zero-day period.
    ///     effective_date (Date | None): The start date; None derives it from
    ///         the evaluation date.
    ///     nominal (float | None): The notional; None keeps the core default.
    ///     payment_lag (int | None): The days between accrual end and payment;
    ///         None keeps the core default.
    ///     discounting_term_structure (YieldTermStructure | None): The curve
    ///         the flows discount on; None keeps the core default.
    ///     averaging_method (RateAveraging | None): Whether the overnight
    ///         fixings compound or are averaged; None keeps the core default.
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

    /// Build the priced swap.
    ///
    /// Returns:
    ///     OvernightIndexedSwap: The swap, already carrying its discounting
    ///         engine.
    ///
    /// Raises:
    ///     ItofinError: If effective_date is unset and no evaluation date is
    ///         set to derive the start from; if the schedule or the overnight
    ///         leg is degenerate; or if the par-rate fill fails to price.
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
