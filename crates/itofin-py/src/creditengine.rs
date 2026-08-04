//! Facade for the mid-point credit-default-swap engine:
//! [`PyMidPointCdsEngine`].
//!
//! The engine prices each live premium period of a
//! [`CreditDefaultSwap`](crate::credit::PyCreditDefaultSwap) against the default
//! probability over that period, placing the default at the period's mid-point,
//! and discounts on a separate yield curve.
//!
//! Deferred (visible): the core's `include_settlement_date_flows` override
//! (`midpointcdsengine.rs:73`) is not exposed and is always passed as `None`, so
//! the settlement-date flow decision follows the settings' own flags - the
//! shape the C++ cached-value fixture uses (`creditdefaultswap.cpp:96`).

use crate::credit::PyDefaultProbabilityTermStructure;
use crate::curve::PyYieldTermStructure;
use crate::settings::PySettings;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::MidPointCdsEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;

/// Python `MidPointCdsEngine`: the mid-point credit-default-swap engine
/// (`pricingengines::credit::midpointcdsengine::MidPointCdsEngine`).
///
/// Infallible at construction: it only stores the two curve handles, the
/// recovery rate and the settings, and registers as an observer of both curves.
/// Every precondition (an empty handle, an unset evaluation date) is reported
/// when the instrument is priced.
///
/// The `settings` passed here must be the same object the instrument this
/// engine prices was built with, or the two resolve their dates against
/// different evaluation dates and the NPV is silently wrong.
#[pyclass(name = "MidPointCdsEngine", unsendable)]
pub struct PyMidPointCdsEngine {
    inner: SharedMut<MidPointCdsEngine>,
}

#[pymethods]
impl PyMidPointCdsEngine {
    /// An engine reading default probabilities off `probability`, paying
    /// `1 - recovery` of the notional on a default, and discounting on
    /// `discount`.
    #[new]
    fn new(
        probability: &PyDefaultProbabilityTermStructure,
        recovery: f64,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> Self {
        PyMidPointCdsEngine {
            inner: shared_mut(MidPointCdsEngine::new(
                probability.handle(),
                recovery,
                discount.handle(),
                None,
                settings.inner(),
            )),
        }
    }
}

impl PyMidPointCdsEngine {
    /// The erased engine the instrument facades install via
    /// `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
