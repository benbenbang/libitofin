//! Facades for the credit bootstrap helpers: the [`PyDefaultProbabilityHelper`]
//! base and the concrete [`PySpreadCdsHelper`].
//!
//! The credit twin of [`crate::helpers`], which carries the yield-side rate
//! helpers, split the same way the core splits them: a credit helper fits a
//! default-probability curve, not a yield curve, so it implements a separate
//! trait (`defaultprobabilityhelpers.rs:66`) and cannot share the `dyn
//! RateHelper` base. The base holds the already-upcast
//! `Shared<dyn DefaultProbabilityHelper>` and the concrete subclasses supply
//! only their constructors, mirroring the [`crate::credit`] base/subclass idiom.
//!
//! `UpfrontCdsHelper` (`defaultprobabilityhelpers.hpp:170`) has no core port yet
//! and is omitted here rather than stubbed; it follows within EPIC Credit
//! (#676).

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDateGeneration, PyDayCounter, PyFrequency,
    PyPeriod,
};
use libitofin::shared::Shared;
use libitofin::termstructures::credit::defaultprobabilityhelpers::{
    DefaultProbabilityHelper, SpreadCdsHelper,
};
use libitofin::types::Integer;
use pyo3::prelude::*;

/// Python `DefaultProbabilityHelper`: the shared base for every credit
/// bootstrap helper
/// (`termstructures::credit::defaultprobabilityhelpers::DefaultProbabilityHelper`).
///
/// Holds the erased `Shared<dyn DefaultProbabilityHelper>` and exposes the two
/// dates the bootstrap places a curve node by. Concrete helpers such as
/// [`PySpreadCdsHelper`] subclass this and supply only their constructor.
#[pyclass(name = "DefaultProbabilityHelper", subclass, unsendable)]
pub struct PyDefaultProbabilityHelper {
    inner: Shared<dyn DefaultProbabilityHelper>,
}

#[pymethods]
impl PyDefaultProbabilityHelper {
    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// The latest date the helper needs curve data at (equal to the pillar
    /// date).
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }
}

impl PyDefaultProbabilityHelper {
    /// The base half of a concrete helper's [`PyClassInitializer`] chain.
    pub(crate) fn from_shared(inner: Shared<dyn DefaultProbabilityHelper>) -> Self {
        PyDefaultProbabilityHelper { inner }
    }

    /// A clone of the upcast helper, for the piecewise credit-curve facade,
    /// which takes a list of helpers and threads each into the bootstrap.
    #[allow(dead_code)]
    pub(crate) fn shared(&self) -> Shared<dyn DefaultProbabilityHelper> {
        Shared::clone(&self.inner)
    }
}

/// Python `SpreadCdsHelper`: the bootstrap helper fitting a CDS quoted as a
/// running spread
/// (`termstructures::credit::defaultprobabilityhelpers::SpreadCdsHelper`).
///
/// The helper rebuilds its schedule and its contract off the evaluation date
/// held by `settings`, so it tracks that date rather than freezing a maturity
/// at construction. It retains the caller's [`PySimpleQuote`], so a later
/// `set_value` re-drives the bootstrap, and it observes `discount_curve`.
///
/// Fallible, unlike the [`PyFlatHazardRate`](crate::credit::PyFlatHazardRate)
/// chain it otherwise mirrors: the core rejects the three post-Big-Bang rules
/// (`DateGeneration.OldCDS` / `.CDS` / `.CDS2015`), whose maturity comes from an
/// unported `cdsMaturity` (`defaultprobabilityhelpers.rs:314-319`). Passing one
/// raises [`struct@crate::ItofinError`] instead of building a schedule that ends
/// on the wrong date.
#[pyclass(name = "SpreadCdsHelper", extends = PyDefaultProbabilityHelper, unsendable)]
pub struct PySpreadCdsHelper;

#[pymethods]
impl PySpreadCdsHelper {
    /// A helper fitting `running_spread` over `tenor`, on the C++ default CDS
    /// terms.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        running_spread: &PySimpleQuote,
        tenor: &PyPeriod,
        settlement_days: Integer,
        calendar: &PyCalendar,
        frequency: &PyFrequency,
        payment_convention: &PyBusinessDayConvention,
        rule: PyDateGeneration,
        day_counter: &PyDayCounter,
        recovery_rate: f64,
        discount_curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helper = SpreadCdsHelper::new(
            running_spread.handle(),
            tenor.inner(),
            settlement_days,
            calendar.inner(),
            frequency.inner(),
            payment_convention.inner(),
            rule.inner(),
            day_counter.inner(),
            recovery_rate,
            discount_curve.handle(),
            settings.inner(),
        )
        .map_err(PyQlError::from)? as Shared<dyn DefaultProbabilityHelper>;
        Ok(
            PyClassInitializer::from(PyDefaultProbabilityHelper::from_shared(helper))
                .add_subclass(PySpreadCdsHelper),
        )
    }
}
