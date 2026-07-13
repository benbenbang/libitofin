//! Coupon paying a Libor-type index.
//!
//! Port of `ql/cashflows/iborcoupon.{hpp,cpp}`. An [`IborCoupon`] is a
//! [`FloatingRateCoupon`] on a concrete [`IborIndex`]: it carries the same
//! nominal, accrual dates, gearing, spread and fixing lag, and prices through a
//! [`FloatingRateCouponPricer`] (the [`BlackIborCouponPricer`] swaplet path).
//! Over the base it adds [`has_fixed`](IborCoupon::has_fixed) and keeps its own
//! concrete [`IborIndex`] handle (the C++ `iborIndex_`).
//!
//! [`BlackIborCouponPricer`]: super::couponpricer::BlackIborCouponPricer
//!
//! ## The index fixing
//!
//! C++ overrides `indexFixing()` to branch on `hasFixed()`: a determined coupon
//! returns the required past fixing, an undetermined one forecasts. That branch
//! is the same one [`Index::fixing`](crate::indexes::index::Index::fixing)
//! already makes: for a fixing date strictly before today (or on today under
//! `enforcesTodaysHistoricFixings`) it requires the stored fixing, otherwise it
//! forecasts. So [`index_fixing`](IborCoupon::index_fixing) delegates to the
//! base [`FloatingRateCoupon::index_fixing`], which yields the identical value
//! for every determined case - the only cases this slice's C++ oracles reach.
//! The forecast branch is nonetheless live: `rate()` on an unfixed coupon over a
//! linked curve returns the index's natural forecast, i.e. the indexed-coupon
//! convention, not the par approximation. That behaviour is deliberate and
//! pinned by `an_unfixed_coupon_forecasts_in_indexed_mode_not_par`, and holds
//! until the par mode is threaded explicitly (see the divergence below).
//!
//! ## Divergences from QuantLib
//!
//! The C++ `indexFixing()` forecast branch uses a specialized per-coupon
//! `forecastFixing(valueDate, endDate, spanningTime)` whose dates depend on the
//! `IborCoupon::Settings` singleton (`usingAtParCoupons`): par coupons roll the
//! estimation end off the next fixing date, indexed coupons use the index
//! maturity. That flag is a global singleton, which D5 forbids, and its whole
//! effect is confined to the forecast branch. The base
//! [`FloatingRateCoupon::index_fixing`] forecast follows the index's natural
//! maturity, i.e. the indexed-coupon convention (`QL_USE_INDEXED_COUPON`). The
//! par-coupon approximation - the C++ compile-time default in this tree - and
//! the cached-data machinery it needs are deferred to the cap/floor slice, where
//! the mode is threaded explicitly per D5 rather than read from a global. The
//! C++ oracles reach only determined fixings, so the deferral changes no number
//! they pin; the indexed-mode forecast the base takes is itself pinned by a
//! test, so this divergence is a deliberate, locked behaviour rather than a
//! latent one.
//!
//! `fixingValueDate`/`fixingMaturityDate`/`fixingEndDate`/`spanningTime` and the
//! `IborLeg` builder are omitted for the same reason: they exist to feed the
//! deferred par/indexed forecast and the leg (#7.10).

use super::coupon::{Coupon, CouponBase};
use super::couponpricer::FloatingRateCouponPricer;
use super::floatingratecoupon::FloatingRateCoupon;
use crate::errors::QlResult;
use crate::fail;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::{Shared, SharedMut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Rate, Real, Spread};

/// A coupon paying a Libor-type index (`ql/cashflows/iborcoupon.hpp`).
///
/// Built with [`new`](Self::new); a pricer is attached with
/// [`set_pricer`](Self::set_pricer). Its [`Coupon`] (and hence [`CashFlow`])
/// face delegates to the embedded [`FloatingRateCoupon`].
///
/// [`CashFlow`]: crate::cashflow::CashFlow
pub struct IborCoupon {
    base: FloatingRateCoupon,
    ibor_index: Shared<IborIndex>,
}

impl IborCoupon {
    /// Builds an ibor coupon over `index`.
    ///
    /// Mirrors the C++ constructor: it composes a [`FloatingRateCoupon`] over
    /// the same index and keeps a concrete handle to it (the C++ `iborIndex_`).
    /// A `None` `fixing_days` or `day_counter` defaults to the index's, as in the
    /// base; a null gearing is rejected there.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payment_date: Date,
        nominal: Real,
        accrual_start_date: Date,
        accrual_end_date: Date,
        fixing_days: Option<Natural>,
        index: Shared<IborIndex>,
        gearing: Real,
        spread: Spread,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        day_counter: Option<DayCounter>,
        is_in_arrears: bool,
        ex_coupon_date: Option<Date>,
        fixing_convention: BusinessDayConvention,
    ) -> QlResult<IborCoupon> {
        let base = FloatingRateCoupon::new(
            payment_date,
            nominal,
            accrual_start_date,
            accrual_end_date,
            fixing_days,
            index.clone(),
            gearing,
            spread,
            ref_period_start,
            ref_period_end,
            day_counter,
            is_in_arrears,
            ex_coupon_date,
            fixing_convention,
        )?;
        Ok(IborCoupon {
            base,
            ibor_index: index,
        })
    }

    /// The concrete ibor index (`iborIndex()`).
    pub fn ibor_index(&self) -> &Shared<IborIndex> {
        &self.ibor_index
    }

    /// The coupon's fixing date (`fixingDate`).
    pub fn fixing_date(&self) -> Date {
        self.base.fixing_date()
    }

    /// The index fixing at the coupon's [`fixing_date`](Self::fixing_date).
    ///
    /// Delegates to the base: for every determined case this is the required
    /// past fixing the C++ `indexFixing()` returns; an unfixed coupon forecasts
    /// in the indexed-coupon convention rather than the deferred par
    /// approximation (see the module docs).
    pub fn index_fixing(&self) -> QlResult<Rate> {
        self.base.index_fixing()
    }

    /// Whether the coupon's fixing is already determined (`hasFixed`).
    ///
    /// The fixing is determined when its date is before today, or on today when
    /// today's historic fixings are enforced or a historical fixing is on record
    /// (`iborcoupon.cpp:93`). Unlike the C++ global evaluation date, D5's lives
    /// on the index [`Settings`](crate::settings::Settings) and may be unset, so
    /// this returns [`QlResult`] and errors when it is, as the fixing decision
    /// tree does.
    pub fn has_fixed(&self) -> QlResult<bool> {
        let settings = self.ibor_index.settings();
        let today = match settings.evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: an ibor coupon needs a reference date"),
        };
        let fixing_date = self.fixing_date();
        if fixing_date > today {
            Ok(false)
        } else if fixing_date < today || settings.enforces_todays_historic_fixings() {
            Ok(true)
        } else {
            Ok(self.ibor_index.has_historical_fixing(fixing_date))
        }
    }

    /// The currently attached pricer, if one has been set.
    pub fn pricer(&self) -> Option<SharedMut<dyn FloatingRateCouponPricer>> {
        self.base.pricer()
    }

    /// Attaches `pricer` (`setPricer`).
    pub fn set_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
        self.base.set_pricer(pricer);
    }
}

impl AsObservable for IborCoupon {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl Coupon for IborCoupon {
    fn coupon_base(&self) -> &CouponBase {
        self.base.coupon_base()
    }

    fn amount(&self) -> QlResult<Real> {
        self.base.amount()
    }

    fn rate(&self) -> QlResult<Rate> {
        self.base.rate()
    }

    fn day_counter(&self) -> DayCounter {
        self.base.day_counter()
    }

    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        self.base.accrued_amount(date)
    }
}

#[cfg(test)]
mod tests {
    //! Oracles from `cashflows.cpp`, reproducing the two tests that build the
    //! actual `IborCoupon` + `BlackIborCouponPricer` type.
    //!
    //! `USDLibor` and `Euribor3M` construction differs only in currency,
    //! calendar and convention, none of which the assertions depend on: the
    //! amount check is self-consistent (past fixing times the coupon's own
    //! accrual period), and the has-fixed check turns only on the fixing date
    //! against today. `testFixedIborCouponWithoutForecastCurve` uses a hand-built
    //! TARGET/Actual/360 6M index in place of `USDLibor(6M)`, and
    //! `testIborCouponKnowsWhenitHasFixed` uses the ported [`Euribor`] 3M.

    use super::*;
    use crate::currency::Currency;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    use super::super::couponpricer::BlackIborCouponPricer;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn ibor6m(settings: Shared<Settings<Date>>) -> Shared<IborIndex> {
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

    fn pricer() -> SharedMut<dyn FloatingRateCouponPricer> {
        shared_mut(BlackIborCouponPricer::new()) as SharedMut<dyn FloatingRateCouponPricer>
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

    /// Builds an ibor coupon spanning the value-to-maturity period of
    /// `fixing_date`, as the C++ `iborCouponForFixingDate` helper does.
    fn coupon_for_fixing_date(index: Shared<IborIndex>, fixing_date: Date) -> IborCoupon {
        let start_date = index.value_date(fixing_date).unwrap();
        let end_date = index.maturity_date(fixing_date).unwrap();
        let coupon = IborCoupon::new(
            end_date,
            100.0,
            start_date,
            end_date,
            Some(index.fixing_days()),
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
        coupon.set_pricer(pricer());
        coupon
    }

    /// `testFixedIborCouponWithoutForecastCurve` (cashflows.cpp:536): a coupon
    /// whose fixing has already been recorded prices off the store with no
    /// forecast curve linked, and its amount is `pastFixing * nominal *
    /// accrualPeriod` to 1e-8.
    #[test]
    fn a_past_ibor_coupon_prices_off_the_store() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = ibor6m(settings);
        let calendar = index.fixing_calendar();

        let fixing_date = calendar.advance(
            today,
            -2,
            TimeUnit::Months,
            BusinessDayConvention::Following,
            false,
        );
        let past_fixing = 0.01;
        index.add_fixing(fixing_date, past_fixing).unwrap();

        let coupon = coupon_for_fixing_date(index, fixing_date);

        let amount = coupon.amount().unwrap();
        let expected = past_fixing * coupon.nominal() * coupon.accrual_period();
        assert!(
            (amount - expected).abs() < 1e-8,
            "amount {amount} vs expected {expected}"
        );
    }

    /// `testIborCouponKnowsWhenitHasFixed` (cashflows.cpp:576): `has_fixed`
    /// tracks the fixing date against today (and the enforce/historical rules on
    /// today), and `rate` errors when a determined fixing is missing from the
    /// store.
    #[test]
    fn an_ibor_coupon_knows_when_it_has_fixed() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = Euribor::three_months(Handle::empty(), settings.clone());
        let index = shared(index);
        let calendar = index.fixing_calendar();

        let yesterday = calendar.advance(
            today,
            -1,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let tomorrow = calendar.advance(
            today,
            1,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        {
            let coupon = coupon_for_fixing_date(index.clone(), yesterday);
            index.clear_fixings();
            assert!(coupon.has_fixed().unwrap());
            assert!(coupon.rate().is_err());
        }

        {
            let coupon = coupon_for_fixing_date(index.clone(), today);
            settings.set_enforces_todays_historic_fixings(false);
            index.clear_fixings();
            assert!(!coupon.has_fixed().unwrap());
        }

        {
            let coupon = coupon_for_fixing_date(index.clone(), today);
            settings.set_enforces_todays_historic_fixings(false);
            index.add_fixing(coupon.fixing_date(), 0.01).unwrap();
            assert!(coupon.has_fixed().unwrap());
        }

        {
            let coupon = coupon_for_fixing_date(index.clone(), today);
            settings.set_enforces_todays_historic_fixings(true);
            index.clear_fixings();
            assert!(coupon.has_fixed().unwrap());
            assert!(coupon.rate().is_err());
        }

        {
            let coupon = coupon_for_fixing_date(index.clone(), tomorrow);
            assert!(!coupon.has_fixed().unwrap());
        }
    }

    /// The in-arrears swaplet path is refused (the convexity adjustment needs the
    /// unported optionlet volatility), rather than returning a silent wrong
    /// number.
    #[test]
    fn an_in_arrears_coupon_refuses_to_price() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = ibor6m(settings);

        let start = Date::new(15, Month::January, 2026);
        let end = Date::new(15, Month::July, 2026);
        let coupon = IborCoupon::new(
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
            true,
            None,
            BusinessDayConvention::Preceding,
        )
        .unwrap();
        coupon.set_pricer(pricer());

        assert!(coupon.rate().is_err());
    }

    /// Pins the documented forecast divergence. The forecast branch through the
    /// coupon is live: an unfixed coupon over a linked curve prices off the
    /// index's own forecast fixing, which follows the indexed-coupon convention
    /// (the index's natural maturity), not this tree's compile-time-default par
    /// approximation. `rate()` is exactly `gearing * forecastFixing + spread`,
    /// so if the deferred par mode ever silently replaced it the number would
    /// move and this test would catch it.
    #[test]
    fn an_unfixed_coupon_forecasts_in_indexed_mode_not_par() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = shared(IborIndex::new(
            "foo".into(),
            Period::new(6, TimeUnit::Months),
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            flat_curve(today, 0.03),
            settings,
        ));

        let fixing_date = Date::new(15, Month::July, 2026);
        let start_date = index.value_date(fixing_date).unwrap();
        let end_date = index.maturity_date(start_date).unwrap();
        let gearing = 2.0;
        let spread = 0.01;
        let coupon = IborCoupon::new(
            end_date,
            100.0,
            start_date,
            end_date,
            Some(index.fixing_days()),
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
        .unwrap();
        coupon.set_pricer(pricer());

        assert!(!coupon.has_fixed().unwrap());
        assert_eq!(coupon.fixing_date(), fixing_date);
        let forecast = index.forecast_fixing(fixing_date).unwrap();
        let expected = gearing * forecast + spread;
        assert!((coupon.rate().unwrap() - expected).abs() < 1e-14);
    }
}
