//! Facade for the D5 evaluation-date [`Settings`].

use crate::time::PyDate;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Date;
use pyo3::prelude::*;

/// Python `Settings`: the explicit, non-global evaluation-date store (D5).
///
/// Wraps a `Shared<Settings<Date>>` so the exact same object threads into every
/// option built against it; there is no global singleton. The inner `Shared` is
/// `Rc`-based and therefore `!Send`, hence `unsendable`.
#[pyclass(name = "Settings", unsendable)]
pub struct PySettings {
    inner: Shared<Settings<Date>>,
}

#[pymethods]
impl PySettings {
    #[new]
    fn new() -> Self {
        PySettings {
            inner: shared(Settings::<Date>::new()),
        }
    }

    /// Sets the evaluation date, notifying observers if it changed.
    fn set_evaluation_date(&self, date: &PyDate) {
        self.inner.set_evaluation_date(date.inner());
    }

    /// Sets whether cash flows falling on today's date enter an NPV.
    ///
    /// The flag is three-valued, as in the core (`settings.rs:141`): `True` and
    /// `False` decide the question outright, and `None` clears it, restoring the
    /// unset state in which each pricing site applies its own default. `value`
    /// is required, so clearing is always deliberate.
    #[pyo3(signature = (value))]
    fn set_include_todays_cash_flows(&self, value: Option<bool>) {
        self.inner.set_include_todays_cash_flows(value);
    }

    /// The current setting of
    /// [`set_include_todays_cash_flows`](Self::set_include_todays_cash_flows),
    /// or `None` while it is unset.
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
