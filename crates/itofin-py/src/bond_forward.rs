//! Plain-bond forwards retaining the original Python preparation graph.
use crate::bonds::{PyBond, adjust, advance, finite};
use crate::curve::PyYieldTermStructure;
use crate::fra::PyPosition;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter};
use crate::{ItofinError, PyQlError};
use libitofin::instrument::Instrument;
use libitofin::instruments::BondForward;
use libitofin::time::{businessdayconvention::BusinessDayConvention, timeunit::TimeUnit};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Live plain fixed/zero bond forward with a dirty delivery strike.
#[gen_stub_pyclass]
#[pyclass(name = "BondForward", unsendable, module = "itofin.instruments")]
pub struct PyBondForward {
    inner: BondForward,
    bond: Py<PyBond>,
    calendar: PyCalendar,
    settlement_days: u32,
    frozen: bool,
}
impl PyBondForward {
    fn check_settlement(&self, py: Python<'_>) -> PyResult<()> {
        let bond = self.bond.borrow(py);
        let plain = bond.forward_plain()?;
        let evaluation = plain
            .borrow()
            .settings()
            .evaluation_date()
            .ok_or_else(|| ItofinError::new_err("evaluation date required"))?;
        advance(
            &self.calendar,
            evaluation,
            i64::from(self.settlement_days),
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )?;
        Ok(())
    }
    fn prepare(&self, py: Python<'_>, force: bool) -> PyResult<()> {
        self.check_settlement(py)?;
        self.bond.borrow_mut(py).refresh_forward();
        if !force && (self.inner.base().is_calculated() || self.frozen) {
            return Ok(());
        }
        if self.inner.is_expired().map_err(PyQlError::from)? {
            return Ok(());
        }
        self.bond.borrow_mut(py).prepare_forward(force)
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyBondForward {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(bond, value_date, delivery_date, position, strike, settlement_days, day_counter, calendar, convention, discount, income_discount=None))]
    fn new(
        py: Python<'_>,
        bond: Py<PyBond>,
        value_date: &PyDate,
        delivery_date: &PyDate,
        position: PyPosition,
        strike: f64,
        settlement_days: u32,
        day_counter: &PyDayCounter,
        calendar: &PyCalendar,
        convention: PyBusinessDayConvention,
        discount: &PyYieldTermStructure,
        income_discount: Option<&PyYieldTermStructure>,
    ) -> PyResult<Self> {
        if settlement_days > i32::MAX as u32 {
            return Err(ItofinError::new_err("settlement days overflow"));
        }
        let plain = bond.borrow(py).forward_plain()?;
        adjust(calendar, delivery_date.inner(), convention.inner())?;
        let inner = if let Some(income) = income_discount {
            BondForward::with_income_curve(
                plain,
                value_date.inner(),
                delivery_date.inner(),
                position.inner(),
                finite(strike)?,
                settlement_days,
                day_counter.inner(),
                calendar.inner(),
                convention.inner(),
                discount.handle(),
                income.handle(),
            )
        } else {
            BondForward::new(
                plain,
                value_date.inner(),
                delivery_date.inner(),
                position.inner(),
                finite(strike)?,
                settlement_days,
                day_counter.inner(),
                calendar.inner(),
                convention.inner(),
                discount.handle(),
            )
        }
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner,
            bond,
            calendar: calendar.clone(),
            settlement_days,
            frozen: false,
        })
    }
    fn npv(&mut self, py: Python<'_>) -> PyResult<f64> {
        self.prepare(py, false)?;
        finite(self.inner.npv().map_err(PyQlError::from)?)
    }
    fn fair_forward_price(&mut self, py: Python<'_>) -> PyResult<f64> {
        self.prepare(py, false)?;
        finite(self.inner.fair_forward_price().map_err(PyQlError::from)?)
    }
    fn clean_forward_price(&mut self, py: Python<'_>) -> PyResult<f64> {
        self.prepare(py, false)?;
        finite(self.inner.clean_forward_price().map_err(PyQlError::from)?)
    }
    fn settlement_date(&self, py: Python<'_>) -> PyResult<PyDate> {
        self.check_settlement(py)?;
        Ok(PyDate::from_inner(
            self.inner.settlement_date().map_err(PyQlError::from)?,
        ))
    }
    fn delivery_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.delivery_date())
    }
    fn value_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.value_date())
    }
    fn strike(&self) -> f64 {
        self.inner.strike()
    }
    fn position(&self) -> PyPosition {
        match self.inner.position() {
            libitofin::position::Position::Long => PyPosition::Long,
            libitofin::position::Position::Short => PyPosition::Short,
        }
    }
    fn is_expired(&self, py: Python<'_>) -> PyResult<bool> {
        self.check_settlement(py)?;
        Ok(self.inner.is_expired().map_err(PyQlError::from)?)
    }
    fn freeze(&mut self) {
        self.inner.base_mut().freeze();
        self.frozen = true;
    }
    fn unfreeze(&mut self) {
        self.inner.base_mut().unfreeze();
        self.frozen = false;
    }
    fn recalculate(&mut self, py: Python<'_>) -> PyResult<()> {
        self.prepare(py, true)?;
        Ok(self.inner.recalculate().map_err(PyQlError::from)?)
    }
}
