//! Python callbacks and external variables for the global yield bootstrap.

use crate::market::PySimpleQuote;
use crate::time::PyDate;
use crate::{ItofinError, PyQlError};
use libitofin::errors::{QlError, QlResult};
use libitofin::quotes::SimpleQuote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::globalbootstrap::{FallibleAdditionalDates, GlobalBootstrap};
use libitofin::termstructures::globalbootstrapvars::SimpleQuoteVariables;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::cell::Cell;

thread_local! {
    static CALLBACK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct CallbackGuard;

impl CallbackGuard {
    fn enter() -> Self {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}

impl Drop for CallbackGuard {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(crate) fn ensure_quote_mutation_allowed() -> PyResult<()> {
    if CALLBACK_DEPTH.with(|depth| depth.get() != 0) {
        return Err(ItofinError::new_err(
            "SimpleQuote mutation is not allowed inside bootstrap callbacks",
        ));
    }
    Ok(())
}

/// Mutable quotes solved jointly with the global curve nodes.
///
/// Guesses and bounds may be shorter than quotes. Missing guesses default to
/// zero; missing bounds leave the variable unconstrained. A supplied guess must
/// be strictly above its lower bound. The curve retains the underlying quotes.
#[gen_stub_pyclass]
#[pyclass(
    name = "SimpleQuoteVariables",
    unsendable,
    module = "itofin.termstructures"
)]
pub struct PySimpleQuoteVariables {
    quotes: Vec<Shared<SimpleQuote>>,
    initial_guesses: Vec<f64>,
    lower_bounds: Vec<f64>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySimpleQuoteVariables {
    /// Configure external quotes, optional initial guesses, and lower bounds.
    #[new]
    #[pyo3(signature = (quotes, initial_guesses = None, lower_bounds = None))]
    fn new(
        quotes: Vec<PyRef<PySimpleQuote>>,
        initial_guesses: Option<Vec<f64>>,
        lower_bounds: Option<Vec<f64>>,
    ) -> PyResult<Self> {
        let result = Self {
            quotes: quotes.iter().map(|quote| quote.shared()).collect(),
            initial_guesses: initial_guesses.unwrap_or_default(),
            lower_bounds: lower_bounds.unwrap_or_default(),
        };
        result.build().map_err(PyQlError::from)?;
        for (i, _) in result.quotes.iter().enumerate() {
            let guess = result.initial_guesses.get(i).copied().unwrap_or(0.0);
            if !guess.is_finite()
                || result
                    .lower_bounds
                    .get(i)
                    .is_some_and(|bound| !bound.is_finite() || guess <= *bound)
            {
                return Err(ItofinError::new_err(
                    "initial guesses must be finite and strictly above finite lower bounds",
                ));
            }
        }
        Ok(result)
    }
}

impl PySimpleQuoteVariables {
    pub(crate) fn build(&self) -> QlResult<SimpleQuoteVariables> {
        SimpleQuoteVariables::new(
            self.quotes.clone(),
            self.initial_guesses.clone(),
            self.lower_bounds.clone(),
        )
    }
}

fn callback_error(name: &str, error: PyErr) -> QlError {
    QlError::new(format!("{name} callback failed: {error}"), file!(), line!())
}

pub(crate) fn global_bootstrap(
    py: Python<'_>,
    helpers: Vec<Shared<dyn RateHelper>>,
    penalties: Option<Py<PyAny>>,
    dates: Option<Py<PyAny>>,
    variables: Option<&PySimpleQuoteVariables>,
) -> PyResult<GlobalBootstrap> {
    for (name, callback) in [
        ("additional_penalties", &penalties),
        ("additional_dates", &dates),
    ] {
        if callback
            .as_ref()
            .is_some_and(|callback| !callback.bind(py).is_callable())
        {
            return Err(ItofinError::new_err(format!("{name} must be callable")));
        }
    }
    let residual_count = shared(Cell::new(None));
    let date_count = Shared::clone(&residual_count);
    let dates: Box<FallibleAdditionalDates> = Box::new(move || {
        date_count.set(None);
        Python::attach(|py| {
            let _guard = CallbackGuard::enter();
            let Some(callback) = &dates else {
                return Ok(Vec::new());
            };
            let result = callback
                .bind(py)
                .call0()
                .and_then(|value| value.extract::<Vec<PyRef<PyDate>>>());
            result
                .map(|dates| dates.iter().map(|date| date.inner()).collect())
                .map_err(|error| callback_error("additional_dates", error))
        })
    });
    let mut bootstrap = GlobalBootstrap::with_fallible_penalties(
        helpers,
        Some(dates),
        None,
        None,
        Vec::new(),
        move |times, data| {
            let values = Python::attach(|py| {
                let _guard = CallbackGuard::enter();
                let Some(callback) = &penalties else {
                    return Ok(Vec::new());
                };
                callback
                    .bind(py)
                    .call1((times.to_vec(), data.to_vec()))
                    .and_then(|value| value.extract::<Vec<f64>>())
                    .map_err(|error| callback_error("additional_penalties", error))
            })?;
            libitofin::require!(
                values.iter().all(|value| value.is_finite()),
                "additional_penalties must return finite residuals"
            );
            libitofin::require!(
                residual_count
                    .get()
                    .is_none_or(|count| count == values.len()),
                "additional_penalties residual count changed during calculation"
            );
            residual_count.set(Some(values.len()));
            Ok(values)
        },
    );
    if let Some(variables) = variables {
        bootstrap = bootstrap
            .with_additional_variables(Box::new(variables.build().map_err(PyQlError::from)?));
    }
    Ok(bootstrap)
}
