//! Volatility-carrying pricer for year-on-year inflation coupons.
//!
//! Port of the optionlet half of `YoYInflationCouponPricer` and of its three
//! distribution-bearing descendants
//! (`ql/cashflows/inflationcouponpricer.{hpp,cpp}`): `capletRate`,
//! `floorletRate` and the `optionletRate` machinery under them
//! (`.cpp:60-123`, `:174-211`). The swaplet-only
//! [`SwapletYoYInflationCouponPricer`](super::SwapletYoYInflationCouponPricer)
//! stays where it is, next to the trait and the coupon it was written for.
//!
//! ## Divergences from QuantLib
//!
//! C++ spells the three distributions as three classes differing in exactly one
//! line, their `optionletPriceImp` (`.cpp:180-211`); nothing dispatches on the
//! concrete type, so the classes buy only their three constructors. The port
//! folds them into one [`YoYInflationOptionletCouponPricer`] carrying a
//! [`YoYOptionletDistribution`], with a constructor apiece.
//!
//! The unit-displaced case reads `blackFormula(effStrike + 1, forward + 1,
//! stdDev)` in C++ (`.cpp:190-200`); here it is
//! [`black_formula`] with `displacement = 1.0`, which the port adds to the
//! forward *and* the strike (`blackformula.rs:115-116`) - the same two sums.
//!
//! `optionletPrice`, `capletPrice` and `floorletPrice` (`.cpp:60-70`, `:89-94`)
//! have no port: they accrue and discount an optionlet rate for the
//! `YoYInflationCapFloor` engines, which are deferred to `#851` along with the
//! instrument. [`swaplet_price`](YoYInflationCouponPricer::swaplet_price), which
//! the trait requires, is kept.

use super::coupon::Coupon;
use super::yoyinflationcoupon::{YoYInflationCoupon, YoYInflationCouponPricer};
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::pricingengines::inflation::yoy_optionlet_price;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::volatility::YoYOptionletVolatilitySurface;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real, Spread, Time};
use crate::{fail, require};

/// The distribution an optionlet is valued under.
///
/// The one line separating QuantLib's three vol-dependent year-on-year pricers
/// (`inflationcouponpricer.cpp:180-211`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoYOptionletDistribution {
    /// Lognormal: `blackFormula(type, effStrike, forward, stdDev)`.
    Black,
    /// Lognormal in `1 + rate`, the usual quoting convention for an inflation
    /// rate that may be negative: `blackFormula(type, effStrike + 1, forward +
    /// 1, stdDev)`.
    UnitDisplaced,
    /// Normal: `bachelierBlackFormula(type, effStrike, forward, stdDev)`.
    Bachelier,
}

/// Year-on-year inflation coupon pricer carrying an optionlet volatility.
///
/// Prices the swaplet as
/// [`SwapletYoYInflationCouponPricer`](super::SwapletYoYInflationCouponPricer)
/// does, and adds the caplet and floorlet a `CappedFlooredYoYInflationCoupon`
/// reads through [`caplet_rate`](YoYInflationCouponPricer::caplet_rate) and
/// [`floorlet_rate`](YoYInflationCouponPricer::floorlet_rate).
pub struct YoYInflationOptionletCouponPricer {
    distribution: YoYOptionletDistribution,
    caplet_vol: Handle<dyn YoYOptionletVolatilitySurface>,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
    gearing: Real,
    spread: Spread,
    accrual_period: Time,
    index_fixing: Option<QlResult<Rate>>,
    fixing_date: Option<Date>,
    discount: Option<QlResult<Real>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl YoYInflationOptionletCouponPricer {
    /// Builds a pricer over `caplet_vol`, discounting on
    /// `nominal_term_structure`, registering for changes in both
    /// (`inflationcouponpricer.cpp:37-57`).
    ///
    /// An empty nominal handle still yields rates - which is the point of
    /// `swapletRate` reading the coupon rather than a curve - but refuses
    /// prices. An empty volatility handle refuses every optionlet.
    fn new(
        distribution: YoYOptionletDistribution,
        caplet_vol: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationOptionletCouponPricer {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        let mut pricer = YoYInflationOptionletCouponPricer {
            distribution,
            caplet_vol: Handle::empty(),
            nominal_term_structure: Handle::empty(),
            gearing: 0.0,
            spread: 0.0,
            accrual_period: 0.0,
            index_fixing: None,
            fixing_date: None,
            discount: None,
            observable,
            forwarder,
        };
        let observer = pricer.forwarder.clone() as SharedMut<dyn Observer>;
        caplet_vol.register_observer(&observer);
        nominal_term_structure.register_observer(&observer);
        pricer.caplet_vol = caplet_vol;
        pricer.nominal_term_structure = nominal_term_structure;
        pricer
    }

    /// A pricer valuing optionlets under the lognormal model
    /// (`BlackYoYInflationCouponPricer`). See [`new`](Self::new).
    pub fn black(
        caplet_vol: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationOptionletCouponPricer {
        Self::new(
            YoYOptionletDistribution::Black,
            caplet_vol,
            nominal_term_structure,
        )
    }

    /// A pricer valuing optionlets under the unit-displaced lognormal model
    /// (`UnitDisplacedBlackYoYInflationCouponPricer`). See [`new`](Self::new).
    pub fn unit_displaced(
        caplet_vol: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationOptionletCouponPricer {
        Self::new(
            YoYOptionletDistribution::UnitDisplaced,
            caplet_vol,
            nominal_term_structure,
        )
    }

    /// A pricer valuing optionlets under the normal model
    /// (`BachelierYoYInflationCouponPricer`). See [`new`](Self::new).
    pub fn bachelier(
        caplet_vol: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationOptionletCouponPricer {
        Self::new(
            YoYOptionletDistribution::Bachelier,
            caplet_vol,
            nominal_term_structure,
        )
    }

    /// The distribution optionlets are valued under.
    pub fn distribution(&self) -> YoYOptionletDistribution {
        self.distribution
    }

    /// The optionlet volatility the pricer reads.
    pub fn caplet_volatility(&self) -> &Handle<dyn YoYOptionletVolatilitySurface> {
        &self.caplet_vol
    }

    /// The nominal curve the pricer discounts on.
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }

    /// The rate of an optionlet struck at `eff_strike` (`optionletRate`,
    /// `.cpp:96-123`).
    ///
    /// A coupon fixing on or before the volatility surface's base date is
    /// already determined and pays its intrinsic `max(a - b, 0)` with no
    /// volatility read. The comparison is against the *surface's* base date and
    /// not the evaluation date, which is where this parts company with
    /// [`BlackIborCouponPricer`](super::BlackIborCouponPricer)
    /// (`couponpricer.rs:200`): a year-on-year fixing is published a whole
    /// observation lag late, so a coupon can be determined well before it fixes.
    ///
    /// # Errors
    ///
    /// Before [`initialize`](YoYInflationCouponPricer::initialize) has run, when
    /// the volatility handle is empty, when the coupon's fixing is unpublished,
    /// or as the underlying formula.
    pub fn optionlet_rate(&self, option_type: OptionType, eff_strike: Rate) -> QlResult<Rate> {
        let Some(fixing_date) = self.fixing_date else {
            fail!("pricer not initialized: no coupon captured");
        };
        require!(!self.caplet_vol.is_empty(), "missing optionlet volatility");
        let surface = self.caplet_vol.current_link()?;
        let forward = self.index_fixing()?;

        if fixing_date <= surface.base_date()? {
            let (a, b) = match option_type {
                OptionType::Call => (forward, eff_strike),
                OptionType::Put => (eff_strike, forward),
            };
            return Ok((a - b).max(0.0));
        }

        let std_dev = surface
            .total_variance(fixing_date, eff_strike, Period::new(0, TimeUnit::Days))?
            .sqrt();
        // `optionletPriceImp` (`.cpp:180-211`), shared with the cap/floor
        // engines; a rate rather than a price, so nothing is discounted.
        yoy_optionlet_price(
            self.distribution,
            option_type,
            eff_strike,
            forward,
            std_dev,
            1.0,
        )
    }

    /// The captured fixing, or the reason there is none.
    fn index_fixing(&self) -> QlResult<Rate> {
        let Some(index_fixing) = &self.index_fixing else {
            fail!("pricer not initialized: no coupon captured");
        };
        index_fixing.clone()
    }

    /// The discount factor applied to a payment on `payment_date`
    /// (`.cpp:146-153`), as its twin on
    /// [`SwapletYoYInflationCouponPricer`](super::SwapletYoYInflationCouponPricer).
    fn discount_at(&self, payment_date: Date) -> QlResult<Real> {
        let curve = self.nominal_term_structure.current_link()?;
        if payment_date > curve.reference_date()? {
            curve.discount_date(payment_date, false)
        } else {
            Ok(1.0)
        }
    }
}

impl AsObservable for YoYInflationOptionletCouponPricer {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl YoYInflationCouponPricer for YoYInflationOptionletCouponPricer {
    /// Captures what the rate readers need, the fixing date among it: the
    /// optionlet branch keys on it, and C++ reads it back through the coupon
    /// pointer it stores (`.cpp:98`) where the port stores no back-reference.
    fn initialize(&mut self, coupon: &YoYInflationCoupon) {
        self.gearing = coupon.gearing();
        self.spread = coupon.spread();
        self.accrual_period = coupon.accrual_period();
        self.index_fixing = Some(coupon.index_fixing());
        self.fixing_date = Some(coupon.fixing_date());
        self.discount = if self.nominal_term_structure.is_empty() {
            None
        } else {
            Some(self.discount_at(coupon.coupon_base().payment_date()))
        };
    }

    fn swaplet_rate(&self) -> QlResult<Rate> {
        Ok(self.gearing * self.index_fixing()? + self.spread)
    }

    fn swaplet_price(&self) -> QlResult<Real> {
        let Some(discount) = &self.discount else {
            fail!("no nominal term structure provided");
        };
        Ok(self.swaplet_rate()? * self.accrual_period * discount.clone()?)
    }

    fn caplet_rate(&self, effective_cap: Rate) -> QlResult<Rate> {
        Ok(self.gearing * self.optionlet_rate(OptionType::Call, effective_cap)?)
    }

    fn floorlet_rate(&self, effective_floor: Rate) -> QlResult<Rate> {
        Ok(self.gearing * self.optionlet_rate(OptionType::Put, effective_floor)?)
    }
}

#[cfg(test)]
mod tests {
    //! The numbers this pricer produces are pinned coupon-side, against the
    //! wrapper that reads it (`capflooredyoyinflationcoupon.rs`); what is left
    //! here is what a coupon cannot reach - the refusals before a coupon has
    //! been captured, and the refusal of a pricer holding no surface.

    use super::*;
    use crate::cashflows::yoyinflationcoupon::SwapletYoYInflationCouponPricer;

    #[test]
    fn the_swaplet_pricer_refuses_an_optionlet() {
        let pricer = SwapletYoYInflationCouponPricer::new();

        let capped = pricer.caplet_rate(0.03).expect_err("no volatility is held");
        assert!(
            capped.message().contains("needs a volatility"),
            "err was: {capped}"
        );
        let floored = pricer
            .floorlet_rate(0.03)
            .expect_err("no volatility is held");
        assert!(
            floored.message().contains("needs a volatility"),
            "err was: {floored}"
        );
    }

    #[test]
    fn an_optionlet_before_initialize_is_an_error() {
        let pricer = YoYInflationOptionletCouponPricer::black(Handle::empty(), Handle::empty());

        let err = pricer
            .optionlet_rate(OptionType::Call, 0.03)
            .expect_err("no coupon was captured");
        assert!(
            err.message().contains("pricer not initialized"),
            "err was: {err}"
        );
    }

    /// The surface is read before the determined/undetermined branch, so an
    /// empty handle refuses even an optionlet that would have been intrinsic.
    #[test]
    fn an_optionlet_without_a_surface_is_an_error() {
        let mut pricer = YoYInflationOptionletCouponPricer::black(Handle::empty(), Handle::empty());
        pricer.fixing_date = Some(Date::new(1, crate::time::date::Month::June, 2026));
        pricer.index_fixing = Some(Ok(0.02));

        let err = pricer
            .optionlet_rate(OptionType::Call, 0.03)
            .expect_err("no volatility surface is linked");
        assert!(
            err.message().contains("missing optionlet volatility"),
            "err was: {err}"
        );
    }
}
