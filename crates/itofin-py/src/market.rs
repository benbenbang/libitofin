//! Facades for the market inputs: [`PySimpleQuote`] and [`PyBlackScholesProcess`].

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::time::{PyDate, PyDayCounter};
use crate::vol::PyBlackVolTermStructure;
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// Python `SimpleQuote`: a mutable, observable market element (D1).
///
/// Wraps a `Shared<SimpleQuote>` so the same interior-mutable quote can be
/// read while observers are notified of a change. The inner `Shared` is
/// `Rc`-based and therefore `!Send`, hence `unsendable`.
#[pyclass(name = "SimpleQuote", unsendable)]
pub struct PySimpleQuote {
    inner: Shared<SimpleQuote>,
}

#[pymethods]
impl PySimpleQuote {
    #[new]
    fn new(value: f64) -> Self {
        PySimpleQuote {
            inner: shared(SimpleQuote::new(value)),
        }
    }

    /// The stored value, erroring when the quote is unset.
    fn value(&self) -> PyResult<f64> {
        Ok(self.inner.value().map_err(PyQlError::from)?)
    }

    /// Sets a new value, notifying observers when it actually changes.
    fn set_value(&self, value: f64) {
        self.inner.set_value(value);
    }
}

impl PySimpleQuote {
    /// A `Handle` wrapping the retained quote, for the rate-helper facades
    /// (#528) whose ctors take `Handle<dyn Quote>`. The handle clones the same
    /// inner `Shared`, so a later `set_value` on this `PySimpleQuote` is
    /// observed by any helper built from it (the laziness contract T5 checks).
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn Quote> {
        Handle::new(Shared::clone(&self.inner) as Shared<dyn Quote>)
    }
}

/// Python `BlackScholesProcess`: a flat-market generalized Black-Scholes
/// process (processes/blackscholesprocess.rs).
///
/// The `Handle<dyn ...>` plumbing is assembled internally from scalar inputs
/// so it never crosses the PyO3 boundary. The Python constructor takes the
/// conventional `(risk_free_rate, dividend_yield, ...)` order; the core's
/// `new` takes `(x0, dividend_yield, risk_free_rate, vol)`, so the two curves
/// are bound by name and placed in the core's order at the single call site.
#[pyclass(name = "BlackScholesProcess", unsendable)]
pub struct PyBlackScholesProcess {
    inner: Shared<GeneralizedBlackScholesProcess>,
}

#[pymethods]
impl PyBlackScholesProcess {
    #[new]
    fn new(
        spot: f64,
        risk_free_rate: f64,
        dividend_yield: f64,
        volatility: f64,
        reference_date: &PyDate,
        day_counter: &PyDayCounter,
    ) -> Self {
        let ref_date = reference_date.inner();
        let dc = day_counter.inner();

        let x0 = Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>);
        let risk_free_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            risk_free_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let dividend_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            dividend_yield,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let vol = Handle::new(
            shared(BlackConstantVol::new(ref_date, None, volatility, dc))
                as Shared<dyn BlackVolTermStructure>,
        );

        PyBlackScholesProcess {
            inner: shared(GeneralizedBlackScholesProcess::new(
                x0,
                dividend_curve,
                risk_free_curve,
                vol,
            )),
        }
    }

    /// Builds a process from term-structure objects instead of scalars: a spot
    /// level plus the risk-free curve, dividend curve, and Black vol surface.
    ///
    /// The three legs are bound by name and placed in the core's
    /// `(x0, dividend, risk_free, vol)` order at the single call site, the same
    /// r/q footgun the scalar constructor guards against.
    #[staticmethod]
    fn from_curves(
        spot: f64,
        risk_free: &PyYieldTermStructure,
        dividend: &PyYieldTermStructure,
        vol: &PyBlackVolTermStructure,
    ) -> Self {
        let x0 = Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>);
        PyBlackScholesProcess {
            inner: shared(GeneralizedBlackScholesProcess::new(
                x0,
                dividend.handle(),
                risk_free.handle(),
                vol.handle(),
            )),
        }
    }

    /// The continuously compounded zero rate carried by the risk-free curve at
    /// the reference date; the pin that the r/q arg-order was not swapped.
    fn risk_free_rate(&self) -> PyResult<f64> {
        Ok(zero_rate(&self.inner.risk_free_rate()).map_err(PyQlError::from)?)
    }

    /// The continuously compounded zero rate carried by the dividend curve at
    /// the reference date; the pin that the r/q arg-order was not swapped.
    fn dividend_yield(&self) -> PyResult<f64> {
        Ok(zero_rate(&self.inner.dividend_yield()).map_err(PyQlError::from)?)
    }
}

impl PyBlackScholesProcess {
    /// Clones the inner `Shared` so the pricing-engine facade (#487) can thread
    /// the same process into an `AnalyticEuropeanEngine`.
    pub(crate) fn inner(&self) -> Shared<GeneralizedBlackScholesProcess> {
        Shared::clone(&self.inner)
    }
}

/// The continuously compounded zero rate at the reference date (`t = 0`),
/// read back with the same convention the flat curve was built with.
fn zero_rate(curve: &Handle<dyn YieldTermStructure>) -> libitofin::errors::QlResult<f64> {
    Ok(curve
        .current_link()?
        .zero_rate(0.0, Compounding::Continuous, Frequency::Annual, true)?
        .rate())
}
