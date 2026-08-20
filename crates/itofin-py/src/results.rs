//! The frozen [`Results`] snapshot the instrument facades hand back.
//!
//! Every instrument caches the outputs of its last valuation in the core
//! `InstrumentResults` bundle (`instrument.rs:36-45`), and the live accessors
//! (`npv()`, the greeks, `fair_rate()`, ...) re-run the lazy calculation
//! whenever an observed input moved. [`Results`] is the opposite: a plain copy
//! of that bundle taken at one moment, which no later input change can move.

use crate::time::PyDate;
use libitofin::instrument::InstrumentBase;
use libitofin::time::date::Date;
use libitofin::types::Real;
use pyo3::prelude::*;
use std::collections::BTreeMap;

/// Python `Results`: a read-only snapshot of one instrument valuation
/// (core `instrument::InstrumentResults`).
///
/// Handed back by every facade's `results()`, which forces the calculation and
/// then copies the four fields out. The object holds no borrow on the
/// instrument and no handle to its inputs, so it keeps reporting the valuation
/// it was taken from even after the instrument reprices.
///
/// Every field is optional because the core stores it that way: an engine
/// fills what it computes and leaves the rest unset. The analytic European
/// engine, for instance, provides a value but neither an error estimate nor a
/// valuation date.
///
/// `additional_results` is REAL-ONLY. The core keeps the engine's extra outputs
/// as `Shared<dyn Any>` and the only sanctioned downcast in this crate is to
/// `Real` (`option.rs`'s `exercise_probability`), so a tag holding anything
/// else is omitted from the dict rather than guessed at.
#[pyclass(name = "Results", frozen)]
pub struct Results {
    npv: Option<Real>,
    error_estimate: Option<Real>,
    valuation_date: Option<Date>,
    additional_results: BTreeMap<String, Real>,
}

#[pymethods]
impl Results {
    /// The net present value, or `None` when the engine provided none.
    #[getter]
    fn npv(&self) -> Option<f64> {
        self.npv
    }

    /// The standard error on the value, or `None` on the engines that do not
    /// produce one - every analytic engine here.
    #[getter]
    fn error_estimate(&self) -> Option<f64> {
        self.error_estimate
    }

    /// The date the value refers to, or `None` when the engine did not say.
    #[getter]
    fn valuation_date(&self) -> Option<PyDate> {
        self.valuation_date.map(PyDate::from_inner)
    }

    /// The engine's extra named outputs, restricted to the real-valued tags;
    /// see the class docs.
    #[getter]
    fn additional_results(&self) -> BTreeMap<String, f64> {
        self.additional_results.clone()
    }
}

impl Results {
    /// Copies the instrument's currently cached results out of `base`.
    ///
    /// The caller forces the calculation first: this reads the cache, it does
    /// not fill it.
    pub(crate) fn snapshot(base: &InstrumentBase) -> Results {
        let results = base.results();
        Results {
            npv: results.value,
            error_estimate: results.error_estimate,
            valuation_date: results.valuation_date,
            additional_results: results
                .additional_results
                .iter()
                .filter_map(|(tag, value)| {
                    value
                        .as_ref()
                        .downcast_ref::<Real>()
                        .map(|value| (tag.clone(), *value))
                })
                .collect(),
        }
    }
}
