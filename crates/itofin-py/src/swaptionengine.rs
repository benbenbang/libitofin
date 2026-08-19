//! Facade for the Black-style swaption pricing engines:
//! [`PyCashAnnuityModel`], [`PyBlackSwaptionEngine`] and
//! [`PyBachelierSwaptionEngine`].
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

/// Python `CashAnnuityModel`: which date a cash-settled par-yield annuity
/// discounts to (`pricingengines::swaption::CashAnnuityModel`).
///
/// A fieldless pyo3 enum. Only the `Cash` / `ParYieldCurve` settlement pair
/// reads it; every other pair takes the fixed-leg BPS annuity and is
/// insensitive to the choice. The facade defaults to `SwapRate`, the branch
/// every ported core test exercises, where C++ defaults to `DiscountCurve`.
#[pyclass(name = "CashAnnuityModel", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCashAnnuityModel {
    SwapRate,
    DiscountCurve,
}

impl PyCashAnnuityModel {
    /// The core [`CashAnnuityModel`] this variant stands for.
    fn inner(&self) -> CashAnnuityModel {
        match self {
            PyCashAnnuityModel::SwapRate => CashAnnuityModel::SwapRate,
            PyCashAnnuityModel::DiscountCurve => CashAnnuityModel::DiscountCurve,
        }
    }
}

/// Python `BlackSwaptionEngine`: the shifted-lognormal Black-formula swaption
/// engine (`pricingengines::swaption::BlackSwaptionEngine`).
///
/// European-only, and it prices the underlying swap itself: it installs its own
/// discounting engine on the swap silently, so the swap needs none of its own.
/// The `settings` handed here must be the same object driving the swaption and
/// its swap, or the evaluation dates disagree and the NPV is silently wrong.
///
/// The surface's volatility type is checked against the Black formula at
/// pricing time, not construction, so a normal-volatility surface surfaces as
/// an `ItofinError` from `Swaption.npv()`.
#[pyclass(name = "BlackSwaptionEngine", unsendable)]
pub struct PyBlackSwaptionEngine {
    inner: SharedMut<BlackSwaptionEngine>,
}

#[pymethods]
impl PyBlackSwaptionEngine {
    /// An engine reading volatilities off `vol` and discounting on `discount`.
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

    /// An engine over a flat volatility quote, which it wraps in a constant
    /// surface on a null calendar whose reference date tracks the evaluation
    /// date. `displacement` is the surface's lognormal shift.
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

/// Python `BachelierSwaptionEngine`: the normal-volatility swaption engine
/// (`pricingengines::swaption::BachelierSwaptionEngine`), the Bachelier spec of
/// the same template [`PyBlackSwaptionEngine`] instantiates.
///
/// European-only, and it prices the underlying swap itself: it installs its own
/// discounting engine on the swap silently, so the swap needs none of its own.
/// The `settings` handed here must be the same object driving the swaption and
/// its swap, or the evaluation dates disagree and the NPV is silently wrong.
///
/// The surface's volatility type is checked against the normal formula at
/// pricing time, not construction, so a shifted-lognormal surface surfaces as
/// an `ItofinError` from `Swaption.npv()`.
#[pyclass(name = "BachelierSwaptionEngine", unsendable)]
pub struct PyBachelierSwaptionEngine {
    inner: SharedMut<BachelierSwaptionEngine>,
}

#[pymethods]
impl PyBachelierSwaptionEngine {
    /// An engine reading normal volatilities off `vol` and discounting on
    /// `discount`.
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

    /// An engine over a flat normal volatility quote, which it wraps in a
    /// constant surface on a null calendar whose reference date tracks the
    /// evaluation date. `displacement` is kept for signature parity with
    /// [`PyBlackSwaptionEngine::with_flat_vol`]; the normal model has no shift
    /// and ignores it.
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
