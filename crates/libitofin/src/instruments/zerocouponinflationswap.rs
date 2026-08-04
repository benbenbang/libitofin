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

#[cfg(test)]
mod tests {
    //! `test-suite/inflation.cpp`'s zero-coupon swap oracles all price off a
    //! *bootstrapped* inflation curve, which arrives later in #705. These tests
    //! price the same instrument on a **directly built**
    //! [`ZeroInflationCurve`] instead, so they are self-contained: a
    //! directly-built curve does not reprice its own swaps to zero, and the
    //! computed values are pinned rather than asserted against zero.
    //!
    //! Every expected number is derived by hand in the doc comment that asserts
    //! it, off the two seeded fixings and the single curve node the forecast
    //! lands on, so no expectation shares arithmetic with the code under test.

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::Index;
    use crate::indexes::inflation::UkRpi;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::shared::{SharedMut, shared_mut};
    use crate::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
    use crate::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::unitedkingdom::{Market, UnitedKingdom};
    use crate::time::date::Month::{July, June, September};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    const NOMINAL: Real = 1_000_000.0;
    const FIXED_RATE: Rate = 0.025;
    const NOMINAL_RATE: Rate = 0.05;

    /// The June 2026 figure, the base observation of a swap starting 1 September
    /// 2026 under a three-month lag.
    const JUNE_FIXING: Real = 196.0;
    /// The July 2026 figure, which is also the curve's base date, so it is the
    /// figure every forecast compounds off.
    const JULY_FIXING: Real = 200.0;
    /// The curve's zero rate at its second node, 1 June 2031.
    const NODE_RATE: Rate = 0.03;

    fn today() -> Date {
        Date::new(1, September, 2026)
    }

    /// A Monday, so the fixed and inflation payment dates are the maturity
    /// itself.
    fn maturity() -> Date {
        Date::new(1, September, 2031)
    }

    /// A Saturday: `ModifiedFollowing` moves both payment dates to Monday 8
    /// September 2031 while the year fraction stays on the 6th.
    fn weekend_maturity() -> Date {
        Date::new(6, September, 2031)
    }

    fn curve_base_date() -> Date {
        Date::new(1, July, 2026)
    }

    fn lag() -> Period {
        Period::new(3, TimeUnit::Months)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// UK RPI with June and July 2026 published, linked to a three-node zero
    /// inflation curve whose middle node sits exactly on 1 June 2031 - the
    /// period a September 2031 maturity observes under a three-month lag - so
    /// the forecast reads that node's rate with no interpolation of its own.
    fn an_index(settings: &Shared<Settings<Date>>) -> Shared<ZeroInflationIndex> {
        let curve = shared(
            ZeroInflationCurve::new(
                today(),
                vec![
                    curve_base_date(),
                    Date::new(1, June, 2031),
                    Date::new(1, July, 2036),
                ],
                vec![0.02, NODE_RATE, 0.04],
                Frequency::Monthly,
                Actual360::new(),
                Linear,
            )
            .expect("a well-formed zero inflation curve"),
        );
        let index = shared(
            UkRpi::new(Shared::clone(settings))
                .with_term_structure(Handle::new(curve as Shared<dyn ZeroInflationTermStructure>)),
        );
        index
            .add_fixing(Date::new(1, June, 2026), JUNE_FIXING)
            .expect("a published figure");
        index
            .add_fixing(curve_base_date(), JULY_FIXING)
            .expect("a published figure");
        index
    }

    /// A flat 5 % continuously-compounded nominal curve anchored at the
    /// evaluation date, so a payment `t` years out discounts by `exp(-0.05 t)`.
    fn a_discount_engine(settings: Shared<Settings<Date>>) -> SharedMut<dyn PricingEngine> {
        let curve = shared(FlatForward::with_rate(
            today(),
            NOMINAL_RATE,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;
        shared_mut(DiscountingSwapEngine::new(
            Handle::new(curve),
            None,
            None,
            None,
            settings,
        )) as SharedMut<dyn PricingEngine>
    }

    fn a_swap(swap_type: SwapType, maturity: Date, fixed_rate: Rate) -> ZeroCouponInflationSwap {
        let settings = settings_today();
        let index = an_index(&settings);
        let mut swap = ZeroCouponInflationSwap::new(
            swap_type,
            NOMINAL,
            today(),
            maturity,
            UnitedKingdom::new(Market::Settlement),
            BusinessDayConvention::ModifiedFollowing,
            Actual365Fixed::new(),
            fixed_rate,
            index,
            lag(),
            CpiInterpolationType::Flat,
            None,
            None,
            Shared::clone(&settings),
        )
        .expect("a three-month lag covers UK RPI's one-month availability");
        swap.base_mut()
            .set_pricing_engine(a_discount_engine(settings));
        swap
    }

    /// The two amounts the legs exchange, both derived off the seeded figures.
    ///
    /// Inflation leg: the maturity observation is 1 September 2031 less three
    /// months, so June 2031, forecast off the curve base date 1 July 2026 as
    /// `200 * 1.03^t` with `t = Act/360(1 Jul 2026, 1 Jun 2031) = 1796/360 =
    /// 4.988888888888889`, giving `I(T) = 231.7786790231386`. The base
    /// observation is 1 September 2026 less three months, June 2026, on record
    /// as 196. Growth only, so the flow pays `1e6 * (231.7786790231386/196 - 1)
    /// = 182544.28073029904`.
    ///
    /// Fixed leg: `T = Act/365F(1 Sep 2026, 1 Sep 2031) = 1826/365 =
    /// 5.002739726027397`, so `1e6 * (1.025^T - 1) = 131484.7563692576`.
    #[test]
    fn the_two_legs_pay_the_hand_derived_amounts() {
        let swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);
        let flow = swap.inflation_cash_flow();

        assert!((flow.base_fixing().unwrap() - JUNE_FIXING).abs() < 1e-10);
        assert!((flow.index_fixing().unwrap() - 231.7786790231386).abs() < 1e-10);
        assert!((flow.amount().unwrap() - 182544.28073029904).abs() < 1e-6);
        assert!((swap.fixed_leg()[0].amount().unwrap() - 131484.7563692576).abs() < 1e-6);
    }

    /// Both flows pay on 1 September 2031, five years and one day out, so both
    /// discount by `exp(-0.05 * 1826/365) = 0.7786941053394887`. A `Payer`
    /// receives fixed and pays inflation (`cpp:104-107`), so the leg NPVs are
    /// `+131484.7563692576 * df = 102386.4047267397` and
    /// `-182544.28073029904 * df = -142146.15536812067`, summing to
    /// `-39759.750641380975`.
    #[test]
    fn a_payer_swap_prices_to_the_hand_derived_npv() {
        let mut swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);

        assert!((swap.fixed_leg_npv().unwrap() - 102386.4047267397).abs() < 1e-6);
        assert!((swap.inflation_leg_npv().unwrap() + 142146.15536812067).abs() < 1e-6);
        assert!((swap.npv().unwrap() + 39759.750641380975).abs() < 1e-6);
    }

    /// `fairRate` de-compounds the index ratio over the swap's own `T`:
    /// `(231.7786790231386/196)^(365/1826) - 1 = 0.03408325777213217`.
    ///
    /// The round trip that follows - a swap struck at that rate is worth
    /// nothing - checks the formula but **not** `T`: `(1+K)^T` collapses to the
    /// index ratio for *any* `T`, so a port that took `T` off the adjusted
    /// payment date would still price to zero here. Only the amount literals
    /// above and
    /// [`an_adjusted_payment_date_leaves_the_year_fraction_raw`] discriminate
    /// `T`.
    #[test]
    fn the_fair_rate_de_compounds_the_index_ratio() {
        let swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);
        let fair = swap.fair_rate().unwrap();
        assert!((fair - 0.03408325777213217).abs() < 1e-12);

        let mut struck_at_fair = a_swap(SwapType::Payer, maturity(), fair);
        assert!(struck_at_fair.npv().unwrap().abs() < 1e-6);
    }

    /// `fixedLegBPS` is computed in closed form, `df * N * ((1+K+1e-4)^T -
    /// (1+K)^T) = 0.7786941053394887 * 1e6 * (1.0251^T - 1.025^T) =
    /// 430.1148492112715`.
    ///
    /// The engine's own leg BPS is zero and must not stand in for it: the fixed
    /// leg is a [`SimpleCashFlow`], which the BPS pass reads as insensitive to
    /// the rate (`cpp:141-145`).
    ///
    /// [`SimpleCashFlow`]: crate::cashflows::SimpleCashFlow
    #[test]
    fn the_fixed_leg_bps_is_analytic_not_the_engines_zero() {
        let mut swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);

        assert_eq!(swap.swap_mut().leg_bps(0).unwrap(), 0.0);
        assert!((swap.fixed_leg_bps().unwrap() - 430.1148492112715).abs() < 1e-8);
    }

    /// The type names the inflation leg, so a `Receiver` receives inflation and
    /// pays fixed: every leg multiplier flips and the NPV negates
    /// (`cpp:108-111`).
    #[test]
    fn a_receiver_swap_negates_the_payer_npv() {
        let mut payer = a_swap(SwapType::Payer, maturity(), FIXED_RATE);
        let mut receiver = a_swap(SwapType::Receiver, maturity(), FIXED_RATE);

        assert!(!payer.swap().payer(0).unwrap(), "a payer receives fixed");
        assert!(payer.swap().payer(1).unwrap(), "a payer pays inflation");
        assert!(receiver.swap().payer(0).unwrap());
        assert!(!receiver.swap().payer(1).unwrap());
        assert!((receiver.npv().unwrap() + payer.npv().unwrap()).abs() < 1e-6);
    }

    /// The H2 pin: a maturity on a Saturday.
    ///
    /// `ModifiedFollowing` on the UK calendar moves both payment dates from
    /// Saturday 6 September 2031 to Monday the 8th, but `T` stays on the raw
    /// maturity (`cpp:92` reads `maturityDate_`, stored pre-adjustment at
    /// `cpp:50`): `T = 1831/365 = 5.016438356164383`, so the fixed leg pays
    /// `1e6 * (1.025^T - 1) = 131867.55144569263`. A port that took `T` off the
    /// adjusted date would use `1833/365` and pay `132020.70573500003`, over
    /// 150 out.
    ///
    /// The discounting *does* use the adjusted date: `exp(-0.05 * 1833/365) =
    /// 0.7779477702508455`. The inflation observation is unmoved - 6 June 2031
    /// still lands in the June 2031 period - so the indexed flow pays the same
    /// `182544.28073029904` and the NPV is `-39423.84855056528`.
    ///
    /// [`fair_rate`](ZeroCouponInflationSwap::fair_rate) and
    /// [`fixed_leg_bps`](ZeroCouponInflationSwap::fixed_leg_bps) take their own
    /// year fraction, so they are pinned here too: `(231.7786790231386/196)^
    /// (365/1831) - 1 = 0.033988620914166656` and `0.7779477702508455 * 1e6 *
    /// (1.0251^T - 1.025^T) = 431.02529041491573`. On the adjusted date they
    /// would answer `0.0339509131467679` and `431.55460061691315`. The Monday
    /// fixture cannot tell these apart - there the raw and adjusted maturities
    /// coincide.
    #[test]
    fn an_adjusted_payment_date_leaves_the_year_fraction_raw() {
        let mut swap = a_swap(SwapType::Payer, weekend_maturity(), FIXED_RATE);

        assert_eq!(swap.maturity_date(), weekend_maturity());
        assert_eq!(
            swap.fixed_leg()[0].date(),
            Date::new(8, September, 2031),
            "the payment date is adjusted"
        );
        assert_eq!(
            swap.inflation_leg()[0].date(),
            Date::new(8, September, 2031)
        );

        assert!((swap.fixed_leg()[0].amount().unwrap() - 131867.55144569263).abs() < 1e-6);
        assert!((swap.inflation_cash_flow().amount().unwrap() - 182544.28073029904).abs() < 1e-6);
        assert!((swap.npv().unwrap() + 39423.84855056528).abs() < 1e-6);

        assert!((swap.fair_rate().unwrap() - 0.033988620914166656).abs() < 1e-12);
        assert!((swap.fixed_leg_bps().unwrap() - 431.02529041491573).abs() < 1e-8);
    }

    /// The two date accessors are genuine overrides (`hpp:93-94`), not the base
    /// [`Swap`]'s min/max over the legs: on the weekend fixture the base answers
    /// the adjusted payment date for both, 8 September 2031.
    #[test]
    fn the_raw_dates_override_the_bases_span_of_the_legs() {
        let swap = a_swap(SwapType::Payer, weekend_maturity(), FIXED_RATE);
        let adjusted = Date::new(8, September, 2031);

        assert_eq!(swap.start_date(), today());
        assert_eq!(swap.maturity_date(), weekend_maturity());
        assert_eq!(swap.swap().start_date().unwrap(), adjusted);
        assert_eq!(swap.swap().maturity_date().unwrap(), adjusted);
    }

    /// `baseDate_` and `obsDate_` are taken from the indexed flow (`cpp:86-87`),
    /// which reports both raw and unsnapped: 1 September 2026 and 1 September
    /// 2031 each less the three-month lag.
    #[test]
    fn the_observation_dates_come_from_the_indexed_flow() {
        let swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);

        assert_eq!(swap.base_date(), Date::new(1, June, 2026));
        assert_eq!(swap.obs_date(), Date::new(1, June, 2031));
        assert_eq!(swap.base_date(), swap.inflation_cash_flow().base_date());
        assert_eq!(swap.obs_date(), swap.inflation_cash_flow().fixing_date());

        assert_eq!(swap.swap_type(), SwapType::Payer);
        assert_eq!(swap.nominal(), NOMINAL);
        assert_eq!(swap.fixed_rate(), FIXED_RATE);
        assert_eq!(swap.observation_lag(), lag());
        assert_eq!(swap.observation_interpolation(), CpiInterpolationType::Flat);
        assert_eq!(swap.inflation_index().name(), "UK RPI");
        assert!(swap.inflation_cash_flow().growth_only());
    }

    /// The inflation calendar and convention default to the fixed-leg ones and
    /// are stored resolved, as C++ assigns them at `cpp:71-74`.
    #[test]
    fn the_inflation_calendar_defaults_to_the_fixed_one() {
        let swap = a_swap(SwapType::Payer, maturity(), FIXED_RATE);

        assert_eq!(
            swap.inflation_calendar().name(),
            swap.fixed_calendar().name()
        );
        assert_eq!(swap.inflation_convention(), swap.fixed_convention());
        assert_eq!(
            swap.fixed_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
    }

    /// A lag the index cannot observe through is rejected at construction
    /// (`cpp:56-69`). Under `Flat` the bar is the availability lag itself; under
    /// `Linear` the interpolation eats a further publication period, so a
    /// one-month lag fails there while clearing the `Flat` bar.
    #[test]
    fn a_lag_the_index_cannot_observe_through_is_rejected() {
        let settings = settings_today();
        let build = |lag: Period, interpolation: CpiInterpolationType| {
            ZeroCouponInflationSwap::new(
                SwapType::Payer,
                NOMINAL,
                today(),
                maturity(),
                UnitedKingdom::new(Market::Settlement),
                BusinessDayConvention::ModifiedFollowing,
                Actual365Fixed::new(),
                FIXED_RATE,
                an_index(&settings),
                lag,
                interpolation,
                None,
                None,
                Shared::clone(&settings),
            )
            .map(|_| ())
        };
        let month = Period::new(1, TimeUnit::Months);

        assert!(build(month, CpiInterpolationType::Flat).is_ok());
        assert!(
            build(Period::new(0, TimeUnit::Months), CpiInterpolationType::Flat)
                .unwrap_err()
                .message()
                .contains("fixings that do not yet exist")
        );
        assert!(
            build(month, CpiInterpolationType::Linear)
                .unwrap_err()
                .message()
                .contains("inconsistency between swap observation lag")
        );
        assert!(build(lag(), CpiInterpolationType::Linear).is_ok());
    }
}
