//! Flat hazard-rate curve.
//!
//! Port of `ql/termstructures/credit/flathazardrate.{hpp,cpp}`: a credit curve
//! quoting one hazard rate for every maturity, backed by a quote handle or a
//! plain value, with the closed-form survival probability `exp(-h t)`
//! (`flathazardrate.hpp:72-74`).
//!
//! ## Divergences from QuantLib
//!
//! - The C++ constructors register with the quote only on the two
//!   `Handle<Quote>` overloads (`flathazardrate.cpp:32,47`); the two `Rate`
//!   overloads wrap the value in a fresh `SimpleQuote` and skip the
//!   registration (`flathazardrate.cpp:39,55`). This port registers uniformly
//!   for all four, keeping the idiom of its direct sibling
//!   [`FlatForward`](crate::termstructures::yields::FlatForward). The C++ skip
//!   is an unobservable optimization rather than a behaviour: the wrapped quote
//!   is private and unreachable, so nothing can ever call `set_value` on it and
//!   the subscription can never fire.
//! - Unlike [`FlatForward`](crate::termstructures::yields::FlatForward) this
//!   curve caches nothing, since C++ reads the quote live on every
//!   `hazardRateImpl` (`flathazardrate.hpp:59`). The subscription therefore
//!   resets no state and exists only to forward quote notifications to the
//!   structure's own observers, which a consumer caching off this curve needs.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::hazardratestructure::HazardRateStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Rate, Real, Time};

/// Flat hazard-rate curve.
pub struct FlatHazardRate {
    base: TermStructureBase,
    hazard_rate: Handle<dyn Quote>,
    _listener: SharedMut<ResetThenNotify>,
}

impl FlatHazardRate {
    fn assemble(base: TermStructureBase, hazard_rate: Handle<dyn Quote>) -> FlatHazardRate {
        let listener = ResetThenNotify::delivering(base.updater(), || {});
        hazard_rate.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        FlatHazardRate {
            base,
            hazard_rate,
            _listener: listener,
        }
    }

    fn wrap(value: Rate) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    /// Quote-backed curve with a fixed reference date.
    pub fn new(
        reference_date: Date,
        hazard_rate: Handle<dyn Quote>,
        day_counter: DayCounter,
    ) -> FlatHazardRate {
        let base = TermStructureBase::with_reference_date(reference_date, None, Some(day_counter));
        Self::assemble(base, hazard_rate)
    }

    /// Value-backed curve with a fixed reference date.
    pub fn with_rate(
        reference_date: Date,
        hazard_rate: Rate,
        day_counter: DayCounter,
    ) -> FlatHazardRate {
        Self::new(reference_date, Self::wrap(hazard_rate), day_counter)
    }

    fn hazard_rate_value(&self) -> QlResult<Rate> {
        self.hazard_rate.current_link()?.value()
    }
}

impl AsObservable for FlatHazardRate {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for FlatHazardRate {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl HazardRateStructure for FlatHazardRate {
    fn hazard_rate_curve_impl(&self, _t: Time) -> QlResult<Rate> {
        self.hazard_rate_value()
    }
}

impl DefaultProbabilityTermStructure for FlatHazardRate {
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        Ok((-self.hazard_rate_value()? * t).exp())
    }

    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        self.default_density_from_hazard_rate(t)
    }

    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.hazard_rate_curve_impl(t)
    }
}
