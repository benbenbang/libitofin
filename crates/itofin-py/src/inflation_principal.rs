//! Standalone inflation principal flows with retained one-flow leg conversions.
use crate::bonds::{advance, finite};
use crate::cashflows::{PyCashFlow, PyLeg};
use crate::inflation::{PyCpiInterpolationType, PyZeroInflationIndex};
use crate::time::{PyCalendar, PyDate, PyPeriod};
use crate::{ItofinError, PyQlError};
use libitofin::cashflow::CashFlow;
use libitofin::cashflows::{CpiCashFlow, ZeroInflationCashFlow};
use libitofin::event::Event;
use libitofin::indexes::inflationindex::InflationIndex;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, timeunit::TimeUnit,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

#[path = "inflation_principal_flow.rs"]
mod principal_flow;
use principal_flow::{FiniteCpiFlow, FiniteZeroFlow};

/// Principal scaled by a live CPI observation divided by a fixed explicit base.
#[gen_stub_pyclass]
#[pyclass(name = "CPICashFlow", unsendable, module = "itofin.cashflows")]
pub struct PyCpiCashFlow {
    inner: Shared<FiniteCpiFlow>,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiCashFlow {
    /// Build without reading fixings; growth_only subtracts the uninflated nominal.
    ///
    /// The explicit finite nonzero denominator stays fixed. Observation, index
    /// histories and linked curves remain live. Invalid dates and lags fail.
    #[new]
    #[pyo3(signature=(nominal,index,base_cpi,observation_date,observation_lag,interpolation,payment_date,growth_only=true))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        nominal: f64,
        index: &PyZeroInflationIndex,
        base_cpi: f64,
        observation_date: &PyDate,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        payment_date: &PyDate,
        growth_only: bool,
    ) -> PyResult<Self> {
        let index = index.shared();
        let settings = index.inflation_base().settings().clone();
        let payment = payment_date.inner();
        checked_date(payment)?;
        Ok(Self {
            inner: shared(FiniteCpiFlow(
                CpiCashFlow::new(
                    nominal,
                    index,
                    base_cpi,
                    observation_date.inner(),
                    observation_lag.inner(),
                    interpolation.inner(),
                    payment,
                    growth_only,
                )
                .map_err(PyQlError::from)?,
            )),
            settings,
        })
    }
    /// Undiscounted payment; missing fixings, curves or non-finite results fail.
    fn amount(&self) -> PyResult<f64> {
        finite(self.inner.amount().map_err(PyQlError::from)?)
    }
    /// The fixed explicit denominator supplied at construction.
    fn base_fixing(&self) -> f64 {
        self.inner.base_fixing()
    }
    /// The live lagged CPI level used in the numerator.
    fn index_fixing(&self) -> PyResult<f64> {
        finite(self.inner.index_fixing().map_err(PyQlError::from)?)
    }
    /// Payment date; no fixing is read.
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.inner.date())
    }
    /// Return a retained generic flow; the original wrapper may be released.
    fn as_cashflow(&self) -> PyCashFlow {
        PyCashFlow::from_inner(self.inner.clone())
    }
    /// Return a one-flow leg requiring the index's identical Settings for NPV.
    fn as_leg(&self) -> PyLeg {
        PyLeg::with_cpi_settings(vec![self.inner.clone()], self.settings.clone())
    }
}

fn checked_date(date: Date) -> PyResult<()> {
    if !(367..=109_574).contains(&date.serial_number()) {
        return Err(ItofinError::new_err(
            "inflation principal date outside 1901-2199",
        ));
    }
    Ok(())
}

/// Principal scaled by the ratio of two live lagged inflation observations.
#[gen_stub_pyclass]
#[pyclass(
    name = "ZeroInflationCashFlow",
    unsendable,
    module = "itofin.cashflows"
)]
pub struct PyZeroInflationCashFlow {
    inner: Shared<FiniteZeroFlow>,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyZeroInflationCashFlow {
    /// Build without reading fixings; both lagged start and end CPI remain live.
    ///
    /// Unlike CPICashFlow, historical base-fixing corrections change this flow.
    /// The observation lag is nonnegative and dates must remain within 1901-2199.
    #[new]
    #[pyo3(signature=(nominal,index,start_date,end_date,observation_lag,interpolation,payment_date,growth_only=true))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        nominal: f64,
        index: &PyZeroInflationIndex,
        start_date: &PyDate,
        end_date: &PyDate,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        payment_date: &PyDate,
        growth_only: bool,
    ) -> PyResult<Self> {
        finite(nominal)?;
        let lag = observation_lag.inner();
        if !matches!(
            lag.units(),
            TimeUnit::Days | TimeUnit::Weeks | TimeUnit::Months | TimeUnit::Years
        ) {
            return Err(ItofinError::new_err(
                "unsupported inflation observation lag unit",
            ));
        }
        if lag.length() < 0 {
            return Err(ItofinError::new_err("negative inflation observation lag"));
        }
        let start = start_date.inner();
        let end = end_date.inner();
        checked_date(start)?;
        checked_date(end)?;
        checked_date(payment_date.inner())?;
        let arithmetic = PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new());
        for observation in [start, end] {
            advance(
                &arithmetic,
                observation,
                -i64::from(lag.length()),
                lag.units(),
                BusinessDayConvention::Unadjusted,
                false,
            )?;
        }
        let index = index.shared();
        let settings = index.inflation_base().settings().clone();
        Ok(Self {
            inner: shared(FiniteZeroFlow(ZeroInflationCashFlow::new(
                nominal,
                index,
                interpolation.inner(),
                start,
                end,
                lag,
                payment_date.inner(),
                growth_only,
            ))),
            settings,
        })
    }
    /// Undiscounted payment; missing fixings, curves or non-finite results fail.
    fn amount(&self) -> PyResult<f64> {
        finite(self.inner.amount().map_err(PyQlError::from)?)
    }
    /// Read the live lagged start-date denominator, without taking a snapshot.
    fn base_fixing(&self) -> PyResult<f64> {
        finite(self.inner.base_fixing().map_err(PyQlError::from)?)
    }
    /// Read the live lagged end-date numerator.
    fn index_fixing(&self) -> PyResult<f64> {
        finite(self.inner.index_fixing().map_err(PyQlError::from)?)
    }
    /// Payment date; no fixing is read.
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.inner.date())
    }
    /// Return a retained finite-checked generic flow.
    fn as_cashflow(&self) -> PyCashFlow {
        PyCashFlow::from_inner(self.inner.clone())
    }
    /// Return a one-flow leg requiring the index's identical Settings for NPV.
    fn as_leg(&self) -> PyLeg {
        PyLeg::with_cpi_settings(vec![self.inner.clone()], self.settings.clone())
    }
}
