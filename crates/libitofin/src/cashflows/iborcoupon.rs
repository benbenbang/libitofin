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
