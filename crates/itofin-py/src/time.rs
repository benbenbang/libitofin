//! Facades for the time primitives: [`PyDate`], [`PyDayCounter`], [`PyCalendar`].

use crate::ItofinError;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::{NullCalendar, Target};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::daycounters::actualactual::{ActualActual, Convention};
use libitofin::time::daycounters::thirty360::{Convention as Thirty360Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::imm;
use libitofin::time::period::Period;
use libitofin::time::schedule::{MakeSchedule, Schedule};
use libitofin::time::timeunit::TimeUnit;
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

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

    /// Wraps a core [`Date`] returned from a term-structure query.
    pub(crate) fn from_inner(inner: Date) -> Self {
        PyDate { inner }
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

    /// `Thirty360(Thirty360::BondBasis)`: the fixed-leg day count the Hull-White
    /// swaption-calibration oracle anchors on (`hullwhite.rs:835`).
    #[staticmethod]
    fn thirty360_bond_basis() -> Self {
        PyDayCounter {
            inner: Thirty360::with_convention(Thirty360Convention::BondBasis),
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
    pub(crate) fn inner(&self) -> Period {
        self.inner
    }
}

/// Maps a `{"Days", "Weeks", "Months", "Years"}` string to a [`TimeUnit`],
/// the same set [`PyPeriod`] accepts; an unknown unit returns
/// [`struct@ItofinError`] rather than reaching the core.
fn parse_time_unit(unit: &str) -> PyResult<TimeUnit> {
    match unit {
        "Days" => Ok(TimeUnit::Days),
        "Weeks" => Ok(TimeUnit::Weeks),
        "Months" => Ok(TimeUnit::Months),
        "Years" => Ok(TimeUnit::Years),
        other => Err(ItofinError::new_err(format!(
            "unknown time unit {other:?}, expected one of Days, Weeks, Months, Years"
        ))),
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

    /// Rolls `date` to the nearest business day per `convention`.
    ///
    /// The core `Calendar::adjust` `assert!`s on the null date (calendar.rs:248);
    /// `PyDate` cannot build one today, but the guard mirrors the `PySchedule`
    /// precedent so no input reaches a core `assert!` across the PyO3 boundary.
    fn adjust(&self, date: &PyDate, convention: &PyBusinessDayConvention) -> PyResult<PyDate> {
        if date.inner() == Date::null() {
            return Err(ItofinError::new_err("cannot adjust the null date"));
        }
        Ok(PyDate::from_inner(
            self.inner.adjust(date.inner(), convention.inner()),
        ))
    }

    /// Advances `date` by `n` `unit`s, adjusting the result per `convention`.
    ///
    /// `unit` is a string in {"Days", "Weeks", "Months", "Years"} mapped to the
    /// core [`TimeUnit`]; an unknown unit returns [`struct@ItofinError`] rather
    /// than reaching the core. The null-date guard mirrors [`Self::adjust`].
    fn advance(
        &self,
        date: &PyDate,
        n: i32,
        unit: &str,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
    ) -> PyResult<PyDate> {
        let unit = parse_time_unit(unit)?;
        if date.inner() == Date::null() {
            return Err(ItofinError::new_err("cannot advance the null date"));
        }
        Ok(PyDate::from_inner(self.inner.advance(
            date.inner(),
            n,
            unit,
            convention.inner(),
            end_of_month,
        )))
    }

    fn __repr__(&self) -> String {
        format!("Calendar({})", self.inner.name())
    }
}

impl PyCalendar {
    /// The wrapped core [`Calendar`] (cheap `Rc` clone).
    pub(crate) fn inner(&self) -> Calendar {
        self.inner.clone()
    }
}

/// Python `Frequency`: the coupon frequencies the swaption fixture needs.
///
/// A fieldless pyo3 enum exposing `Frequency.Annual` / `Frequency.Semiannual`;
/// only the variants the Jamshidian fixture uses are surfaced.
#[pyclass(name = "Frequency", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFrequency {
    Annual,
    Semiannual,
}

impl PyFrequency {
    /// The core [`Frequency`] this variant stands for.
    pub(crate) fn inner(&self) -> Frequency {
        match self {
            PyFrequency::Annual => Frequency::Annual,
            PyFrequency::Semiannual => Frequency::Semiannual,
        }
    }
}

/// Python `BusinessDayConvention`: the holiday-rolling rules the fixture needs.
///
/// A fieldless pyo3 enum exposing the `Following`, `ModifiedFollowing` and
/// `Unadjusted` variants; the adjustment logic itself lives in the core
/// calendar.
#[pyclass(name = "BusinessDayConvention", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyBusinessDayConvention {
    ModifiedFollowing,
    Following,
    Unadjusted,
}

impl PyBusinessDayConvention {
    /// The core [`BusinessDayConvention`] this variant stands for.
    pub(crate) fn inner(&self) -> BusinessDayConvention {
        match self {
            PyBusinessDayConvention::ModifiedFollowing => BusinessDayConvention::ModifiedFollowing,
            PyBusinessDayConvention::Following => BusinessDayConvention::Following,
            PyBusinessDayConvention::Unadjusted => BusinessDayConvention::Unadjusted,
        }
    }
}

/// Python `Schedule`: a sequence of coupon dates built through `MakeSchedule`.
///
/// The core `Schedule::new` (via `MakeSchedule::build`) `panic!`s on degenerate
/// input - a null date or an effective date not strictly before the termination
/// date - and a panic unwinding across the PyO3 boundary is an abort/UB hazard.
/// The constructor supplies every builder input, so the `build`-level checks are
/// unreachable; the date ordering is the one piece of user input, so it is
/// validated first and returns [`struct@ItofinError`] before the core is
/// touched. `date` likewise bounds-checks the index the core would otherwise
/// panic on.
#[pyclass(name = "Schedule", unsendable)]
pub struct PySchedule {
    inner: Schedule,
}

#[pymethods]
impl PySchedule {
    #[new]
    fn new(
        start: &PyDate,
        end: &PyDate,
        frequency: &PyFrequency,
        calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
    ) -> PyResult<Self> {
        if start.inner() >= end.inner() {
            return Err(ItofinError::new_err(format!(
                "schedule start ({}) is not strictly before end ({})",
                start.inner(),
                end.inner()
            )));
        }
        let convention = convention.inner();
        let inner = MakeSchedule::new()
            .from(start.inner())
            .to(end.inner())
            .with_frequency(frequency.inner())
            .with_calendar(calendar.inner())
            .with_convention(convention)
            .with_termination_date_convention(convention)
            .forwards()
            .build();
        Ok(PySchedule { inner })
    }

    /// The number of dates in the schedule (one more than the period count).
    fn size(&self) -> usize {
        self.inner.dates().len()
    }

    /// The `i`-th date, erroring when `i` is out of range.
    fn date(&self, i: usize) -> PyResult<PyDate> {
        let dates = self.inner.dates();
        if i >= dates.len() {
            return Err(ItofinError::new_err(format!(
                "schedule date index {i} out of range [0, {})",
                dates.len()
            )));
        }
        Ok(PyDate { inner: dates[i] })
    }

    /// All the schedule dates, as a Python list.
    fn dates(&self) -> Vec<PyDate> {
        self.inner
            .dates()
            .iter()
            .map(|&inner| PyDate { inner })
            .collect()
    }
}

impl PySchedule {
    /// The wrapped core [`Schedule`] (clone), for the swap facades in X2.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Schedule {
        self.inner.clone()
    }
}

/// Whether `date` is an IMM date: the third Wednesday of the month, and of
/// March, June, September or December only when `main_cycle` is set.
///
/// The free-function form QuantLib-SWIG exposes for `IMM::isIMMdate`; it is the
/// way to build a valid IMM start date for the futures rate helper from Python.
#[pyfunction]
#[pyo3(signature = (date, main_cycle = false))]
fn is_imm_date(date: &PyDate, main_cycle: bool) -> bool {
    imm::is_imm_date(date.inner(), main_cycle)
}

/// The next IMM date strictly following `date`, restricted to the March/June/
/// September/December cycle when `main_cycle` is set (`IMM::nextDate`).
#[pyfunction]
#[pyo3(signature = (date, main_cycle = false))]
fn next_imm_date(date: &PyDate, main_cycle: bool) -> PyDate {
    PyDate::from_inner(imm::next_date(date.inner(), main_cycle))
}

/// Registers the module-level IMM free functions on the `time` submodule.
pub(crate) fn add_functions(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(is_imm_date, module)?)?;
    module.add_function(wrap_pyfunction!(next_imm_date, module)?)?;
    Ok(())
}
