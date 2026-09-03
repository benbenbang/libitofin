//! Facade for the Black cap/floor pricing engine: BlackCapFloorEngine.
//!
//! The engine prices each optionlet of a CapFloor with the Black 1976 formula
//! over an optionlet volatility surface, discounting on a separate yield curve.
//!
//! Both core constructors are exposed. The surface form takes a
//! OptionletVolatilityStructure and an optional displacement; the flat-vol form
//! takes a single quote and wraps it in a moving constant surface with zero
//! settlement days on a null calendar, so that surface's reference date IS the
//! evaluation date carried by `settings`.
//!
//! Deferred (visible): the Bachelier cap/floor engine is not exposed - the core
//! prices only the shifted-lognormal path, so a normal-volatility surface is
//! rejected by the constructor rather than bound to a second engine here.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::optionletvol::PyOptionletVolatilityStructure;
use crate::settings::PySettings;
use crate::time::PyDayCounter;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::capfloor::BlackCapFloorEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;

/// The shifted-lognormal Black-formula cap/floor engine, one Black 1976
/// optionlet per coupon.
///
/// Only the shifted-lognormal path is priced in the core, so a normal-volatility
/// surface is rejected by the constructor rather than bound to a Bachelier
/// engine. The instrument this engine prices must resolve its dates against the
/// same Settings object the engine does.
#[pyclass(name = "BlackCapFloorEngine", unsendable)]
pub struct PyBlackCapFloorEngine {
    inner: SharedMut<BlackCapFloorEngine>,
}

#[pymethods]
impl PyBlackCapFloorEngine {
    /// Build an engine reading volatilities off vol and discounting on discount.
    ///
    /// Fallible at construction, unlike the swaption engines.
    ///
    /// Args:
    ///     vol (OptionletVolatilityStructure): The optionlet surface
    ///         volatilities are read off; must be shifted-lognormal.
    ///     discount (YieldTermStructure): The curve the optionlets are
    ///         discounted on.
    ///     displacement (float | None): The lognormal shift; None adopts the
    ///         surface's own.
    ///
    /// Raises:
    ///     ItofinError: If the surface handle is empty, the surface is
    ///         normal-volatility, or a given displacement differs from the
    ///         surface's own.
    #[new]
    #[pyo3(signature = (vol, discount, displacement = None))]
    fn new(
        vol: &PyOptionletVolatilityStructure,
        discount: &PyYieldTermStructure,
        displacement: Option<f64>,
    ) -> PyResult<Self> {
        Ok(PyBlackCapFloorEngine {
            inner: shared_mut(
                BlackCapFloorEngine::new(discount.handle(), vol.handle(), displacement)
                    .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Build an engine over a flat volatility quote.
    ///
    /// The quote is wrapped internally in a constant optionlet surface on a
    /// null calendar whose reference date tracks the evaluation date.
    /// displacement carries no default, mirroring the swaption engine: a
    /// trailing settings cannot follow a defaulted argument.
    ///
    /// Args:
    ///     discount (YieldTermStructure): The curve the optionlets are
    ///         discounted on.
    ///     vol (SimpleQuote): The flat Black volatility.
    ///     day_counter (DayCounter): The day count the constant surface
    ///         measures time on.
    ///     displacement (float): The constant surface's lognormal shift.
    ///     settings (Settings): The explicit settings the constant surface's
    ///         reference date tracks.
    ///
    /// Returns:
    ///     BlackCapFloorEngine: The engine over the flat surface.
    #[staticmethod]
    fn with_flat_vol(
        discount: &PyYieldTermStructure,
        vol: &PySimpleQuote,
        day_counter: &PyDayCounter,
        displacement: f64,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyBlackCapFloorEngine {
            inner: shared_mut(
                BlackCapFloorEngine::with_flat_vol(
                    discount.handle(),
                    vol.handle(),
                    day_counter.inner(),
                    displacement,
                    settings.inner(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Return the lognormal shift the engine applies to forwards and strikes.
    ///
    /// Returns:
    ///     float: The displacement.
    fn displacement(&self) -> f64 {
        self.inner.borrow().displacement()
    }
}

impl PyBlackCapFloorEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
