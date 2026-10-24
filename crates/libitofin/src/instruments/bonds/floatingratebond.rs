//! Scheduled ibor bonds, including geared, capped and floored coupons.
//!
//! Port of `ql/instruments/bonds/floatingratebond.{hpp,cpp}`. The existing
//! [`Bond`] supplies settlement, redemption, accrual and discount-engine pricing.
//! In-arrears convexity adjustment is outside this constructor's scope.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{
    CappedFlooredCoupon, Coupon, CouponBase, FixedRateCoupon, FloatingRateCouponPricer, IborCoupon,
    IborLeg,
};
use crate::errors::QlResult;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::Bond;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real, Spread};
use crate::utilities::null::Null;

/// An ibor bond retaining its coupons and inferred principal payments.
///
/// Composes [`Bond`] rather than inheriting it. Floating coupon handles remain
/// available after erasure into the base's cash-flow leg. Zero gearing produces
/// a fixed coupon, as in QuantLib's `FloatingLeg`, not an ibor coupon.
pub struct FloatingRateBond {
    bond: Bond,
    index: Shared<IborIndex>,
    ibor_coupons: Vec<Option<Shared<IborCoupon>>>,
}

impl FloatingRateBond {
    /// Builds a plain ibor bond with exact per-period outstanding balances.
    ///
    /// Balances must be positive, finite and nonincreasing. Principal is repaid
    /// at par on the preceding coupon payment date when a balance changes,
    /// with the remaining balance repaid at maturity. The existing [`Bond`]
    /// near-equality rule can coalesce numerically close balance changes.
    /// Empty spreads default to zero; shorter spreads repeat the last value.
    /// Coupons use unit gearing, the default Black pricer and no optionality.
    ///
    /// # Errors
    ///
    /// Rejects invalid balances, dates, spreads or settings identity. Settlement,
    /// effective coupon fixing and index fixing lags cannot exceed the supported
    /// maximum-minus-minimum Date span. Propagates [`IborLeg`] and [`Bond`] errors.
    #[allow(clippy::too_many_arguments)]
    pub fn new_amortizing(
        settlement_days: Natural,
        notionals: Vec<Real>,
        schedule: Schedule,
        index: Shared<IborIndex>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        fixing_days: Option<Natural>,
        spreads: Vec<Spread>,
        issue_date: Option<Date>,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        fixing_convention: BusinessDayConvention,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(schedule.len() >= 2, "schedule spans no period");
        let supported_days = (Date::max_date() - Date::min_date()) as u32;
        require!(
            settlement_days <= supported_days
                && fixing_days.unwrap_or(index.fixing_days()) <= supported_days
                && index.fixing_days() <= supported_days,
            "bond or index fixing days exceed supported date range"
        );
        require!(
            notionals.len() == schedule.len() - 1,
            "one notional per schedule period required"
        );
        require!(
            notionals.iter().all(|n| n.is_finite() && *n > 0.0)
                && notionals.windows(2).all(|n| n[1] <= n[0]),
            "notionals must be positive finite and nonincreasing"
        );
        require!(
            spreads.len() <= notionals.len() && spreads.iter().all(|s| s.is_finite()),
            "finite spreads must not exceed schedule periods"
        );
        require!(
            std::ptr::eq(&*settings, Index::settings(&*index)),
            "bond and index must share settings"
        );
        require!(
            schedule.dates().iter().all(|d| *d != Date::null())
                && schedule.dates().windows(2).all(|d| d[0] < d[1])
                && issue_date.is_none_or(|d| d != Date::null()),
            "non-null increasing schedule and issue dates required"
        );
        require!(
            ex_coupon_period.is_none_or(|p| p.length() >= 0),
            "ex-coupon period must be nonnegative"
        );
        let maturity = schedule.end_date();
        let calendar = schedule.calendar().clone();
        let mut leg = IborLeg::new(schedule, Shared::clone(&index))
            .with_notionals(notionals)
            .with_payment_day_counter(accrual_day_counter)
            .with_payment_adjustment(payment_convention)
            .with_spreads(spreads)
            .with_fixing_convention(fixing_convention);
        if let Some(days) = fixing_days {
            leg = leg.with_fixing_days(days);
        }
        if let Some(period) = ex_coupon_period.filter(|p| p.length() != 0) {
            leg = leg.with_ex_coupon_period(
                period,
                ex_coupon_calendar,
                ex_coupon_convention,
                ex_coupon_end_of_month,
            );
        }
        let coupons = leg.coupons()?;
        let ibor_coupons = coupons.iter().map(|c| Some(Shared::clone(c))).collect();
        let cashflows = coupons
            .into_iter()
            .map(|c| c as Shared<dyn CashFlow>)
            .collect();
        let mut bond =
            Bond::from_coupons(settlement_days, calendar, issue_date, cashflows, settings)?;
        bond.set_maturity_date(maturity);
        bond.base().register_with(Index::observable(&*index));
        Ok(Self {
            bond,
            index,
            ibor_coupons,
        })
    }

    /// Builds coupons in advance on the schedule's calendar and one redemption.
    ///
    /// Empty gearings and spreads default to 1 and 0; shorter vectors repeat
    /// their last element. `None` fixing days use the index's lag. Caps/floors
    /// apply to the full geared, spread-inclusive rate. `Null::null()` cap/floor
    /// entries retain QuantLib's per-entry absent-option sentinel. Empty cap/floor
    /// vectors receive the existing default Black pricer. For nonempty vectors,
    /// attach a pricer with [`set_coupon_pricer`](Self::set_coupon_pricer), even
    /// if every entry is the sentinel.
    /// A zero gearing instead pays the spread, first floored then capped, and
    /// needs neither fixings nor a pricer. The explicit settings should be the
    /// same handle used by the index and the pricing engine. Index settings
    /// identity and finite numeric inputs are checked explicitly, unlike C++.
    ///
    /// # Errors
    ///
    /// Propagates schedule, vector-size, cap/floor and redemption preconditions
    /// from [`IborLeg`], [`CappedFlooredCoupon`] and [`Bond`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        face_amount: Real,
        schedule: Schedule,
        index: Shared<IborIndex>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        fixing_days: Option<Natural>,
        gearings: Vec<Real>,
        spreads: Vec<Spread>,
        caps: Vec<Rate>,
        floors: Vec<Rate>,
        redemption: Real,
        issue_date: Option<Date>,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        fixing_convention: BusinessDayConvention,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(schedule.len() >= 2, "schedule spans no period");
        require!(
            std::ptr::eq(&*settings, Index::settings(&*index)),
            "bond and index must share settings"
        );
        require!(
            face_amount.is_finite()
                && redemption.is_finite()
                && ((redemption / 100.0) * face_amount).is_finite(),
            "face amount, redemption and scaled redemption must be finite"
        );
        require!(
            gearings
                .iter()
                .chain(&spreads)
                .chain(&caps)
                .chain(&floors)
                .all(|x| x.is_finite()),
            "coupon parameters must be finite"
        );
        require!(
            (0..schedule.len()).all(|i| schedule.date(i) != Date::null()),
            "schedule dates must not be null"
        );
        let maturity = schedule.end_date();
        let calendar = schedule.calendar().clone();
        let mut leg = IborLeg::new(schedule, Shared::clone(&index))
            .with_notional(face_amount)
            .with_payment_day_counter(accrual_day_counter.clone())
            .with_payment_adjustment(payment_convention)
            .with_gearings(
                gearings
                    .iter()
                    .map(|&g| if g == 0.0 { 1.0 } else { g })
                    .collect(),
            )
            .with_spreads(spreads.clone())
            .with_caps(caps.clone())
            .with_floors(floors.clone())
            .with_fixing_convention(fixing_convention);
        if let Some(days) = fixing_days {
            leg = leg.with_fixing_days(days);
        }
        if let Some(period) = ex_coupon_period.filter(|p| p.length() != 0) {
            leg = leg.with_ex_coupon_period(
                period,
                ex_coupon_calendar,
                ex_coupon_convention,
                ex_coupon_end_of_month,
            );
        }
        let raw = leg.coupons()?;
        let mut cashflows: Leg = Vec::with_capacity(raw.len() + 1);
        let mut ibor_coupons = Vec::with_capacity(raw.len());
        for (i, coupon) in raw.into_iter().enumerate() {
            let cap = pick(&caps, i).filter(|&rate| !rate.is_null());
            let floor = pick(&floors, i).filter(|&rate| !rate.is_null());
            let flow: Shared<dyn CashFlow> = if pick(&gearings, i).unwrap_or(1.0) == 0.0 {
                let mut rate = pick(&spreads, i).unwrap_or(0.0);
                if let Some(floor) = floor {
                    rate = rate.max(floor);
                }
                if let Some(cap) = cap {
                    rate = rate.min(cap);
                }
                ibor_coupons.push(None);
                shared(IndexedFixedCoupon {
                    _index: Shared::clone(&index),
                    coupon: FixedRateCoupon::from_rate(
                        coupon.coupon_base().payment_date(),
                        face_amount,
                        rate,
                        accrual_day_counter.clone(),
                        coupon.accrual_start_date(),
                        coupon.accrual_end_date(),
                        Some(coupon.reference_period_start()),
                        Some(coupon.reference_period_end()),
                        coupon.coupon_base().ex_coupon_date(),
                    ),
                })
            } else {
                ibor_coupons.push(Some(Shared::clone(&coupon)));
                if cap.is_some() || floor.is_some() {
                    shared(CappedFlooredCoupon::new(coupon, cap, floor)?)
                } else {
                    coupon
                }
            };
            cashflows.push(flow);
        }
        let mut bond = Bond::new(settlement_days, calendar, None, cashflows, settings)?;
        bond.set_issue_date(issue_date);
        bond.add_redemptions_to_cashflows(&[redemption])?;
        bond.set_maturity_date(maturity);
        bond.base().register_with(Index::observable(&*index));
        require!(!bond.cashflows().is_empty(), "bond with no cashflows!");
        require!(
            bond.redemptions().len() == 1,
            "multiple redemptions created"
        );
        Ok(Self {
            bond,
            index,
            ibor_coupons,
        })
    }

    /// The index the floating coupons observe.
    pub fn index(&self) -> &Shared<IborIndex> {
        &self.index
    }

    /// Floating coupon handles in schedule order; `None` denotes a fixed coupon.
    ///
    /// Capped/floored entries expose their underlying ibor coupon. Full coupon
    /// rates and cap/floor amounts are read from [`Bond::cashflows`].
    pub fn ibor_coupons(&self) -> &[Option<Shared<IborCoupon>>] {
        &self.ibor_coupons
    }

    /// Attaches a pricer to every floating coupon, including capped/floored ones.
    /// Fixed coupons are left unchanged. Pricer replacement invalidates prices.
    pub fn set_coupon_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
        for coupon in self.ibor_coupons.iter().flatten() {
            coupon.set_pricer(SharedMut::clone(&pricer));
        }
    }

    /// The base bond, for settlement, cash-flow inspection and accrual.
    pub fn bond(&self) -> &Bond {
        &self.bond
    }

    /// The mutable base, for attaching engines and retrieving lazy prices.
    pub fn bond_mut(&mut self) -> &mut Bond {
        &mut self.bond
    }

    /// Consumes the wrapper while preserving the base's cash flows and observers.
    pub fn into_bond(self) -> Bond {
        self.bond
    }
}

fn pick(values: &[Real], i: usize) -> Option<Real> {
    values.get(i).or_else(|| values.last()).copied()
}

struct IndexedFixedCoupon {
    coupon: FixedRateCoupon,
    _index: Shared<IborIndex>,
}

impl AsObservable for IndexedFixedCoupon {
    fn observable(&self) -> &Observable {
        self.coupon.observable()
    }
}

impl Coupon for IndexedFixedCoupon {
    fn coupon_base(&self) -> &CouponBase {
        self.coupon.coupon_base()
    }
    fn amount(&self) -> QlResult<Real> {
        Coupon::amount(&self.coupon)
    }
    fn rate(&self) -> QlResult<Rate> {
        self.coupon.rate()
    }
    fn day_counter(&self) -> DayCounter {
        self.coupon.day_counter()
    }
    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        self.coupon.accrued_amount(date)
    }
}
