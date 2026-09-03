//! Facades for the time primitives: Date, DayCounter, Calendar.

use crate::ItofinError;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::calendars::{NullCalendar, Target, WeekendsOnly};
use libitofin::time::date::{Date, Month, SerialNumber};
use libitofin::time::dategenerationrule::DateGeneration;
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
use pyo3::{IntoPyObjectExt, wrap_pyfunction};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const MIN_SERIAL: i64 = 367;
const MAX_SERIAL: i64 = 109_574;

/// The hash of `value`, for the `__hash__` facades.
///
/// Each caller feeds this exactly what its `__eq__` compares, so equal objects
/// hash equal: `DayCounter` its name, `Period` its canonical (normalized) form.
fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

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

/// A calendar date with a validation guard.
///
/// Every constructor and every arithmetic result is range-checked before the
/// core is reached, so an out-of-range date is an error rather than a panic.
#[pyclass(name = "Date", unsendable)]
pub struct PyDate {
    inner: Date,
}

#[pymethods]
impl PyDate {
    /// Build a date from its three components.
    ///
    /// Args:
    ///     day (int): The day of the month, within the length of that month.
    ///     month (int): The month, from 1 to 12.
    ///     year (int): The year, from 1901 to 2199.
    ///
    /// Raises:
    ///     ItofinError: If month is outside [1, 12], year is outside
    ///         [1901, 2199], or day is outside the length of that month.
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

    /// The year.
    #[getter]
    fn year(&self) -> i32 {
        self.inner.year()
    }

    /// The month, from 1 to 12.
    #[getter]
    fn month(&self) -> i32 {
        self.inner.month().ordinal()
    }

    /// The day of the month.
    #[getter]
    fn day(&self) -> i32 {
        self.inner.day_of_month()
    }

    /// Shift the date forward by a number of calendar days.
    ///
    /// Args:
    ///     days (int): The number of calendar days to add.
    ///
    /// Returns:
    ///     Date: The shifted date.
    ///
    /// Raises:
    ///     ItofinError: If the result falls outside the representable date
    ///         range.
    fn __add__(&self, days: i32) -> PyResult<Self> {
        self.shifted(days as i64)
    }

    /// The signed number of days from other to this date.
    ///
    /// The other operand may also be an int, which shifts the date back by that
    /// many calendar days and returns a Date. Anything else raises TypeError.
    ///
    /// Args:
    ///     other (Date): The date to measure from.
    ///
    /// Returns:
    ///     int: The signed day count between the two dates.
    fn __sub__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if let Ok(other_date) = other.cast::<PyDate>() {
            let subtrahend = other_date.borrow().inner;
            let days: SerialNumber = self.inner - subtrahend;
            return days.into_py_any(py);
        }
        let days: i32 = other.extract()?;
        self.shifted(-(days as i64))?.into_py_any(py)
    }

    /// Whether the two dates are the same calendar day.
    ///
    /// Args:
    ///     other (object): The date to compare against.
    ///
    /// Returns:
    ///     bool: True when both stand for the same day.
    fn __eq__(&self, other: &PyDate) -> bool {
        self.inner == other.inner
    }

    /// Hashes the calendar day, the field equality compares.
    ///
    /// Returns:
    ///     int: The hash of the date, so equal dates hash equal.
    fn __hash__(&self) -> u64 {
        hash_of(&self.inner)
    }

    /// Return the constructor form of the date.
    ///
    /// Returns:
    ///     str: The date as Date(day, month, year).
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
    /// The wrapped core Date (cheaply `Copy`).
    pub(crate) fn inner(&self) -> Date {
        self.inner
    }

    /// Wraps a core Date returned from a term-structure query.
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

/// A year-fraction convention.
#[pyclass(name = "DayCounter", unsendable)]
pub struct PyDayCounter {
    inner: DayCounter,
}

#[pymethods]
impl PyDayCounter {
    /// The Actual/360 convention.
    ///
    /// Returns:
    ///     DayCounter: An Actual/360 day counter.
    #[staticmethod]
    fn actual360() -> Self {
        PyDayCounter {
            inner: Actual360::new(),
        }
    }

    /// The Actual/365 (Fixed) convention.
    ///
    /// Returns:
    ///     DayCounter: An Actual/365 (Fixed) day counter.
    #[staticmethod]
    fn actual365_fixed() -> Self {
        PyDayCounter {
            inner: Actual365Fixed::new(),
        }
    }

    /// The Actual/Actual (ISDA) convention.
    ///
    /// Returns:
    ///     DayCounter: An Actual/Actual day counter on the ISDA convention.
    #[staticmethod]
    fn actual_actual_isda() -> Self {
        PyDayCounter {
            inner: ActualActual::with_convention(Convention::ISDA),
        }
    }

    /// The 30/360 (Bond Basis) convention.
    ///
    /// Returns:
    ///     DayCounter: A 30/360 day counter on the bond-basis convention.
    #[staticmethod]
    fn thirty360_bond_basis() -> Self {
        PyDayCounter {
            inner: Thirty360::with_convention(Thirty360Convention::BondBasis),
        }
    }

    /// The period [d1, d2] as a fraction of a year under this convention.
    ///
    /// Args:
    ///     d1 (Date): The start of the period.
    ///     d2 (Date): The end of the period.
    ///
    /// Returns:
    ///     float: The year fraction between the two dates.
    fn year_fraction(&self, d1: &PyDate, d2: &PyDate) -> f64 {
        self.inner.year_fraction(d1.inner(), d2.inner())
    }

    /// Equality by convention name, so two independently built Actual360s
    /// are equal.
    ///
    /// Args:
    ///     other (object): The day counter to compare against.
    ///
    /// Returns:
    ///     bool: True when both carry the same convention name.
    fn __eq__(&self, other: &PyDayCounter) -> bool {
        self.inner == other.inner
    }

    /// Hashes the convention name, the field equality compares.
    ///
    /// Returns:
    ///     int: The hash of the convention name, so equal day counters hash equal.
    fn __hash__(&self) -> u64 {
        hash_of(&self.inner.name())
    }

    /// Return the day counter and its convention name.
    ///
    /// Returns:
    ///     str: The day counter as DayCounter(name).
    fn __repr__(&self) -> String {
        format!("DayCounter({})", self.inner.name())
    }
}

impl PyDayCounter {
    /// The wrapped core DayCounter (cheap `Rc` clone).
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> DayCounter {
        self.inner.clone()
    }

    /// Wraps a core DayCounter a facade read back off an object it built.
    ///
    /// The result carries no factory identity, but equality is by convention
    /// name, so it compares equal to the factory call that built it.
    pub(crate) fn from_inner(inner: DayCounter) -> Self {
        PyDayCounter { inner }
    }
}

/// A signed length in one calendar unit (unit: Days, Weeks, Months, Years).
#[pyclass(name = "Period", unsendable)]
pub struct PyPeriod {
    inner: Period,
}

#[pymethods]
impl PyPeriod {
    /// Build a period of n units.
    ///
    /// Args:
    ///     n (int): The length, which may be negative.
    ///     unit (str): One of "Days", "Weeks", "Months", "Years".
    ///
    /// Raises:
    ///     ItofinError: If unit is not one of the four accepted strings.
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

    /// Semantic equality: 7 Days equals 1 Week and 12 Months equals 1 Year,
    /// while an undecidable pair such as 30 Days against 1 Month is not equal.
    ///
    /// Args:
    ///     other (object): The period to compare against.
    ///
    /// Returns:
    ///     bool: True when the two lengths are decidably the same.
    fn __eq__(&self, other: &PyPeriod) -> bool {
        self.inner == other.inner
    }

    /// Hashes the canonical form, so equal periods hash equal.
    ///
    /// Normalizing collapses 7 Days onto 1 Week, 12 Months onto 1 Year and
    /// every zero length onto 0 Days before the hash is taken.
    ///
    /// Returns:
    ///     int: The hash of the normalized length and unit.
    fn __hash__(&self) -> u64 {
        let normalized = self.inner.normalized();
        hash_of(&(normalized.length(), normalized.units()))
    }

    /// Return the constructor form of the period.
    ///
    /// Returns:
    ///     str: The period as Period(length, unit).
    fn __repr__(&self) -> String {
        format!("Period({}, {:?})", self.inner.length(), self.inner.units())
    }
}

impl PyPeriod {
    /// The wrapped core Period (cheaply `Copy`).
    pub(crate) fn inner(&self) -> Period {
        self.inner
    }

    /// A facade over a core Period, for the inspectors that return one.
    pub(crate) fn from_inner(inner: Period) -> Self {
        PyPeriod { inner }
    }
}

/// Maps a `{"Days", "Weeks", "Months", "Years"}` string to a TimeUnit, the same
/// set Period accepts; an unknown unit returns ItofinError rather than reaching
/// the core.
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

/// A business-day calendar.
#[pyclass(name = "Calendar", unsendable)]
pub struct PyCalendar {
    inner: Calendar,
}

#[pymethods]
impl PyCalendar {
    /// The TARGET calendar.
    ///
    /// Returns:
    ///     Calendar: The TARGET business-day calendar.
    #[staticmethod]
    fn target() -> Self {
        PyCalendar {
            inner: Target::new(),
        }
    }

    /// The calendar holding no holidays at all.
    ///
    /// Returns:
    ///     Calendar: The null calendar, on which every day is a business day.
    #[staticmethod]
    fn null_calendar() -> Self {
        PyCalendar {
            inner: NullCalendar::new(),
        }
    }

    /// The weekends-only calendar: Saturdays and Sundays are holidays and no
    /// other day is. The calendar the ISDA CDS conventions roll on.
    ///
    /// It is not substitutable by the null calendar, which holds no holidays at
    /// all, nor by a national calendar, which adds public holidays.
    ///
    /// Returns:
    ///     Calendar: The weekends-only calendar.
    #[staticmethod]
    fn weekends_only() -> Self {
        PyCalendar {
            inner: WeekendsOnly::new(),
        }
    }

    /// The UK settlement calendar. Only the Settlement market is exposed:
    /// the Exchange and Metals markets share an identical business-day rule in
    /// the core and differ solely in their name.
    ///
    /// Returns:
    ///     Calendar: The UK settlement calendar.
    #[staticmethod]
    fn united_kingdom() -> Self {
        PyCalendar {
            inner: UnitedKingdom::new(Market::Settlement),
        }
    }

    /// Roll a date to the nearest business day.
    ///
    /// Args:
    ///     date (Date): The date to roll.
    ///     convention (BusinessDayConvention): The rolling rule to apply.
    ///
    /// Returns:
    ///     Date: The adjusted date, unchanged when it is already a business day.
    fn adjust(&self, date: &PyDate, convention: &PyBusinessDayConvention) -> PyResult<PyDate> {
        if date.inner() == Date::null() {
            return Err(ItofinError::new_err("cannot adjust the null date"));
        }
        Ok(PyDate::from_inner(
            self.inner.adjust(date.inner(), convention.inner()),
        ))
    }

    /// Advance a date by n units and adjust the result.
    ///
    /// Args:
    ///     date (Date): The date to advance from.
    ///     n (int): The number of units to advance, which may be negative.
    ///     unit (str): One of "Days", "Weeks", "Months", "Years".
    ///     convention (BusinessDayConvention): The rule the advanced date is rolled under.
    ///     end_of_month (bool): Keep the result on the month end when the starting
    ///         date is one.
    ///
    /// Returns:
    ///     Date: The advanced and adjusted date.
    ///
    /// Raises:
    ///     ItofinError: If unit is not one of the four accepted strings.
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

    /// Return the calendar and its name.
    ///
    /// Returns:
    ///     str: The calendar as Calendar(name).
    fn __repr__(&self) -> String {
        format!("Calendar({})", self.inner.name())
    }
}

impl PyCalendar {
    /// The wrapped core Calendar (cheap `Rc` clone).
    pub(crate) fn inner(&self) -> Calendar {
        self.inner.clone()
    }

    /// Wraps a core Calendar a facade read back off an object it built.
    ///
    /// The result carries no factory identity; the calendar it stands for is
    /// readable through Self.__repr__(), which prints the core name.
    pub(crate) fn from_inner(inner: Calendar) -> Self {
        PyCalendar { inner }
    }
}

/// A coupon or fixing frequency.
///
/// Only the variants the ported fixtures use are surfaced; new ones are
/// appended, so the integer values of the existing variants are unchanged.
#[pyclass(name = "Frequency", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFrequency {
    Annual,
    Semiannual,
    Quarterly,
    Monthly,
}

impl PyFrequency {
    /// The core Frequency this variant stands for.
    pub(crate) fn inner(&self) -> Frequency {
        match self {
            PyFrequency::Annual => Frequency::Annual,
            PyFrequency::Semiannual => Frequency::Semiannual,
            PyFrequency::Quarterly => Frequency::Quarterly,
            PyFrequency::Monthly => Frequency::Monthly,
        }
    }

    /// The variant standing for `frequency`, for the facades that read one back
    /// off a core object.
    ///
    /// The core enum carries thirteen values against the four surfaced here, so
    /// this is partial: a frequency with no counterpart is reported as
    /// ItofinError rather than mapped onto a neighbour.
    pub(crate) fn from_inner(frequency: Frequency) -> PyResult<PyFrequency> {
        match frequency {
            Frequency::Annual => Ok(PyFrequency::Annual),
            Frequency::Semiannual => Ok(PyFrequency::Semiannual),
            Frequency::Quarterly => Ok(PyFrequency::Quarterly),
            Frequency::Monthly => Ok(PyFrequency::Monthly),
            other => Err(ItofinError::new_err(format!(
                "frequency {other} is not exposed to Python"
            ))),
        }
    }
}

/// A holiday-rolling rule. Every core variant is covered; the four listed
/// last are appended, so the integer values of the first three are unchanged.
#[pyclass(name = "BusinessDayConvention", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyBusinessDayConvention {
    ModifiedFollowing,
    Following,
    Unadjusted,
    Preceding,
    ModifiedPreceding,
    HalfMonthModifiedFollowing,
    Nearest,
}

impl PyBusinessDayConvention {
    /// The core BusinessDayConvention this variant stands for.
    pub(crate) fn inner(&self) -> BusinessDayConvention {
        match self {
            PyBusinessDayConvention::ModifiedFollowing => BusinessDayConvention::ModifiedFollowing,
            PyBusinessDayConvention::Following => BusinessDayConvention::Following,
            PyBusinessDayConvention::Unadjusted => BusinessDayConvention::Unadjusted,
            PyBusinessDayConvention::Preceding => BusinessDayConvention::Preceding,
            PyBusinessDayConvention::ModifiedPreceding => BusinessDayConvention::ModifiedPreceding,
            PyBusinessDayConvention::HalfMonthModifiedFollowing => {
                BusinessDayConvention::HalfMonthModifiedFollowing
            }
            PyBusinessDayConvention::Nearest => BusinessDayConvention::Nearest,
        }
    }

    /// The variant standing for `convention`, for the facades that read one
    /// back off a core object. Total: every core variant has a counterpart.
    pub(crate) fn from_inner(convention: BusinessDayConvention) -> Self {
        match convention {
            BusinessDayConvention::ModifiedFollowing => PyBusinessDayConvention::ModifiedFollowing,
            BusinessDayConvention::Following => PyBusinessDayConvention::Following,
            BusinessDayConvention::Unadjusted => PyBusinessDayConvention::Unadjusted,
            BusinessDayConvention::Preceding => PyBusinessDayConvention::Preceding,
            BusinessDayConvention::ModifiedPreceding => PyBusinessDayConvention::ModifiedPreceding,
            BusinessDayConvention::HalfMonthModifiedFollowing => {
                PyBusinessDayConvention::HalfMonthModifiedFollowing
            }
            BusinessDayConvention::Nearest => PyBusinessDayConvention::Nearest,
        }
    }
}

/// The rule a Schedule generates its dates by.
///
/// Backward and Forward roll from one end of the range to the other; Zero keeps
/// only the two endpoints; the ThirdWednesday and Twentieth families snap the
/// interior dates onto an IMM Wednesday or the twentieth of the month. A
/// Schedule builds under the three CDS rules, but SpreadCdsHelper rejects them:
/// their maturity comes from a core routine that is not ported yet.
#[pyclass(name = "DateGeneration", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum PyDateGeneration {
    Backward,
    Forward,
    Zero,
    ThirdWednesday,
    ThirdWednesdayInclusive,
    Twentieth,
    TwentiethIMM,
    OldCDS,
    CDS,
    CDS2015,
}

impl PyDateGeneration {
    /// The core DateGeneration this variant stands for.
    pub(crate) fn inner(self) -> DateGeneration {
        match self {
            PyDateGeneration::Backward => DateGeneration::Backward,
            PyDateGeneration::Forward => DateGeneration::Forward,
            PyDateGeneration::Zero => DateGeneration::Zero,
            PyDateGeneration::ThirdWednesday => DateGeneration::ThirdWednesday,
            PyDateGeneration::ThirdWednesdayInclusive => DateGeneration::ThirdWednesdayInclusive,
            PyDateGeneration::Twentieth => DateGeneration::Twentieth,
            PyDateGeneration::TwentiethIMM => DateGeneration::TwentiethIMM,
            PyDateGeneration::OldCDS => DateGeneration::OldCDS,
            PyDateGeneration::CDS => DateGeneration::CDS,
            PyDateGeneration::CDS2015 => DateGeneration::CDS2015,
        }
    }
}

/// A sequence of coupon dates built through MakeSchedule.
///
/// termination_convention rolls the last date only, and defaults to
/// convention. CDS conventions need the two to differ: a credit helper leaves
/// its maturity unadjusted while paying Following.
#[pyclass(name = "Schedule", unsendable)]
pub struct PySchedule {
    inner: Schedule,
}

#[pymethods]
impl PySchedule {
    /// Build the schedule.
    ///
    /// Args:
    ///     start (Date): The effective date, which must be strictly before end.
    ///     end (Date): The termination date.
    ///     frequency (Frequency): The coupon frequency the interior dates are spaced at.
    ///     calendar (Calendar): The calendar the dates roll on.
    ///     convention (BusinessDayConvention): The rule every date but the last is rolled under.
    ///     rule (DateGeneration): The date-generation rule; defaults to Forward.
    ///     termination_convention (BusinessDayConvention | None): The rule the last date is rolled under;
    ///         None applies convention to it as well.
    ///
    /// Raises:
    ///     ItofinError: If start is not strictly before end.
    #[new]
    #[pyo3(signature = (
        start,
        end,
        frequency,
        calendar,
        convention,
        rule = PyDateGeneration::Forward,
        termination_convention = None,
    ))]
    fn new(
        start: &PyDate,
        end: &PyDate,
        frequency: &PyFrequency,
        calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        rule: PyDateGeneration,
        termination_convention: Option<PyBusinessDayConvention>,
    ) -> PyResult<Self> {
        if start.inner() >= end.inner() {
            return Err(ItofinError::new_err(format!(
                "schedule start ({}) is not strictly before end ({})",
                start.inner(),
                end.inner()
            )));
        }
        let convention = convention.inner();
        let termination_convention = termination_convention
            .map(|convention| convention.inner())
            .unwrap_or(convention);
        let inner = MakeSchedule::new()
            .from(start.inner())
            .to(end.inner())
            .with_frequency(frequency.inner())
            .with_calendar(calendar.inner())
            .with_convention(convention)
            .with_termination_date_convention(termination_convention)
            .with_rule(rule.inner())
            .build();
        Ok(PySchedule { inner })
    }

    /// The number of dates in the schedule.
    ///
    /// Returns:
    ///     int: The date count, one more than the number of periods.
    fn size(&self) -> usize {
        self.inner.dates().len()
    }

    /// The i-th date in the schedule.
    ///
    /// Args:
    ///     i (int): The zero-based index into the dates.
    ///
    /// Returns:
    ///     Date: The date at that index.
    ///
    /// Raises:
    ///     ItofinError: If i is out of range.
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

    /// All the schedule dates.
    ///
    /// Returns:
    ///     list[Date]: The dates in order, from the effective date to the termination date.
    fn dates(&self) -> Vec<PyDate> {
        self.inner
            .dates()
            .iter()
            .map(|&inner| PyDate { inner })
            .collect()
    }
}

impl PySchedule {
    /// The wrapped core Schedule (clone), for the swap facades in X2.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Schedule {
        self.inner.clone()
    }
}

/// Whether date is an IMM date.
///
/// An IMM date is the third Wednesday of the month, and of March, June,
/// September or December only when main_cycle is set.
///
/// Args:
///     date (Date): The date to test.
///     main_cycle (bool): Restrict the test to the March/June/September/December
///         cycle.
///
/// Returns:
///     bool: True when date is an IMM date under the selected cycle.
#[pyfunction]
#[pyo3(signature = (date, main_cycle = false))]
fn is_imm_date(date: &PyDate, main_cycle: bool) -> bool {
    imm::is_imm_date(date.inner(), main_cycle)
}

/// The next IMM date strictly following date.
///
/// Args:
///     date (Date): The date to start from; the result is strictly after it.
///     main_cycle (bool): Restrict the result to the March/June/September/December
///         cycle.
///
/// Returns:
///     Date: The next IMM date under the selected cycle.
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
