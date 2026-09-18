//! External quote variables for the global yield bootstrap.

use crate::market::PySimpleQuote;
use crate::{ItofinError, PyQlError};
use libitofin::errors::QlResult;
use libitofin::quotes::SimpleQuote;
use libitofin::shared::Shared;
use libitofin::termstructures::globalbootstrapvars::SimpleQuoteVariables;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

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
