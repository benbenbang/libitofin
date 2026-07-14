//! Coupon paying a variable index-based rate.
//!
//! Port of `ql/cashflows/floatingratecoupon.{hpp,cpp}`. A
//! [`FloatingRateCoupon`] is a [`Coupon`] carrying an interest-rate index, a
//! gearing and spread, a fixing lag and an in-arrears flag, and a
//! [`FloatingRateCouponPricer`]. It computes no rate itself:
//! [`rate`](Coupon::rate) requires a pricer, hands the coupon to
//! [`initialize`](FloatingRateCouponPricer::initialize), and returns
//! [`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate)
//! (`floatingratecoupon.cpp:88`). Gearing and spread are folded in by the
//! pricer, so the coupon reapplies neither.
//!
//! ## Reaching the index
//!
//! The coupon stores an abstract index. C++ holds
//! `shared_ptr<InterestRateIndex>`, which is also an `Index`; the Rust port
//! split those into [`InterestRateIndex`] (object-safe: the tenor/fixing
//! algebra) and [`Index`] (the fixing decision tree, but object-*un*safe
//! because `add_fixings` is generic). A stored `dyn InterestRateIndex` can
//! therefore reach the tenor face but not [`Index::fixing`].
//!
//! [`FloatingIndex`] bridges the gap: a supertrait of [`InterestRateIndex`]
//! (so it stays object-safe) that re-exposes the one live [`Index`] call the
//! coupon makes, [`fixing`](FloatingIndex::fixing). The blanket impl answers it
//! from [`Index::fixing`], so `Shared<dyn FloatingIndex>` carries both faces.
//! Everything else the coupon reads off the index - the fixing calendar,
//! fixing days, day counter, its observable and its settings - is fixed at
//! construction and captured there.
//!
//! ## Divergences from QuantLib
//!
//! The C++ coupon is a `LazyObject` caching `rate_`. As with the rest of the
//! cash-flow layer the cache is omitted: [`rate`](Coupon::rate) reruns the
//! pricer each call, which is a pure function of the same inputs, so the value
//! is unchanged. The behavioural half of the lazy object is kept: the coupon
//! forwards notifications from its index, pricer and the evaluation date to its
//! own observers.
//!
//! `QL_REQUIRE(index_, "no index provided")` has no port: the index is a
//! non-null [`Shared`], so its presence is structural. `price()` (amount times
//! a discount factor) and `convexityAdjustmentImpl`'s dead `gearing == 0`
//! branch are omitted; a null gearing is rejected at construction, so the
//! adjustment is unconditional.

use std::cell::RefCell;

use super::coupon::{Coupon, CouponBase};
use super::couponpricer::FloatingRateCouponPricer;
use crate::errors::QlResult;
use crate::fail;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::require;
use crate::shared::{Shared, SharedMut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real, Spread};

/// An [`InterestRateIndex`] viewed as its coupon needs it: the tenor face plus
/// the one [`Index`] call the coupon makes at rate time.
///
/// This is the object-safe bridge over the object-unsafe [`Index`] trait. It
/// is a supertrait of [`InterestRateIndex`] and is blanket-implemented for
/// every index, so any concrete index coerces into `Shared<dyn FloatingIndex>`.
pub trait FloatingIndex: InterestRateIndex {
    /// The index fixing on `fixing_date` (`Index::fixing` with today's fixing
    /// left to the store, never forecast).
    fn fixing(&self, fixing_date: Date) -> QlResult<Rate>;
}

impl<T: InterestRateIndex> FloatingIndex for T {
    fn fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        Index::fixing(self, fixing_date, false)
    }
}

/// Base floating-rate coupon.
///
/// Built with [`new`](Self::new); a pricer is attached later with
/// [`set_pricer`](Self::set_pricer). Its [`Coupon`], and hence [`CashFlow`] and
/// [`Event`], faces come from the blanket impls on [`Coupon`].
///
/// [`CashFlow`]: crate::cashflow::CashFlow
/// [`Event`]: crate::event::Event
pub struct FloatingRateCoupon {
    base: CouponBase,
    index: Shared<dyn FloatingIndex>,
    day_counter: DayCounter,
    fixing_days: Natural,
    gearing: Real,
    spread: Spread,
    is_in_arrears: bool,
    fixing_convention: BusinessDayConvention,
    fixing_calendar: Calendar,
    pricer: RefCell<Option<SharedMut<dyn FloatingRateCouponPricer>>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl FloatingRateCoupon {
    /// Builds a coupon over `index`.
    ///
    /// A `None` `fixing_days` defaults to the index's, and a `None`
    /// `day_counter` to the index's, exactly as the C++ constructor's
    /// `Null<Natural>` and empty-day-counter checks do. The coupon registers
    /// its forwarding observer with the index and with the evaluation date the
    /// index reads (the two `registerWith` calls). A null gearing is rejected.
    #[allow(clippy::too_many_arguments)]
    pub fn new<I: InterestRateIndex + 'static>(
        payment_date: Date,
        nominal: Real,
        accrual_start_date: Date,
        accrual_end_date: Date,
        fixing_days: Option<Natural>,
        index: Shared<I>,
        gearing: Real,
        spread: Spread,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        day_counter: Option<DayCounter>,
        is_in_arrears: bool,
        ex_coupon_date: Option<Date>,
        fixing_convention: BusinessDayConvention,
    ) -> QlResult<FloatingRateCoupon> {
        require!(gearing != 0.0, "Null gearing not allowed");

        let fixing_days = fixing_days.unwrap_or_else(|| index.fixing_days());
        let day_counter = day_counter.unwrap_or_else(|| index.day_counter().clone());
        let fixing_calendar = Index::fixing_calendar(&*index);

        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        Index::observable(&*index).register_observer(&observer);
        Index::settings(&*index).register_eval_date_observer(&observer);

        let index: Shared<dyn FloatingIndex> = index;
        Ok(FloatingRateCoupon {
            base: CouponBase::new(
                payment_date,
                nominal,
                accrual_start_date,
                accrual_end_date,
                ref_period_start,
                ref_period_end,
                ex_coupon_date,
            ),
            index,
            day_counter,
            fixing_days,
            gearing,
            spread,
            is_in_arrears,
            fixing_convention,
            fixing_calendar,
            pricer: RefCell::new(None),
            observable,
            forwarder,
        })
    }

    /// The floating index.
    pub fn index(&self) -> &Shared<dyn FloatingIndex> {
        &self.index
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

    /// Whether the coupon fixes in arrears.
    pub fn is_in_arrears(&self) -> bool {
        self.is_in_arrears
    }

    /// The business-day convention used to compute the fixing date.
    pub fn fixing_convention(&self) -> BusinessDayConvention {
        self.fixing_convention
    }

    /// The fixing date: the accrual start (or end, in arrears) moved back
    /// `fixing_days` business days on the index's fixing calendar under the
    /// coupon's own fixing convention (`floatingratecoupon.cpp:78`).
    pub fn fixing_date(&self) -> Date {
        let ref_date = if self.is_in_arrears {
            self.accrual_end_date()
        } else {
            self.accrual_start_date()
        };
        self.fixing_calendar.advance(
            ref_date,
            -(self.fixing_days as Integer),
            TimeUnit::Days,
            self.fixing_convention,
            false,
        )
    }

    /// The index fixing at the coupon's [`fixing_date`](Self::fixing_date).
    pub fn index_fixing(&self) -> QlResult<Rate> {
        self.index.fixing(self.fixing_date())
    }

    /// The convexity-adjusted fixing, `(rate - spread) / gearing`.
    pub fn adjusted_fixing(&self) -> QlResult<Rate> {
        Ok((self.rate()? - self.spread) / self.gearing)
    }

    /// The convexity adjustment, the adjusted fixing less the index fixing.
    pub fn convexity_adjustment(&self) -> QlResult<Rate> {
        Ok(self.adjusted_fixing()? - self.index_fixing()?)
    }

    /// The currently attached pricer, if one has been set.
    pub fn pricer(&self) -> Option<SharedMut<dyn FloatingRateCouponPricer>> {
        self.pricer.borrow().clone()
    }

    /// Attaches `pricer`, re-pointing the coupon's observation from the old
    /// pricer to the new one and notifying observers
    /// (`FloatingRateCoupon::setPricer`).
    pub fn set_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
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

impl AsObservable for FloatingRateCoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Coupon for FloatingRateCoupon {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::currency::Currency;
    use crate::fail;
    use crate::handle::Handle;
    use crate::indexes::iborindex::IborIndex;
    use crate::patterns::observable::Observable;
    use crate::settings::Settings;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn start() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn end() -> Date {
        Date::new(15, Month::July, 2026)
    }

    fn payment() -> Date {
        Date::new(17, Month::July, 2026)
    }

    fn ibor(settings: Shared<Settings<Date>>) -> Shared<IborIndex> {
        shared(IborIndex::new(
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
        ))
    }

    fn coupon_on(
        index: Shared<IborIndex>,
        fixing_days: Option<Natural>,
        gearing: Real,
        spread: Spread,
        is_in_arrears: bool,
    ) -> FloatingRateCoupon {
        FloatingRateCoupon::new(
            payment(),
            100.0,
            start(),
            end(),
            fixing_days,
            index,
            gearing,
            spread,
            None,
            None,
            None,
            is_in_arrears,
            None,
            BusinessDayConvention::Preceding,
        )
        .unwrap()
    }

    fn coupon(fixing_days: Option<Natural>, gearing: Real, spread: Spread) -> FloatingRateCoupon {
        coupon_on(
            ibor(shared(Settings::new())),
            fixing_days,
            gearing,
            spread,
            false,
        )
    }

    /// Records that `initialize` ran and with which coupon, and returns a fixed
    /// swaplet rate. Stands in for the (unported) real pricers.
    struct RecordingPricer {
        swaplet: Rate,
        calls: SharedMut<usize>,
        seen_gearing: SharedMut<Option<Real>>,
        observable: Observable,
    }

    impl RecordingPricer {
        fn new(
            swaplet: Rate,
        ) -> (
            SharedMut<RecordingPricer>,
            SharedMut<usize>,
            SharedMut<Option<Real>>,
        ) {
            let calls = shared_mut(0usize);
            let seen_gearing = shared_mut(None);
            let pricer = shared_mut(RecordingPricer {
                swaplet,
                calls: calls.clone(),
                seen_gearing: seen_gearing.clone(),
                observable: Observable::new(),
            });
            (pricer, calls, seen_gearing)
        }
    }

    impl AsObservable for RecordingPricer {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl FloatingRateCouponPricer for RecordingPricer {
        fn initialize(&mut self, coupon: &FloatingRateCoupon) {
            *self.calls.borrow_mut() += 1;
            *self.seen_gearing.borrow_mut() = Some(coupon.gearing());
        }

        fn swaplet_rate(&self) -> QlResult<Rate> {
            Ok(self.swaplet)
        }

        fn swaplet_rate_for(&self, _index_fixing: QlResult<Rate>) -> QlResult<Rate> {
            Ok(self.swaplet)
        }

        fn caplet_rate(&self, _effective_cap: Rate, _forward: QlResult<Rate>) -> QlResult<Rate> {
            fail!("caplet rate not priced by the recording stub")
        }

        fn floorlet_rate(
            &self,
            _effective_floor: Rate,
            _forward: QlResult<Rate>,
        ) -> QlResult<Rate> {
            fail!("floorlet rate not priced by the recording stub")
        }
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

    #[test]
    fn rate_without_a_pricer_is_an_error() {
        let coupon = coupon(None, 1.0, 0.0);
        let err = coupon.rate().unwrap_err();
        assert!(err.message().contains("pricer not set"));
    }

    #[test]
    fn rate_and_amount_route_through_the_pricer() {
        let coupon = coupon(None, 2.0, 0.0);
        let (pricer, calls, seen_gearing) = RecordingPricer::new(0.05);
        coupon.set_pricer(pricer as SharedMut<dyn FloatingRateCouponPricer>);

        assert_eq!(coupon.rate().unwrap(), 0.05);
        assert_eq!(*calls.borrow(), 1, "initialize ran once per rate query");
        assert_eq!(
            *seen_gearing.borrow(),
            Some(2.0),
            "initialize received the coupon"
        );

        let expected = 0.05 * coupon.accrual_period() * coupon.nominal();
        assert!((coupon.amount().unwrap() - expected).abs() < 1e-15);
    }

    #[test]
    fn adjusted_fixing_strips_the_spread_and_gearing() {
        let coupon = coupon(None, 2.0, 0.01);
        let (pricer, ..) = RecordingPricer::new(0.05);
        coupon.set_pricer(pricer as SharedMut<dyn FloatingRateCouponPricer>);

        assert!((coupon.adjusted_fixing().unwrap() - 0.02).abs() < 1e-15);
    }

    #[test]
    fn the_recording_stub_does_not_price_optionlets() {
        let (pricer, ..) = RecordingPricer::new(0.05);
        let pricer = pricer.borrow();
        assert!(
            pricer
                .caplet_rate(0.03, Ok(0.05))
                .unwrap_err()
                .message()
                .contains("recording stub")
        );
        assert!(pricer.floorlet_rate(0.01, Ok(0.05)).is_err());
    }

    #[test]
    fn set_pricer_swaps_which_pricer_is_observed() {
        let coupon = coupon(None, 1.0, 0.0);
        let flag = shared_mut(Flag::default());
        coupon
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        let (p1, ..) = RecordingPricer::new(0.01);
        let p1 = p1 as SharedMut<dyn FloatingRateCouponPricer>;
        coupon.set_pricer(p1.clone());
        flag.borrow_mut().up = false;
        p1.borrow().observable().notify_observers();
        assert!(flag.borrow().up, "the attached pricer is observed");

        let (p2, ..) = RecordingPricer::new(0.02);
        let p2 = p2 as SharedMut<dyn FloatingRateCouponPricer>;
        coupon.set_pricer(p2.clone());
        flag.borrow_mut().up = false;

        p1.borrow().observable().notify_observers();
        assert!(
            !flag.borrow().up,
            "the replaced pricer is no longer observed"
        );
        p2.borrow().observable().notify_observers();
        assert!(flag.borrow().up, "the new pricer is observed");
    }

    #[test]
    fn the_fixing_date_moves_back_from_the_accrual_start_or_end() {
        let calendar = Target::new();

        let normal = coupon(Some(2), 1.0, 0.0);
        assert_eq!(
            normal.fixing_date(),
            calendar.advance(
                start(),
                -2,
                TimeUnit::Days,
                BusinessDayConvention::Preceding,
                false
            )
        );

        let in_arrears = coupon_on(ibor(shared(Settings::new())), Some(2), 1.0, 0.0, true);
        assert_eq!(
            in_arrears.fixing_date(),
            calendar.advance(
                end(),
                -2,
                TimeUnit::Days,
                BusinessDayConvention::Preceding,
                false
            )
        );
    }

    #[test]
    fn fixing_days_defaults_to_the_index() {
        assert_eq!(coupon(None, 1.0, 0.0).fixing_days(), 2);
        assert_eq!(coupon(Some(0), 1.0, 0.0).fixing_days(), 0);
    }

    #[test]
    fn index_fixing_reads_the_store_through_the_index() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(20, Month::January, 2026));
        let index = ibor(settings);
        let coupon = coupon_on(index.clone(), Some(2), 1.0, 0.0, false);

        let fixing_date = coupon.fixing_date();
        index.add_fixing(fixing_date, 0.025).unwrap();

        assert_eq!(coupon.index_fixing().unwrap(), 0.025);
    }
}
