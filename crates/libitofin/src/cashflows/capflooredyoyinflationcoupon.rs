//! Year-on-year inflation coupon with an added cap and/or floor.
//!
//! Port of the year-on-year half of `ql/cashflows/capflooredinflationcoupon.{hpp,cpp}`.
//! A [`CappedFlooredYoYInflationCoupon`] wraps a [`YoYInflationCoupon`] and
//! layers a cap and/or floor on its rate: `rate = underlying + max(floor -
//! underlying, 0) - max(underlying - cap, 0)`, the floorlet and caplet coming
//! from the underlying's pricer (`.cpp:92-105`).
//!
//! ## Shape
//!
//! C++ derives the wrapper from `YoYInflationCoupon` *and* holds the wrapped
//! coupon as `underlying_` (`capflooredinflationcoupon.hpp:66`, `.cpp:57-76`),
//! so every accessor has to choose between the two copies of the same state at
//! runtime (`underlying_ != nullptr ? underlying_->rate() : ...`). The port
//! keeps only the composition, as [`CappedFlooredCoupon`] does for the ibor
//! leg: the wrapper holds a [`Shared`] [`YoYInflationCoupon`], delegates its
//! [`Coupon`] face to it, and overrides [`rate`](Coupon::rate) to add the
//! optionlets. There is one copy of the state, so the branch has nothing to
//! choose between.
//!
//! ## Divergences from QuantLib
//!
//! `setPricer` installs on the wrapper *and* the underlying in C++ (`.cpp:78-85`),
//! the classic "composition loses virtual dispatch" hazard.
//! [`set_pricer`](CappedFlooredYoYInflationCoupon::set_pricer) installs once, on
//! the underlying, which is the instance both the wrapper's rate path and the
//! underlying's read: they cannot diverge.
//!
//! The C++ `cap()` and `floor()` accessors (`.cpp:106-122`), which undo the
//! gearing-sign swap to report the level originally passed, have no port - as
//! for [`CappedFlooredCoupon`], nothing reads them. The stored levels are
//! reachable through [`effective_cap`](CappedFlooredYoYInflationCoupon::effective_cap)
//! and [`effective_floor`](CappedFlooredYoYInflationCoupon::effective_floor),
//! which are what the pricer is struck at.
//!
//! [`CappedFlooredCoupon`]: super::CappedFlooredCoupon

use super::coupon::{Coupon, CouponBase};
use super::yoyinflationcoupon::{YoYInflationCoupon, YoYInflationCouponPricer};
use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real};
use crate::{fail, require};

/// A year-on-year inflation coupon capped and/or floored.
///
/// Built from an underlying [`YoYInflationCoupon`] with [`new`](Self::new). A
/// pricer carrying an optionlet volatility - a
/// [`YoYInflationOptionletCouponPricer`](super::YoYInflationOptionletCouponPricer) -
/// is attached with [`set_pricer`](Self::set_pricer); the swaplet-only pricer
/// rates the underlying but refuses the optionlets.
pub struct CappedFlooredYoYInflationCoupon {
    underlying: Shared<YoYInflationCoupon>,
    is_capped: bool,
    is_floored: bool,
    cap: Rate,
    floor: Rate,
}

impl CappedFlooredYoYInflationCoupon {
    /// Wraps `underlying` with an optional `cap` and `floor`
    /// (`CappedFlooredYoYInflationCoupon::setCommon`, `.cpp:25-54`).
    ///
    /// A non-positive gearing swaps the roles: the passed cap becomes the
    /// effective floor and vice versa (`.cpp:40-49`), so a capped
    /// negative-gearing coupon is floored internally.
    ///
    /// # Errors
    ///
    /// When both a cap and a floor are given, the cap must not sit below the
    /// floor.
    pub fn new(
        underlying: Shared<YoYInflationCoupon>,
        cap: Option<Rate>,
        floor: Option<Rate>,
    ) -> QlResult<CappedFlooredYoYInflationCoupon> {
        let mut is_capped = false;
        let mut is_floored = false;
        let mut cap_value = 0.0;
        let mut floor_value = 0.0;

        if underlying.gearing() > 0.0 {
            if let Some(cap) = cap {
                is_capped = true;
                cap_value = cap;
            }
            if let Some(floor) = floor {
                is_floored = true;
                floor_value = floor;
            }
        } else {
            if let Some(cap) = cap {
                is_floored = true;
                floor_value = cap;
            }
            if let Some(floor) = floor {
                is_capped = true;
                cap_value = floor;
            }
        }

        if let (Some(cap), Some(floor)) = (cap, floor) {
            let cap_at_least_floor = cap >= floor;
            require!(
                cap_at_least_floor,
                "cap level ({cap}) less than floor level ({floor})"
            );
        }

        Ok(CappedFlooredYoYInflationCoupon {
            underlying,
            is_capped,
            is_floored,
            cap: cap_value,
            floor: floor_value,
        })
    }

    /// The wrapped coupon.
    pub fn underlying(&self) -> &Shared<YoYInflationCoupon> {
        &self.underlying
    }

    /// Whether a cap applies (`isCapped`).
    pub fn is_capped(&self) -> bool {
        self.is_capped
    }

    /// Whether a floor applies (`isFloored`).
    pub fn is_floored(&self) -> bool {
        self.is_floored
    }

    /// The de-spread, de-geared cap the caplet is struck at,
    /// `(cap - spread) / gearing` (`effectiveCap`, `.cpp:126-128`).
    ///
    /// Computed off the *stored* level, which the gearing-sign swap may already
    /// have taken from the floor argument, and not off what the caller passed.
    pub fn effective_cap(&self) -> Rate {
        (self.cap - self.underlying.spread()) / self.underlying.gearing()
    }

    /// The de-spread, de-geared floor the floorlet is struck at,
    /// `(floor - spread) / gearing` (`effectiveFloor`, `.cpp:130-132`). See
    /// [`effective_cap`](Self::effective_cap) on the stored level.
    pub fn effective_floor(&self) -> Rate {
        (self.floor - self.underlying.spread()) / self.underlying.gearing()
    }

    /// Attaches `pricer` to the underlying coupon
    /// (`CappedFlooredYoYInflationCoupon::setPricer`).
    ///
    /// One install, not the C++ two: the wrapper reads the underlying's pricer
    /// for both the swaplet and the optionlets.
    pub fn set_pricer(&self, pricer: SharedMut<dyn YoYInflationCouponPricer>) {
        self.underlying.set_pricer(pricer);
    }
}

impl AsObservable for CappedFlooredYoYInflationCoupon {
    fn observable(&self) -> &Observable {
        self.underlying.observable()
    }
}

impl Coupon for CappedFlooredYoYInflationCoupon {
    fn coupon_base(&self) -> &CouponBase {
        self.underlying.coupon_base()
    }

    fn amount(&self) -> QlResult<Real> {
        Ok(self.rate()? * self.accrual_period() * self.nominal())
    }

    /// `swapletRate + floorletRate - capletRate` (`.cpp:92-105`).
    ///
    /// The underlying's rate runs first, and with it the pricer's `initialize`,
    /// so the optionlets read a pricer already holding this coupon's fixing.
    fn rate(&self) -> QlResult<Rate> {
        let swaplet = self.underlying.rate()?;
        if !self.is_capped && !self.is_floored {
            return Ok(swaplet);
        }
        let Some(pricer) = self.underlying.pricer() else {
            fail!("pricer not set");
        };
        let mut rate = swaplet;
        if self.is_floored {
            rate += pricer.borrow().floorlet_rate(self.effective_floor())?;
        }
        if self.is_capped {
            rate -= pricer.borrow().caplet_rate(self.effective_cap())?;
        }
        Ok(rate)
    }

    fn day_counter(&self) -> DayCounter {
        self.underlying.day_counter()
    }

    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        if date <= self.accrual_start_date() || date > self.coupon_base().payment_date() {
            Ok(0.0)
        } else {
            Ok(self.nominal() * self.rate()? * self.accrued_period(date))
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wrapper's own logic, kept clear of a volatility surface: the
    //! gearing-sign swap, the cap-versus-floor guard, and the single pricer
    //! install. What the optionlets are worth is pinned in
    //! `capflooredyoyinflationcoupon_oracle`.

    use super::*;
    use crate::currency::Currency;
    use crate::indexes::Region;
    use crate::indexes::index::Index;
    use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
    use crate::settings::Settings;
    use crate::shared::{shared, shared_mut};
    use crate::time::date::Month::{February, November};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Spread;

    use super::super::yoyinflationcoupon::SwapletYoYInflationCouponPricer;

    /// A coupon over UK `YY_RPI` whose observed month, November 2020, is
    /// published: it rates without a curve.
    fn published_coupon(gearing: Real, spread: Spread) -> Shared<YoYInflationCoupon> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, February, 2022));
        let index = shared(YoYInflationIndex::new(
            "YY_RPI".into(),
            Region::uk(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::gbp(),
            settings,
        ));
        index
            .add_fixing(Date::new(1, November, 2020), 0.02935)
            .expect("publishing a rate");
        let accrual_end = Date::new(10, February, 2021);
        shared(YoYInflationCoupon::new(
            accrual_end,
            1_000_000.0,
            accrual_end - Period::new(1, TimeUnit::Years),
            accrual_end,
            0,
            index,
            Period::new(3, TimeUnit::Months),
            CpiInterpolationType::Flat,
            Actual360::new(),
            gearing,
            spread,
            None,
            None,
        ))
    }

    fn swaplet_pricer() -> SharedMut<dyn YoYInflationCouponPricer> {
        shared_mut(SwapletYoYInflationCouponPricer::new())
            as SharedMut<dyn YoYInflationCouponPricer>
    }

    /// A negative gearing swaps the roles (`.cpp:40-49`): a passed cap becomes
    /// the effective floor, so a "capped" negative-gearing coupon is floored.
    #[test]
    fn a_negative_gearing_swaps_cap_and_floor() {
        let coupon =
            CappedFlooredYoYInflationCoupon::new(published_coupon(-1.5, 0.12), Some(0.10), None)
                .expect("one level is always consistent");

        assert!(coupon.is_floored() && !coupon.is_capped());
        let expected = (0.10 - 0.12) / -1.5;
        assert!((coupon.effective_floor() - expected).abs() < 1e-15);
    }

    /// The effective levels are de-spread and de-geared (`.cpp:126-132`).
    #[test]
    fn the_effective_levels_undo_the_gearing_and_spread() {
        let coupon = CappedFlooredYoYInflationCoupon::new(
            published_coupon(2.5, 0.0035),
            Some(0.08),
            Some(0.01),
        )
        .expect("the cap sits above the floor");

        assert!(coupon.is_capped() && coupon.is_floored());
        assert!((coupon.effective_cap() - (0.08 - 0.0035) / 2.5).abs() < 1e-15);
        assert!((coupon.effective_floor() - (0.01 - 0.0035) / 2.5).abs() < 1e-15);
    }

    /// A cap below its floor is rejected at construction (`.cpp:50-53`).
    #[test]
    fn a_cap_below_its_floor_is_rejected() {
        let err = CappedFlooredYoYInflationCoupon::new(
            published_coupon(1.0, 0.0),
            Some(0.02),
            Some(0.03),
        )
        .err()
        .expect("a cap below its floor is an error");
        assert!(err.message().contains("less than floor"), "err was: {err}");
    }

    /// One install, read by the rate path: after a swap the underlying carries
    /// exactly the new instance, so the wrapper and the underlying can never
    /// price against different pricers.
    #[test]
    fn set_pricer_installs_the_one_instance_the_rate_path_reads() {
        let coupon = CappedFlooredYoYInflationCoupon::new(published_coupon(1.0, 0.0), None, None)
            .expect("one level is always consistent");

        let pricer = swaplet_pricer();
        coupon.set_pricer(pricer.clone());
        assert!(SharedMut::ptr_eq(
            &coupon
                .underlying()
                .pricer()
                .expect("a pricer was installed"),
            &pricer
        ));
    }

    /// With neither level set the wrapper is its underlying: no pricer is asked
    /// for an optionlet, so the swaplet-only pricer suffices.
    #[test]
    fn an_uncapped_unfloored_wrapper_rates_as_its_underlying() {
        let underlying = published_coupon(2.5, 0.0035);
        let coupon = CappedFlooredYoYInflationCoupon::new(Shared::clone(&underlying), None, None)
            .expect("one level is always consistent");
        coupon.set_pricer(swaplet_pricer());

        let expected = underlying.rate().expect("November 2020 is published");
        assert!((coupon.rate().expect("no optionlet is read") - expected).abs() < 1e-15);
    }

    /// A capped coupon carrying only the swaplet pricer refuses: the optionlet
    /// needs a volatility surface the pricer does not hold.
    #[test]
    fn a_capped_coupon_on_a_swaplet_pricer_refuses() {
        let coupon =
            CappedFlooredYoYInflationCoupon::new(published_coupon(1.0, 0.0), Some(0.02), None)
                .expect("one level is always consistent");
        coupon.set_pricer(swaplet_pricer());

        let err = coupon.rate().expect_err("the caplet needs a volatility");
        assert!(
            err.message().contains("needs a volatility"),
            "err was: {err}"
        );
    }
}
