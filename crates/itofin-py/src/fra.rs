//! Facades for the forward rate agreement: Position and ForwardRateAgreement.
//!
//! The FRA prices without an engine, so the facade exposes the valuation
//! accessors directly; no `set_engine` step exists. The core keeps its value
//! and maturity dates private with no accessors, so the facade stores both at
//! construction - they are immutable on the core instrument too - with the
//! maturity adjusted through the instrument's own calendar and convention, the
//! same computation the core constructor runs.

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

/// The side taken in a contract.
///
/// A fieldless enum; Long is an FRA purchase (a future long loan, short
/// deposit), Short an FRA sale. The signed settlement multiplier the two
/// variants stand for stays in the core.
#[pyclass(name = "Position", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPosition {
    Long,
    Short,
}

impl PyPosition {
    /// The core Position this variant stands for.
    pub(crate) fn inner(self) -> Position {
        match self {
            PyPosition::Long => Position::Long,
            PyPosition::Short => Position::Short,
        }
    }
}

/// A forward rate agreement over an Ibor index.
///
/// The FRA prices without an engine, so the valuation accessors work as soon
/// as it is built. It settles and expires on its value date - the day the
/// underlying loan begins - not on the later maturity date.
///
/// Passing None for the discount curve discounts on the forwarding curve
/// instead.
#[pyclass(name = "ForwardRateAgreement", unsendable)]
pub struct PyForwardRateAgreement {
    inner: ForwardRateAgreement,
    value_date: Date,
    maturity_date: Date,
}

#[pymethods]
impl PyForwardRateAgreement {
    /// Build the indexed-coupon FRA.
    ///
    /// The maturity is the index's own maturity of the value date and the
    /// forward rate is the index fixing.
    ///
    /// Args:
    ///     index (IborIndex): The index the forward rate is forecast by.
    ///     value_date (Date): The day the underlying loan begins.
    ///     fra_type (Position): The side taken, Long or Short.
    ///     strike_forward_rate (float): The simple rate agreed on.
    ///     notional_amount (float): The notional the settlement accrues on.
    ///     discount_curve (YieldTermStructure | None): The curve the
    ///         settlement discounts on; None discounts on the index's
    ///         forwarding curve instead.
    ///
    /// Raises:
    ///     ItofinError: If the notional is not positive or the maturity
    ///         cannot be derived from the value date.
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

    /// Build the FRA over an explicit [value_date, maturity_date] window.
    ///
    /// The forward rate is the par approximation off the index's forwarding
    /// curve; the maturity is adjusted on the index's fixing calendar under
    /// the index's convention.
    ///
    /// Args:
    ///     index (IborIndex): The index supplying the forwarding curve and
    ///         the conventions.
    ///     value_date (Date): The day the underlying loan begins.
    ///     maturity_date (Date): The day the underlying loan ends, before
    ///         adjustment.
    ///     fra_type (Position): The side taken, Long or Short.
    ///     strike_forward_rate (float): The simple rate agreed on.
    ///     notional_amount (float): The notional the settlement accrues on.
    ///     discount_curve (YieldTermStructure | None): The curve the
    ///         settlement discounts on; None discounts on the index's
    ///         forwarding curve instead.
    ///
    /// Returns:
    ///     ForwardRateAgreement: The explicit-window FRA.
    ///
    /// Raises:
    ///     ItofinError: If the notional is not positive or the value date is
    ///         not earlier than the adjusted maturity date.
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

    /// Return the forward rate associated with the FRA term.
    ///
    /// Returns:
    ///     float: The simple forward rate over the FRA window.
    ///
    /// Raises:
    ///     ItofinError: If the index has no forwarding curve or fixing
    ///         covering the term.
    fn forward_rate(&mut self) -> PyResult<f64> {
        Ok(self.inner.forward_rate().map_err(PyQlError::from)?.rate())
    }

    /// Return the payoff on the value date.
    ///
    /// Returns:
    ///     float: The settlement amount, signed by the position.
    ///
    /// Raises:
    ///     ItofinError: On an expired FRA, which has no settlement amount.
    fn amount(&mut self) -> PyResult<f64> {
        Ok(self.inner.amount().map_err(PyQlError::from)?)
    }

    /// Return the settlement amount discounted to the value date.
    ///
    /// Discounts on the discount curve, or on the index's forwarding curve
    /// when none was given.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set or the curves cannot
    ///         cover the term.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }

    /// Return the day the underlying loan begins.
    ///
    /// The FRA settles and expires on this date.
    ///
    /// Returns:
    ///     Date: The value date.
    fn value_date(&self) -> PyDate {
        PyDate::from_inner(self.value_date)
    }

    /// Return the day the underlying loan ends.
    ///
    /// Adjusted on the index's fixing calendar under the index's convention.
    ///
    /// Returns:
    ///     Date: The adjusted maturity date.
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
