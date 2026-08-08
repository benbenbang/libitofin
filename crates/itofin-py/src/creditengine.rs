//! Facades for the two credit-default-swap engines: [`PyMidPointCdsEngine`] and
//! [`PyIsdaCdsEngine`].
//!
//! The mid-point engine prices each live premium period of a
//! [`CreditDefaultSwap`](crate::credit::PyCreditDefaultSwap) against the default
//! probability over that period, placing the default at the period's mid-point,
//! and discounts on a separate yield curve. The ISDA engine prices the same
//! contract the way the ISDA standard model does, integrating both legs over the
//! pillar dates of the two curves rather than over the premium schedule alone.
//!
//! Deferred (visible): the core's `include_settlement_date_flows` override
//! (`midpointcdsengine.rs:73`, `isdacdsengine.rs:127`) is not exposed on either
//! engine and is always passed as `None`, so the settlement-date flow decision
//! follows the settings' own flags - the shape the C++ cached-value fixture uses
//! (`creditdefaultswap.cpp:96`).

use crate::credit::PyDefaultProbabilityTermStructure;
use crate::curve::PyYieldTermStructure;
use crate::settings::PySettings;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::MidPointCdsEngine;
use libitofin::pricingengines::credit::IsdaCdsEngine;
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

/// Python `IsdaCdsEngine`: the ISDA standard-model credit-default-swap engine
/// (`pricingengines::credit::isdacdsengine::IsdaCdsEngine`).
///
/// Both legs are integrated over the pillar dates of the two curves the engine
/// is built with, so the price follows the standard model rather than the
/// premium schedule alone.
///
/// The model is specified against curves of a fixed shape, and the engine
/// refuses anything else when it prices (`isdacdsengine.rs:162-205`): both
/// curves must count Act/365 (Fixed) and be referenced at the evaluation date,
/// and the contract must settle its accrual, pay at the default time and carry a
/// face-value claim. Construction is infallible, as for
/// [`PyMidPointCdsEngine`], so every one of those is reported as
/// [`struct@crate::ItofinError`] from
/// [`npv`](crate::credit::PyCreditDefaultSwap::npv), not from `__init__`.
///
/// The `settings` passed here must be the same object the instrument this
/// engine prices was built with, or the two resolve their dates against
/// different evaluation dates and the NPV is silently wrong.
///
/// Deferred (visible): the three fidelity flags are left at the C++ defaults
/// `Taylor` / `HalfDayBias` / `Piecewise` that the core constructor bakes in
/// (`isdacdsengine.rs:139-141`); the `with_fidelity` builder that chooses them
/// (`:148-158`) and the three enums it takes are not exposed (#814).
#[pyclass(name = "IsdaCdsEngine", unsendable)]
pub struct PyIsdaCdsEngine {
    inner: SharedMut<IsdaCdsEngine>,
}

#[pymethods]
impl PyIsdaCdsEngine {
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
        PyIsdaCdsEngine {
            inner: shared_mut(IsdaCdsEngine::new(
                probability.handle(),
                recovery,
                discount.handle(),
                None,
                settings.inner(),
            )),
        }
    }
}

impl PyIsdaCdsEngine {
    /// The erased engine the instrument facades install via
    /// `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
