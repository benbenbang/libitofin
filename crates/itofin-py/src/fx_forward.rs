//! Snapshot-spot FX forwards with explicit dates and retained live curves.
use crate::PyQlError;
use crate::currency::PyCurrency;
use crate::curve::PyYieldTermStructure;
use crate::fra::PyPosition;
use crate::time::PyDate;
use libitofin::exchangerate::ExchangeRate;
use libitofin::fxforward::FxForward;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Fixed-date forward with an immutable quote-per-base spot snapshot.
///
/// Long receives base and pays quote. NPV is in quote currency at the quote
/// curve reference date. Explicit evaluation dates control expiry only.
/// This is not a live spot quote, rolling spot lag or full FX engine facade.
#[gen_stub_pyclass]
#[pyclass(name = "FxForward", unsendable, module = "itofin.instruments")]
pub struct PyFxForward {
    inner: FxForward,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyFxForward {
    /// Copy currencies and spot; retain the two live discount curves.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        base_currency: &PyCurrency,
        quote_currency: &PyCurrency,
        spot: f64,
        base_curve: &PyYieldTermStructure,
        quote_curve: &PyYieldTermStructure,
        base_notional: f64,
        strike: f64,
        spot_settlement_date: &PyDate,
        delivery_date: &PyDate,
        position: PyPosition,
    ) -> PyResult<Self> {
        let spot = ExchangeRate::checked_new(base_currency.inner(), quote_currency.inner(), spot)
            .map_err(PyQlError::from)?;
        let base = base_curve.handle();
        let quote = quote_curve.handle();
        let inner = FxForward::new(
            spot,
            base,
            quote,
            base_notional,
            strike,
            spot_settlement_date.inner(),
            delivery_date.inner(),
            matches!(position, PyPosition::Long),
        )
        .map_err(PyQlError::from)?;
        Ok(Self { inner })
    }
    /// Fair quote-per-base rate; does not apply expiry.
    fn fair_forward_rate(&self) -> PyResult<f64> {
        Ok(self.inner.fair_forward_rate().map_err(PyQlError::from)?)
    }
    /// Quote-currency NPV at the quote curve reference date, not evaluation date.
    fn npv(&self, evaluation_date: &PyDate) -> PyResult<f64> {
        Ok(self
            .inner
            .npv(evaluation_date.inner())
            .map_err(PyQlError::from)?
            .value())
    }
    /// True only when delivery is strictly before the explicit evaluation date.
    fn is_expired(&self, evaluation_date: &PyDate) -> PyResult<bool> {
        Ok(self
            .inner
            .is_expired(evaluation_date.inner())
            .map_err(PyQlError::from)?)
    }
    /// Immutable quote-currency units per one base-currency unit.
    fn spot(&self) -> f64 {
        self.inner.spot().rate()
    }
    /// Contractual quote-per-base delivery rate.
    fn strike(&self) -> f64 {
        self.inner.strike()
    }
    /// Positive notional denominated in the base currency.
    fn base_notional(&self) -> f64 {
        self.inner.base_notional()
    }
    /// Fixed, unadjusted value date of the supplied spot snapshot.
    fn spot_settlement_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.spot_settlement_date())
    }
    /// Fixed, unadjusted contractual delivery date.
    fn delivery_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.delivery_date())
    }
    /// Long receives base and pays quote; short reverses both legs.
    fn position(&self) -> PyPosition {
        if self.inner.is_long() {
            PyPosition::Long
        } else {
            PyPosition::Short
        }
    }
    /// Independent copy of the base-currency specification.
    fn base_currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.spot().source().clone())
    }
    /// Independent copy of the quote currency, which denominates NPV.
    fn quote_currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.spot().target().clone())
    }
}
