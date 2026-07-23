//! Facades for the time primitives: [`PyDate`], [`PyDayCounter`], [`PyCalendar`].

use crate::ItofinError;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::{NullCalendar, Target};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::daycounters::actualactual::{ActualActual, Convention};
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use pyo3::prelude::*;

const MIN_SERIAL: i64 = 367;
const MAX_SERIAL: i64 = 109_574;

/// Days in `month` (1-based) for `year`, using the Gregorian leap rule.
///
/// Replicated in the facade because the core's `month_length`/`is_leap` are the
/// oracle for arithmetic but this guard must stand on its own: no input reaches
/// a core `assert!`. `month` must already be validated in `1..=12`.
fn days_in_month(month: i32, year: i32) -> i32 {
    const LENGTHS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    if month == 2 && leap {
        29
    } else {
        LENGTHS[(month - 1) as usize]
    }
}

/// Python `Date`: a calendar date with a mandatory validation guard.
///
/// The core's `Date::new`, `Month::from_ordinal` and `Date + i32` all `panic!`
/// on out-of-range input, and a panic unwinding across the PyO3 boundary is an
/// abort/UB hazard. Every constructor here validates first and returns
/// [`struct@ItofinError`] before touching the core.
#[pyclass(name = "Date", unsendable)]
pub struct PyDate {
    inner: Date,
}

#[pymethods]
impl PyDate {
    #[new]
    fn new(day: i32, month: i32, year: i32) -> PyResult<Self> {
        if !(1..=12).contains(&month) {
            return Err(ItofinError::new_err(format!(
                "month {month} outside [1, 12]"
            )));
        }
        if !(1901..=2199).contains(&year) {
            return Err(ItofinError::new_err(format!(
                "year {year} outside [1901, 2199]"
            )));
        }
        let len = days_in_month(month, year);
        if !(1..=len).contains(&day) {
            return Err(ItofinError::new_err(format!(
                "day {day} outside [1, {len}] for month {month} of {year}"
            )));
        }
        Ok(PyDate {
            inner: Date::new(day, Month::from_ordinal(month), year),
        })
    }

    #[getter]
    fn year(&self) -> i32 {
        self.inner.year()
    }

    #[getter]
    fn month(&self) -> i32 {
        self.inner.month().ordinal()
    }

    #[getter]
    fn day(&self) -> i32 {
        self.inner.day_of_month()
    }

    fn __add__(&self, days: i32) -> PyResult<Self> {
        self.shifted(days as i64)
    }

    fn __sub__(&self, days: i32) -> PyResult<Self> {
        self.shifted(-(days as i64))
    }

    fn __eq__(&self, other: &PyDate) -> bool {
        self.inner == other.inner
    }

    fn __repr__(&self) -> String {
        format!(
            "Date({}, {}, {})",
            self.inner.day_of_month(),
            self.inner.month().ordinal(),
            self.inner.year()
        )
    }
}

impl PyDate {
    /// The wrapped core [`Date`] (cheaply `Copy`).
    pub(crate) fn inner(&self) -> Date {
        self.inner
    }

    /// Shifts the date by `days`, guarding the serial range in `i64` so the
    /// core's `from_serial` never sees an out-of-range value or an `i32`
    /// overflow.
    fn shifted(&self, days: i64) -> PyResult<Self> {
        let target = self.inner.serial_number() as i64 + days;
        if !(MIN_SERIAL..=MAX_SERIAL).contains(&target) {
            return Err(ItofinError::new_err(format!(
                "date arithmetic result serial {target} outside [{MIN_SERIAL}, {MAX_SERIAL}]"
            )));
        }
        Ok(PyDate {
            inner: self.inner + days as i32,
        })
    }
}

/// Python `DayCounter`: the year-fraction convention factories.
#[pyclass(name = "DayCounter", unsendable)]
pub struct PyDayCounter {
    inner: DayCounter,
}

#[pymethods]
impl PyDayCounter {
    #[staticmethod]
    fn actual360() -> Self {
        PyDayCounter {
            inner: Actual360::new(),
        }
    }

    #[staticmethod]
    fn actual365_fixed() -> Self {
        PyDayCounter {
            inner: Actual365Fixed::new(),
        }
    }

    /// `ActualActual(ActualActual::ISDA)`: the day count the Heston/Hull-White
    /// flat-curve oracles anchor on (`test-suite` `flatRate`).
    #[staticmethod]
    fn actual_actual_isda() -> Self {
        PyDayCounter {
            inner: ActualActual::with_convention(Convention::ISDA),
        }
    }

    fn __repr__(&self) -> String {
        format!("DayCounter({})", self.inner.name())
    }
}

impl PyDayCounter {
    /// The wrapped core [`DayCounter`] (cheap `Rc` clone).
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> DayCounter {
        self.inner.clone()
    }
}

/// Python `Period`: a signed length in one calendar unit.
///
/// The unit is taken as a string in {"Days", "Weeks", "Months", "Years"} and
/// mapped to the core [`TimeUnit`]; an unknown unit returns
/// [`struct@ItofinError`] rather than reaching the core.
#[pyclass(name = "Period", unsendable)]
pub struct PyPeriod {
    inner: Period,
}

#[pymethods]
impl PyPeriod {
    #[new]
    fn new(n: i32, unit: &str) -> PyResult<Self> {
        let units = match unit {
            "Days" => TimeUnit::Days,
            "Weeks" => TimeUnit::Weeks,
            "Months" => TimeUnit::Months,
            "Years" => TimeUnit::Years,
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown time unit {other:?}, expected one of Days, Weeks, Months, Years"
                )));
            }
        };
        Ok(PyPeriod {
            inner: Period::new(n, units),
        })
    }

    fn __repr__(&self) -> String {
        format!("Period({}, {:?})", self.inner.length(), self.inner.units())
    }
}

impl PyPeriod {
    /// The wrapped core [`Period`] (cheaply `Copy`).
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Period {
        self.inner
    }
}

/// Python `Calendar`: the business-calendar factories.
#[pyclass(name = "Calendar", unsendable)]
pub struct PyCalendar {
    inner: Calendar,
}

#[pymethods]
impl PyCalendar {
    #[staticmethod]
    fn target() -> Self {
        PyCalendar {
            inner: Target::new(),
        }
    }

    #[staticmethod]
    fn null_calendar() -> Self {
        PyCalendar {
            inner: NullCalendar::new(),
        }
    }

    fn __repr__(&self) -> String {
        format!("Calendar({})", self.inner.name())
    }
}

impl PyCalendar {
    /// The wrapped core [`Calendar`] (cheap `Rc` clone).
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Calendar {
        self.inner.clone()
    }
}
