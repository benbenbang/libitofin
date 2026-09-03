//! Facade for the D5 evaluation-date [`Settings`].

use crate::time::PyDate;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Date;
use pyo3::prelude::*;

/// The explicit, non-global evaluation-date store (D5).
///
/// There is no global singleton: the exact settings object passed to a
/// construction is the one it reads, so instruments built against different
/// Settings do not see each other's evaluation date.
#[pyclass(name = "Settings", unsendable)]
pub struct PySettings {
    inner: Shared<Settings<Date>>,
}

#[pymethods]
impl PySettings {
    /// Create settings with no evaluation date set.
    #[new]
    fn new() -> Self {
        PySettings {
            inner: shared(Settings::<Date>::new()),
        }
    }

    /// Set the evaluation date, notifying observers if it changed.
    ///
    /// The new date is in place before the notification goes out, so an
    /// observer that recomputes on the update reads the date that triggered it.
    ///
    /// Args:
    ///     date (time.Date): The new evaluation date. Observers are notified only when this
    ///         differs from the date already set.
    fn set_evaluation_date(&self, date: &PyDate) {
        self.inner.set_evaluation_date(date.inner());
    }

    /// Set whether cash flows on today's date enter an NPV; None clears.
    ///
    /// The flag is three-valued, as in the core.
    ///
    /// Args:
    ///     value (bool | None): True or False decides the question outright; None clears it,
    ///         restoring the unset state in which each pricing site applies its
    ///         own default. The argument is required, so clearing is always
    ///         deliberate.
    #[pyo3(signature = (value))]
    fn set_include_todays_cash_flows(&self, value: Option<bool>) {
        self.inner.set_include_todays_cash_flows(value);
    }

    /// Return the current setting, or None while it is unset.
    ///
    /// Returns:
    ///     bool | None: The three-valued flag last set, or None if it has never been set or
    ///     was cleared.
    fn include_todays_cash_flows(&self) -> Option<bool> {
        self.inner.include_todays_cash_flows()
    }
}

impl PySettings {
    /// Clones the inner `Shared` so downstream facades can thread the same
    /// settings object into their constructions.
    pub(crate) fn inner(&self) -> Shared<Settings<Date>> {
        Shared::clone(&self.inner)
    }
}
