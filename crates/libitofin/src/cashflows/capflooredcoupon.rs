//! Floating-rate coupon with an added cap and/or floor.
//!
//! Port of `ql/cashflows/capflooredcoupon.{hpp,cpp}`. A [`CappedFlooredCoupon`]
//! wraps a floating coupon and layers a cap and/or floor on its rate:
//! `rate = underlying + max(floor - underlying, 0) - max(underlying - cap, 0)`,
//! the floorlet and caplet coming from the underlying's pricer. Only the ibor
//! variant is in scope; [`CappedFlooredIborCoupon`] is its constructor.
//!
//! ## Shape
//!
//! C++ derives `CappedFlooredCoupon` from `FloatingRateCoupon` and holds the
//! wrapped coupon as `underlying_`. The port keeps only the composition: the
//! wrapper holds a [`Shared`] [`IborCoupon`] and delegates its [`Coupon`] face -
//! dates, nominal, day counter - to it, overriding [`rate`](Coupon::rate) to
//! add the caplet/floorlet. It carries no pricer of its own; the C++ base's
//! `pricer_` is unused once `rate()` is overridden, and every optionlet reads
//! the underlying's pricer.
//!
//! ## Divergences from QuantLib
//!
//! `setPricer` forwards to the wrapper *and* the underlying in C++ (one call,
//! two installs), the classic "composition loses virtual dispatch" hazard. The
//! port has a single install: [`set_pricer`](CappedFlooredCoupon::set_pricer)
//! sets the underlying's pricer, and both the wrapper's rate path and the
//! underlying's read that one instance, so they cannot diverge.
//!
//! `CappedFlooredCoupon` is a `LazyObject` in C++, caching `rate_` behind
//! `performCalculations`. As with the rest of the cash-flow layer the cache is
//! omitted: [`rate`](Coupon::rate) recomputes each call from the same inputs.
//! The behavioural half is kept - the wrapper's observable *is* the underlying's
//! (`observable()`): the cap and floor are fixed at construction, so the wrapper
//! has nothing of its own to notify, and observers registering on it see every
//! index, pricer or evaluation-date change the underlying broadcasts. This
//! stands in for the C++ `registerWith(underlying_)` re-broadcast.
//!
//! `CappedFlooredCmsCoupon` (needs a swap index) and the visitor `accept`
//! overrides have no port here.

use super::coupon::{Coupon, CouponBase};
use super::couponpricer::FloatingRateCouponPricer;
use super::iborcoupon::IborCoupon;
use crate::errors::QlResult;
use crate::indexes::iborindex::IborIndex;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::{Shared, SharedMut, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Rate, Real, Spread};
use crate::{fail, require};

/// A floating-rate coupon capped and/or floored.
///
/// Built from an underlying [`IborCoupon`] with [`new`](Self::new), or directly
/// through [`CappedFlooredIborCoupon::new`]. A pricer carrying the optionlet
/// volatility is attached with [`set_pricer`](Self::set_pricer).
pub struct CappedFlooredCoupon {
    underlying: Shared<IborCoupon>,
    is_capped: bool,
    is_floored: bool,
    cap: Rate,
    floor: Rate,
}

impl CappedFlooredCoupon {
    /// Wraps `underlying` with an optional `cap` and `floor`
    /// (`CappedFlooredCoupon::CappedFlooredCoupon`).
    ///
    /// A negative gearing swaps the roles: the passed cap becomes the effective
    /// floor and vice versa (`capflooredcoupon.cpp:46-64`), so a capped
    /// negative-gearing coupon is floored internally.
    ///
    /// # Errors
    ///
    /// When both a cap and a floor are given, the cap must not sit below the
    /// floor.
    pub fn new(
        underlying: Shared<IborCoupon>,
        cap: Option<Rate>,
        floor: Option<Rate>,
    ) -> QlResult<CappedFlooredCoupon> {
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

        Ok(CappedFlooredCoupon {
            underlying,
            is_capped,
            is_floored,
            cap: cap_value,
            floor: floor_value,
        })
    }

    /// The wrapped coupon.
    pub fn underlying(&self) -> &Shared<IborCoupon> {
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
    /// `(cap - spread) / gearing` (`effectiveCap`).
    fn effective_cap(&self) -> Rate {
        (self.cap - self.underlying.spread()) / self.underlying.gearing()
    }

    /// The de-spread, de-geared floor the floorlet is struck at,
    /// `(floor - spread) / gearing` (`effectiveFloor`).
    fn effective_floor(&self) -> Rate {
        (self.floor - self.underlying.spread()) / self.underlying.gearing()
    }

    /// Attaches `pricer` to the underlying coupon (`CappedFlooredCoupon::setPricer`).
    ///
    /// One install, not the C++ two: the wrapper reads the underlying's pricer
    /// for both the swaplet and the optionlets.
    pub fn set_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
        self.underlying.set_pricer(pricer);
    }
}

impl AsObservable for CappedFlooredCoupon {
    fn observable(&self) -> &Observable {
        self.underlying.observable()
    }
}

impl Coupon for CappedFlooredCoupon {
    fn coupon_base(&self) -> &CouponBase {
        self.underlying.coupon_base()
    }

    fn amount(&self) -> QlResult<Real> {
        Ok(self.rate()? * self.accrual_period() * self.nominal())
    }

    fn rate(&self) -> QlResult<Rate> {
        let swaplet = self.underlying.rate()?;
        let Some(pricer) = self.underlying.pricer() else {
            fail!("pricer not set");
        };
        let forward = self.underlying.index_fixing();
        let mut rate = swaplet;
        if self.is_floored {
            rate += pricer
                .borrow()
                .floorlet_rate(self.effective_floor(), forward.clone())?;
        }
        if self.is_capped {
            rate -= pricer.borrow().caplet_rate(self.effective_cap(), forward)?;
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

/// Constructor for an ibor coupon wrapped in a cap and/or floor
/// (`CappedFlooredIborCoupon`).
///
/// C++ derives this from [`CappedFlooredCoupon`] purely to build the underlying
/// [`IborCoupon`] and carry a visitor override. With no visitor, it reduces to
/// the constructor, which builds the coupon and returns the wrapper.
pub struct CappedFlooredIborCoupon;

impl CappedFlooredIborCoupon {
    /// Builds an [`IborCoupon`] and wraps it with `cap` and `floor`.
    ///
    /// Returns the base [`CappedFlooredCoupon`]: the C++ subclass adds only the
    /// construction and a visitor, so with no visitor the constructor yields the
    /// base directly.
    ///
    /// # Errors
    ///
    /// As [`IborCoupon::new`] and [`CappedFlooredCoupon::new`].
    #[allow(clippy::too_many_arguments, clippy::new_ret_no_self)]
    pub fn new(
        payment_date: Date,
        nominal: Real,
        accrual_start_date: Date,
        accrual_end_date: Date,
        fixing_days: Option<Natural>,
        index: Shared<IborIndex>,
        gearing: Real,
        spread: Spread,
        cap: Option<Rate>,
        floor: Option<Rate>,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        day_counter: Option<DayCounter>,
        is_in_arrears: bool,
        ex_coupon_date: Option<Date>,
        fixing_convention: BusinessDayConvention,
    ) -> QlResult<CappedFlooredCoupon> {
        let underlying = IborCoupon::new(
            payment_date,
            nominal,
            accrual_start_date,
            accrual_end_date,
            fixing_days,
            index,
            gearing,
            spread,
            ref_period_start,
            ref_period_end,
            day_counter,
            is_in_arrears,
            ex_coupon_date,
            fixing_convention,
        )?;
        CappedFlooredCoupon::new(shared(underlying), cap, floor)
    }
}

#[cfg(test)]
mod tests {
    //! The wrapper's own logic - the gearing-sign cap/floor swap, the
    //! decomposition on a determined coupon, and the single-install pin - kept
    //! independent of a curve or a volatility surface. The full Black-model
    //! decomposition is pinned against `capflooredcoupon.cpp`.

    use super::*;
    use crate::currency::Currency;
    use crate::handle::Handle;
    use crate::indexes::index::Index;
    use crate::settings::Settings;
    use crate::shared::shared_mut;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn determined_coupon(gearing: Real, spread: Spread) -> Shared<IborCoupon> {
        let today = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let index = shared(IborIndex::new(
            "foo".into(),
            Period::new(6, TimeUnit::Months),
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            Handle::<dyn YieldTermStructure>::empty(),
            settings,
        ));
        let start = Date::new(3, Month::June, 2026);
        let end = Date::new(3, Month::December, 2026);
        let coupon = shared(
            IborCoupon::new(
                end,
                100.0,
                start,
                end,
                Some(2),
                index.clone(),
                gearing,
                spread,
                None,
                None,
                None,
                false,
                None,
                BusinessDayConvention::Preceding,
            )
            .unwrap(),
        );
        index.add_fixing(coupon.fixing_date(), 0.05).unwrap();
        coupon
    }

    fn pricer() -> SharedMut<dyn FloatingRateCouponPricer> {
        shared_mut(super::super::couponpricer::BlackIborCouponPricer::new())
            as SharedMut<dyn FloatingRateCouponPricer>
    }

    /// A collar clamps the determined fixing into `[floor, cap]`: at a 0.05
    /// fixing with cap 0.03 and floor 0.02 the coupon caps to 0.03.
    #[test]
    fn a_collar_clamps_the_determined_rate() {
        let coupon =
            CappedFlooredCoupon::new(determined_coupon(1.0, 0.0), Some(0.03), Some(0.02)).unwrap();
        coupon.set_pricer(pricer());

        assert!(coupon.is_capped() && coupon.is_floored());
        assert!((coupon.rate().unwrap() - 0.03).abs() < 1e-15);
    }

    /// A floor lifts the determined fixing: at 0.05 with a 0.06 floor and no cap
    /// the coupon floors to 0.06.
    #[test]
    fn a_floor_lifts_the_determined_rate() {
        let coupon =
            CappedFlooredCoupon::new(determined_coupon(1.0, 0.0), None, Some(0.06)).unwrap();
        coupon.set_pricer(pricer());

        assert!(coupon.is_floored() && !coupon.is_capped());
        assert!((coupon.rate().unwrap() - 0.06).abs() < 1e-15);
    }

    /// A negative gearing swaps the roles: a passed cap becomes the effective
    /// floor (`capflooredcoupon.cpp:55-63`), so a "capped" negative-gearing
    /// coupon is floored internally.
    #[test]
    fn a_negative_gearing_swaps_cap_and_floor() {
        let coupon =
            CappedFlooredCoupon::new(determined_coupon(-1.5, 0.0), Some(0.10), None).unwrap();

        assert!(coupon.is_floored() && !coupon.is_capped());
    }

    /// One pricer install, read by the rate path: after a swap the underlying
    /// carries exactly the new instance, so the wrapper and the underlying can
    /// never price against different pricers.
    #[test]
    fn set_pricer_installs_the_one_instance_the_rate_path_reads() {
        let coupon =
            CappedFlooredCoupon::new(determined_coupon(1.0, 0.0), Some(0.03), None).unwrap();

        let p1 = pricer();
        coupon.set_pricer(p1.clone());
        assert!(SharedMut::ptr_eq(
            &coupon.underlying().pricer().unwrap(),
            &p1
        ));
        let first = coupon.rate().unwrap();

        let p2 = pricer();
        coupon.set_pricer(p2.clone());
        assert!(SharedMut::ptr_eq(
            &coupon.underlying().pricer().unwrap(),
            &p2
        ));
        assert!((coupon.rate().unwrap() - first).abs() < 1e-15);
    }

    /// A cap below its floor is rejected at construction.
    #[test]
    fn a_cap_below_its_floor_is_rejected() {
        let err = CappedFlooredCoupon::new(determined_coupon(1.0, 0.0), Some(0.02), Some(0.03))
            .err()
            .expect("a cap below its floor is an error");
        assert!(err.message().contains("less than floor"));
    }
}
