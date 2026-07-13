//! Overnight indexed swap (OIS): fixed versus compounded overnight rate.
//!
//! Port of `ql/instruments/overnightindexedswap.{hpp,cpp}`: `class
//! OvernightIndexedSwap : public FixedVsFloatingSwap`
//! (`overnightindexedswap.hpp:42`), the modern benchmark rates instrument. It
//! builds a [`FixedRateLeg`](crate::cashflows::FixedRateLeg) and an
//! [`OvernightLeg`], hands them to the base, and overrides one thing:
//! `setupFloatingArguments`.
//!
//! Like [`VanillaSwap`](super::VanillaSwap) the port composes rather than
//! inherits: [`OvernightIndexedSwap`] holds a [`FixedVsFloatingSwap`] and
//! delegates the [`Instrument`] face to it, reaching the base's fair-rate,
//! leg-NPV/BPS and other accessors through
//! [`fixed_vs_floating`](OvernightIndexedSwap::fixed_vs_floating). It keeps its
//! own `overnightIndex_`, `paymentLag_`, `paymentCalendar_` and
//! `averagingMethod_`, exactly as C++ keeps those members on top of the base.
//!
//! ## The constructor
//!
//! C++ has four overloads (`hpp:44/60/76/93`) that all funnel into the master
//! two-schedule, vector-nominal ctor (`hpp:93` / `cpp:127`). This port provides
//! that master ctor as [`new`](OvernightIndexedSwap::new) and the
//! single-nominal, two-schedule convenience (`hpp:76`) as
//! [`with_nominal`](OvernightIndexedSwap::with_nominal), which is the shape
//! `MakeOIS` (#331) drives. The two single-schedule overloads (`hpp:44/60`),
//! which reuse one schedule for both legs, are deferred.
//!
//! As in [`VanillaSwap`](super::VanillaSwap), the staging is inverted: C++
//! constructs the base then overwrites `legs_[1] = OvernightLeg(...)`
//! (`cpp:151`), whereas the port builds the [`OvernightLeg`] first and passes it
//! down to [`FixedVsFloatingSwap::new`]. Same final state.
//!
//! ## Deviations, all by existing design decisions or the composition shift
//!
//! - The base wants a `Shared<IborIndex>` but an OIS pays an
//!   [`OvernightIndex`]. C++ upcasts the one `shared_ptr`; the port hands the
//!   base the same-identity inner index through
//!   [`OvernightIndex::ibor_index`]. That stored index is inert on the OIS path
//!   (the base reads it only for a fixed-day-count fallback the OIS never
//!   triggers, and through an `ibor_index()` accessor the OIS does not expose),
//!   so single identity is a fidelity nicety rather than a correctness need.
//! - The base's `floating_day_count` is C++'s empty `DayCounter()` (`cpp:129`).
//!   The port has no null day counter, so it passes the overnight index's own
//!   day counter, which is exactly what the unconfigured overnight coupons
//!   accrue on; the base's `floating_day_count()` accessor therefore reports
//!   that rather than an empty one.
//! - `paymentCalendar_` is stored resolved (empty falls back to the overnight
//!   schedule's calendar, matching the `OvernightLeg` rule at `cpp:159`) rather
//!   than as C++'s possibly-empty ctor argument: the port has no empty
//!   `Calendar` sentinel.
//! - `setupFloatingArguments` is not a method override. The base takes a
//!   [`FloatingArgumentsFn`] closure; [`new`](OvernightIndexedSwap::new)
//!   supplies one capturing the concrete [`OvernightIndexedCoupon`]s the leg was
//!   built from, because `fixingDate` and the per-coupon spread are not on the
//!   erased [`Coupon`] face.
//! - `coupon->amount()`'s C++ catch-and-`Null<Real>` (`cpp:186`) becomes `?`:
//!   the port has no `Null<Real>` sentinel (D4/D10), and the generic-`Swap`
//!   engine path (#322) never reaches this closure, so the choice is inert.
//! - `telescopicValueDates`, `lookbackDays`, `lockoutDays` and
//!   `applyObservationShift` are not accepted: [`OvernightLeg`] (#329) does not
//!   yet thread them, so they are deferred with the leg rather than accepted and
//!   ignored. Their inspectors are likewise deferred.
//!
//! ## Numeric oracles: deferred to `MakeOIS` (#331)
//!
//! All three of `test-suite/overnightindexedswap.cpp`'s numeric oracles
//! (`testCachedValue` :367, `testFairRate` :284, `testFairSpread` :325)
//! construct the swap through `MakeOIS`, whose settlement-days dispatch, EOM
//! rules and schedule generation (`makeois.cpp`) this ticket does not port. The
//! cached NPV and the fair-value self-consistency land with `MakeOIS` (#331);
//! this ticket covers the faithful type and its construction, closing with a
//! discounting-engine smoke NPV rather than a cached assertion.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{Coupon, OvernightIndexedCoupon, OvernightLeg, RateAveraging};
use crate::errors::QlResult;
use crate::indexes::OvernightIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::fixedvsfloatingswap::{
    FixedVsFloatingSwap, FixedVsFloatingSwapArguments, FloatingArgumentsFn,
};
use crate::instruments::swap::SwapType;
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::schedule::Schedule;
use crate::types::{Integer, Rate, Real, Spread};

/// Overnight indexed swap: a fixed leg versus a compounded overnight leg.
///
/// Composes a [`FixedVsFloatingSwap`]; build with [`new`](Self::new) (the master
/// two-schedule ctor) or [`with_nominal`](Self::with_nominal) (the single-nominal
/// convenience `MakeOIS` drives), reach the base's accessors through
/// [`fixed_vs_floating`](Self::fixed_vs_floating) /
/// [`fixed_vs_floating_mut`](Self::fixed_vs_floating_mut), and price it through
/// its [`Instrument`] face.
pub struct OvernightIndexedSwap {
    base: FixedVsFloatingSwap,
    overnight_index: Shared<OvernightIndex>,
    payment_lag: Integer,
    payment_calendar: Calendar,
    averaging_method: RateAveraging,
}

impl OvernightIndexedSwap {
    /// Builds an OIS over a single `nominal` shared by both legs (the C++
    /// single-nominal, two-schedule ctor, `overnightindexedswap.hpp:76`), the
    /// shape `MakeOIS` uses.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    #[allow(clippy::too_many_arguments)]
    pub fn with_nominal(
        swap_type: SwapType,
        nominal: Real,
        fixed_schedule: Schedule,
        fixed_rate: Rate,
        fixed_day_count: DayCounter,
        overnight_schedule: Schedule,
        overnight_index: Shared<OvernightIndex>,
        spread: Spread,
        payment_lag: Integer,
        payment_adjustment: BusinessDayConvention,
        payment_calendar: Option<Calendar>,
        averaging_method: RateAveraging,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<OvernightIndexedSwap> {
        OvernightIndexedSwap::new(
            swap_type,
            vec![nominal],
            fixed_schedule,
            fixed_rate,
            fixed_day_count,
            vec![nominal],
            overnight_schedule,
            overnight_index,
            spread,
            payment_lag,
            payment_adjustment,
            payment_calendar,
            averaging_method,
            settings,
        )
    }

    /// Builds an OIS from separate fixed and overnight schedules and per-coupon
    /// nominals (the C++ master ctor, `overnightindexedswap.hpp:93` /
    /// `overnightindexedswap.cpp:127`).
    ///
    /// The fixed leg (`legs_[0]`) is built by the base from `fixed_schedule` /
    /// `fixed_rate` / `fixed_day_count`; the overnight leg (`legs_[1]`) is built
    /// here as an [`OvernightLeg`] over `overnight_schedule` / `overnight_index`
    /// carrying `spread`, `payment_lag`, `payment_adjustment`, the resolved
    /// payment calendar and `averaging_method`. Payer flags follow `swap_type`:
    /// a `Payer` pays the fixed leg.
    ///
    /// # Errors
    ///
    /// Propagates the overnight-leg build (an empty schedule or unsupported
    /// averaging, say) and the base construction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        fixed_nominals: Vec<Real>,
        fixed_schedule: Schedule,
        fixed_rate: Rate,
        fixed_day_count: DayCounter,
        overnight_nominals: Vec<Real>,
        overnight_schedule: Schedule,
        overnight_index: Shared<OvernightIndex>,
        spread: Spread,
        payment_lag: Integer,
        payment_adjustment: BusinessDayConvention,
        payment_calendar: Option<Calendar>,
        averaging_method: RateAveraging,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<OvernightIndexedSwap> {
        let resolved_calendar =
            payment_calendar.unwrap_or_else(|| overnight_schedule.calendar().clone());

        let coupons =
            OvernightLeg::new(overnight_schedule.clone(), Shared::clone(&overnight_index))
                .with_notionals(overnight_nominals.clone())
                .with_spread(spread)
                .with_payment_lag(payment_lag)
                .with_payment_adjustment(payment_adjustment)
                .with_payment_calendar(resolved_calendar.clone())
                .with_averaging_method(averaging_method)
                .coupons()?;
        let floating_leg: Leg = coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect();

        let floating_arguments: FloatingArgumentsFn =
            Box::new(move |_swap, args| fill_floating_arguments(&coupons, args));

        let base = FixedVsFloatingSwap::new(
            swap_type,
            fixed_nominals,
            fixed_schedule,
            fixed_rate,
            Some(fixed_day_count),
            overnight_nominals,
            overnight_schedule,
            overnight_index.ibor_index(),
            spread,
            overnight_index.day_counter().clone(),
            None,
            payment_lag,
            Some(resolved_calendar.clone()),
            floating_leg,
            floating_arguments,
            settings,
        )?;

        Ok(OvernightIndexedSwap {
            base,
            overnight_index,
            payment_lag,
            payment_calendar: resolved_calendar,
            averaging_method,
        })
    }

    /// The embedded fixed-vs-floating base (its fair-rate, fixed-leg and nominal
    /// accessors).
    pub fn fixed_vs_floating(&self) -> &FixedVsFloatingSwap {
        &self.base
    }

    /// The embedded base, mutably (the on-demand-pricing accessors: `fairRate`,
    /// `fixedLegNPV` and the like).
    pub fn fixed_vs_floating_mut(&mut self) -> &mut FixedVsFloatingSwap {
        &mut self.base
    }

    /// The overnight index the floating leg pays (`overnightIndex()`).
    pub fn overnight_index(&self) -> &Shared<OvernightIndex> {
        &self.overnight_index
    }

    /// The overnight leg's nominal per coupon (`overnightNominals()`).
    pub fn overnight_nominals(&self) -> &[Real] {
        self.base.floating_nominals()
    }

    /// The overnight schedule (`overnightSchedule()`).
    pub fn overnight_schedule(&self) -> &Schedule {
        self.base.floating_schedule()
    }

    /// The overnight leg (`overnightLeg()`).
    pub fn overnight_leg(&self) -> &Leg {
        self.base.floating_leg()
    }

    /// The spread over the overnight index (`spread()`, on the base).
    pub fn spread(&self) -> Spread {
        self.base.spread()
    }

    /// The more frequent of the two legs' schedules (`paymentFrequency()`):
    /// `std::max` over the underlying `Frequency` values.
    pub fn payment_frequency(&self) -> Frequency {
        let fixed = self.base.fixed_schedule().tenor().frequency();
        let floating = self.base.floating_schedule().tenor().frequency();
        if floating as i16 > fixed as i16 {
            floating
        } else {
            fixed
        }
    }

    /// The business days between an overnight coupon's accrual end and its
    /// payment (`paymentLag()`).
    pub fn payment_lag(&self) -> Integer {
        self.payment_lag
    }

    /// The calendar the overnight payment dates are adjusted on
    /// (`paymentCalendar()`), stored resolved.
    pub fn payment_calendar(&self) -> &Calendar {
        &self.payment_calendar
    }

    /// The overnight averaging method (`averagingMethod()`).
    pub fn averaging_method(&self) -> RateAveraging {
        self.averaging_method
    }

    /// The overnight leg's basis-point sensitivity (`overnightLegBPS()`), priced
    /// on demand.
    ///
    /// # Errors
    ///
    /// As [`FixedVsFloatingSwap::floating_leg_bps`].
    pub fn overnight_leg_bps(&mut self) -> QlResult<Real> {
        self.base.floating_leg_bps()
    }

    /// The overnight leg's NPV (`overnightLegNPV()`), priced on demand.
    ///
    /// # Errors
    ///
    /// As [`FixedVsFloatingSwap::floating_leg_npv`].
    pub fn overnight_leg_npv(&mut self) -> QlResult<Real> {
        self.base.floating_leg_npv()
    }
}

/// Fills the floating-leg argument vectors from the swap's
/// [`OvernightIndexedCoupon`]s (the C++
/// `OvernightIndexedSwap::setupFloatingArguments`,
/// `overnightindexedswap.cpp:167`). The per-coupon spread is read from the
/// coupon, not the swap, matching C++ `coupon->spread()`.
fn fill_floating_arguments(
    coupons: &[Shared<OvernightIndexedCoupon>],
    args: &mut FixedVsFloatingSwapArguments,
) -> QlResult<()> {
    let n = coupons.len();
    args.floating_reset_dates = Vec::with_capacity(n);
    args.floating_pay_dates = Vec::with_capacity(n);
    args.floating_nominals = Vec::with_capacity(n);
    args.floating_fixing_dates = Vec::with_capacity(n);
    args.floating_accrual_times = Vec::with_capacity(n);
    args.floating_spreads = Vec::with_capacity(n);
    args.floating_coupons = Vec::with_capacity(n);

    for coupon in coupons {
        args.floating_reset_dates.push(coupon.accrual_start_date());
        args.floating_pay_dates
            .push(coupon.coupon_base().payment_date());
        args.floating_nominals.push(coupon.nominal());
        args.floating_fixing_dates.push(coupon.fixing_date());
        args.floating_accrual_times.push(coupon.accrual_period());
        args.floating_spreads.push(coupon.spread());
        args.floating_coupons.push(Coupon::amount(coupon.as_ref())?);
    }
    Ok(())
}

impl Instrument for OvernightIndexedSwap {
    fn base(&self) -> &InstrumentBase {
        self.base.base()
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.base.base_mut()
    }

    fn is_expired(&self) -> QlResult<bool> {
        self.base.is_expired()
    }

    fn setup_expired(&mut self) {
        self.base.setup_expired();
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.base.setup_arguments(arguments)
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.base.fetch_results(results)
    }
}
