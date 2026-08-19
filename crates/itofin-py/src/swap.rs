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

/// Python `SwapType`: which side of the named leg the swap is seen from
/// (`instruments::swap::SwapType`, re-exported as `instruments::SwapType`).
///
/// A fieldless pyo3 enum exposing `SwapType.Payer` / `SwapType.Receiver`; the
/// signed `+1`/`-1` leg multiplier stays in the core.
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

/// Python `VanillaSwap`: a fixed-vs-Ibor interest-rate swap
/// (`instruments::vanillaswap::VanillaSwap`).
///
/// Built with [`VanillaSwap::new`] and immediately lowered to its
/// [`FixedVsFloatingSwap`] base via `into_fixed_vs_floating` (the shape X3's
/// swaption consumes), held behind a `SharedMut`. The ctor is fallible
/// (`vanillaswap.rs:88`): it builds the floating [`IborLeg`], so a degenerate
/// leg surfaces as an `ItofinError`. Pricing needs an engine: call
/// [`set_engine`](Self::set_engine) before [`fair_rate`](Self::fair_rate) or
/// [`npv`](Self::npv).
#[pyclass(name = "VanillaSwap", unsendable)]
pub struct PyVanillaSwap {
    inner: SharedMut<FixedVsFloatingSwap>,
}

#[pymethods]
impl PyVanillaSwap {
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

    /// Attaches a [`DiscountingSwapEngine`] over `curve` so the swap prices.
    ///
    /// The engine is built with the settings-driven flow defaults
    /// (`include_settlement_date_flows`, `settlement_date`, `npv_date` all
    /// unset) and installed on the swap's [`InstrumentBase`] via
    /// `set_pricing_engine`.
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

    /// The fair fixed rate that zeroes the swap NPV (`fairRate()`).
    ///
    /// Fallible: an engine must be attached and the swap non-expired.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// The swap NPV under the attached engine.
    ///
    /// Fallible: an engine must be attached (`set_engine`).
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// The swap nominal (`nominal()`).
    fn nominal(&self) -> PyResult<f64> {
        Ok(self.inner.borrow().nominal().map_err(PyQlError::from)?)
    }

    /// The fixed-leg rate (`fixedRate()`).
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

/// Python `MakeVanillaSwap`: the market-convention builder for a
/// [`PyVanillaSwap`] (`instruments::makevanillaswap::MakeVanillaSwap`).
///
/// Derives the start and end dates, both schedules, the fixed-leg tenor and day
/// count and the discounting engine from a swap tenor and an Ibor index, so the
/// caller states conventions instead of hand-building two [`PySchedule`]s.
/// `fixed_rate=None` builds a par swap: the fair rate is computed and written
/// into the fixed leg, so the result prices to a zero NPV
/// (`makevanillaswap.rs:364-376`).
///
/// The core's `with_*` chain consumes the builder by value, which a `#[pyclass]`
/// cannot hand out, so the overrides are constructor keywords instead and the
/// chain is assembled inside [`build`](Self::build). Only the four the pass-A
/// swaption fixture needs are exposed - `effective_date`, `nominal`,
/// `fixed_leg_tenor`, `fixed_leg_day_count`. Every other core override
/// (termination date, settlement days, the leg calendars / conventions /
/// end-of-month flags, the discounting term structure, the indexed-coupon mode)
/// is deferred and keeps its core default; the discounting curve is therefore
/// always the index's forwarding curve.
///
/// [`build`](Self::build) returns a swap that already carries its
/// [`DiscountingSwapEngine`] (`makevanillaswap.rs:514-522`), so it prices
/// straight away. Calling [`PyVanillaSwap::set_engine`] on it replaces that
/// engine with one built on the settings-driven flow defaults.
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

    /// Builds the priced swap.
    ///
    /// Fallible: without an `effective_date` the start date is derived from the
    /// evaluation date, so an unset one is an error (D10); the fixed-leg
    /// defaults are only known for EUR and USD indexes; and the par-rate fill
    /// propagates a pricing failure.
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
