//! Facades for the plain-vanilla swap stack: [`PySwapType`], [`PyVanillaSwap`]
//! and the [`PyMakeVanillaSwap`] builder.
//!
//! [`PyVanillaSwap`] wraps a `SharedMut<FixedVsFloatingSwap>` (the shape a
//! swaption underlying needs, X3) and is priced with a [`DiscountingSwapEngine`]
//! attached through [`set_engine`](PyVanillaSwap::set_engine), so the facade
//! pins a real number rather than a construction-only object.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::results::Results;
use crate::settings::PySettings;
use crate::time::{PyDate, PyDayCounter, PyPeriod, PySchedule};
use libitofin::indexes::IborIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::{FixedVsFloatingSwap, MakeVanillaSwap, SwapType, VanillaSwap};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::date::Date;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use pyo3::prelude::*;

/// Which side of the named leg the swap is seen from.
///
/// A fieldless enum; the signed leg multiplier the two variants stand for
/// stays in the core.
#[pyclass(name = "SwapType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySwapType {
    Payer,
    Receiver,
}

impl PySwapType {
    /// The core [`SwapType`] this variant stands for.
    pub(crate) fn inner(&self) -> SwapType {
        match self {
            PySwapType::Payer => SwapType::Payer,
            PySwapType::Receiver => SwapType::Receiver,
        }
    }
}

/// A fixed-vs-Ibor interest-rate swap.
///
/// Pricing needs an engine: call set_engine before fair_rate or npv.
#[pyclass(name = "VanillaSwap", unsendable)]
pub struct PyVanillaSwap {
    inner: SharedMut<FixedVsFloatingSwap>,
}

#[pymethods]
impl PyVanillaSwap {
    /// Build the swap from both schedules spelled out.
    ///
    /// Args:
    ///     swap_type (SwapType): Whether the fixed leg is paid or received.
    ///     nominal (float): The notional both legs accrue on.
    ///     fixed_schedule (Schedule): The fixed leg's payment schedule.
    ///     fixed_rate (float): The rate the fixed leg accrues at.
    ///     fixed_day_count (DayCounter): The day count of the fixed leg.
    ///     float_schedule (Schedule): The floating leg's payment schedule.
    ///     ibor_index (IborIndex): The index the floating leg fixes off.
    ///     spread (float): The spread added to every floating fixing.
    ///     floating_day_count (DayCounter): The day count of the floating leg.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If the floating leg cannot be built, a degenerate leg
    ///         being the usual cause.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        swap_type: &PySwapType,
        nominal: f64,
        fixed_schedule: &PySchedule,
        fixed_rate: f64,
        fixed_day_count: &PyDayCounter,
        float_schedule: &PySchedule,
        ibor_index: &PyIborIndex,
        spread: f64,
        floating_day_count: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let swap = VanillaSwap::new(
            swap_type.inner(),
            nominal,
            fixed_schedule.inner(),
            fixed_rate,
            fixed_day_count.inner(),
            float_schedule.inner(),
            ibor_index.inner(),
            spread,
            floating_day_count.inner(),
            None,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyVanillaSwap {
            inner: shared_mut(swap.into_fixed_vs_floating()),
        })
    }

    /// Attach a discounting engine over curve so the swap prices.
    ///
    /// The engine is built with the settings-driven flow defaults, leaving the
    /// settlement date, the NPV date and the settlement-date-flows flag unset.
    ///
    /// Args:
    ///     curve (YieldTermStructure): The curve the flows discount on.
    ///     settings (Settings): The settings the engine resolves its dates
    ///         against.
    fn set_engine(&mut self, curve: &PyYieldTermStructure, settings: &PySettings) {
        let engine = shared_mut(DiscountingSwapEngine::new(
            curve.handle(),
            None,
            None,
            None,
            settings.inner(),
        )) as SharedMut<dyn PricingEngine>;
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine);
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the attached engine refuses the swap.
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

    /// Attach a discounting engine over curve and return the NPV.
    ///
    /// set_engine followed by npv, in one call, and it takes the same two
    /// arguments for the same reason.
    ///
    /// Args:
    ///     curve (YieldTermStructure): The curve the flows discount on.
    ///     settings (Settings): The settings the engine resolves its dates
    ///         against.
    ///
    /// Returns:
    ///     float: The swap value under the freshly built engine.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, curve: &PyYieldTermStructure, settings: &PySettings) -> PyResult<f64> {
        self.set_engine(curve, settings);
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

    /// Return the fixed rate that zeroes the swap NPV.
    ///
    /// Returns:
    ///     float: The fair fixed rate.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached or the swap has expired.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// Return the swap NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// Return the notional both legs accrue on.
    ///
    /// Returns:
    ///     float: The single nominal.
    ///
    /// Raises:
    ///     ItofinError: If the legs carry per-coupon nominals, which leaves no
    ///         single one to report.
    fn nominal(&self) -> PyResult<f64> {
        Ok(self.inner.borrow().nominal().map_err(PyQlError::from)?)
    }

    /// Return the fixed-leg rate.
    ///
    /// Returns:
    ///     float: The rate the fixed leg accrues at.
    fn fixed_rate(&self) -> f64 {
        self.inner.borrow().fixed_rate()
    }
}

impl PyVanillaSwap {
    /// A clone of the inner swap for the swaption facade (X3), which takes the
    /// underlying as a `SharedMut<FixedVsFloatingSwap>`.
    pub(crate) fn inner(&self) -> SharedMut<FixedVsFloatingSwap> {
        SharedMut::clone(&self.inner)
    }

    /// Wraps an already-lowered swap, the shape [`PyMakeVanillaSwap::build`]
    /// hands back.
    fn from_inner(inner: SharedMut<FixedVsFloatingSwap>) -> PyVanillaSwap {
        PyVanillaSwap { inner }
    }
}

/// Market-convention builder for a VanillaSwap.
///
/// Derives the start and end dates, both schedules, the fixed-leg tenor and
/// day count and the discounting engine from a swap tenor and an Ibor index,
/// so the caller states conventions instead of hand-building two schedules.
/// ``fixed_rate=None`` builds a par swap: the fair rate is computed and written
/// into the fixed leg, so the result prices to a zero NPV.
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the overrides as constructor keywords and
/// assembles the chain inside build(). Only four overrides are exposed; every
/// other core one keeps its default, so the discounting curve is always the
/// index's forwarding curve. The built swap already carries its
/// DiscountingSwapEngine.
#[pyclass(name = "MakeVanillaSwap", unsendable)]
pub struct PyMakeVanillaSwap {
    swap_tenor: Period,
    ibor_index: Shared<IborIndex>,
    fixed_rate: Option<f64>,
    forward_start: Period,
    settings: Shared<Settings<Date>>,
    effective_date: Option<Date>,
    nominal: Option<f64>,
    fixed_leg_tenor: Option<Period>,
    fixed_leg_day_count: Option<DayCounter>,
}

#[pymethods]
impl PyMakeVanillaSwap {
    /// Store the configuration the chain is assembled from in build().
    ///
    /// Args:
    ///     swap_tenor (Period): The length of the swap.
    ///     ibor_index (IborIndex): The index the floating leg fixes off, and
    ///         whose forwarding curve discounts.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     fixed_rate (float | None): The rate of the fixed leg; None builds a
    ///         par swap.
    ///     forward_start (Period | None): The delay before the swap starts;
    ///         None starts it spot, at a zero-day period.
    ///     effective_date (Date | None): The start date; None derives it from
    ///         the evaluation date.
    ///     nominal (float | None): The notional; None keeps the core default.
    ///     fixed_leg_tenor (Period | None): The fixed-leg payment tenor; None
    ///         takes the currency's market convention.
    ///     fixed_leg_day_count (DayCounter | None): The fixed-leg day count;
    ///         None takes the currency's market convention.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        swap_tenor,
        ibor_index,
        settings,
        fixed_rate = None,
        forward_start = None,
        effective_date = None,
        nominal = None,
        fixed_leg_tenor = None,
        fixed_leg_day_count = None,
    ))]
    fn new(
        swap_tenor: &PyPeriod,
        ibor_index: &PyIborIndex,
        settings: &PySettings,
        fixed_rate: Option<f64>,
        forward_start: Option<&PyPeriod>,
        effective_date: Option<&PyDate>,
        nominal: Option<f64>,
        fixed_leg_tenor: Option<&PyPeriod>,
        fixed_leg_day_count: Option<&PyDayCounter>,
    ) -> Self {
        PyMakeVanillaSwap {
            swap_tenor: swap_tenor.inner(),
            ibor_index: ibor_index.inner(),
            fixed_rate,
            forward_start: forward_start
                .map(PyPeriod::inner)
                .unwrap_or_else(|| Period::new(0, TimeUnit::Days)),
            settings: settings.inner(),
            effective_date: effective_date.map(PyDate::inner),
            nominal,
            fixed_leg_tenor: fixed_leg_tenor.map(PyPeriod::inner),
            fixed_leg_day_count: fixed_leg_day_count.map(PyDayCounter::inner),
        }
    }

    /// Build the priced swap.
    ///
    /// Returns:
    ///     VanillaSwap: The swap, already carrying its discounting engine.
    ///
    /// Raises:
    ///     ItofinError: If effective_date is unset and no evaluation date is
    ///         set to derive the start from; if the index is neither EUR nor
    ///         USD, the two the fixed-leg defaults are known for; or if the
    ///         par-rate fill fails to price.
    fn build(&self) -> PyResult<PyVanillaSwap> {
        let mut maker = MakeVanillaSwap::new(
            self.swap_tenor,
            Shared::clone(&self.ibor_index),
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
        if let Some(tenor) = self.fixed_leg_tenor {
            maker = maker.with_fixed_leg_tenor(tenor);
        }
        if let Some(day_count) = &self.fixed_leg_day_count {
            maker = maker.with_fixed_leg_day_count(day_count.clone());
        }
        let swap = maker.build().map_err(PyQlError::from)?;
        Ok(PyVanillaSwap::from_inner(shared_mut(
            swap.into_fixed_vs_floating(),
        )))
    }
}
