//! Market-convention builder for payer swaptions on vanilla SwapIndex underlyings.

use crate::PyQlError;
use crate::swapindex::PySwapIndex;
use crate::swaption::{PySettlementMethod, PySettlementType, PySwaption};
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyPeriod};
use libitofin::indexes::SwapIndex;
use libitofin::instruments::{MakeSwaption, SettlementMethod, SettlementType};
use libitofin::shared::Shared;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::date::Date;
use libitofin::time::period::Period;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Build a payer vanilla swaption from a SwapIndex and exactly one option date source.
///
/// Specify option_tenor or fixing_date. A missing strike requests the forward
/// swap's fair rate. Overnight swap indexes and underlying-type selection are
/// not supported. Attach a pricing engine to the resulting Swaption separately.
#[gen_stub_pyclass]
#[pyclass(name = "MakeSwaption", unsendable, module = "itofin.instruments")]
pub struct PyMakeSwaption {
    index: Shared<SwapIndex>,
    option_tenor: Option<Period>,
    fixing_date: Option<Date>,
    strike: Option<f64>,
    nominal: f64,
    settlement_type: SettlementType,
    settlement_method: SettlementMethod,
    option_convention: BusinessDayConvention,
    exercise_date: Option<Date>,
    exercise_calendar: Option<Calendar>,
    indexed_coupons: Option<bool>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMakeSwaption {
    /// Retain the index and overrides for repeatable build calls.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (swap_index, option_tenor=None, strike=None, *, fixing_date=None,
        nominal=1.0, settlement_type=PySettlementType::Physical,
        settlement_method=PySettlementMethod::PhysicalOTC,
        option_convention=PyBusinessDayConvention::ModifiedFollowing,
        exercise_date=None, exercise_calendar=None, indexed_coupons=None))]
    fn new(
        swap_index: &PySwapIndex,
        option_tenor: Option<&PyPeriod>,
        strike: Option<f64>,
        fixing_date: Option<&PyDate>,
        nominal: f64,
        settlement_type: PySettlementType,
        settlement_method: PySettlementMethod,
        option_convention: PyBusinessDayConvention,
        exercise_date: Option<&PyDate>,
        exercise_calendar: Option<&PyCalendar>,
        indexed_coupons: Option<bool>,
    ) -> PyResult<Self> {
        if option_tenor.is_some() == fixing_date.is_some() {
            return Err(PyValueError::new_err(
                "specify exactly one of option_tenor and fixing_date",
            ));
        }
        Ok(Self {
            index: swap_index.inner(),
            option_tenor: option_tenor.map(PyPeriod::inner),
            fixing_date: fixing_date.map(PyDate::inner),
            strike,
            nominal,
            settlement_type: settlement_type.inner(),
            settlement_method: settlement_method.inner(),
            option_convention: option_convention.inner(),
            exercise_date: exercise_date.map(PyDate::inner),
            exercise_calendar: exercise_calendar.map(PyCalendar::inner),
            indexed_coupons,
        })
    }

    /// Build the swaption, propagating date, forwarding and coupon-setting errors.
    fn build(&self) -> PyResult<PySwaption> {
        let mut maker = match (self.option_tenor, self.fixing_date) {
            (Some(tenor), None) => {
                MakeSwaption::new(Shared::clone(&self.index), tenor, self.strike)
            }
            (None, Some(date)) => {
                MakeSwaption::with_fixing_date(Shared::clone(&self.index), date, self.strike)
            }
            _ => {
                return Err(PyValueError::new_err(
                    "specify exactly one of option_tenor and fixing_date",
                ));
            }
        };
        maker = maker
            .with_nominal(self.nominal)
            .with_settlement_type(self.settlement_type)
            .with_settlement_method(self.settlement_method)
            .with_option_convention(self.option_convention)
            .with_indexed_coupons(self.indexed_coupons);
        if let Some(date) = self.exercise_date {
            maker = maker.with_exercise_date(date);
        }
        if let Some(calendar) = &self.exercise_calendar {
            maker = maker.with_exercise_calendar(calendar.clone());
        }
        Ok(PySwaption::from_inner(
            maker.build().map_err(PyQlError::from)?,
        ))
    }
}
