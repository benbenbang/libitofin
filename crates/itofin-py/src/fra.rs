//! Facades for the forward rate agreement: [`PyPosition`] and
//! [`PyForwardRateAgreement`].
//!
//! The FRA prices without an engine (`forwardrateagreement.rs`), so the facade
//! exposes the valuation accessors directly; no `set_engine` step exists. The
//! core keeps its value and maturity dates private with no accessors, so the
//! facade stores both at construction - they are immutable on the core
//! instrument too - with the maturity adjusted through the instrument's own
//! calendar and convention, the same computation the core constructor runs.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::time::PyDate;
use libitofin::handle::Handle;
use libitofin::indexes::InterestRateIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::ForwardRateAgreement;
use libitofin::position::Position;
use libitofin::shared::Shared;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::Date;
use pyo3::prelude::*;

/// Python `Position`: the side taken in a contract (core `position::Position`).
///
/// A fieldless pyo3 enum exposing `Position.Long` / `Position.Short`; the
/// signed `+1`/`-1` settlement multiplier stays in the core.
#[pyclass(name = "Position", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPosition {
    Long,
    Short,
}

impl PyPosition {
    /// The core [`Position`] this variant stands for.
    pub(crate) fn inner(self) -> Position {
        match self {
            PyPosition::Long => Position::Long,
            PyPosition::Short => Position::Short,
        }
    }
}

/// Python `ForwardRateAgreement`: a FRA over an Ibor index
/// (`instruments::forwardrateagreement::ForwardRateAgreement`).
///
/// The default constructor is the indexed-coupon one (`useIndexedCoupon =
/// true`): the maturity is the index's own maturity of the value date and the
/// forward rate is the index fixing. `with_maturity` is the explicit-window
/// constructor, forward-rated by the par approximation off the index's
/// forwarding curve. Passing `None` for the discount curve discounts on the
/// forwarding curve instead (`forwardrateagreement.rs:311`).
#[pyclass(name = "ForwardRateAgreement", unsendable)]
pub struct PyForwardRateAgreement {
    inner: ForwardRateAgreement,
    value_date: Date,
    maturity_date: Date,
}

#[pymethods]
impl PyForwardRateAgreement {
    #[new]
    #[pyo3(signature = (index, value_date, fra_type, strike_forward_rate, notional_amount, discount_curve))]
    fn new(
        index: &PyIborIndex,
        value_date: &PyDate,
        fra_type: &PyPosition,
        strike_forward_rate: f64,
        notional_amount: f64,
        discount_curve: Option<&PyYieldTermStructure>,
    ) -> PyResult<Self> {
        let index = index.inner();
        let maturity = index
            .maturity_date(value_date.inner())
            .map_err(PyQlError::from)?;
        let fra = ForwardRateAgreement::new(
            Shared::clone(&index),
            value_date.inner(),
            fra_type.inner(),
            strike_forward_rate,
            notional_amount,
            curve_or_empty(discount_curve),
        )
        .map_err(PyQlError::from)?;
        let maturity = fra
            .calendar()
            .adjust(maturity, fra.business_day_convention());
        Ok(PyForwardRateAgreement {
            inner: fra,
            value_date: value_date.inner(),
            maturity_date: maturity,
        })
    }

    /// The explicit-window constructor (`with_maturity`,
    /// `forwardrateagreement.rs:120`, `useIndexedCoupon = false`): the FRA
    /// spans `[value_date, maturity_date]` with the maturity adjusted on the
    /// index's fixing calendar under the index's convention.
    ///
    /// Fallible: the notional must be positive and the value date earlier than
    /// the adjusted maturity date.
    #[staticmethod]
    #[pyo3(signature = (index, value_date, maturity_date, fra_type, strike_forward_rate, notional_amount, discount_curve))]
    #[allow(clippy::too_many_arguments)]
    fn with_maturity(
        index: &PyIborIndex,
        value_date: &PyDate,
        maturity_date: &PyDate,
        fra_type: &PyPosition,
        strike_forward_rate: f64,
        notional_amount: f64,
        discount_curve: Option<&PyYieldTermStructure>,
    ) -> PyResult<Self> {
        let fra = ForwardRateAgreement::with_maturity(
            index.inner(),
            value_date.inner(),
            maturity_date.inner(),
            fra_type.inner(),
            strike_forward_rate,
            notional_amount,
            curve_or_empty(discount_curve),
        )
        .map_err(PyQlError::from)?;
        let maturity = fra
            .calendar()
            .adjust(maturity_date.inner(), fra.business_day_convention());
        Ok(PyForwardRateAgreement {
            inner: fra,
            value_date: value_date.inner(),
            maturity_date: maturity,
        })
    }

    /// The relevant forward rate associated with the FRA term (`forwardRate`),
    /// as a simple Simple/Once rate.
    ///
    /// Fallible: the index needs a forwarding curve (or a fixing) covering the
    /// term.
    fn forward_rate(&mut self) -> PyResult<f64> {
        Ok(self.inner.forward_rate().map_err(PyQlError::from)?.rate())
    }

    /// The payoff on the value date (`amount`).
    ///
    /// Fallible: an expired FRA has no settlement amount (the core surfaces
    /// the C++ undefined read as an error, D10).
    fn amount(&mut self) -> PyResult<f64> {
        Ok(self.inner.amount().map_err(PyQlError::from)?)
    }

    /// The NPV: the settlement amount discounted to the value date on the
    /// discount curve, or on the forwarding curve when none was given.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }

    /// The value date: the day the underlying loan begins, on which the FRA
    /// settles and expires.
    fn value_date(&self) -> PyDate {
        PyDate::from_inner(self.value_date)
    }

    /// The maturity date of the underlying loan, adjusted on the index's
    /// fixing calendar under the index's convention.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.maturity_date)
    }
}

/// The discount handle for the core ctors: the curve's own handle, or the
/// empty handle whose fallback is the index's forwarding curve.
fn curve_or_empty(curve: Option<&PyYieldTermStructure>) -> Handle<dyn YieldTermStructure> {
    match curve {
        Some(curve) => curve.handle(),
        None => Handle::empty(),
    }
}
