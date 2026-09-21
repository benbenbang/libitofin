//! Concrete Bermudan exercise and Hull-White lattice engine facades.

use crate::PyQlError;
use crate::hullwhite::PyHullWhite;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::exercise::{BermudanExercise, Exercise};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::swaption::TreeSwaptionEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// A copied, sorted set of exercise dates. Duplicate dates are retained.
#[gen_stub_pyclass]
#[pyclass(name = "BermudanExercise", unsendable, module = "itofin.instruments")]
pub struct PyBermudanExercise {
    inner: Shared<dyn Exercise>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBermudanExercise {
    /// Build an exercise schedule; an empty list raises ItofinError.
    #[new]
    fn new(dates: Vec<PyRef<'_, PyDate>>) -> PyResult<Self> {
        Ok(Self {
            inner: shared(
                BermudanExercise::new(dates.iter().map(|date| date.inner()).collect(), false)
                    .map_err(PyQlError::from)?,
            ) as Shared<dyn Exercise>,
        })
    }

    /// Return a copy of the sorted exercise dates.
    fn dates(&self) -> Vec<PyDate> {
        self.inner
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }
}

impl PyBermudanExercise {
    pub(crate) fn inner(&self) -> Shared<dyn Exercise> {
        Shared::clone(&self.inner)
    }
}

/// A Hull-White tree engine with a positive number of time steps.
#[gen_stub_pyclass]
#[pyclass(
    name = "TreeSwaptionEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyTreeSwaptionEngine {
    inner: SharedMut<TreeSwaptionEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyTreeSwaptionEngine {
    /// Retain the model and settings; zero steps raise ItofinError.
    #[new]
    fn new(model: &PyHullWhite, time_steps: usize, settings: &PySettings) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                TreeSwaptionEngine::new(model.inner(), time_steps, settings.inner())
                    .map_err(PyQlError::from)?,
            ),
        })
    }
}

impl PyTreeSwaptionEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
