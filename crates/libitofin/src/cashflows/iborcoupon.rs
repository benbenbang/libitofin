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
//! returns the required past fixing, an undetermined one forecasts. The
//! determined branch is the same one
//! [`Index::fixing`](crate::indexes::index::Index::fixing) already makes, so
//! [`index_fixing`](IborCoupon::index_fixing) delegates it to the base
//! [`FloatingRateCoupon::index_fixing`], yielding the identical value for every
//! determined case. The forecast branch is mode-aware: it computes the cached
//! `fixingValueDate`/`fixingEndDate`/`spanningTime` (`couponpricer.cpp:56-88`)
//! and reads the specialized 3-arg
//! [`forecast_fixing_between`](IborIndex::forecast_fixing_between)
//! (`iborcoupon.cpp:126`) rather than the index's natural forecast.
//!
//! ## Par vs indexed coupons (`usingAtParCoupons`)
//!
//! The forecast estimation end depends on the coupon convention. A **par**
//! coupon rolls `fixingEndDate` off the next fixing date derived from the
//! coupon's own accrual end (the approximation that lets a vanilla floater
//! telescope to par); an **indexed** coupon uses the index's natural maturity,
//! so its forecast equals `index.forecast_fixing(fixingDate)`. The two coincide
//! only when the coupon spans exactly the index tenor; on a stub they diverge.
//!
//! C++ toggles the convention through the `IborCoupon::Settings` singleton
//! (`iborcoupon.hpp:110`, `usingAtParCoupons_ = true` by default), which D5
//! forbids. The port threads the flag explicitly on
//! [`Settings`](crate::settings::Settings::using_at_par_coupons) - alongside the
//! evaluation date and the D11 fixing store - defaulting to par to match the
//! tree's compile-time default. Only the forecast branch reads it; a determined
//! fixing is mode-independent. The `IborCoupon::Settings` singleton itself is
//! the sole divergence: the behaviour it governs is reproduced, the global is
//! not.

use super::coupon::{Coupon, CouponBase};
use super::couponpricer::FloatingRateCouponPricer;
use super::floatingratecoupon::FloatingRateCoupon;
use crate::errors::QlResult;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::{Shared, SharedMut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real, Spread, Time};
use crate::{fail, require};

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

    /// The index fixing at the coupon's [`fixing_date`](Self::fixing_date)
    /// (`IborCoupon::indexFixing`).
    ///
    /// A determined coupon returns the required past fixing, delegated to the
    /// base and mode-independent. An undetermined coupon forecasts through the
    /// specialized 3-arg [`forecast_fixing_between`](IborIndex::forecast_fixing_between)
    /// over the cached par or indexed dates (see [`forecast_fixing_dates`]),
    /// replacing the base's natural forecast.
    ///
    /// [`forecast_fixing_dates`]: Self::forecast_fixing_dates
    pub fn index_fixing(&self) -> QlResult<Rate> {
        if self.has_fixed()? {
            self.base.index_fixing()
        } else {
            let (value_date, end_date, spanning_time) = self.forecast_fixing_dates()?;
            self.ibor_index
                .forecast_fixing_between(value_date, end_date, spanning_time)
        }
    }

    /// The cached forecast dates `(fixingValueDate, fixingEndDate, spanningTime)`
    /// the 3-arg forecast reads (`IborCouponPricer::initializeCachedData`,
    /// `couponpricer.cpp:56-88`).
    ///
    /// `fixingValueDate` moves the fixing date forward the index's fixing days
    /// (identical to [`value_date`](InterestRateIndex::value_date)) and
    /// `fixingMaturityDate` is the index maturity off it. Under the par
    /// convention a non-in-arrears coupon rolls `fixingEndDate` off its own
    /// accrual end - back the *coupon's* fixing days to the next fixing date,
    /// then forward the *index's* fixing days, floored at `fixingValueDate + 1`
    /// so the estimation period spans at least a day; an indexed coupon (or any
    /// in-arrears coupon) uses `fixingMaturityDate`. The convention is read
    /// explicitly from [`Settings`](crate::settings::Settings::using_at_par_coupons),
    /// not a global singleton.
    fn forecast_fixing_dates(&self) -> QlResult<(Date, Date, Time)> {
        let index = &self.ibor_index;
        let calendar = index.fixing_calendar();
        let fixing_value_date = index.value_date(self.fixing_date())?;
        let fixing_maturity_date = index.maturity_date(fixing_value_date)?;

        let using_at_par = index.settings().using_at_par_coupons();
        let fixing_end_date = if !using_at_par || self.base.is_in_arrears() {
            fixing_maturity_date
        } else {
            let next_fixing_date = calendar.advance(
                self.accrual_end_date(),
                -(self.base.fixing_days() as Integer),
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let end_date = calendar.advance(
                next_fixing_date,
                index.fixing_days() as Integer,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            end_date.max(fixing_value_date + 1)
        };

        let spanning_time = index
            .day_counter()
            .year_fraction(fixing_value_date, fixing_end_date);
        let positive_time = spanning_time > 0.0;
        require!(
            positive_time,
            "cannot calculate forward rate between {fixing_value_date:?} and {fixing_end_date:?}: non positive time ({spanning_time}) using {} daycounter",
            index.day_counter().name()
        );
        Ok((fixing_value_date, fixing_end_date, spanning_time))
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
        Ok(self.rate()? * self.accrual_period() * self.nominal())
    }

    fn rate(&self) -> QlResult<Rate> {
        let Some(pricer) = self.base.pricer() else {
            fail!("pricer not set");
        };
        pricer.borrow_mut().initialize(&self.base);
        let index_fixing = self.index_fixing();
        pricer.borrow().swaplet_rate_for(index_fixing)
    }

    fn day_counter(&self) -> DayCounter {
        self.base.day_counter()
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

    /// Behind the explicit `using_at_par_coupons = false` flag an unfixed coupon
    /// forecasts in the indexed-coupon convention: `fixingEndDate` is the index's
    /// natural maturity, so the forecast is exactly `index.forecast_fixing`. This
    /// is the flag-gated half of the mode switch; the default (par) is pinned by
    /// `a_par_coupon_forecasts_over_its_own_accrual_end`.
    #[test]
    fn an_unfixed_coupon_forecasts_in_indexed_mode_behind_the_flag() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        settings.set_using_at_par_coupons(false);
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

    /// Builds a 6M index on a flat continuous `Actual/360` curve at `rate` and a
    /// short-stub coupon whose accrual end (3M) genuinely mismatches the index
    /// tenor, so the par and indexed forecasts differ. Returns the index, the
    /// coupon, its gearing and spread.
    fn stub_coupon(rate: Rate) -> (Shared<IborIndex>, IborCoupon, Real, Spread) {
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
            flat_curve(today, rate),
            settings,
        ));

        let fixing_date = Date::new(15, Month::July, 2026);
        let start_date = index.value_date(fixing_date).unwrap();
        let end_date = index.fixing_calendar().advance_by_period(
            start_date,
            Period::new(3, TimeUnit::Months),
            BusinessDayConvention::Following,
            false,
        );
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
        (index, coupon, gearing, spread)
    }

    /// The default (par) forecast rolls `fixingEndDate` off the coupon's own
    /// accrual end, not the index maturity. On the flat continuous `Actual/360`
    /// curve the simple forward over `[fixingValueDate, parFixingEndDate]` is
    /// analytically `(exp(rate * t) - 1) / t` with `t` the day-count fraction of
    /// that span; the par dates are reconstructed here from the calendar, not
    /// read back from the coupon. `rate()` is `gearing * forecast + spread`.
    #[test]
    fn a_par_coupon_forecasts_over_its_own_accrual_end() {
        let rate = 0.03;
        let (index, coupon, gearing, spread) = stub_coupon(rate);
        let day_counter = Actual360::new();

        let fixing_value_date = index.value_date(coupon.fixing_date()).unwrap();
        let calendar = index.fixing_calendar();
        let next_fixing_date = calendar.advance(
            coupon.accrual_end_date(),
            -(index.fixing_days() as i32),
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let par_fixing_end_date = calendar
            .advance(
                next_fixing_date,
                index.fixing_days() as i32,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            )
            .max(fixing_value_date + 1);

        let t = day_counter.year_fraction(fixing_value_date, par_fixing_end_date);
        let forecast = ((rate * t).exp() - 1.0) / t;
        let expected = gearing * forecast + spread;

        assert!(!coupon.has_fixed().unwrap());
        assert!((coupon.rate().unwrap() - expected).abs() < 1e-14);
    }

    /// On the stub the par default and the indexed convention genuinely diverge:
    /// par prices over the coupon's 3M accrual end, indexed over the index's 6M
    /// natural maturity. The default par `rate()` must differ from the indexed
    /// `gearing * index.forecast_fixing + spread` by well more than tolerance.
    #[test]
    fn par_and_indexed_forecasts_differ_on_a_stub() {
        let (index, coupon, gearing, spread) = stub_coupon(0.03);
        let par_rate = coupon.rate().unwrap();
        let indexed = gearing * index.forecast_fixing(coupon.fixing_date()).unwrap() + spread;
        assert!(
            (par_rate - indexed).abs() > 1e-6,
            "par {par_rate} vs indexed {indexed} should differ on a stub"
        );
    }

    /// `amount()` and `accrued_amount()` route through the mode-aware
    /// [`rate`](Coupon::rate), not the base's natural indexed forecast. C++ gets
    /// this from the virtual `rate()`; the port's embedded-base composition does
    /// not dispatch back down, so the override is explicit. On the stub, where
    /// par and indexed diverge, an unfixed coupon's amount would carry the
    /// indexed fixing without the routing - here it must equal
    /// `rate() * accrual_period * nominal`, and `accrued_amount` must share the
    /// same rate.
    #[test]
    fn amount_routes_through_the_mode_aware_rate() {
        let (_index, coupon, _gearing, _spread) = stub_coupon(0.03);
        assert!(!coupon.has_fixed().unwrap());
        let rate = coupon.rate().unwrap();

        let expected_amount = rate * coupon.accrual_period() * coupon.nominal();
        assert!((coupon.amount().unwrap() - expected_amount).abs() < 1e-14);

        let payment_date = coupon.coupon_base().payment_date();
        let expected_accrued = coupon.nominal() * rate * coupon.accrued_period(payment_date);
        assert!((coupon.accrued_amount(payment_date).unwrap() - expected_accrued).abs() < 1e-14);
    }
}
