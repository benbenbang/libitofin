//! Facades for the European swaption stack: [`PyEuropeanExercise`],
//! [`PySettlementType`], [`PySettlementMethod`] and [`PySwaption`].
//!
//! [`PySwaption`] wraps a [`Swaption`] by value and prices it through the
//! [`JamshidianSwaptionEngine`] built on a [`PyHullWhite`](crate::hullwhite::PyHullWhite)
//! model (`set_jamshidian_engine`); the engine reads the swap's arguments, so
//! the underlying swap needs no discounting engine of its own.
//!
//! Deferred (visible): the Bermudan `TreeSwaptionEngine` and a `BermudanExercise`
//! facade are omitted. `BermudanExercise` has no public constructor on `main`
//! (the core tree tests build one through a private stub), so there is nothing
//! to wrap; only the European Jamshidian path is exposed here.

use crate::PyQlError;
use crate::hullwhite::PyHullWhite;
use crate::settings::PySettings;
use crate::swap::PyVanillaSwap;
use crate::time::PyDate;
use libitofin::exercise::{EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{SettlementMethod, SettlementType, Swaption};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;

/// Python `EuropeanExercise`: a single-date exercise schedule
/// (`exercise::EuropeanExercise`).
///
/// Wraps a `Shared<dyn Exercise>` so the [`Swaption`] constructor, which takes
/// the exercise as a trait object, can hold the same value.
#[pyclass(name = "EuropeanExercise", unsendable)]
pub struct PyEuropeanExercise {
    inner: Shared<dyn Exercise>,
}

#[pymethods]
impl PyEuropeanExercise {
    #[new]
    fn new(date: &PyDate) -> Self {
        PyEuropeanExercise {
            inner: shared(EuropeanExercise::new(date.inner())) as Shared<dyn Exercise>,
        }
    }
}

impl PyEuropeanExercise {
    /// A clone of the inner exercise for the swaption facade, which takes the
    /// exercise as a `Shared<dyn Exercise>`.
    pub(crate) fn inner(&self) -> Shared<dyn Exercise> {
        Shared::clone(&self.inner)
    }
}

/// Python `SettlementType`: how a swaption settles on exercise
/// (`instruments::swaption::SettlementType`).
///
/// A fieldless pyo3 enum exposing `SettlementType.Physical` / `SettlementType.Cash`.
#[pyclass(name = "SettlementType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySettlementType {
    Physical,
    Cash,
}

impl PySettlementType {
    /// The core [`SettlementType`] this variant stands for.
    fn inner(&self) -> SettlementType {
        match self {
            PySettlementType::Physical => SettlementType::Physical,
            PySettlementType::Cash => SettlementType::Cash,
        }
    }
}

/// Python `SettlementMethod`: the settlement mechanics under a
/// [`SettlementType`] (`instruments::swaption::SettlementMethod`).
///
/// Physical pairs with `PhysicalOTC` / `PhysicalCleared`; cash pairs with
/// `CollateralizedCashPrice` / `ParYieldCurve`. The consistency check runs at
/// pricing time (`SwaptionArguments::validate`), not construction, so a
/// mismatched pair only surfaces as an `ItofinError` from `npv()`.
#[pyclass(name = "SettlementMethod", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySettlementMethod {
    PhysicalOTC,
    PhysicalCleared,
    CollateralizedCashPrice,
    ParYieldCurve,
}

impl PySettlementMethod {
    /// The core [`SettlementMethod`] this variant stands for.
    fn inner(&self) -> SettlementMethod {
        match self {
            PySettlementMethod::PhysicalOTC => SettlementMethod::PhysicalOTC,
            PySettlementMethod::PhysicalCleared => SettlementMethod::PhysicalCleared,
            PySettlementMethod::CollateralizedCashPrice => {
                SettlementMethod::CollateralizedCashPrice
            }
            PySettlementMethod::ParYieldCurve => SettlementMethod::ParYieldCurve,
        }
    }
}

/// Python `Swaption`: a European option to enter a [`VanillaSwap`](PyVanillaSwap)
/// (`instruments::swaption::Swaption`).
///
/// Built with [`Swaption::new`] (infallible): it registers with the underlying
/// swap and the settings evaluation date (D5). Pricing needs an engine: call
/// [`set_jamshidian_engine`](Self::set_jamshidian_engine) before [`npv`](Self::npv).
/// The (settlement type, method) consistency check runs at pricing time, so a
/// mismatched pair surfaces as an `ItofinError` from `npv()`, not the ctor.
#[pyclass(name = "Swaption", unsendable)]
pub struct PySwaption {
    inner: Swaption,
}

#[pymethods]
impl PySwaption {
    #[new]
    fn new(
        swap: &PyVanillaSwap,
        exercise: &PyEuropeanExercise,
        settlement_type: &PySettlementType,
        settlement_method: &PySettlementMethod,
        settings: &PySettings,
    ) -> Self {
        PySwaption {
            inner: Swaption::new(
                swap.inner(),
                exercise.inner(),
                settlement_type.inner(),
                settlement_method.inner(),
                settings.inner(),
            ),
        }
    }

    /// Attaches a [`JamshidianSwaptionEngine`] built on `model` so the swaption
    /// prices analytically off the Hull-White dynamics.
    ///
    /// The engine (`jamshidianswaptionengine.rs:92`, infallible) is European-only:
    /// a non-European exercise errors at pricing time. It is installed on the
    /// swaption's [`InstrumentBase`](libitofin::instrument) via `set_pricing_engine`.
    fn set_jamshidian_engine(&mut self, model: &PyHullWhite) {
        let engine = shared_mut(JamshidianSwaptionEngine::new(model.inner()))
            as SharedMut<dyn PricingEngine>;
        self.inner.base_mut().set_pricing_engine(engine);
    }

    /// The swaption NPV under the attached engine.
    ///
    /// Fallible: an engine must be attached ([`set_jamshidian_engine`](Self::set_jamshidian_engine))
    /// and the (settlement type, method) pair consistent.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }
}
