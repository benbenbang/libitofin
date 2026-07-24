//! Facades for the plain-vanilla swap stack: [`PySwapType`] and
//! [`PyVanillaSwap`].
//!
//! [`PyVanillaSwap`] wraps a `SharedMut<FixedVsFloatingSwap>` (the shape a
//! swaption underlying needs, X3) and is priced with a [`DiscountingSwapEngine`]
//! attached through [`set_engine`](PyVanillaSwap::set_engine), so the facade
//! pins a real number rather than a construction-only object.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyEuribor;
use crate::settings::PySettings;
use crate::time::{PyDayCounter, PySchedule};
use libitofin::instrument::Instrument;
use libitofin::instruments::{FixedVsFloatingSwap, SwapType, VanillaSwap};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{SharedMut, shared_mut};
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
    fn inner(&self) -> SwapType {
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
        ibor_index: &PyEuribor,
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
}
