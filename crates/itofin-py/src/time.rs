//! Facades for the time primitives: [`PyDate`], [`PyDayCounter`], [`PyCalendar`].

use crate::ItofinError;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::calendars::{NullCalendar, Target, WeekendsOnly};
use libitofin::time::date::{Date, Month};
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

    /// Wraps a core [`DayCounter`] a facade read back off an object it built.
    ///
    /// The result carries no factory identity, so it compares only through its
    /// `repr`.
    pub(crate) fn from_inner(inner: DayCounter) -> Self {
        PyDayCounter { inner }
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

    /// A facade over a core [`Period`], for the inspectors that return one.
    pub(crate) fn from_inner(inner: Period) -> Self {
        PyPeriod { inner }
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

    /// The weekends-only calendar: every Saturday and Sunday is a holiday and no
    /// other day is.
    ///
    /// The ISDA CDS conventions roll on it, and it is not substitutable by
    /// [`Self::null_calendar`] (which holds no holidays at all) or by a national
    /// calendar (which adds public holidays).
    #[staticmethod]
    fn weekends_only() -> Self {
        PyCalendar {
            inner: WeekendsOnly::new(),
        }
    }

    /// The UK settlement calendar, the one the RPI inflation fixtures roll on.
    ///
    /// Only [`Market::Settlement`] is exposed: the Exchange and Metals markets
    /// share an identical `is_business_day` body in the core
    /// (`unitedkingdom.rs:60-61`) and differ solely in `name()`, so a market
    /// argument would select between three calendars that behave alike.
    #[staticmethod]
    fn united_kingdom() -> Self {
        PyCalendar {
            inner: UnitedKingdom::new(Market::Settlement),
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

/// Python `Frequency`: the coupon and fixing frequencies the fixtures need.
///
/// A fieldless pyo3 enum exposing `Frequency.Annual` / `Frequency.Semiannual` /
/// `Frequency.Quarterly` / `Frequency.Monthly`; only the variants the
/// Jamshidian, CDS and inflation fixtures use are surfaced. `Monthly` is the
/// frequency every ported inflation index publishes at, and the only one
/// [`ZeroInflationTermStructure`](crate::inflation::PyZeroInflationTermStructure)
/// fixtures build curves under. New variants are appended, so the pyo3
/// discriminants of the existing ones are unchanged.
#[pyclass(name = "Frequency", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFrequency {
    Annual,
    Semiannual,
    Quarterly,
    Monthly,
}

impl PyFrequency {
    /// The core [`Frequency`] this variant stands for.
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
    /// [`struct@ItofinError`] rather than mapped onto a neighbour.
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

/// Python `DateGeneration`: the rule a `Schedule` generates its dates by
/// (core `time::dategenerationrule::DateGeneration`).
///
/// A fieldless pyo3 enum exposing every rule the core ports. `Backward` and
/// `Forward` roll from one end of the range to the other; `Zero` keeps only the
/// two endpoints; the `ThirdWednesday` and `Twentieth` families snap the
/// interior dates onto an IMM Wednesday or the twentieth of the month, the
/// convention CDS schedules are quoted under.
///
/// The three post-Big-Bang rules (`OldCDS`, `CDS`, `CDS2015`) are surfaced here
/// because a `Schedule` builds under them, but
/// [`SpreadCdsHelper`](crate::credithelpers::PySpreadCdsHelper) rejects them:
/// their maturity comes from `cdsMaturity`, which the core has not ported
/// (`defaultprobabilityhelpers.rs:314-319`). That rejection surfaces as an
/// [`struct@crate::ItofinError`] rather than a silently wrong schedule.
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
    /// The core [`DateGeneration`] this variant stands for.
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
///
/// `rule` selects the date-generation rule and defaults to
/// [`PyDateGeneration::Forward`], which is exactly what the builder's
/// `forwards()` sets (`schedule.rs:852-853`), so an omitted `rule` reproduces
/// every schedule this facade built before. The rule-dependent `panic!`s in
/// `Schedule::new` (`schedule.rs:151-189`) all guard a non-null `first_date` or
/// `next_to_last_date`, neither of which this constructor supplies, so no rule
/// reaches them.
///
/// `termination_convention` rolls the last date only (`schedule.rs:415-421`)
/// and defaults to `convention`, reproducing every schedule this facade built
/// before it took one. CDS conventions need the two to differ: a credit helper
/// leaves its maturity unadjusted (`defaultprobabilityhelpers.rs:512`) while
/// paying `Following`, so under a twentieth rule a maturity landing on a
/// weekend stays on the twentieth. Rolling it instead lengthens the contract by
/// the roll, which the bootstrap round trip measures as a 3.6e-6 spread error
/// on the one pillar it hits.
#[pyclass(name = "Schedule", unsendable)]
pub struct PySchedule {
    inner: Schedule,
}

#[pymethods]
impl PySchedule {
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
