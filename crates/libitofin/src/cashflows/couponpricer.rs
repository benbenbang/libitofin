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
use crate::patterns::observable::AsObservable;
use crate::types::Rate;

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

    /// The rate of a caplet struck at `effective_cap` (`capletRate`).
    ///
    /// The cap/floor slice is not ported; a base-slice pricer returns `Err`.
    fn caplet_rate(&self, effective_cap: Rate) -> QlResult<Rate>;

    /// The rate of a floorlet struck at `effective_floor` (`floorletRate`).
    ///
    /// The cap/floor slice is not ported; a base-slice pricer returns `Err`.
    fn floorlet_rate(&self, effective_floor: Rate) -> QlResult<Rate>;
}
