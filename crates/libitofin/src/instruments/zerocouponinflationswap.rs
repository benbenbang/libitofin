//! Zero-coupon inflation-indexed swap.
//!
//! Port of `ql/instruments/zerocouponinflationswap.{hpp,cpp}`: `class
//! ZeroCouponInflationSwap : public Swap` (`zerocouponinflationswap.hpp:68`),
//! quoted as a fixed rate `K` that at inception matches the inflation growth,
//!
//! ```text
//! P_n(0,T) N [(1+K)^T - 1] = P_n(0,T) N [I(T)/I(0) - 1]
//! ```
//!
//! Both legs hold exactly one flow, swapped at maturity: a
//! [`SimpleCashFlow`] for the fixed side and a [`ZeroInflationCashFlow`] for the
//! indexed one. Nothing accrues, so neither flow is a coupon and no schedule is
//! needed. The passed [`SwapType`] refers to the *inflation* leg: a
//! [`Payer`](SwapType::Payer) pays inflation and receives fixed
//! (`zerocouponinflationswap.cpp:103-114`).
//!
//! A zero-coupon inflation swap is simple enough that the plain
//! [`DiscountingSwapEngine`](crate::pricingengines::DiscountingSwapEngine)
//! prices it; this ticket adds no engine of its own.
//!
//! ## What the constructor fixes
//!
//! - `growth_only` is hard-coded `true` (`cpp:79`): a swap exchanges growth, not
//!   notionals, so the indexed flow pays `N (I(T)/I(0) - 1)`.
//! - The two payment dates are the maturity adjusted on each side's calendar and
//!   convention (`cpp:76-77`), but the year fraction `T` behind the fixed amount
//!   is taken on the **raw, unadjusted** member dates (`cpp:92`, reading
//!   `maturityDate_` which `cpp:50` stores pre-adjustment). A maturity that lands
//!   on a weekend therefore pays on the following business day while still
//!   accruing to the weekend date.
//! - [`start_date`](Self::start_date) and [`maturity_date`](Self::maturity_date)
//!   report those raw dates (`hpp:93-94`), overriding the base
//!   [`Swap`]'s min/max over the legs, which would answer the adjusted payment
//!   date for both.
//!
//! ## Divergences from QuantLib
//!
//! - C++ recovers the indexed flow in `fairRate` with a
//!   `dynamic_pointer_cast<IndexedCashFlow>` and requires the cast to succeed
//!   (`cpp:124-126`). The port keeps the typed
//!   [`Shared<ZeroInflationCashFlow>`](ZeroInflationCashFlow) the constructor
//!   built (D3: `Rc` has no projection and this crate does not downcast), so the
//!   cast and its failure branch have no counterpart.
//! - `detail::CPI::effectiveInterpolationType` (`cpp:57`) reduces to the identity
//!   here: it exists to fold the deprecated `AsIndex` into `Flat`, and `AsIndex`
//!   is deliberately unported (see [`CpiInterpolationType`]). The compatibility
//!   check is therefore a plain match on the two live variants.
//! - `infCalendar`/`infConvention` are taken as [`Option`]s rather than as C++'s
//!   empty `Calendar()` / default `BusinessDayConvention()` sentinels, which the
//!   port has no equivalent of. They are stored *resolved* to the fixed-leg ones
//!   when unset, which is what C++ does too - it assigns to the members at
//!   `cpp:71-74`, so its own `inflationCalendar()` accessor already returns the
//!   resolved value.
//! - `registerWith(inflationCashFlow)` (`cpp:101`) has no explicit counterpart:
//!   [`Swap::new`] registers with every flow of every leg, which covers it.
//! - The evaluation-date [`Settings`] handle is passed in (D5), as every other
//!   swap in the crate takes it.
//!
//! ## Deferred
//!
//! - `adjustInfObsDates = true`. The flag only ever gates a branch this port does
//!   not have, so it is omitted from the signature rather than accepted and
//!   ignored; a caller needing adjusted observation dates fails to compile.
//!   `adjustObservationDates()` goes with it.
//! - The nested `arguments`/`engine` types (`hpp:146-154`). Nothing populates
//!   them: there is no `setupArguments` override in the C++ translation unit, so
//!   the instrument reuses the generic [`SwapArguments`]/[`SwapResults`]
//!   machinery and the extra `fixedRate` argument field is never read.
//! - The [`Receiver`](SwapType::Receiver) side is pinned structurally (its NPV is
//!   the [`Payer`](SwapType::Payer)'s negated) rather than by its own hand-derived
//!   numbers.
//!
//! [`SimpleCashFlow`]: crate::cashflows::SimpleCashFlow
//! [`SwapArguments`]: crate::instruments::SwapArguments
//! [`SwapResults`]: crate::instruments::SwapResults

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{SimpleCashFlow, ZeroInflationCashFlow};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, InflationIndex, ZeroInflationIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::swap::{Swap, SwapType};
use crate::pricingengine::{Arguments, Results};
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Rate, Real};

/// A basis point, the shift [`fixed_leg_bps`](ZeroCouponInflationSwap::fixed_leg_bps)
/// measures against (`cpp:150`).
const BASIS_POINT: Real = 1.0e-4;

/// Zero-coupon inflation-indexed swap: one fixed flow against one
/// inflation-indexed flow, both at maturity.
///
/// Composes a two-leg [`Swap`] (`legs[0]` fixed, `legs[1]` indexed) and prices
/// through its [`Instrument`] face; the base's own accessors are reachable
/// through [`swap`](Self::swap) / [`swap_mut`](Self::swap_mut).
pub struct ZeroCouponInflationSwap {
    swap: Swap,
    swap_type: SwapType,
    nominal: Real,
    start_date: Date,
    maturity_date: Date,
    fixed_calendar: Calendar,
    fixed_convention: BusinessDayConvention,
    fixed_rate: Rate,
    inflation_index: Shared<ZeroInflationIndex>,
    observation_lag: Period,
    observation_interpolation: CpiInterpolationType,
    inflation_calendar: Calendar,
    inflation_convention: BusinessDayConvention,
    day_counter: DayCounter,
    inflation_cash_flow: Shared<ZeroInflationCashFlow>,
}

impl ZeroCouponInflationSwap {
    /// Builds the swap from its two dates, the quoted rate and the inflation
    /// index it is indexed on (`zerocouponinflationswap.cpp:35-115`).
    ///
    /// `maturity` is pre-adjustment: each leg's payment date is it adjusted on
    /// that leg's calendar and convention, while the year fraction driving the
    /// fixed amount stays on the raw date. `inflation_calendar` and
    /// `inflation_convention` fall back to the fixed-leg ones when `None`.
    ///
    /// # Errors
    ///
    /// The observation lag must let the index observe fixings that exist. Under
    /// [`Flat`](CpiInterpolationType::Flat) the index's availability lag must not
    /// exceed it; under [`Linear`](CpiInterpolationType::Linear) a further
    /// publication period is consumed by the interpolation, so the lag less that
    /// period must still cover the availability lag (`cpp:56-69`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        nominal: Real,
        start_date: Date,
        maturity: Date,
        fixed_calendar: Calendar,
        fixed_convention: BusinessDayConvention,
        day_counter: DayCounter,
        fixed_rate: Rate,
        inflation_index: Shared<ZeroInflationIndex>,
        observation_lag: Period,
        observation_interpolation: CpiInterpolationType,
        inflation_calendar: Option<Calendar>,
        inflation_convention: Option<BusinessDayConvention>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<ZeroCouponInflationSwap> {
        let availability_lag = inflation_index.availability_lag();
        match observation_interpolation {
            CpiInterpolationType::Linear => {
                let publication = Period::try_from(inflation_index.frequency())?;
                let covered = observation_lag - publication >= availability_lag;
                require!(
                    covered,
                    "inconsistency between swap observation lag {observation_lag}, \
                     interpolated index period {publication} and index availability \
                     {availability_lag}: need (obsLag-index period) >= availLag"
                );
            }
            CpiInterpolationType::Flat => {
                let covered = availability_lag <= observation_lag;
                require!(
                    covered,
                    "index tries to observe inflation fixings that do not yet exist: \
                     availability lag {availability_lag} versus obs lag = {observation_lag}"
                );
            }
        }

        let inflation_calendar = inflation_calendar.unwrap_or_else(|| fixed_calendar.clone());
        let inflation_convention = inflation_convention.unwrap_or(fixed_convention);

        let inflation_pay_date = inflation_calendar.adjust(maturity, inflation_convention);
        let fixed_pay_date = fixed_calendar.adjust(maturity, fixed_convention);

        let inflation_cash_flow = shared(ZeroInflationCashFlow::new(
            nominal,
            Shared::clone(&inflation_index),
            observation_interpolation,
            start_date,
            maturity,
            observation_lag,
            inflation_pay_date,
            true,
        ));

        let time = day_counter.year_fraction(start_date, maturity);
        let fixed_amount = nominal * ((1.0 + fixed_rate).powf(time) - 1.0);
        let fixed_cash_flow = shared(SimpleCashFlow::new(fixed_amount, fixed_pay_date)?);

        let fixed_leg: Leg = vec![fixed_cash_flow as Shared<dyn CashFlow>];
        let inflation_leg: Leg = vec![Shared::clone(&inflation_cash_flow) as Shared<dyn CashFlow>];

        let payer = match swap_type {
            SwapType::Payer => vec![false, true],
            SwapType::Receiver => vec![true, false],
        };
        let swap = Swap::new(vec![fixed_leg, inflation_leg], payer, settings)?;

        Ok(ZeroCouponInflationSwap {
            swap,
            swap_type,
            nominal,
            start_date,
            maturity_date: maturity,
            fixed_calendar,
            fixed_convention,
            fixed_rate,
            inflation_index,
            observation_lag,
            observation_interpolation,
            inflation_calendar,
            inflation_convention,
            day_counter,
            inflation_cash_flow,
        })
    }

    /// The embedded two-leg base.
    pub fn swap(&self) -> &Swap {
        &self.swap
    }

    /// The embedded base, mutably (its on-demand-pricing accessors).
    pub fn swap_mut(&mut self) -> &mut Swap {
        &mut self.swap
    }

    /// Whether the *inflation* leg is paid or received (`type()`).
    pub fn swap_type(&self) -> SwapType {
        self.swap_type
    }

    /// The notional both flows are scaled by (`nominal()`).
    pub fn nominal(&self) -> Real {
        self.nominal
    }

    /// The contract start date, raw (`hpp:93`).
    pub fn start_date(&self) -> Date {
        self.start_date
    }

    /// The contract maturity, raw and pre-adjustment (`hpp:94`).
    pub fn maturity_date(&self) -> Date {
        self.maturity_date
    }

    /// The calendar the fixed payment date is adjusted on (`fixedCalendar()`).
    pub fn fixed_calendar(&self) -> &Calendar {
        &self.fixed_calendar
    }

    /// The convention the fixed payment date is adjusted with
    /// (`fixedConvention()`).
    pub fn fixed_convention(&self) -> BusinessDayConvention {
        self.fixed_convention
    }

    /// The day counter the fixed amount's year fraction is taken on
    /// (`dayCounter()`).
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }

    /// The quoted rate `K` (`fixedRate()`).
    pub fn fixed_rate(&self) -> Rate {
        self.fixed_rate
    }

    /// The index the second leg is indexed on (`inflationIndex()`).
    pub fn inflation_index(&self) -> &Shared<ZeroInflationIndex> {
        &self.inflation_index
    }

    /// The lag both observations are taken at (`observationLag()`).
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// How the observations interpolate within their period
    /// (`observationInterpolation()`).
    pub fn observation_interpolation(&self) -> CpiInterpolationType {
        self.observation_interpolation
    }

    /// The calendar the indexed payment date is adjusted on, resolved
    /// (`inflationCalendar()`).
    pub fn inflation_calendar(&self) -> &Calendar {
        &self.inflation_calendar
    }

    /// The convention the indexed payment date is adjusted with, resolved
    /// (`inflationConvention()`).
    pub fn inflation_convention(&self) -> BusinessDayConvention {
        self.inflation_convention
    }

    /// The date the base fixing is observed at, taken from the indexed flow
    /// (`cpp:86`).
    pub fn base_date(&self) -> Date {
        self.inflation_cash_flow.base_date()
    }

    /// The date the maturity fixing is observed at, taken from the indexed flow
    /// (`cpp:87`).
    pub fn obs_date(&self) -> Date {
        self.inflation_cash_flow.fixing_date()
    }

    /// The fixed leg: one flow that is not a coupon (`fixedLeg()`).
    pub fn fixed_leg(&self) -> &Leg {
        &self.swap.legs()[0]
    }

    /// The inflation leg: one flow that is not a coupon (`inflationLeg()`).
    pub fn inflation_leg(&self) -> &Leg {
        &self.swap.legs()[1]
    }

    /// The indexed flow itself, typed.
    pub fn inflation_cash_flow(&self) -> &Shared<ZeroInflationCashFlow> {
        &self.inflation_cash_flow
    }

    /// The fixed leg's NPV (`fixedLegNPV()`), priced on demand.
    ///
    /// # Errors
    ///
    /// As [`Swap::leg_npv`].
    pub fn fixed_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(0)
    }

    /// The inflation leg's NPV (`inflationLegNPV()`), priced on demand.
    ///
    /// # Errors
    ///
    /// As [`Swap::leg_npv`].
    pub fn inflation_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(1)
    }

    /// The rate that would make this swap worth nothing (`fairRate()`,
    /// `cpp:118-138`).
    ///
    /// The indexed flow pays growth only, so `amount/notional + 1` is the index
    /// ratio `I(T)/I(0)`, and the fair rate is that ratio de-compounded over the
    /// same `T` the fixed amount uses. The flow is read directly rather than
    /// through the engine: C++ calls no `calculate()` here either.
    ///
    /// # Errors
    ///
    /// Propagates the indexed flow's amount, which forecasts off the index's
    /// inflation curve and so needs one linked.
    pub fn fair_rate(&self) -> QlResult<Rate> {
        let growth = self.inflation_cash_flow.amount()? / self.inflation_cash_flow.notional() + 1.0;
        let time = self
            .day_counter
            .year_fraction(self.start_date, self.maturity_date);
        Ok(growth.powf(1.0 / time) - 1.0)
    }

    /// The fixed leg's sensitivity to a basis point on `K` (`fixedLegBPS()`,
    /// `cpp:140-155`), computed analytically.
    ///
    /// The engine's own `leg_bps[0]` is **zero** and must not be used: the fixed
    /// leg is a [`SimpleCashFlow`], which the BPS pass treats as insensitive to
    /// the rate, and the leg compounds annually so a linear-in-rate coupon would
    /// not answer this either (the C++ comment at `cpp:141-145`). The shift is
    /// therefore repriced in closed form, signed by the leg's payer multiplier
    /// and discounted at its end discount factor.
    ///
    /// # Errors
    ///
    /// The engine must have populated the fixed leg's end discount factor (the
    /// C++ `QL_REQUIRE` against `Null<DiscountFactor>`).
    ///
    /// [`SimpleCashFlow`]: crate::cashflows::SimpleCashFlow
    pub fn fixed_leg_bps(&mut self) -> QlResult<Real> {
        let discount = self.swap.end_discounts(0)?;
        let sign = if self.swap.payer(0)? { -1.0 } else { 1.0 };
        let time = self
            .day_counter
            .year_fraction(self.start_date, self.maturity_date);
        let shifted = (1.0 + self.fixed_rate + BASIS_POINT).powf(time);
        Ok(sign * discount * self.nominal * (shifted - (1.0 + self.fixed_rate).powf(time)))
    }
}

impl Instrument for ZeroCouponInflationSwap {
    fn base(&self) -> &InstrumentBase {
        self.swap.base()
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.swap.base_mut()
    }

    fn is_expired(&self) -> QlResult<bool> {
        self.swap.is_expired()
    }

    fn setup_expired(&mut self) {
        self.swap.setup_expired();
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(arguments)
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)
    }
}
