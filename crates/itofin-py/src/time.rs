//! Facades for the time primitives: Date, DayCounter, Calendar.

use crate::ItofinError;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::{
    Argentina, Australia, Austria, Botswana, Brazil, Canada, Chile, China, Croatia, CzechRepublic,
    Denmark, Finland, France, Germany, HongKong, Hungary, Iceland, India, Indonesia, Israel, Italy,
    Japan, JointCalendar, JointCalendarRule, Malta, Mexico, Montenegro, NewZealand, NorthMacedonia,
    Norway, NullCalendar, Poland, Romania, Russia, SaudiArabia, Serbia, Singapore, Slovakia,
    Slovenia, SouthAfrica, SouthKorea, Sweden, Switzerland, Taiwan, Target, Thailand, Turkey,
    Ukraine, UnitedKingdom, UnitedStates, Uzbekistan, WeekendsOnly,
};
use libitofin::time::date::{Date, Month, SerialNumber, Year};
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
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_methods_from_python, gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction,
    gen_stub_pymethods,
};
use pyo3_stub_gen::inventory::submit;
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
#[gen_stub_pyclass]
#[pyclass(name = "Date", unsendable, module = "itofin.time")]
pub struct PyDate {
    inner: Date,
}

#[gen_stub_pymethods]
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
    #[gen_stub(skip)]
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

submit! {
    gen_methods_from_python! {
        r#"
        class PyDate:
            @overload
            def __sub__(self, days: int) -> Date:
                """Shift the date back by a number of calendar days.

                Args:
                    days (int): The number of calendar days to subtract.

                Returns:
                    Date: The shifted date.

                Raises:
                    ItofinError: If the result falls outside the representable date
                        range.
                """

            @overload
            def __sub__(self, other: Date) -> int:
                """The signed number of days from other to this date.

                Args:
                    other (Date): The date to measure from.

                Returns:
                    int: The signed day count between the two dates.
                """
        "#
    }
}

/// A year-fraction convention.
#[gen_stub_pyclass]
#[pyclass(name = "DayCounter", unsendable, module = "itofin.time")]
pub struct PyDayCounter {
    inner: DayCounter,
}

#[gen_stub_pymethods]
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
#[gen_stub_pyclass]
#[pyclass(name = "Period", unsendable, module = "itofin.time")]
pub struct PyPeriod {
    inner: Period,
}

#[gen_stub_pymethods]
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

/// Resolves a name against the `(name, variant)` table of one national
/// calendar (or of the joint-calendar rules), ignoring ASCII case, so "NYSE",
/// "Nyse" and "nyse" all select the same variant. An unknown name is an
/// ItofinError listing the accepted ones rather than a silent fall-through.
fn parse_name<M: Copy>(what: &str, value: &str, accepted: &[(&str, M)]) -> PyResult<M> {
    accepted
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value))
        .map(|&(_, variant)| variant)
        .ok_or_else(|| {
            let names: Vec<&str> = accepted.iter().map(|(name, _)| *name).collect();
            ItofinError::new_err(format!(
                "unknown {what} {value:?}, expected one of {}",
                names.join(", ")
            ))
        })
}

/// The core date behind `date`, or an ItofinError when it is the null date,
/// which no calendar query accepts. The same guard `adjust` applies inline.
fn non_null(date: &PyDate, action: &str) -> PyResult<Date> {
    if date.inner() == Date::null() {
        return Err(ItofinError::new_err(format!(
            "cannot {action} the null date"
        )));
    }
    Ok(date.inner())
}

impl PyCalendar {
    /// The core date behind `date`, once it is neither null nor past the
    /// calendar's tabulated horizon.
    ///
    /// The horizon guard stands in front of the core `assert!` in the
    /// Indonesia and Saudi Arabia calendars, so an out-of-table year surfaces
    /// as an ItofinError rather than a panic.
    fn checked(&self, date: &PyDate, action: &str) -> PyResult<Date> {
        let date = non_null(date, action)?;
        if let Some(horizon) = self.horizon
            && date.year() > horizon
        {
            return Err(ItofinError::new_err(format!(
                "cannot {action} a date in {}: the holidays of {} are tabulated only through {horizon}",
                date.year(),
                self.inner.name()
            )));
        }
        Ok(date)
    }
}

/// A business-day calendar.
#[gen_stub_pyclass]
#[pyclass(name = "Calendar", unsendable, module = "itofin.time")]
pub struct PyCalendar {
    inner: Calendar,
    /// The last year the calendar's holidays are tabulated for, when the core
    /// asserts on later dates (Indonesia, Saudi Arabia); None otherwise.
    horizon: Option<Year>,
}

#[gen_stub_pymethods]
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
            horizon: None,
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
            horizon: None,
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
            horizon: None,
        }
    }

    /// The Botswanan calendar.
    ///
    /// Returns:
    ///     Calendar: The Botswanan calendar.
    #[staticmethod]
    fn botswana() -> Self {
        PyCalendar {
            inner: Botswana::new(),
            horizon: None,
        }
    }

    /// The Danish calendar.
    ///
    /// Returns:
    ///     Calendar: The Danish calendar.
    #[staticmethod]
    fn denmark() -> Self {
        PyCalendar {
            inner: Denmark::new(),
            horizon: None,
        }
    }

    /// The Finnish calendar.
    ///
    /// Returns:
    ///     Calendar: The Finnish calendar.
    #[staticmethod]
    fn finland() -> Self {
        PyCalendar {
            inner: Finland::new(),
            horizon: None,
        }
    }

    /// The Hungarian calendar.
    ///
    /// Returns:
    ///     Calendar: The Hungarian calendar.
    #[staticmethod]
    fn hungary() -> Self {
        PyCalendar {
            inner: Hungary::new(),
            horizon: None,
        }
    }

    /// The Japanese calendar.
    ///
    /// Returns:
    ///     Calendar: The Japanese calendar.
    #[staticmethod]
    fn japan() -> Self {
        PyCalendar {
            inner: Japan::new(),
            horizon: None,
        }
    }

    /// The Norwegian calendar.
    ///
    /// Returns:
    ///     Calendar: The Norwegian calendar.
    #[staticmethod]
    fn norway() -> Self {
        PyCalendar {
            inner: Norway::new(),
            horizon: None,
        }
    }

    /// The South African calendar.
    ///
    /// Returns:
    ///     Calendar: The South African calendar.
    #[staticmethod]
    fn south_africa() -> Self {
        PyCalendar {
            inner: SouthAfrica::new(),
            horizon: None,
        }
    }

    /// The Swedish calendar.
    ///
    /// Returns:
    ///     Calendar: The Swedish calendar.
    #[staticmethod]
    fn sweden() -> Self {
        PyCalendar {
            inner: Sweden::new(),
            horizon: None,
        }
    }

    /// The Swiss calendar.
    ///
    /// Returns:
    ///     Calendar: The Swiss calendar.
    #[staticmethod]
    fn switzerland() -> Self {
        PyCalendar {
            inner: Switzerland::new(),
            horizon: None,
        }
    }

    /// The Thai calendar.
    ///
    /// Returns:
    ///     Calendar: The Thai calendar.
    #[staticmethod]
    fn thailand() -> Self {
        PyCalendar {
            inner: Thailand::new(),
            horizon: None,
        }
    }

    /// The Turkish calendar.
    ///
    /// Returns:
    ///     Calendar: The Turkish calendar.
    #[staticmethod]
    fn turkey() -> Self {
        PyCalendar {
            inner: Turkey::new(),
            horizon: None,
        }
    }

    /// The Argentine calendar.
    ///
    /// Args:
    ///     market (str): "Merval", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Argentine calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Merval"))]
    fn argentina(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::argentina::Market;
        let market = parse_name("Argentina market", market, &[("Merval", Market::Merval)])?;
        Ok(PyCalendar {
            inner: Argentina::new(market),
            horizon: None,
        })
    }

    /// The Australian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "ASX"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Australian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn australia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::australia::Market;
        let market = parse_name(
            "Australia market",
            market,
            &[("Settlement", Market::Settlement), ("ASX", Market::Asx)],
        )?;
        Ok(PyCalendar {
            inner: Australia::new(market),
            horizon: None,
        })
    }

    /// The Austrian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "Exchange"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Austrian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn austria(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::austria::Market;
        let market = parse_name(
            "Austria market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("Exchange", Market::Exchange),
            ],
        )?;
        Ok(PyCalendar {
            inner: Austria::new(market),
            horizon: None,
        })
    }

    /// The Brazilian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "Exchange"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Brazilian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn brazil(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::brazil::Market;
        let market = parse_name(
            "Brazil market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("Exchange", Market::Exchange),
            ],
        )?;
        Ok(PyCalendar {
            inner: Brazil::new(market),
            horizon: None,
        })
    }

    /// The Canadian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "TSX"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Canadian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn canada(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::canada::Market;
        let market = parse_name(
            "Canada market",
            market,
            &[("Settlement", Market::Settlement), ("TSX", Market::Tsx)],
        )?;
        Ok(PyCalendar {
            inner: Canada::new(market),
            horizon: None,
        })
    }

    /// The Chilean calendar.
    ///
    /// Args:
    ///     market (str): "SSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Chilean calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "SSE"))]
    fn chile(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::chile::Market;
        let market = parse_name("Chile market", market, &[("SSE", Market::Sse)])?;
        Ok(PyCalendar {
            inner: Chile::new(market),
            horizon: None,
        })
    }

    /// The Chinese calendar.
    ///
    /// Args:
    ///     market (str): One of "SSE", "IB"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Chinese calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "SSE"))]
    fn china(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::china::Market;
        let market = parse_name(
            "China market",
            market,
            &[("SSE", Market::Sse), ("IB", Market::Ib)],
        )?;
        Ok(PyCalendar {
            inner: China::new(market),
            horizon: None,
        })
    }

    /// The Croatian calendar.
    ///
    /// Args:
    ///     market (str): "ZSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Croatian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "ZSE"))]
    fn croatia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::croatia::Market;
        let market = parse_name("Croatia market", market, &[("ZSE", Market::Zse)])?;
        Ok(PyCalendar {
            inner: Croatia::new(market),
            horizon: None,
        })
    }

    /// The Czech calendar.
    ///
    /// Args:
    ///     market (str): "PSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Czech calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "PSE"))]
    fn czech_republic(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::czechrepublic::Market;
        let market = parse_name("CzechRepublic market", market, &[("PSE", Market::Pse)])?;
        Ok(PyCalendar {
            inner: CzechRepublic::new(market),
            horizon: None,
        })
    }

    /// The French calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "Exchange"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The French calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn france(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::france::Market;
        let market = parse_name(
            "France market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("Exchange", Market::Exchange),
            ],
        )?;
        Ok(PyCalendar {
            inner: France::new(market),
            horizon: None,
        })
    }

    /// The German calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "FrankfurtStockExchange", "Xetra", "Eurex", "Euwax"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The German calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn germany(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::germany::Market;
        let market = parse_name(
            "Germany market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("FrankfurtStockExchange", Market::FrankfurtStockExchange),
                ("Xetra", Market::Xetra),
                ("Eurex", Market::Eurex),
                ("Euwax", Market::Euwax),
            ],
        )?;
        Ok(PyCalendar {
            inner: Germany::new(market),
            horizon: None,
        })
    }

    /// The Hong Kong calendar.
    ///
    /// Args:
    ///     market (str): "HKEx", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Hong Kong calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "HKEx"))]
    fn hong_kong(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::hongkong::Market;
        let market = parse_name("HongKong market", market, &[("HKEx", Market::HKEx)])?;
        Ok(PyCalendar {
            inner: HongKong::new(market),
            horizon: None,
        })
    }

    /// The Icelandic calendar.
    ///
    /// Args:
    ///     market (str): "ICEX", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Icelandic calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "ICEX"))]
    fn iceland(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::iceland::Market;
        let market = parse_name("Iceland market", market, &[("ICEX", Market::Icex)])?;
        Ok(PyCalendar {
            inner: Iceland::new(market),
            horizon: None,
        })
    }

    /// The Indian calendar.
    ///
    /// Args:
    ///     market (str): "NSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Indian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "NSE"))]
    fn india(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::india::Market;
        let market = parse_name("India market", market, &[("NSE", Market::Nse)])?;
        Ok(PyCalendar {
            inner: India::new(market),
            horizon: None,
        })
    }

    /// The Indonesian calendar.
    ///
    /// Its public holidays are tabulated through 2014 only, as in QuantLib;
    /// every query on this calendar raises ItofinError for a later date
    /// rather than silently omitting holidays.
    ///
    /// Args:
    ///     market (str): One of "BEJ", "JSX", "IDX"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Indonesian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "BEJ"))]
    fn indonesia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::indonesia::Market;
        let market = parse_name(
            "Indonesia market",
            market,
            &[
                ("BEJ", Market::Bej),
                ("JSX", Market::Jsx),
                ("IDX", Market::Idx),
            ],
        )?;
        Ok(PyCalendar {
            inner: Indonesia::new(market),
            horizon: Some(libitofin::time::calendars::indonesia::HOLIDAY_HORIZON),
        })
    }

    /// The Israeli calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "TASE", "SHIR", "Telbor"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Israeli calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn israel(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::israel::Market;
        let market = parse_name(
            "Israel market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("TASE", Market::Tase),
                ("SHIR", Market::Shir),
                ("Telbor", Market::Telbor),
            ],
        )?;
        Ok(PyCalendar {
            inner: Israel::new(market),
            horizon: None,
        })
    }

    /// The Italian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "Exchange"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Italian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn italy(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::italy::Market;
        let market = parse_name(
            "Italy market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("Exchange", Market::Exchange),
            ],
        )?;
        Ok(PyCalendar {
            inner: Italy::new(market),
            horizon: None,
        })
    }

    /// The Maltese calendar.
    ///
    /// Args:
    ///     market (str): "MSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Maltese calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "MSE"))]
    fn malta(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::malta::Market;
        let market = parse_name("Malta market", market, &[("MSE", Market::Mse)])?;
        Ok(PyCalendar {
            inner: Malta::new(market),
            horizon: None,
        })
    }

    /// The Mexican calendar.
    ///
    /// Args:
    ///     market (str): "BMV", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Mexican calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "BMV"))]
    fn mexico(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::mexico::Market;
        let market = parse_name("Mexico market", market, &[("BMV", Market::Bmv)])?;
        Ok(PyCalendar {
            inner: Mexico::new(market),
            horizon: None,
        })
    }

    /// The Montenegrin calendar.
    ///
    /// Args:
    ///     market (str): "MNSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Montenegrin calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "MNSE"))]
    fn montenegro(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::montenegro::Market;
        let market = parse_name("Montenegro market", market, &[("MNSE", Market::Mnse)])?;
        Ok(PyCalendar {
            inner: Montenegro::new(market),
            horizon: None,
        })
    }

    /// The New Zealand calendar.
    ///
    /// Args:
    ///     market (str): One of "Wellington", "Auckland"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The New Zealand calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Wellington"))]
    fn new_zealand(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::newzealand::Market;
        let market = parse_name(
            "NewZealand market",
            market,
            &[
                ("Wellington", Market::Wellington),
                ("Auckland", Market::Auckland),
            ],
        )?;
        Ok(PyCalendar {
            inner: NewZealand::new(market),
            horizon: None,
        })
    }

    /// The North Macedonian calendar.
    ///
    /// Args:
    ///     market (str): "MSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The North Macedonian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "MSE"))]
    fn north_macedonia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::northmacedonia::Market;
        let market = parse_name("NorthMacedonia market", market, &[("MSE", Market::Mse)])?;
        Ok(PyCalendar {
            inner: NorthMacedonia::new(market),
            horizon: None,
        })
    }

    /// The Polish calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "WSE"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Polish calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn poland(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::poland::Market;
        let market = parse_name(
            "Poland market",
            market,
            &[("Settlement", Market::Settlement), ("WSE", Market::Wse)],
        )?;
        Ok(PyCalendar {
            inner: Poland::new(market),
            horizon: None,
        })
    }

    /// The Romanian calendar.
    ///
    /// Args:
    ///     market (str): One of "Public", "BVB"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Romanian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Public"))]
    fn romania(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::romania::Market;
        let market = parse_name(
            "Romania market",
            market,
            &[("Public", Market::Public), ("BVB", Market::Bvb)],
        )?;
        Ok(PyCalendar {
            inner: Romania::new(market),
            horizon: None,
        })
    }

    /// The Russian calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "MOEX"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Russian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn russia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::russia::Market;
        let market = parse_name(
            "Russia market",
            market,
            &[("Settlement", Market::Settlement), ("MOEX", Market::Moex)],
        )?;
        Ok(PyCalendar {
            inner: Russia::new(market),
            horizon: None,
        })
    }

    /// The Saudi Arabian calendar.
    ///
    /// Its Eid holidays are tabulated through 2022 only, as in QuantLib;
    /// every query on this calendar raises ItofinError for a later date
    /// rather than silently omitting holidays.
    ///
    /// Args:
    ///     market (str): "Tadawul", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Saudi Arabian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Tadawul"))]
    fn saudi_arabia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::saudiarabia::Market;
        let market = parse_name(
            "SaudiArabia market",
            market,
            &[("Tadawul", Market::Tadawul)],
        )?;
        Ok(PyCalendar {
            inner: SaudiArabia::new(market),
            horizon: Some(libitofin::time::calendars::saudiarabia::HOLIDAY_HORIZON),
        })
    }

    /// The Serbian calendar.
    ///
    /// Args:
    ///     market (str): "BSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Serbian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "BSE"))]
    fn serbia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::serbia::Market;
        let market = parse_name("Serbia market", market, &[("BSE", Market::Bse)])?;
        Ok(PyCalendar {
            inner: Serbia::new(market),
            horizon: None,
        })
    }

    /// The Singapore calendar.
    ///
    /// Args:
    ///     market (str): "SGX", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Singapore calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "SGX"))]
    fn singapore(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::singapore::Market;
        let market = parse_name("Singapore market", market, &[("SGX", Market::Sgx)])?;
        Ok(PyCalendar {
            inner: Singapore::new(market),
            horizon: None,
        })
    }

    /// The Slovak calendar.
    ///
    /// Args:
    ///     market (str): "BSSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Slovak calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "BSSE"))]
    fn slovakia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::slovakia::Market;
        let market = parse_name("Slovakia market", market, &[("BSSE", Market::Bsse)])?;
        Ok(PyCalendar {
            inner: Slovakia::new(market),
            horizon: None,
        })
    }

    /// The Slovenian calendar.
    ///
    /// Args:
    ///     market (str): "LSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Slovenian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "LSE"))]
    fn slovenia(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::slovenia::Market;
        let market = parse_name("Slovenia market", market, &[("LSE", Market::Lse)])?;
        Ok(PyCalendar {
            inner: Slovenia::new(market),
            horizon: None,
        })
    }

    /// The South Korean calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "KRX"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The South Korean calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn south_korea(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::southkorea::Market;
        let market = parse_name(
            "SouthKorea market",
            market,
            &[("Settlement", Market::Settlement), ("KRX", Market::Krx)],
        )?;
        Ok(PyCalendar {
            inner: SouthKorea::new(market),
            horizon: None,
        })
    }

    /// The Taiwanese calendar.
    ///
    /// Args:
    ///     market (str): "TSEC", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Taiwanese calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "TSEC"))]
    fn taiwan(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::taiwan::Market;
        let market = parse_name("Taiwan market", market, &[("TSEC", Market::Tsec)])?;
        Ok(PyCalendar {
            inner: Taiwan::new(market),
            horizon: None,
        })
    }

    /// The Ukrainian calendar.
    ///
    /// Args:
    ///     market (str): "USE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Ukrainian calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "USE"))]
    fn ukraine(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::ukraine::Market;
        let market = parse_name("Ukraine market", market, &[("USE", Market::Use)])?;
        Ok(PyCalendar {
            inner: Ukraine::new(market),
            horizon: None,
        })
    }

    /// The UK calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "Exchange", "Metals"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The UK calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn united_kingdom(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::unitedkingdom::Market;
        let market = parse_name(
            "UnitedKingdom market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("Exchange", Market::Exchange),
                ("Metals", Market::Metals),
            ],
        )?;
        Ok(PyCalendar {
            inner: UnitedKingdom::new(market),
            horizon: None,
        })
    }

    /// The US calendar.
    ///
    /// Args:
    ///     market (str): One of "Settlement", "NYSE", "GovernmentBond", "NERC", "LiborImpact", "FederalReserve", "SOFR"; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The US calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "Settlement"))]
    fn united_states(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::unitedstates::Market;
        let market = parse_name(
            "UnitedStates market",
            market,
            &[
                ("Settlement", Market::Settlement),
                ("NYSE", Market::Nyse),
                ("GovernmentBond", Market::GovernmentBond),
                ("NERC", Market::Nerc),
                ("LiborImpact", Market::LiborImpact),
                ("FederalReserve", Market::FederalReserve),
                ("SOFR", Market::Sofr),
            ],
        )?;
        Ok(PyCalendar {
            inner: UnitedStates::new(market),
            horizon: None,
        })
    }

    /// The Uzbek calendar.
    ///
    /// Args:
    ///     market (str): "UZSE", the only market; matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The Uzbek calendar for that market.
    ///
    /// Raises:
    ///     ItofinError: If market is not one of the accepted names.
    #[staticmethod]
    #[pyo3(signature = (market = "UZSE"))]
    fn uzbekistan(market: &str) -> PyResult<Self> {
        use libitofin::time::calendars::uzbekistan::Market;
        let market = parse_name("Uzbekistan market", market, &[("UZSE", Market::Uzse)])?;
        Ok(PyCalendar {
            inner: Uzbekistan::new(market),
            horizon: None,
        })
    }

    /// A calendar combining several others.
    ///
    /// Args:
    ///     calendars (list[Calendar]): The calendars to combine; at least one.
    ///     rule (str): "JoinHolidays" makes a day a holiday when it is one on
    ///         any calendar; "JoinBusinessDays" makes it a business day when it
    ///         is one on any calendar. Matched ignoring case.
    ///
    /// Returns:
    ///     Calendar: The joint calendar.
    ///
    /// Raises:
    ///     ItofinError: If calendars is empty or rule is not one of the two
    ///         accepted names.
    #[staticmethod]
    #[pyo3(signature = (calendars, rule = "JoinHolidays"))]
    fn joint(calendars: Vec<PyRef<'_, PyCalendar>>, rule: &str) -> PyResult<Self> {
        let rule = parse_name(
            "joint-calendar rule",
            rule,
            &[
                ("JoinHolidays", JointCalendarRule::JoinHolidays),
                ("JoinBusinessDays", JointCalendarRule::JoinBusinessDays),
            ],
        )?;
        if calendars.is_empty() {
            return Err(ItofinError::new_err(
                "a joint calendar needs at least one calendar",
            ));
        }
        let calendars = calendars.iter().map(|c| c.inner()).collect();
        Ok(PyCalendar {
            inner: JointCalendar::new(calendars, rule),
            horizon: None,
        })
    }

    /// The calendar's name, as the core reports it.
    #[getter]
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Whether date is a business day on this calendar.
    ///
    /// Args:
    ///     date (Date): The date to test.
    ///
    /// Returns:
    ///     bool: True when date is neither a weekend day nor a holiday.
    ///
    /// Raises:
    ///     ItofinError: If date is the null date or past the calendar's
    ///         tabulated horizon.
    fn is_business_day(&self, date: &PyDate) -> PyResult<bool> {
        Ok(self.inner.is_business_day(self.checked(date, "test")?))
    }

    /// Whether date is a holiday on this calendar, weekends included.
    ///
    /// Args:
    ///     date (Date): The date to test.
    ///
    /// Returns:
    ///     bool: True when date is not a business day.
    ///
    /// Raises:
    ///     ItofinError: If date is the null date or past the calendar's
    ///         tabulated horizon.
    fn is_holiday(&self, date: &PyDate) -> PyResult<bool> {
        Ok(self.inner.is_holiday(self.checked(date, "test")?))
    }

    /// Whether date falls on this calendar's weekend, which for a market whose
    /// weekend moved over time depends on the date and not only its weekday.
    ///
    /// Args:
    ///     date (Date): The date to test.
    ///
    /// Returns:
    ///     bool: True when date is a weekend day.
    ///
    /// Raises:
    ///     ItofinError: If date is the null date or past the calendar's
    ///         tabulated horizon.
    fn is_weekend(&self, date: &PyDate) -> PyResult<bool> {
        Ok(self.inner.is_weekend_on(self.checked(date, "test")?))
    }

    /// The holidays between two dates, both inclusive.
    ///
    /// Args:
    ///     from_date (Date): The first date of the range.
    ///     to_date (Date): The last date of the range.
    ///     include_weekends (bool): Also list the weekend days; off by default.
    ///
    /// Returns:
    ///     list[Date]: The holidays in the range, in order.
    ///
    /// Raises:
    ///     ItofinError: If either date is the null date, if to_date is before
    ///         from_date, or if a date is past the calendar's tabulated
    ///         horizon.
    #[pyo3(signature = (from_date, to_date, include_weekends = false))]
    fn holiday_list(
        &self,
        from_date: &PyDate,
        to_date: &PyDate,
        include_weekends: bool,
    ) -> PyResult<Vec<PyDate>> {
        let from = self.checked(from_date, "list holidays from")?;
        let to = self.checked(to_date, "list holidays to")?;
        if to < from {
            return Err(ItofinError::new_err(format!(
                "'from' date ({}) must be equal to or earlier than 'to' date ({})",
                from_date.__repr__(),
                to_date.__repr__()
            )));
        }
        Ok(self
            .inner
            .holiday_list(from, to, include_weekends)
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The number of business days between two dates.
    ///
    /// Args:
    ///     from_date (Date): The first date of the range.
    ///     to_date (Date): The last date of the range.
    ///     include_first (bool): Count from_date when it is a business day; on
    ///         by default.
    ///     include_last (bool): Count to_date when it is a business day; off by
    ///         default.
    ///
    /// Returns:
    ///     int: The business-day count, negated when from_date is after to_date.
    ///
    /// Raises:
    ///     ItofinError: If either date is the null date or past the
    ///         calendar's tabulated horizon.
    #[pyo3(signature = (from_date, to_date, include_first = true, include_last = false))]
    fn business_days_between(
        &self,
        from_date: &PyDate,
        to_date: &PyDate,
        include_first: bool,
        include_last: bool,
    ) -> PyResult<i32> {
        let from = self.checked(from_date, "count business days from")?;
        let to = self.checked(to_date, "count business days to")?;
        Ok(self
            .inner
            .business_days_between(from, to, include_first, include_last))
    }

    /// Roll a date to the nearest business day.
    ///
    /// Args:
    ///     date (Date): The date to roll.
    ///     convention (BusinessDayConvention): The rolling rule to apply.
    ///
    /// Returns:
    ///     Date: The adjusted date, unchanged when it is already a business day.
    ///
    /// Raises:
    ///     ItofinError: If date is the null date or past the calendar's
    ///         tabulated horizon.
    fn adjust(&self, date: &PyDate, convention: &PyBusinessDayConvention) -> PyResult<PyDate> {
        let date = self.checked(date, "adjust")?;
        Ok(PyDate::from_inner(
            self.inner.adjust(date, convention.inner()),
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
    ///     ItofinError: If unit is not one of the four accepted strings, or if
    ///         date is the null date or past the calendar's tabulated horizon.
    fn advance(
        &self,
        date: &PyDate,
        n: i32,
        unit: &str,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
    ) -> PyResult<PyDate> {
        let unit = parse_time_unit(unit)?;
        let date = self.checked(date, "advance")?;
        Ok(PyDate::from_inner(self.inner.advance(
            date,
            n,
            unit,
            convention.inner(),
            end_of_month,
        )))
    }

    /// Equality by calendar name, so two independently built TARGET calendars
    /// are equal and a calendar read back off a curve equals its factory call.
    ///
    /// Args:
    ///     other (object): The calendar to compare against.
    ///
    /// Returns:
    ///     bool: True when both carry the same name.
    fn __eq__(&self, other: &PyCalendar) -> bool {
        self.inner.name() == other.inner.name()
    }

    /// Hashes the calendar name, the field equality compares.
    ///
    /// Returns:
    ///     int: The hash of the name, so equal calendars hash equal.
    fn __hash__(&self) -> u64 {
        hash_of(&self.inner.name())
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
        PyCalendar {
            inner,
            horizon: None,
        }
    }
}

/// A coupon or fixing frequency.
///
/// Only the variants the ported fixtures use are surfaced; new ones are
/// appended, so the integer values of the existing variants are unchanged.
#[gen_stub_pyclass_enum]
#[pyclass(name = "Frequency", eq, eq_int, from_py_object, module = "itofin.time")]
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
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "BusinessDayConvention",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.time"
)]
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
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "DateGeneration",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.time"
)]
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
#[gen_stub_pyclass]
#[pyclass(name = "Schedule", unsendable, module = "itofin.time")]
pub struct PySchedule {
    inner: Schedule,
}

#[gen_stub_pymethods]
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
#[gen_stub_pyfunction(module = "itofin.time")]
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
#[gen_stub_pyfunction(module = "itofin.time")]
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
