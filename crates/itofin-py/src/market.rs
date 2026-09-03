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

/// A mutable, observable market element (D1).
///
/// Wraps a single value that pricing inputs observe; setting a new value
/// notifies dependents so any cached valuation recomputes lazily.
#[pyclass(name = "SimpleQuote", unsendable)]
pub struct PySimpleQuote {
    inner: Shared<SimpleQuote>,
}

#[pymethods]
impl PySimpleQuote {
    /// Initialize the quote.
    ///
    /// Args:
    ///     value (float): The initial market value.
    #[new]
    fn new(value: f64) -> Self {
        PySimpleQuote {
            inner: shared(SimpleQuote::new(value)),
        }
    }

    /// Return the current value.
    ///
    /// Returns:
    ///     float: The quote's current market value.
    fn value(&self) -> PyResult<f64> {
        Ok(self.inner.value().map_err(PyQlError::from)?)
    }

    /// Set a new value and notify observers.
    ///
    /// Args:
    ///     value (float): The new value; observers are notified when it actually
    ///         changes, so dependent valuations recompute on next access.
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

/// A generalized Black-Scholes process, built from scalars or curve objects.
///
/// The Handle plumbing is assembled internally, so no handle crosses the
/// binding boundary. The constructor takes the conventional
/// (risk_free_rate, dividend_yield) order and places the two curves in the
/// core's own order at a single call site.
#[pyclass(name = "BlackScholesProcess", unsendable)]
pub struct PyBlackScholesProcess {
    inner: Shared<GeneralizedBlackScholesProcess>,
}

#[pymethods]
impl PyBlackScholesProcess {
    /// Build a flat-market process from scalar inputs.
    ///
    /// Args:
    ///     spot (float): The spot level, held as a quote.
    ///     risk_free_rate (float): The flat risk-free rate, made into a curve
    ///         compounded continuously on an annual frequency.
    ///     dividend_yield (float): The flat dividend yield, made into a curve on the
    ///         same convention as the risk-free rate.
    ///     volatility (float): The flat Black volatility.
    ///     reference_date (Date): The date the three flat curves are anchored on.
    ///     day_counter (DayCounter): The day count the curves accrue on.
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

    /// Build a process from term-structure objects instead of scalars.
    ///
    /// The three legs are bound by name and placed in the core's order at a
    /// single call site, the same risk-free/dividend argument-order footgun the
    /// scalar constructor guards against.
    ///
    /// Args:
    ///     spot (float): The spot level, held as a quote.
    ///     risk_free (YieldTermStructure): The risk-free discount curve.
    ///     dividend (YieldTermStructure): The dividend curve.
    ///     vol (BlackVolTermStructure): The Black volatility surface.
    ///
    /// Returns:
    ///     BlackScholesProcess: A process over the three supplied term structures.
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

    /// Return the risk-free rate carried by the process.
    ///
    /// Returns:
    ///     float: The continuously compounded zero rate on the risk-free curve at the
    ///     reference date.
    fn risk_free_rate(&self) -> PyResult<f64> {
        Ok(zero_rate(&self.inner.risk_free_rate()).map_err(PyQlError::from)?)
    }

    /// Return the dividend yield carried by the process.
    ///
    /// Returns:
    ///     float: The continuously compounded zero rate on the dividend curve at the
    ///     reference date.
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
