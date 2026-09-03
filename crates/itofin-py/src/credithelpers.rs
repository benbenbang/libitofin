//! Facades for the credit bootstrap helpers: the DefaultProbabilityHelper base
//! and the concrete SpreadCdsHelper.
//!
//! The credit twin of helpers(), which carries the yield-side rate helpers,
//! split the same way the core splits them: a credit helper fits a
//! default-probability curve, not a yield curve, so it implements a separate
//! trait and cannot share the RateHelper base. The base holds the helper
//! already upcast and type-erased, and the concrete subclasses supply only
//! their constructors, mirroring the credit() base/subclass idiom.
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

/// Shared base for every credit bootstrap helper.
///
/// A credit helper fits a default-probability curve rather than a yield curve,
/// so it is a separate hierarchy from RateHelper. It exposes the two dates the
/// bootstrap places a curve node by.
#[pyclass(name = "DefaultProbabilityHelper", subclass, unsendable)]
pub struct PyDefaultProbabilityHelper {
    inner: Shared<dyn DefaultProbabilityHelper>,
}

#[pymethods]
impl PyDefaultProbabilityHelper {
    /// Return the date the curve node this helper sets sits at.
    ///
    /// Returns:
    ///     Date: The pillar date.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// Return the latest date the helper needs curve data at.
    ///
    /// Returns:
    ///     Date: The latest date, equal to the pillar date.
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }
}

impl PyDefaultProbabilityHelper {
    /// The base half of a concrete helper's initializer chain.
    pub(crate) fn from_shared(inner: Shared<dyn DefaultProbabilityHelper>) -> Self {
        PyDefaultProbabilityHelper { inner }
    }

    /// A clone of the upcast helper, for the piecewise credit-curve facade,
    /// which takes a list of helpers and threads each into the bootstrap.
    pub(crate) fn shared(&self) -> Shared<dyn DefaultProbabilityHelper> {
        Shared::clone(&self.inner)
    }
}

/// Bootstrap helper fitting a CDS quoted as a running spread.
///
/// The helper rebuilds its schedule and its contract off the evaluation date
/// held by settings, so it tracks that date rather than freezing a maturity at
/// construction. It retains the caller's quote, so a later set_value re-drives
/// the bootstrap, and it observes the discount curve.
///
/// Fallible: under the three post-Big-Bang date-generation rules the maturity
/// is rolled by the CDS maturity convention, which raises ItofinError on a
/// tenor it cannot roll rather than building a schedule that ends on the wrong
/// date.
#[pyclass(name = "SpreadCdsHelper", extends = PyDefaultProbabilityHelper, unsendable)]
pub struct PySpreadCdsHelper;

#[pymethods]
impl PySpreadCdsHelper {
    /// Build the helper on the C++ default CDS terms.
    ///
    /// Args:
    ///     running_spread (SimpleQuote): The quoted spread the helper fits.
    ///     tenor (Period): The length of the contract.
    ///     settlement_days (int): The days between the evaluation date and the
    ///         contract's start.
    ///     calendar (Calendar): The calendar the schedule rolls on.
    ///     frequency (Frequency): The premium payment frequency.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         payment dates.
    ///     rule (DateGeneration): The schedule generation rule.
    ///     day_counter (DayCounter): The day count the premium accrues on.
    ///     recovery_rate (float): The recovery assumed on default.
    ///     discount_curve (YieldTermStructure): The curve the flows discount
    ///         on; the helper observes it.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the schedule is rebuilt off.
    ///
    /// Raises:
    ///     ItofinError: Under the three post-Big-Bang rules OldCDS, CDS and
    ///         CDS2015, whose maturity is rolled by the CDS maturity rule: it
    ///         refuses a tenor it cannot roll, or one it rolls to a contract
    ///         that has already matured, rather than building a schedule that
    ///         ends on the wrong date.
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
