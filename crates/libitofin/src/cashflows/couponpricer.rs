//! Pricers for floating-rate coupons.
//!
//! Port of the `FloatingRateCouponPricer` base of
//! `ql/cashflows/couponpricer.hpp` and the `IborCouponPricer` /
//! `BlackIborCouponPricer` derived from it. A [`FloatingRateCoupon`] does not
//! compute its own rate: it hands itself to a pricer and reads back
//! [`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate)
//! (`floatingratecoupon.cpp:88`). The pricer folds the coupon's gearing and
//! spread into that result, so the coupon reapplies neither. A capped/floored
//! coupon reads back [`caplet_rate`](FloatingRateCouponPricer::caplet_rate) and
//! [`floorlet_rate`](FloatingRateCouponPricer::floorlet_rate) on top.
//!
//! ## Divergences from QuantLib
//!
//! The C++ interface also declares `swapletPrice`, `capletPrice` and
//! `floorletPrice`. None is reached by the ported code: a coupon prices through
//! `amount() * discount`, not the pricer, and a cap/floor decomposition reads
//! the caplet/floorlet *rate* (`capflooredcoupon.cpp:88-95`), not its price. The
//! `*Price` variants - and the forwarding curve's `discount_` and
//! `accrualPeriod_` they alone read (`couponpricer.cpp:170-174`) - are therefore
//! omitted until a consumer needs them, rather than restored.
//!
//! C++ splits the caplet-volatility state onto an intermediate `IborCouponPricer`
//! base. With only the Black pricer in scope (the CMS pricer is deferred) that
//! base folds into [`BlackIborCouponPricer`] itself: the caplet-volatility
//! handle, [`set_caplet_volatility`](BlackIborCouponPricer::set_caplet_volatility)
//! and its registration live there directly rather than on a separate type.
//!
//! The C++ base is both `Observer` and `Observable` (its `update()` forwards
//! notifications). Here [`BlackIborCouponPricer`] forwards a caplet-volatility
//! change on to its own observers through a [`ResetThenNotify`], the same
//! forwarding shape [`FloatingRateCoupon`] uses for its index.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::pricingengines::blackformula::{bachelier_black_formula, black_formula};
use crate::shared::{Shared, SharedMut};
use crate::termstructures::volatility::{OptionletVolatilityStructure, VolatilityType};
use crate::time::date::Date;
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
    /// `gearing * optionletRate(Call, effective_cap)`. A pricer with no
    /// optionlet volatility refuses.
    fn caplet_rate(&self, effective_cap: Rate) -> QlResult<Rate>;

    /// The rate of a floorlet struck at `effective_floor` (`floorletRate`).
    ///
    /// `gearing * optionletRate(Put, effective_floor)`. A pricer with no
    /// optionlet volatility refuses.
    fn floorlet_rate(&self, effective_floor: Rate) -> QlResult<Rate>;
}

/// Black-formula pricer for ibor coupons (`BlackIborCouponPricer` over the
/// `IborCouponPricer` base in `ql/cashflows/couponpricer.{hpp,cpp}`).
///
/// The swaplet rate is `gearing * adjustedFixing + spread`
/// (`couponpricer.hpp:215`); for a non-in-arrears coupon under the default
/// `Black76` timing the adjusted fixing reduces to the coupon's index fixing
/// with no convexity adjustment, so that path needs no volatility. The caplet
/// and floorlet rates take the optionlet path (`couponpricer.cpp:138-168`): a
/// determined coupon (fixing on or before the evaluation date) returns the
/// intrinsic `max`, an undetermined one the Black or Bachelier optionlet value
/// against the caplet-volatility surface.
///
/// It captures the coupon's gearing, spread, in-arrears flag, index fixing,
/// fixing date and the evaluation date when [`initialize`](Self::initialize)
/// runs, mirroring the C++ pricer caching them off the coupon.
///
/// ## Divergences from QuantLib
///
/// The captured index fixing is the base coupon's natural (indexed) forecast,
/// the one [`FloatingRateCoupon::index_fixing`] reads. It equals the mode-aware
/// par/indexed forecast [`IborCoupon`] threads through
/// [`swaplet_rate_for`](FloatingRateCouponPricer::swaplet_rate_for) whenever the
/// coupon spans the index tenor - the whole capped/floored oracle - and diverges
/// only on a stub, where threading the mode into the caplet path is deferred with
/// the same par/indexed split.
///
/// `adjustedFixing()` reduces to the index fixing here: the in-arrears convexity
/// adjustment is refused (as in the swaplet path) and only the `Black76` timing
/// is modelled, so no convexity term applies. The C++ `discount_` and
/// `accrualPeriod_` feed only the omitted `*Price` methods.
///
/// [`IborCoupon`]: super::iborcoupon::IborCoupon
pub struct BlackIborCouponPricer {
    gearing: Real,
    spread: Spread,
    is_in_arrears: bool,
    index_fixing: Option<QlResult<Rate>>,
    fixing_date: Option<Date>,
    eval_date: Option<Date>,
    caplet_vol: Handle<dyn OptionletVolatilityStructure>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl Default for BlackIborCouponPricer {
    fn default() -> Self {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        BlackIborCouponPricer {
            gearing: 1.0,
            spread: 0.0,
            is_in_arrears: false,
            index_fixing: None,
            fixing_date: None,
            eval_date: None,
            caplet_vol: Handle::empty(),
            observable,
            forwarder,
        }
    }
}

impl BlackIborCouponPricer {
    /// Builds a pricer with no caplet volatility and no coupon captured yet; a
    /// coupon is captured on the first
    /// [`initialize`](FloatingRateCouponPricer::initialize).
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a pricer carrying the caplet-volatility surface `vol` (the C++
    /// `BlackIborCouponPricer(v)` constructor).
    pub fn with_vol(vol: Handle<dyn OptionletVolatilityStructure>) -> Self {
        let mut pricer = Self::default();
        pricer.set_caplet_volatility(vol);
        pricer
    }

    /// Attaches the caplet-volatility surface `vol`, registering for its changes
    /// (`IborCouponPricer::setCapletVolatility`).
    ///
    /// The registration is add-only: [`Handle`] exposes no unregister, and the
    /// surface is set once at construction, so a rare re-set would leave the
    /// prior handle notifying a forwarder that harmlessly rebroadcasts.
    pub fn set_caplet_volatility(&mut self, vol: Handle<dyn OptionletVolatilityStructure>) {
        self.caplet_vol = vol;
        let observer = self.forwarder.clone() as SharedMut<dyn Observer>;
        self.caplet_vol.register_observer(&observer);
        self.observable.notify_observers();
    }

    /// The optionlet rate of `option_type` struck at `eff_strike`
    /// (`BlackIborCouponPricer::optionletRate`).
    ///
    /// A determined coupon returns the intrinsic `max`; an undetermined one the
    /// Black (shifted-lognormal) or Bachelier (normal) optionlet value, refusing
    /// when the surface is missing.
    fn optionlet_rate(&self, option_type: OptionType, eff_strike: Rate) -> QlResult<Rate> {
        let Some(fixing_date) = self.fixing_date else {
            fail!("pricer not initialized: no coupon captured");
        };
        let Some(index_fixing) = &self.index_fixing else {
            fail!("pricer not initialized: no coupon captured");
        };
        let forward = index_fixing.clone()?;
        let Some(eval_date) = self.eval_date else {
            fail!("no evaluation date set: a caplet needs a reference date");
        };

        if fixing_date <= eval_date {
            let (a, b) = match option_type {
                OptionType::Call => (forward, eff_strike),
                OptionType::Put => (eff_strike, forward),
            };
            return Ok((a - b).max(0.0));
        }

        require!(!self.caplet_vol.is_empty(), "missing optionlet volatility");
        let surface = self.caplet_vol.current_link()?;
        let std_dev = surface
            .black_variance_date(fixing_date, eff_strike, false)?
            .sqrt();
        let shift = surface.displacement();
        match surface.volatility_type() {
            VolatilityType::ShiftedLognormal => {
                black_formula(option_type, eff_strike, forward, std_dev, 1.0, shift)
            }
            VolatilityType::Normal => {
                bachelier_black_formula(option_type, eff_strike, forward, std_dev, 1.0)
            }
        }
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
        self.fixing_date = Some(coupon.fixing_date());
        self.eval_date = coupon.index().base().settings().evaluation_date();
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

    fn caplet_rate(&self, effective_cap: Rate) -> QlResult<Rate> {
        Ok(self.gearing * self.optionlet_rate(OptionType::Call, effective_cap)?)
    }

    fn floorlet_rate(&self, effective_floor: Rate) -> QlResult<Rate> {
        Ok(self.gearing * self.optionlet_rate(OptionType::Put, effective_floor)?)
    }
}

#[cfg(test)]
mod tests {
    //! The determined-optionlet and refusal paths of `optionletRate`. The
    //! undetermined Black/Bachelier branch is exercised end-to-end against
    //! `capflooredcoupon.cpp` where the leg decomposition pins the numbers.

    use super::*;
    use crate::currency::Currency;
    use crate::handle::Handle;
    use crate::indexes::iborindex::IborIndex;
    use crate::indexes::index::Index;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn index_on(
        settings: Shared<Settings<Date>>,
        forwarding: Handle<dyn YieldTermStructure>,
    ) -> Shared<IborIndex> {
        shared(IborIndex::new(
            "foo".into(),
            Period::new(6, TimeUnit::Months),
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            forwarding,
            settings,
        ))
    }

    fn flat_curve(reference: Date, rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// A determined coupon (fixing on or before the evaluation date) prices its
    /// optionlets off the intrinsic value: the caplet is `max(fixing - cap, 0)`
    /// and the floorlet `max(floor - fixing, 0)`, both scaled by the gearing.
    #[test]
    fn a_determined_coupon_prices_the_intrinsic_optionlet() {
        let today = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let index = index_on(settings, Handle::empty());

        let start = Date::new(3, Month::June, 2026);
        let end = Date::new(3, Month::December, 2026);
        let coupon = FloatingRateCoupon::new(
            end,
            100.0,
            start,
            end,
            Some(2),
            index.clone(),
            1.0,
            0.0,
            None,
            None,
            None,
            false,
            None,
            BusinessDayConvention::Preceding,
        )
        .unwrap();
        index.add_fixing(coupon.fixing_date(), 0.05).unwrap();

        let mut pricer = BlackIborCouponPricer::new();
        pricer.initialize(&coupon);

        assert!((pricer.caplet_rate(0.03).unwrap() - 0.02).abs() < 1e-15);
        assert_eq!(pricer.caplet_rate(0.06).unwrap(), 0.0);
        assert!((pricer.floorlet_rate(0.06).unwrap() - 0.01).abs() < 1e-15);
        assert_eq!(pricer.floorlet_rate(0.03).unwrap(), 0.0);
    }

    /// An undetermined coupon with no optionlet volatility surface refuses,
    /// rather than pricing the caplet as if the volatility were zero.
    #[test]
    fn an_undetermined_coupon_without_a_surface_refuses() {
        let today = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let index = index_on(settings, flat_curve(today, 0.03));

        let start = Date::new(15, Month::June, 2027);
        let end = Date::new(15, Month::December, 2027);
        let coupon = FloatingRateCoupon::new(
            end,
            100.0,
            start,
            end,
            Some(2),
            index,
            1.0,
            0.0,
            None,
            None,
            None,
            false,
            None,
            BusinessDayConvention::Preceding,
        )
        .unwrap();

        let mut pricer = BlackIborCouponPricer::new();
        pricer.initialize(&coupon);

        let err = pricer.caplet_rate(0.03).unwrap_err();
        assert!(err.message().contains("missing optionlet volatility"));
    }

    /// A pricer with no coupon captured yet cannot price an optionlet.
    #[test]
    fn an_uninitialized_pricer_refuses_the_optionlet() {
        let pricer = BlackIborCouponPricer::new();
        assert!(
            pricer
                .caplet_rate(0.03)
                .unwrap_err()
                .message()
                .contains("not initialized")
        );
    }
}
