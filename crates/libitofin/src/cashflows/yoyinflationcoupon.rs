//! Coupon paying a year-on-year inflation rate.
//!
//! Port of `ql/cashflows/yoyinflationcoupon.{hpp,cpp}` and of the swaplet half
//! of `YoYInflationCouponPricer` (`ql/cashflows/inflationcouponpricer.{hpp,cpp}`).
//! A [`YoYInflationCoupon`] carries a [`YoYInflationIndex`], an observation lag
//! and an interpolation rule, a gearing and spread, and a pricer. As with
//! [`FloatingRateCoupon`] it computes no rate itself: [`rate`](Coupon::rate)
//! requires a pricer, hands the coupon to
//! [`initialize`](YoYInflationCouponPricer::initialize) and returns
//! [`swaplet_rate`](YoYInflationCouponPricer::swaplet_rate)
//! (`inflationcoupon.cpp:69-73`).
//!
//! ## Divergences from QuantLib
//!
//! There is no `InflationCoupon` base. C++ needs one so that a single
//! `InflationCouponPricer` interface can serve zero and year-on-year coupons,
//! and pays for it twice: `initialize` recovers the concrete coupon with a
//! `dynamic_cast` (`inflationcouponpricer.cpp:136-138`) and `checkPricerImpl`
//! re-checks the pricer's type at `setPricer` (`yoyinflationcoupon.cpp:55-59`).
//! Here the pricer slot is a `SharedMut<dyn YoYInflationCouponPricer>` keyed on
//! the concrete coupon, so both checks are structural and neither downcast has a
//! port. A base can be lifted out if a `CPICoupon` consumer ever lands.
//!
//! The C++ coupon is a `LazyObject` caching `rate_` (`inflationcoupon.cpp:63-66`).
//! As elsewhere in the cash-flow layer the cache is omitted and the pricer is
//! rerun on each call, which is a pure function of the same inputs. The
//! forwarding half is kept: the coupon rebroadcasts its index, its pricer and
//! the evaluation date to its own observers.
//!
//! Deferred, with the cap/floor slice of `#838`: every vol-dependent pricer
//! (`Black`/`UnitDisplacedBlack`/`Bachelier`) and the `capletRate`,
//! `floorletRate` and `optionletRate` machinery underneath them. The coupon's
//! own `adjustedFixing()` (`yoyinflationcoupon.hpp:60`, `:89`) goes with them:
//! nothing in `ql/` or `test-suite/` calls it, so it has no behaviour to pin
//! and waits for a caller. `price()`
//! (amount times a discount factor, `inflationcoupon.cpp:91-93`) and the
//! `accept(AcyclicVisitor&)` overrides have no port, as for the other coupons.

use std::cell::RefCell;

use super::coupon::{Coupon, CouponBase};
use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::indexes::index::Index;
use crate::indexes::inflationindex::{Cpi, CpiInterpolationType, YoYInflationIndex};
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::shared::{Shared, SharedMut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real, Spread, Time};

/// Coupon paying a year-on-year inflation index.
///
/// Built with [`new`](Self::new); a pricer is attached later with
/// [`set_pricer`](Self::set_pricer). Its [`Coupon`], and hence [`CashFlow`] and
/// [`Event`], faces come from the blanket impls on [`Coupon`].
///
/// [`CashFlow`]: crate::cashflow::CashFlow
/// [`Event`]: crate::event::Event
pub struct YoYInflationCoupon {
    base: CouponBase,
    yoy_index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    fixing_days: Natural,
    day_counter: DayCounter,
    gearing: Real,
    spread: Spread,
    pricer: RefCell<Option<SharedMut<dyn YoYInflationCouponPricer>>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl YoYInflationCoupon {
    /// Builds a coupon over `yoy_index`.
    ///
    /// The argument order is the C++ one (`yoyinflationcoupon.cpp:30-42`), where
    /// `interpolation` precedes `day_counter`. The coupon registers its
    /// forwarding observer with the index and with the evaluation date the index
    /// reads, the two `registerWith` calls of `inflationcoupon.cpp:47-48`. There
    /// is no ex-coupon date: the inflation constructor takes one but no
    /// year-on-year caller passes it.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payment_date: Date,
        nominal: Real,
        accrual_start_date: Date,
        accrual_end_date: Date,
        fixing_days: Natural,
        yoy_index: Shared<YoYInflationIndex>,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        day_counter: DayCounter,
        gearing: Real,
        spread: Spread,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
    ) -> YoYInflationCoupon {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        Index::observable(&*yoy_index).register_observer(&observer);
        Index::settings(&*yoy_index).register_eval_date_observer(&observer);

        YoYInflationCoupon {
            base: CouponBase::new(
                payment_date,
                nominal,
                accrual_start_date,
                accrual_end_date,
                ref_period_start,
                ref_period_end,
                None,
            ),
            yoy_index,
            observation_lag,
            interpolation,
            fixing_days,
            day_counter,
            gearing,
            spread,
            pricer: RefCell::new(None),
            observable,
            forwarder,
        }
    }

    /// The year-on-year index the coupon observes.
    pub fn yoy_index(&self) -> &Shared<YoYInflationIndex> {
        &self.yoy_index
    }

    /// How far back the coupon observes the index.
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// How the observation interpolates between index fixings.
    pub fn interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }

    /// The number of fixing days.
    pub fn fixing_days(&self) -> Natural {
        self.fixing_days
    }

    /// The multiplicative coefficient applied to the index.
    pub fn gearing(&self) -> Real {
        self.gearing
    }

    /// The spread paid over the index fixing.
    pub fn spread(&self) -> Spread {
        self.spread
    }

    /// The date the observation is published on: the reference-period end moved
    /// back by the observation lag, then back `fixing_days` business days under
    /// `ModifiedPreceding` (`inflationcoupon.cpp:87-92`).
    ///
    /// An inflation index has no fixing calendar of its own - it returns a null
    /// one - so the roll is inert unless a caller supplies fixing days.
    pub fn fixing_date(&self) -> Date {
        Index::fixing_calendar(&*self.yoy_index).advance(
            self.reference_period_end() - self.observation_lag,
            -(self.fixing_days as Integer),
            TimeUnit::Days,
            BusinessDayConvention::ModifiedPreceding,
            false,
        )
    }

    /// The year-on-year rate the coupon observes: the lagged rate at the
    /// **accrual end** (`yoyinflationcoupon.cpp:62-64`).
    ///
    /// The override matters: the base `InflationCoupon::indexFixing` reads the
    /// index at [`fixing_date`](Self::fixing_date) (`inflationcoupon.cpp:95-97`),
    /// whereas a year-on-year coupon lags off its accrual end. The fixing date
    /// is kept for the publication calendar it implies, not for the rate.
    pub fn index_fixing(&self) -> QlResult<Rate> {
        Cpi::lagged_yoy_rate(
            &self.yoy_index,
            self.accrual_end_date(),
            self.observation_lag,
            self.interpolation,
        )
    }

    /// The currently attached pricer, if one has been set.
    pub fn pricer(&self) -> Option<SharedMut<dyn YoYInflationCouponPricer>> {
        self.pricer.borrow().clone()
    }

    /// Attaches `pricer`, re-pointing the coupon's observation from the old
    /// pricer to the new one and notifying observers
    /// (`InflationCoupon::setPricer`, `inflationcoupon.cpp:51-59`).
    pub fn set_pricer(&self, pricer: SharedMut<dyn YoYInflationCouponPricer>) {
        let observer = self.forwarder.clone() as SharedMut<dyn Observer>;
        {
            let mut slot = self.pricer.borrow_mut();
            if let Some(old) = slot.as_ref() {
                old.borrow().observable().unregister_observer(&observer);
            }
            pricer.borrow().observable().register_observer(&observer);
            *slot = Some(pricer);
        }
        self.observable.notify_observers();
    }
}

impl AsObservable for YoYInflationCoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Coupon for YoYInflationCoupon {
    fn coupon_base(&self) -> &CouponBase {
        &self.base
    }

    fn amount(&self) -> QlResult<Real> {
        Ok(self.rate()? * self.accrual_period() * self.nominal())
    }

    fn rate(&self) -> QlResult<Rate> {
        let slot = self.pricer.borrow();
        let Some(pricer) = slot.as_ref() else {
            fail!("pricer not set");
        };
        pricer.borrow_mut().initialize(self);
        pricer.borrow().swaplet_rate()
    }

    fn day_counter(&self) -> DayCounter {
        self.day_counter.clone()
    }

    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        if date <= self.accrual_start_date() || date > self.coupon_base().payment_date() {
            Ok(0.0)
        } else {
            Ok(self.nominal() * self.rate()? * self.accrued_period(date))
        }
    }
}

/// Pricer for year-on-year inflation coupons.
///
/// A coupon registers as an observer of its pricer (via [`AsObservable`]) and,
/// on each rate query, calls [`initialize`](Self::initialize) then reads
/// [`swaplet_rate`](Self::swaplet_rate). The cap and floor half of the C++
/// interface (`inflationcouponpricer.hpp:62-68`) is deferred with `#838`.
pub trait YoYInflationCouponPricer: AsObservable {
    /// Caches whatever the pricer needs from `coupon` before a rate is read
    /// (`YoYInflationCouponPricer::initialize`).
    fn initialize(&mut self, coupon: &YoYInflationCoupon);

    /// The coupon's rate, gearing and spread already folded in (`swapletRate`).
    fn swaplet_rate(&self) -> QlResult<Rate>;

    /// The swaplet rate accrued and discounted to today (`swapletPrice`).
    fn swaplet_price(&self) -> QlResult<Real>;
}

/// The rate-and-discount pricer: QuantLib's `YoYInflationCouponPricer` with the
/// volatility-dependent descendants left to `#838`.
///
/// Built either without a nominal curve, with [`new`](Self::new), or over one
/// with [`with_nominal_term_structure`](Self::with_nominal_term_structure). The
/// first still yields rates - which is the point of `swapletRate` reading the
/// coupon rather than a curve (`inflationcouponpricer.cpp:164-167`) - but
/// refuses prices.
pub struct SwapletYoYInflationCouponPricer {
    nominal_term_structure: Handle<dyn YieldTermStructure>,
    gearing: Real,
    spread: Spread,
    accrual_period: Time,
    index_fixing: Option<QlResult<Rate>>,
    discount: Option<QlResult<Real>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl SwapletYoYInflationCouponPricer {
    /// Builds a pricer with no nominal curve and no coupon captured yet
    /// (`YoYInflationCouponPricer() = default`).
    pub fn new() -> Self {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        SwapletYoYInflationCouponPricer {
            nominal_term_structure: Handle::empty(),
            gearing: 0.0,
            spread: 0.0,
            accrual_period: 0.0,
            index_fixing: None,
            discount: None,
            observable,
            forwarder,
        }
    }

    /// Builds a pricer discounting on `nominal_term_structure`, registering for
    /// its changes (`inflationcouponpricer.cpp:37-41`).
    pub fn with_nominal_term_structure(
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> Self {
        let mut pricer = Self::new();
        let observer = pricer.forwarder.clone() as SharedMut<dyn Observer>;
        nominal_term_structure.register_observer(&observer);
        pricer.nominal_term_structure = nominal_term_structure;
        pricer
    }

    /// The nominal curve the pricer discounts on.
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }

    /// The discount factor applied to a payment on `payment_date`
    /// (`inflationcouponpricer.cpp:146-153`): a payment on or before the
    /// curve's reference date is undiscounted.
    fn discount_at(&self, payment_date: Date) -> QlResult<Real> {
        let curve = self.nominal_term_structure.current_link()?;
        if payment_date > curve.reference_date()? {
            curve.discount_date(payment_date, false)
        } else {
            Ok(1.0)
        }
    }
}

impl Default for SwapletYoYInflationCouponPricer {
    fn default() -> Self {
        Self::new()
    }
}

impl AsObservable for SwapletYoYInflationCouponPricer {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl YoYInflationCouponPricer for SwapletYoYInflationCouponPricer {
    /// Captures the scalars the two readers need and nothing else.
    ///
    /// C++ keeps a `const YoYInflationCoupon*` and reads `accrualPeriod()` back
    /// through it at `swapletPrice` (`inflationcouponpricer.cpp:159`); a stored
    /// back-reference needs a lifetime on the pricer, so the accrual period
    /// joins the capture list instead. `paymentDate_` is a C++ member for the
    /// benefit of descendants; here it is only the input to the discount ladder,
    /// so the ladder is run now and the date not kept. A missing fixing, and a
    /// curve that refuses, are captured as the [`Err`] they are rather than
    /// swallowed: `initialize` has no way to report one.
    fn initialize(&mut self, coupon: &YoYInflationCoupon) {
        self.gearing = coupon.gearing();
        self.spread = coupon.spread();
        self.accrual_period = coupon.accrual_period();
        self.index_fixing = Some(coupon.index_fixing());
        self.discount = if self.nominal_term_structure.is_empty() {
            None
        } else {
            Some(self.discount_at(coupon.coupon_base().payment_date()))
        };
    }

    fn swaplet_rate(&self) -> QlResult<Rate> {
        let Some(index_fixing) = &self.index_fixing else {
            fail!("pricer not initialized: no coupon captured");
        };
        Ok(self.gearing * index_fixing.clone()? + self.spread)
    }

    fn swaplet_price(&self) -> QlResult<Real> {
        let Some(discount) = &self.discount else {
            fail!("no nominal term structure provided");
        };
        Ok(self.swaplet_rate()? * self.accrual_period * discount.clone()?)
    }
}

#[cfg(test)]
mod tests {
    //! QuantLib prices no bare year-on-year coupon: `inflation.cpp` reaches one
    //! only through the swap helpers, and never calls `amount()` on it. The
    //! amount oracle below is therefore self-authored against
    //! [`Cpi::lagged_yoy_rate`], whose own numbers `testCpiYoY*Interpolation`
    //! pins (`inflationindex.rs`). The one literal QuantLib supplies is the
    //! fixing date of the 2008 swap's first coupon.

    use super::*;
    use crate::currency::Currency;
    use crate::indexes::Region;
    use crate::interestrate::Compounding;
    use crate::patterns::observable::Observer;
    use crate::settings::Settings;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::Month::{
        August, December, February, January, July, June, March, November, September,
    };
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    const NOMINAL: Real = 1_000_000.0;
    const GEARING: Real = 2.5;
    const SPREAD: Spread = 0.0035;

    fn lag() -> Period {
        Period::new(3, TimeUnit::Months)
    }

    /// The quoted fixture of `testCpiYoYQuotedFlatInterpolation`
    /// (`inflation.cpp:1449-1461`): it is 10 February 2022 and UK YY_RPI has
    /// published `rates`. Every date the coupons read is far behind the
    /// publication horizon, so nothing forecasts off the empty curve handle.
    fn published_index(rates: &[(Date, Rate)]) -> Shared<YoYInflationIndex> {
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
        for &(date, rate) in rates {
            index
                .add_fixing(date, rate)
                .expect("adding a published rate");
        }
        index
    }

    fn rates_2021() -> Vec<(Date, Rate)> {
        vec![
            (Date::new(1, November, 2020), 0.02935),
            (Date::new(1, December, 2020), 0.02954),
            (Date::new(1, January, 2021), 0.02946),
            (Date::new(1, February, 2021), 0.02960),
            (Date::new(1, March, 2021), 0.02969),
        ]
    }

    fn coupon_ending(
        index: &Shared<YoYInflationIndex>,
        accrual_end: Date,
        payment_date: Date,
    ) -> YoYInflationCoupon {
        YoYInflationCoupon::new(
            payment_date,
            NOMINAL,
            accrual_end - Period::new(1, TimeUnit::Years),
            accrual_end,
            0,
            Shared::clone(index),
            lag(),
            CpiInterpolationType::Flat,
            Actual360::new(),
            GEARING,
            SPREAD,
            None,
            None,
        )
    }

    fn swaplet_pricer(coupon: &YoYInflationCoupon) -> SharedMut<SwapletYoYInflationCouponPricer> {
        let pricer = shared_mut(SwapletYoYInflationCouponPricer::new());
        coupon.set_pricer(pricer.clone() as SharedMut<dyn YoYInflationCouponPricer>);
        pricer
    }

    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// The coupon pays `nominal * accrualPeriod * (gearing * indexFixing +
    /// spread)`, with the fixing lagged off the accrual end. A gearing away
    /// from one and a non-zero spread keep either term from hiding a dropped
    /// one.
    #[test]
    fn the_amount_gears_and_spreads_the_lagged_yoy_rate() {
        let index = published_index(&rates_2021());
        let accrual_end = Date::new(10, February, 2021);
        let coupon = coupon_ending(&index, accrual_end, Date::new(12, February, 2021));
        swaplet_pricer(&coupon);

        let fixing = Cpi::lagged_yoy_rate(&index, accrual_end, lag(), CpiInterpolationType::Flat)
            .expect("November 2020 is on record");
        assert!((fixing - 0.02935).abs() < 1e-12, "fixing was {fixing}");

        let expected = NOMINAL * coupon.accrual_period() * (GEARING * fixing + SPREAD);
        let amount = coupon.amount().expect("the observed period is published");
        assert!((amount - expected).abs() < 1e-10, "amount was {amount}");
    }

    /// `inflation.cpp:1176-1178`: the first coupon of the 2008 year-on-year
    /// swap fixes on 13 June 2008, its reference-period end lagged two months
    /// with no fixing days to roll. The second assertion pins that the lag runs
    /// off the *reference* period end and not the accrual end, which the swap's
    /// regular first period leaves indistinguishable.
    #[test]
    fn the_fixing_date_lags_the_reference_period_end() {
        let index = published_index(&[]);
        let fixing_date_with = |ref_period_end: Date| {
            YoYInflationCoupon::new(
                Date::new(13, August, 2008),
                NOMINAL,
                Date::new(13, August, 2007),
                Date::new(13, August, 2008),
                0,
                Shared::clone(&index),
                Period::new(2, TimeUnit::Months),
                CpiInterpolationType::Flat,
                Actual360::new(),
                1.0,
                0.0,
                Some(Date::new(13, August, 2007)),
                Some(ref_period_end),
            )
            .fixing_date()
        };

        assert_eq!(
            fixing_date_with(Date::new(13, August, 2008)),
            Date::new(13, June, 2008)
        );
        assert_eq!(
            fixing_date_with(Date::new(13, September, 2008)),
            Date::new(13, July, 2008)
        );
    }

    /// The coupon captures no fixing at construction: a figure published later
    /// turns a refusal into a number. The D11 store rejects a *conflicting*
    /// value for a date it already holds, so the observable change is the
    /// missing-to-published one rather than a revision.
    #[test]
    fn a_fixing_published_after_construction_is_read_live() {
        let rates = rates_2021();
        let index = published_index(&rates[..4]);
        let coupon = coupon_ending(&index, Date::new(10, June, 2021), Date::new(10, June, 2021));
        swaplet_pricer(&coupon);

        let missing = coupon
            .amount()
            .expect_err("March 2021 is not published yet");
        assert!(
            missing.to_string().contains("Missing UK YY_RPI fixing"),
            "err was: {missing}"
        );

        let (date, rate) = rates[4];
        index
            .add_fixing(date, rate)
            .expect("March 2021 is published");

        let expected = NOMINAL * coupon.accrual_period() * (GEARING * rate + SPREAD);
        let amount = coupon
            .amount()
            .expect("the observed period is now published");
        assert!((amount - expected).abs() < 1e-10, "amount was {amount}");
    }

    /// The coupon registers with the index (`inflationcoupon.cpp:47`), so a
    /// published figure reaches the coupon's own observers. Load-bearing where
    /// the live read above is not: an engine caching an NPV is invalidated only
    /// through this chain, and a deleted registration still reads live.
    #[test]
    fn a_published_fixing_notifies_the_coupons_observers() {
        let rates = rates_2021();
        let index = published_index(&rates[..4]);
        let coupon = coupon_ending(&index, Date::new(10, June, 2021), Date::new(10, June, 2021));

        let flag = shared_mut(Flag::default());
        coupon
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        let (date, rate) = rates[4];
        index
            .add_fixing(date, rate)
            .expect("March 2021 is published");

        assert!(flag.borrow().up, "the index reaches the coupon's observers");
    }

    /// `swapletPrice` accrues and discounts what `swapletRate` returns
    /// (`inflationcouponpricer.cpp:157-160`), reading the accrual period the
    /// C++ pricer takes back off its coupon pointer.
    #[test]
    fn a_swaplet_price_accrues_and_discounts_the_rate() {
        let index = published_index(&rates_2021());
        let payment_date = Date::new(10, March, 2022);
        let coupon = coupon_ending(&index, Date::new(10, February, 2021), payment_date);

        let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
            Date::new(10, February, 2022),
            0.03,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
            as Shared<dyn YieldTermStructure>);
        let pricer =
            shared_mut(SwapletYoYInflationCouponPricer::with_nominal_term_structure(curve.clone()));
        coupon.set_pricer(pricer.clone() as SharedMut<dyn YoYInflationCouponPricer>);

        let rate = coupon.rate().expect("the observed period is published");
        let discount = curve
            .current_link()
            .expect("the curve is linked")
            .discount_date(payment_date, false)
            .expect("the payment is on the curve");
        assert!(discount < 1.0, "the payment discounts, discount {discount}");

        let price = pricer.borrow().swaplet_price().expect("the curve prices");
        let expected = rate * coupon.accrual_period() * discount;
        assert!((price - expected).abs() < 1e-12, "price was {price}");
    }

    #[test]
    fn a_rate_without_a_pricer_is_an_error() {
        let index = published_index(&rates_2021());
        let coupon = coupon_ending(
            &index,
            Date::new(10, February, 2021),
            Date::new(10, February, 2021),
        );

        let err = coupon.rate().expect_err("no pricer is attached");
        assert!(err.message().contains("pricer not set"), "err was: {err}");
    }

    #[test]
    fn a_swaplet_price_without_a_nominal_curve_is_an_error() {
        let index = published_index(&rates_2021());
        let coupon = coupon_ending(
            &index,
            Date::new(10, February, 2021),
            Date::new(10, February, 2021),
        );
        let pricer = swaplet_pricer(&coupon);
        coupon.rate().expect("a rate needs no curve");

        let err = pricer
            .borrow()
            .swaplet_price()
            .expect_err("prices need a nominal curve");
        assert!(
            err.message().contains("no nominal term structure provided"),
            "err was: {err}"
        );
    }

    #[test]
    fn a_swaplet_rate_before_initialize_is_an_error() {
        let pricer = SwapletYoYInflationCouponPricer::new();

        let err = pricer.swaplet_rate().expect_err("no coupon was captured");
        assert!(
            err.message().contains("pricer not initialized"),
            "err was: {err}"
        );
    }
}
