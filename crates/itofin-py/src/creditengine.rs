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

/// The mid-point credit-default-swap engine: each live premium period is
/// priced against the default probability over that period, with the default
/// placed at the period's mid-point.
///
/// Infallible at construction - every precondition (an empty curve handle, an
/// unset evaluation date) is reported when the contract is priced. The core's
/// include_settlement_date_flows override is not exposed and is always None,
/// so the settlement-date flow decision follows the settings' own flags. The
/// contract this engine prices must carry the same Settings object.
#[pyclass(name = "MidPointCdsEngine", unsendable)]
pub struct PyMidPointCdsEngine {
    inner: SharedMut<MidPointCdsEngine>,
}

#[pymethods]
impl PyMidPointCdsEngine {
    /// Build an engine over a default-probability curve and a discount curve.
    ///
    /// Args:
    ///     probability (DefaultProbabilityTermStructure): The curve default
    ///         probabilities are read off.
    ///     recovery (float): The recovery rate; a default pays 1 - recovery of
    ///         the notional.
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         the contract this engine prices was built with.
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

/// How the ISDA engine keeps the integrands' f + h denominators away from zero.
///
/// NoFix adds 10^-50 to them instead; Taylor, the default, replaces the
/// quotient by its Taylor expansion once f + h falls below 10^-4. Spelled NoFix
/// rather than C++'s None, which Python cannot name.
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

/// Whether the premium leg carries the standard model's half-day accrual bias.
///
/// The bias shifts the accrual's tstart back by 1/730 of a year. HalfDayBias,
/// the default, includes it as the model's C code does before version 1.8.2;
/// NoBias leaves it out, as from 1.8.2 on.
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

/// How the ISDA engine treats forward rates inside a coupon period.
///
/// Piecewise, the default, subdivides each period at the integration grid's own
/// nodes; Flat integrates each period in a single step. The two part only where
/// the grid has nodes strictly inside a coupon period, so two flat curves price
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

/// The ISDA standard-model credit-default-swap engine: both legs are
/// integrated over the pillar dates of the two curves the engine is built with
/// rather than over the premium schedule alone.
///
/// Infallible at construction, like MidPointCdsEngine. The model is specified
/// against curves of a fixed shape, so every check - both curves counting
/// Act/365 (Fixed) and referenced at the evaluation date, the contract settling
/// its accrual, paying at the default time and carrying a face-value claim - is
/// reported as ItofinError when the contract is priced, not from __init__. The
/// core's include_settlement_date_flows override is not exposed and is always
/// None. The three fidelity flags are trailing keyword arguments defaulting to
/// the C++ defaults Taylor / HalfDayBias / Piecewise, so an engine built
/// without them prices as before; they are taken here rather than through a
/// with_fidelity method because the core builder consumes the engine while
/// set_isda_engine has already cloned it into the contract. The contract this
/// engine prices must carry the same Settings object.
#[pyclass(name = "IsdaCdsEngine", unsendable)]
pub struct PyIsdaCdsEngine {
    inner: SharedMut<IsdaCdsEngine>,
}

#[pymethods]
impl PyIsdaCdsEngine {
    /// Build an ISDA standard-model engine under the three fidelity flags.
    ///
    /// Args:
    ///     probability (DefaultProbabilityTermStructure): The curve default
    ///         probabilities are read off; must count Act/365 (Fixed) and be
    ///         referenced at the evaluation date, checked at pricing time.
    ///     recovery (float): The recovery rate; a default pays 1 - recovery of
    ///         the notional.
    ///     discount (YieldTermStructure): The curve both legs are discounted
    ///         on, under the same shape requirement.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         the contract this engine prices was built with.
    ///     numerical_fix (NumericalFix): How the integrand denominators are
    ///         kept away from zero. Defaults to Taylor.
    ///     accrual_bias (AccrualBias): Whether the premium leg carries the
    ///         half-day accrual bias. Defaults to HalfDayBias.
    ///     forwards_in_coupon_period (ForwardsInCouponPeriod): How forward
    ///         rates inside a coupon period are integrated. Defaults to
    ///         Piecewise.
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
