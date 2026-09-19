//! European swaptions on vanilla or overnight swaps and the vanilla MakeSwaption builder.

use crate::PyQlError;
use crate::hullwhite::PyHullWhite;
use crate::ois::PyOvernightIndexedSwap;
use crate::results::Results;
use crate::settings::PySettings;
use crate::swap::PyVanillaSwap;
use crate::swaptionengine::{PyBachelierSwaptionEngine, PyBlackSwaptionEngine};
use crate::time::PyDate;
use libitofin::exercise::{EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{SettlementMethod, SettlementType, Swaption};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// A single-date exercise schedule.
///
/// Held as the exercise trait object the swaption constructor takes, so the
/// same value reaches the instrument.
#[gen_stub_pyclass]
#[pyclass(name = "EuropeanExercise", unsendable, module = "itofin.instruments")]
pub struct PyEuropeanExercise {
    inner: Shared<dyn Exercise>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEuropeanExercise {
    /// Build the exercise schedule.
    ///
    /// Args:
    ///     date (Date): The single date the option may be exercised on.
    #[new]
    fn new(date: &PyDate) -> Self {
        PyEuropeanExercise {
            inner: shared(EuropeanExercise::new(date.inner())) as Shared<dyn Exercise>,
        }
    }
}

impl PyEuropeanExercise {
    /// A clone of the inner exercise for the swaption facade, which takes the
    /// exercise type-erased.
    pub(crate) fn inner(&self) -> Shared<dyn Exercise> {
        Shared::clone(&self.inner)
    }
}

/// How a swaption settles on exercise.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "SettlementType",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySettlementType {
    Physical,
    Cash,
}

impl PySettlementType {
    /// The core SettlementType this variant stands for.
    pub(crate) fn inner(&self) -> SettlementType {
        match self {
            PySettlementType::Physical => SettlementType::Physical,
            PySettlementType::Cash => SettlementType::Cash,
        }
    }
}

/// The settlement mechanics under a settlement type.
///
/// Physical pairs with PhysicalOTC or PhysicalCleared, cash with
/// CollateralizedCashPrice or ParYieldCurve. The consistency check runs at
/// pricing time, not construction, so a mismatched pair only surfaces from
/// npv().
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "SettlementMethod",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySettlementMethod {
    PhysicalOTC,
    PhysicalCleared,
    CollateralizedCashPrice,
    ParYieldCurve,
}

impl PySettlementMethod {
    /// The core SettlementMethod this variant stands for.
    pub(crate) fn inner(&self) -> SettlementMethod {
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

/// A European option to enter a vanilla or overnight swap.
///
/// The swaption registers with the underlying swap and with the evaluation
/// date on the Settings it was built with (D5). Pricing needs an engine: call
/// one of the three setters before npv.
#[gen_stub_pyclass]
#[pyclass(name = "Swaption", unsendable, module = "itofin.instruments")]
pub struct PySwaption {
    inner: Swaption,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySwaption {
    /// Build the swaption over swap.
    ///
    /// Args:
    ///     swap (VanillaSwap): The swap the option enters; it needs no
    ///         discounting engine of its own, the swaption engine reading its
    ///         arguments instead.
    ///     exercise (EuropeanExercise): The single exercise date.
    ///     settlement_type (SettlementType): Whether exercise settles
    ///         physically or in cash.
    ///     settlement_method (SettlementMethod): The mechanics under that
    ///         type; an inconsistent pair surfaces from npv(), not here.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the swaption prices against.
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

    /// Build an option on an overnight swap, retaining the same shared underlying.
    #[staticmethod]
    fn from_ois(
        swap: &PyOvernightIndexedSwap,
        exercise: &PyEuropeanExercise,
        settlement_type: &PySettlementType,
        settlement_method: &PySettlementMethod,
        settings: &PySettings,
    ) -> Self {
        Self {
            inner: Swaption::new(
                swap.inner(),
                exercise.inner(),
                settlement_type.inner(),
                settlement_method.inner(),
                settings.inner(),
            ),
        }
    }

    /// Return the single European exercise date.
    fn exercise_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.exercise().dates()[0])
    }

    /// Return the underlying swap's fixed rate.
    fn underlying_fixed_rate(&self) -> f64 {
        self.inner.underlying().borrow().fixed_rate()
    }

    /// Return the underlying swap's nominal.
    fn underlying_nominal(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .underlying()
            .borrow()
            .nominal()
            .map_err(PyQlError::from)?)
    }

    /// Attach a Jamshidian engine so the swaption prices off Hull-White.
    ///
    /// The engine is European-only: a non-European exercise errors at pricing
    /// time.
    ///
    /// Args:
    ///     model (HullWhite): The short-rate model supplying the dynamics.
    fn set_jamshidian_engine(&mut self, model: &PyHullWhite) {
        let engine = shared_mut(JamshidianSwaptionEngine::new(model.inner()))
            as SharedMut<dyn PricingEngine>;
        self.inner.base_mut().set_pricing_engine(engine);
    }

    /// Attach a Black engine, pricing off a swaption volatility surface.
    ///
    /// The engine is built separately, so the same one can be shared across
    /// swaptions. It must carry the same Settings object as this swaption: two
    /// different settings would price the swap and the option on different
    /// dates with no error raised.
    ///
    /// Args:
    ///     engine (BlackSwaptionEngine): The engine and its volatility
    ///         surface.
    fn set_black_engine(&mut self, engine: &PyBlackSwaptionEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Attach a Bachelier engine, pricing off a normal-volatility surface.
    ///
    /// The same-Settings requirement as set_black_engine applies.
    ///
    /// Args:
    ///     engine (BachelierSwaptionEngine): The engine and its
    ///         normal-volatility surface.
    fn set_bachelier_engine(&mut self, engine: &PyBachelierSwaptionEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the (settlement type, method) pair is inconsistent, which
    ///         the core checks here rather than at construction.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self.inner.calculate().map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }

    /// Attach the Black engine and return the NPV.
    ///
    /// set_black_engine followed by npv, in one call. Black is the primary
    /// because it is the standard swaption engine; the Jamshidian and
    /// Bachelier engines keep their own setters.
    ///
    /// Args:
    ///     engine (BlackSwaptionEngine): The engine to install and price on.
    ///
    /// Returns:
    ///     float: The swaption value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyBlackSwaptionEngine) -> PyResult<f64> {
        self.set_black_engine(engine);
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
        Ok(Results::snapshot(self.inner.base()))
    }

    /// Return the swaption NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached or the (settlement type,
    ///         method) pair is inconsistent.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }
}

impl PySwaption {
    pub(crate) fn from_inner(inner: Swaption) -> Self {
        Self { inner }
    }
}
