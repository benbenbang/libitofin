//! Facade for the Black-style swaption pricing engines: CashAnnuityModel,
//! BlackSwaptionEngine and BachelierSwaptionEngine.
//!
//! The core engine is generic over a formula spec, which does not cross FFI
//! (D7), so both spec instantiations - the shifted-lognormal
//! `BlackSwaptionEngine` alias and the normal `BachelierSwaptionEngine` alias -
//! are wrapped as concrete classes here.

use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::swaptionvol::PySwaptionVolatilityStructure;
use crate::time::PyDayCounter;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::swaption::{
    BachelierSwaptionEngine, BlackSwaptionEngine, CashAnnuityModel,
};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;

/// Which date a cash-settled par-yield annuity discounts to.
///
/// Only the (Cash, ParYieldCurve) settlement pair reads it; every other pair
/// takes the fixed-leg BPS annuity and is insensitive to the choice. The
/// swaption engines default to SwapRate, the branch every ported core test
/// exercises, where C++ defaults to DiscountCurve.
#[pyclass(name = "CashAnnuityModel", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCashAnnuityModel {
    SwapRate,
    DiscountCurve,
}

impl PyCashAnnuityModel {
    /// The core CashAnnuityModel this variant stands for.
    fn inner(&self) -> CashAnnuityModel {
        match self {
            PyCashAnnuityModel::SwapRate => CashAnnuityModel::SwapRate,
            PyCashAnnuityModel::DiscountCurve => CashAnnuityModel::DiscountCurve,
        }
    }
}

/// The shifted-lognormal Black-formula swaption engine, European-only.
///
/// It prices the underlying swap itself, so that swap needs no engine of its
/// own. The settings passed here must be the same object driving the swaption
/// and its swap: a mismatch prices the two on different evaluation dates with
/// no error raised. The surface's volatility type is checked against the Black
/// formula at pricing time, not construction, so a normal-volatility surface
/// raises from Swaption.npv().
#[pyclass(name = "BlackSwaptionEngine", unsendable)]
pub struct PyBlackSwaptionEngine {
    inner: SharedMut<BlackSwaptionEngine>,
}

#[pymethods]
impl PyBlackSwaptionEngine {
    /// Build an engine reading volatilities off vol and discounting on discount.
    ///
    /// Args:
    ///     vol (SwaptionVolatilityStructure): The surface volatilities are read
    ///         off.
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         driving the swaption and its swap.
    ///     model (CashAnnuityModel): Which date a cash-settled par-yield
    ///         annuity discounts to. Defaults to SwapRate.
    #[new]
    #[pyo3(signature = (vol, discount, settings, model = PyCashAnnuityModel::SwapRate))]
    fn new(
        vol: &PySwaptionVolatilityStructure,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
        model: PyCashAnnuityModel,
    ) -> Self {
        PyBlackSwaptionEngine {
            inner: shared_mut(BlackSwaptionEngine::new(
                discount.handle(),
                vol.handle(),
                model.inner(),
                settings.inner(),
            )),
        }
    }

    /// Build an engine over a flat volatility quote.
    ///
    /// The quote is wrapped internally in a constant surface on a null calendar
    /// whose reference date tracks the evaluation date.
    ///
    /// Args:
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on.
    ///     vol (SimpleQuote): The flat Black volatility.
    ///     day_counter (DayCounter): The day count the constant surface
    ///         measures time on.
    ///     displacement (float): The constant surface's lognormal shift.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         driving the swaption and its swap.
    ///     model (CashAnnuityModel): Which date a cash-settled par-yield
    ///         annuity discounts to. Defaults to SwapRate.
    ///
    /// Returns:
    ///     BlackSwaptionEngine: The engine over the flat surface.
    #[staticmethod]
    #[pyo3(signature = (discount, vol, day_counter, displacement, settings, model = PyCashAnnuityModel::SwapRate))]
    fn with_flat_vol(
        discount: &PyYieldTermStructure,
        vol: &PySimpleQuote,
        day_counter: &PyDayCounter,
        displacement: f64,
        settings: &PySettings,
        model: PyCashAnnuityModel,
    ) -> Self {
        PyBlackSwaptionEngine {
            inner: shared_mut(BlackSwaptionEngine::with_flat_vol(
                discount.handle(),
                vol.handle(),
                day_counter.inner(),
                displacement,
                model.inner(),
                settings.inner(),
            )),
        }
    }
}

impl PyBlackSwaptionEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

/// The normal-volatility swaption engine, European-only.
///
/// The Bachelier spec of the template BlackSwaptionEngine instantiates: same
/// constructors, same settings requirement, same silent discounting engine on
/// the underlying swap. The surface's volatility type is checked against the
/// normal formula at pricing time, not construction, so a shifted-lognormal
/// surface raises from Swaption.npv().
#[pyclass(name = "BachelierSwaptionEngine", unsendable)]
pub struct PyBachelierSwaptionEngine {
    inner: SharedMut<BachelierSwaptionEngine>,
}

#[pymethods]
impl PyBachelierSwaptionEngine {
    /// Build an engine reading normal volatilities off vol.
    ///
    /// Args:
    ///     vol (SwaptionVolatilityStructure): The surface normal volatilities
    ///         are read off.
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         driving the swaption and its swap.
    ///     model (CashAnnuityModel): Which date a cash-settled par-yield
    ///         annuity discounts to. Defaults to SwapRate.
    #[new]
    #[pyo3(signature = (vol, discount, settings, model = PyCashAnnuityModel::SwapRate))]
    fn new(
        vol: &PySwaptionVolatilityStructure,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
        model: PyCashAnnuityModel,
    ) -> Self {
        PyBachelierSwaptionEngine {
            inner: shared_mut(BachelierSwaptionEngine::new(
                discount.handle(),
                vol.handle(),
                model.inner(),
                settings.inner(),
            )),
        }
    }

    /// Build an engine over a flat normal volatility quote.
    ///
    /// The quote is wrapped internally in a constant surface on a null calendar
    /// whose reference date tracks the evaluation date.
    ///
    /// Args:
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on.
    ///     vol (SimpleQuote): The flat normal volatility.
    ///     day_counter (DayCounter): The day count the constant surface
    ///         measures time on.
    ///     displacement (float): Kept for signature parity with the Black
    ///         engine; the normal model has no shift and ignores it.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         driving the swaption and its swap.
    ///     model (CashAnnuityModel): Which date a cash-settled par-yield
    ///         annuity discounts to. Defaults to SwapRate.
    ///
    /// Returns:
    ///     BachelierSwaptionEngine: The engine over the flat surface.
    #[staticmethod]
    #[pyo3(signature = (discount, vol, day_counter, displacement, settings, model = PyCashAnnuityModel::SwapRate))]
    fn with_flat_vol(
        discount: &PyYieldTermStructure,
        vol: &PySimpleQuote,
        day_counter: &PyDayCounter,
        displacement: f64,
        settings: &PySettings,
        model: PyCashAnnuityModel,
    ) -> Self {
        PyBachelierSwaptionEngine {
            inner: shared_mut(BachelierSwaptionEngine::with_flat_vol(
                discount.handle(),
                vol.handle(),
                day_counter.inner(),
                displacement,
                model.inner(),
                settings.inner(),
            )),
        }
    }
}

impl PyBachelierSwaptionEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
