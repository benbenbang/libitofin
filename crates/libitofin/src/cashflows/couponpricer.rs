//! Pricers for floating-rate coupons.
//!
//! Port of the `FloatingRateCouponPricer` base of
//! `ql/cashflows/couponpricer.hpp`. A [`FloatingRateCoupon`] does not compute
//! its own rate: it hands itself to a pricer and reads back
//! [`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate)
//! (`floatingratecoupon.cpp:88`). The pricer folds the coupon's gearing and
//! spread into that result, so the coupon reapplies neither.
//!
//! ## Scope of this slice
//!
//! Only [`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate) is exercised by
//! the base floating slice. [`caplet_rate`](FloatingRateCouponPricer::caplet_rate)
//! and [`floorlet_rate`](FloatingRateCouponPricer::floorlet_rate) belong to the
//! capped/floored slice, which needs an optionlet volatility surface not yet
//! ported. They are *required*, not provided: a real pricer must implement them
//! consciously, and a base-slice pricer implements them as an explicit `Err`
//! refusal rather than a silent wrong number.
//!
//! ## Divergences from QuantLib
//!
//! The C++ interface also declares `swapletPrice`, `capletPrice` and
//! `floorletPrice`. None is reached by the base slice: a coupon prices through
//! `amount() * discount`, not the pricer, so the `*Price` variants are omitted
//! until a consumer needs them.
//!
//! The C++ base is both `Observer` and `Observable` (its `update()` forwards
//! notifications). Here it is [`AsObservable`] only: the coupon registers as an
//! observer of the pricer, but a base-slice pricer observes nothing. The
//! `Observer` face (a pricer watching a volatility surface) arrives with the
//! cap/floor slice.

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable};
use crate::types::{Rate, Real, Spread};
use crate::{fail, require};

use super::floatingratecoupon::FloatingRateCoupon;

/// Generic pricer for floating-rate coupons.
///
/// A coupon registers as an observer of its pricer (via [`AsObservable`]) and,
/// on each rate query, calls [`initialize`](Self::initialize) then reads
/// [`swaplet_rate`](Self::swaplet_rate).
pub trait FloatingRateCouponPricer: AsObservable {
    /// Caches whatever the pricer needs from `coupon` before a rate is read
    /// (`FloatingRateCouponPricer::initialize`).
    fn initialize(&mut self, coupon: &FloatingRateCoupon);

    /// The coupon's rate, gearing and spread already folded in
    /// (`swapletRate`).
    fn swaplet_rate(&self) -> QlResult<Rate>;

    /// The swaplet rate for a caller-supplied `index_fixing`, gearing and spread
    /// folded in.
    ///
    /// The mode-aware entry: an [`IborCoupon`] threads its par or indexed
    /// forecast in here rather than let the pricer read the base coupon's
    /// natural forecast, which cannot see the par-coupon dates. Gearing, spread
    /// and the in-arrears refusal are the pricer's, as in
    /// [`swaplet_rate`](Self::swaplet_rate).
    ///
    /// [`IborCoupon`]: super::iborcoupon::IborCoupon
    fn swaplet_rate_for(&self, index_fixing: QlResult<Rate>) -> QlResult<Rate>;

    /// The rate of a caplet struck at `effective_cap` (`capletRate`).
    ///
    /// The cap/floor slice is not ported; a base-slice pricer returns `Err`.
    fn caplet_rate(&self, effective_cap: Rate) -> QlResult<Rate>;

    /// The rate of a floorlet struck at `effective_floor` (`floorletRate`).
    ///
    /// The cap/floor slice is not ported; a base-slice pricer returns `Err`.
    fn floorlet_rate(&self, effective_floor: Rate) -> QlResult<Rate>;
}

/// Black-formula pricer for ibor coupons - the swaplet path
/// (`BlackIborCouponPricer` in `ql/cashflows/couponpricer.{hpp,cpp}`).
///
/// The swaplet rate is `gearing * adjustedFixing + spread`
/// (`couponpricer.hpp:215`), and for a non-in-arrears coupon under the default
/// `Black76` timing the adjusted fixing reduces to the coupon's index fixing
/// with no convexity adjustment (`couponpricer.cpp adjustedFixing`), so this
/// path needs no volatility. It captures the coupon's gearing, spread,
/// in-arrears flag and index fixing when [`initialize`](Self::initialize) runs,
/// mirroring the C++ pricer caching them off the coupon.
///
/// ## Divergences from QuantLib
///
/// The C++ pricer also captures the forwarding curve's `discount_`, but only
/// [`swapletPrice`](swaplet-price) reads it - not `swapletRate`. The `*Price`
/// methods (and the caplet/floorlet optionlet path, the in-arrears convexity
/// adjustment, and the `IborCouponPricer` cached dates that feed the par/indexed
/// forecast) belong to the cap/floor slice and are not ported. An in-arrears
/// coupon is refused rather than priced with a missing convexity term.
///
/// [swaplet-price]: FloatingRateCouponPricer::swaplet_rate
pub struct BlackIborCouponPricer {
    gearing: Real,
    spread: Spread,
    is_in_arrears: bool,
    index_fixing: Option<QlResult<Rate>>,
    observable: Observable,
}

impl Default for BlackIborCouponPricer {
    fn default() -> Self {
        BlackIborCouponPricer {
            gearing: 1.0,
            spread: 0.0,
            is_in_arrears: false,
            index_fixing: None,
            observable: Observable::new(),
        }
    }
}

impl BlackIborCouponPricer {
    /// Builds a pricer with no coupon captured yet; a coupon is captured on the
    /// first [`initialize`](FloatingRateCouponPricer::initialize).
    pub fn new() -> Self {
        Self::default()
    }
}

impl AsObservable for BlackIborCouponPricer {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl FloatingRateCouponPricer for BlackIborCouponPricer {
    fn initialize(&mut self, coupon: &FloatingRateCoupon) {
        self.gearing = coupon.gearing();
        self.spread = coupon.spread();
        self.is_in_arrears = coupon.is_in_arrears();
        self.index_fixing = Some(coupon.index_fixing());
    }

    fn swaplet_rate(&self) -> QlResult<Rate> {
        let Some(index_fixing) = &self.index_fixing else {
            fail!("pricer not initialized: no coupon captured");
        };
        self.swaplet_rate_for(index_fixing.clone())
    }

    fn swaplet_rate_for(&self, index_fixing: QlResult<Rate>) -> QlResult<Rate> {
        require!(
            !self.is_in_arrears,
            "in-arrears convexity adjustment not ported: cap/floor slice"
        );
        Ok(self.gearing * index_fixing? + self.spread)
    }

    fn caplet_rate(&self, _effective_cap: Rate) -> QlResult<Rate> {
        fail!("caplet rate not ported: cap/floor slice")
    }

    fn floorlet_rate(&self, _effective_floor: Rate) -> QlResult<Rate> {
        fail!("floorlet rate not ported: cap/floor slice")
    }
}
