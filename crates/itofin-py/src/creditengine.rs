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
use libitofin::pricingengines::credit::{
    AccrualBias, ForwardsInCouponPeriod, IsdaCdsEngine, NumericalFix,
};
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

/// Python `NumericalFix`: how the ISDA engine keeps the integrands'
/// `f_i + h_i` denominators away from zero (core
/// `pricingengines::credit::NumericalFix`).
///
/// `NumericalFix.NoFix` adds `10^-50` to the denominators instead; the default
/// `NumericalFix.Taylor` replaces the quotient by its Taylor expansion once
/// `f_i + h_i` falls below `10^-4`. The core already renames C++'s `None`
/// variant to `NoFix` (`isdacdsengine.rs:71`), which is also what Python needs:
/// `NumericalFix.None` is a syntax error.
#[pyclass(name = "NumericalFix", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyNumericalFix {
    NoFix,
    Taylor,
}

impl PyNumericalFix {
    /// The core [`NumericalFix`] this variant stands for.
    pub(crate) fn inner(self) -> NumericalFix {
        match self {
            PyNumericalFix::NoFix => NumericalFix::NoFix,
            PyNumericalFix::Taylor => NumericalFix::Taylor,
        }
    }
}

/// Python `AccrualBias`: whether the premium leg carries the ISDA standard
/// model's half-day accrual bias (core `pricingengines::credit::AccrualBias`).
///
/// The default `AccrualBias.HalfDayBias` includes the erroneous second term the
/// standard model's C code carries before version 1.8.2, which shifts the
/// accrual's `tstart` back by `1/730` of a year; `AccrualBias.NoBias` leaves it
/// out, as from 1.8.2 on.
#[pyclass(name = "AccrualBias", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyAccrualBias {
    HalfDayBias,
    NoBias,
}

impl PyAccrualBias {
    /// The core [`AccrualBias`] this variant stands for.
    pub(crate) fn inner(self) -> AccrualBias {
        match self {
            PyAccrualBias::HalfDayBias => AccrualBias::HalfDayBias,
            PyAccrualBias::NoBias => AccrualBias::NoBias,
        }
    }
}

/// Python `ForwardsInCouponPeriod`: how the ISDA engine treats forward rates
/// inside a coupon period (core
/// `pricingengines::credit::ForwardsInCouponPeriod`).
///
/// The default `ForwardsInCouponPeriod.Piecewise` subdivides each coupon period
/// at the integration grid's own nodes; `ForwardsInCouponPeriod.Flat`
/// integrates each period in a single step. The two part only where the grid
/// has nodes strictly inside a coupon period, so two flat curves price
/// identically under either.
#[pyclass(name = "ForwardsInCouponPeriod", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyForwardsInCouponPeriod {
    Flat,
    Piecewise,
}

impl PyForwardsInCouponPeriod {
    /// The core [`ForwardsInCouponPeriod`] this variant stands for.
    pub(crate) fn inner(self) -> ForwardsInCouponPeriod {
        match self {
            PyForwardsInCouponPeriod::Flat => ForwardsInCouponPeriod::Flat,
            PyForwardsInCouponPeriod::Piecewise => ForwardsInCouponPeriod::Piecewise,
        }
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
/// The three fidelity flags are trailing keyword arguments defaulting to the
/// C++ defaults `Taylor` / `HalfDayBias` / `Piecewise` the core constructor
/// bakes in (`isdacdsengine.rs:139-141`), so an engine built without them
/// prices exactly as before. They are taken here rather than through a
/// `with_fidelity` method because the core builder consumes the engine
/// (`:148-158`) while [`set_isda_engine`](crate::credit::PyCreditDefaultSwap)
/// has already cloned it into the contract, which a post-construction
/// reconfiguration would leave behind on the unconfigured engine.
#[pyclass(name = "IsdaCdsEngine", unsendable)]
pub struct PyIsdaCdsEngine {
    inner: SharedMut<IsdaCdsEngine>,
}

#[pymethods]
impl PyIsdaCdsEngine {
    /// An engine reading default probabilities off `probability`, paying
    /// `1 - recovery` of the notional on a default, and discounting on
    /// `discount`, under the three fidelity flags.
    #[new]
    #[pyo3(signature = (
        probability,
        recovery,
        discount,
        settings,
        numerical_fix = PyNumericalFix::Taylor,
        accrual_bias = PyAccrualBias::HalfDayBias,
        forwards_in_coupon_period = PyForwardsInCouponPeriod::Piecewise,
    ))]
    fn new(
        probability: &PyDefaultProbabilityTermStructure,
        recovery: f64,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
        numerical_fix: PyNumericalFix,
        accrual_bias: PyAccrualBias,
        forwards_in_coupon_period: PyForwardsInCouponPeriod,
    ) -> Self {
        PyIsdaCdsEngine {
            inner: shared_mut(
                IsdaCdsEngine::new(
                    probability.handle(),
                    recovery,
                    discount.handle(),
                    None,
                    settings.inner(),
                )
                .with_fidelity(
                    numerical_fix.inner(),
                    accrual_bias.inner(),
                    forwards_in_coupon_period.inner(),
                ),
            ),
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
