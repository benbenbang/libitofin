//! Facade for the Black cap/floor pricing engine: [`PyBlackCapFloorEngine`].
//!
//! The engine prices each optionlet of a [`CapFloor`](crate::capfloor::PyCapFloor)
//! with the Black 1976 formula over an optionlet volatility surface, discounting
//! on a separate yield curve.
//!
//! Both core constructors are exposed. The surface form takes a
//! [`PyOptionletVolatilityStructure`] and an optional displacement; the flat-vol
//! form takes a single quote and wraps it in a moving constant surface with zero
//! settlement days on a null calendar, so that surface's reference date IS the
//! evaluation date carried by `settings`.
//!
//! Deferred (visible): the Bachelier cap/floor engine is not exposed - the core
//! prices only the shifted-lognormal path
//! (`blackcapfloorengine.rs:22-25`), so a normal-volatility surface is rejected
//! by the constructor rather than bound to a second engine here.

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

/// Python `BlackCapFloorEngine`: the shifted-lognormal Black-formula cap/floor
/// engine (`pricingengines::capfloor::BlackCapFloorEngine`).
///
/// The `settings` behind the instrument this engine prices must be the same
/// object the engine resolves its own dates against, or the two disagree on the
/// evaluation date and the NPV is silently wrong.
#[pyclass(name = "BlackCapFloorEngine", unsendable)]
pub struct PyBlackCapFloorEngine {
    inner: SharedMut<BlackCapFloorEngine>,
}

#[pymethods]
impl PyBlackCapFloorEngine {
    /// An engine reading volatilities off `vol` and discounting on `discount`.
    ///
    /// Fallible at construction, unlike the swaption engine: the surface must be
    /// shifted-lognormal, and a `displacement` given here must equal the
    /// surface's own (`blackcapfloorengine.rs:75-82`). `None` adopts the
    /// surface's displacement.
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

    /// An engine over a flat volatility quote, which it wraps in a constant
    /// optionlet surface on a null calendar whose reference date tracks the
    /// evaluation date. `displacement` is the surface's lognormal shift.
    ///
    /// `displacement` carries no default, mirroring
    /// [`BlackSwaptionEngine.with_flat_vol`](crate::swaptionengine::PyBlackSwaptionEngine):
    /// a trailing `settings` cannot follow a defaulted argument.
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

    /// The lognormal shift the engine applies to forwards and strikes.
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
