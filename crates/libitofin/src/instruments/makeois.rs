//! Overnight indexed swap builder (`MakeOIS`).
//!
//! Port of `ql/instruments/makeois.{hpp,cpp}`: `class MakeOIS`, the comfortable
//! way to instantiate an [`OvernightIndexedSwap`]. It derives the start and end
//! dates, the two annual schedules and the discounting engine from a swap tenor,
//! an overnight index and a handful of overrides, then hands them to
//! [`OvernightIndexedSwap::with_nominal`] and attaches a
//! [`DiscountingSwapEngine`]. C++'s `operator OvernightIndexedSwap()` and
//! `operator shared_ptr<OvernightIndexedSwap>()` become [`MakeOis::build`],
//! which returns the priced swap.
//!
//! This is the shape all three `test-suite/overnightindexedswap.cpp` oracles
//! (`testCachedValue` :367, `testFairRate` :284, `testFairSpread` :325) build
//! their swaps through, so the cached NPV and the fair-value self-consistency
//! depend on this class reproducing `makeois.cpp`'s date logic exactly.
//!
//! ## Ported knobs
//!
//! The builder exposes the overrides [`OISRateHelper::initialize_dates`] and the
//! swap oracles use:
//! [`with_effective_date`](MakeOis::with_effective_date),
//! [`with_overnight_leg_spread`](MakeOis::with_overnight_leg_spread),
//! [`with_nominal`](MakeOis::with_nominal),
//! [`with_payment_lag`](MakeOis::with_payment_lag),
//! [`with_discounting_term_structure`](MakeOis::with_discounting_term_structure),
//! [`with_averaging_method`](MakeOis::with_averaging_method),
//! [`with_fixed_leg_day_count`](MakeOis::with_fixed_leg_day_count),
//! [`with_settlement_days`](MakeOis::with_settlement_days),
//! [`with_termination_date`](MakeOis::with_termination_date),
//! [`with_payment_frequency`](MakeOis::with_payment_frequency),
//! [`with_payment_adjustment`](MakeOis::with_payment_adjustment),
//! [`with_payment_calendar`](MakeOis::with_payment_calendar),
//! [`with_rule`](MakeOis::with_rule),
//! [`with_convention`](MakeOis::with_convention),
//! [`with_termination_date_convention`](MakeOis::with_termination_date_convention) and
//! [`with_end_of_month`](MakeOis::with_end_of_month). The swap tenor, overnight
//! index, optional fixed rate and forward start are the constructor arguments
//! (`makeois.hpp:40`).
//!
//! ## #262-safe guards on unported knobs
//!
//! Four knobs whose machinery is unported are exposed but accept only their
//! benign default, rejecting any other value at [`build`](MakeOis::build) rather
//! than accepting and silently ignoring it (verified defaults, `oisratehelper.hpp:47/60/61/62`):
//! [`with_telescopic_value_dates`](MakeOis::with_telescopic_value_dates) (default
//! `false`), [`with_lookback_days`](MakeOis::with_lookback_days) (default unset
//! `None`), [`with_lockout_days`](MakeOis::with_lockout_days) (default `0`) and
//! [`with_observation_shift`](MakeOis::with_observation_shift) (default `false`).
//! Telescopic value dates, lookback, lockout and observation shift are all
//! deferred with the [`OvernightLeg`](crate::cashflows::OvernightLeg) (#328/#329),
//! and arithmetic averaging is rejected at the coupon.
//!
//! ## Deferred knobs
//!
//! Still deferred, defaulting to the C++ default (`makeois.hpp:96-137`):
//!
//! - swap type (`receiveFixed` / `withType`): defaults to `Payer`;
//! - the per-leg schedule variants (`withFixedLegCalendar` /
//!   `withOvernightLegCalendar`, `withFixedLegPaymentFrequency`, the `*LegRule` /
//!   `*LegConvention` / `*LegEndOfMonth` splits, `withMaturityEndOfMonth`): the
//!   ported knobs set both legs together, so both schedules share one calendar
//!   (the index fixing calendar), frequency, rule, convention and end-of-month
//!   flag. The [`OISRateHelper`] oracle never sets a per-leg override (its
//!   `fixedCalendar_` / `overnightCalendar_` are empty and `fixedPaymentFrequency_`
//!   is unset, so `initializeDates`' conditional per-leg calls do not fire), so
//!   the two-schedule split is not built;
//! - `withPricingEngine`: the engine is always the
//!   [`DiscountingSwapEngine`] over the discounting curve (set) or the index's
//!   forwarding curve (default), matching `makeois.cpp:145-156/172-180`.
//!
//! [`OISRateHelper`]: crate::termstructures::yields::ratehelpers::OISRateHelper
//! [`OISRateHelper::initialize_dates`]: crate::termstructures::yields::ratehelpers::OISRateHelper
//!
//! ## Settlement-days dispatch onto the newtype
//!
//! C++ dispatches the default settlement days by dynamic type
//! (`makeois.cpp:59-69`): `Sonia` 0, `Corra` 1, else 2. The port's
//! [`OvernightIndex`] is a newtype with no subtype, so the dispatch keys on the
//! index family name instead, case-insensitively: QuantLib's `Sonia` family
//! name is mixed case (`sonia.cpp:28`) while `CORRA` is uppercase
//! (`corra.cpp:26`). Only `Estr` (family `"ESTR"`, so 2) is ported today; the
//! mapping is future-proof for when the other overnight indexes land.

use crate::cashflows::RateAveraging;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::OvernightIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::swap::SwapType;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::DiscountingSwapEngine;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::{Schedule, allows_end_of_month};
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real, Spread};

use super::OvernightIndexedSwap;

/// Builder for an [`OvernightIndexedSwap`] (`ql/instruments/makeois.hpp`).
///
/// Construct with [`new`](Self::new), chain the ported `with_*` overrides, then
/// [`build`](Self::build) to get the priced swap.
pub struct MakeOis {
    swap_tenor: Period,
    overnight_index: Shared<OvernightIndex>,
    fixed_rate: Option<Rate>,
    forward_start: Period,
    settings: Shared<Settings<Date>>,

    effective_date: Option<Date>,
    swap_type: SwapType,
    nominal: Real,
    overnight_spread: Spread,
    payment_lag: Integer,
    payment_adjustment: BusinessDayConvention,
    averaging_method: RateAveraging,
    fixed_day_count: Option<DayCounter>,
    discounting_curve: Option<Handle<dyn YieldTermStructure>>,

    settlement_days: Option<Natural>,
    termination_date: Option<Date>,
    payment_frequency: Frequency,
    payment_calendar: Option<Calendar>,
    schedule_convention: BusinessDayConvention,
    termination_date_convention: BusinessDayConvention,
    rule: DateGeneration,
    end_of_month: Option<bool>,

    telescopic_value_dates: bool,
    lookback_days: Option<Natural>,
    lockout_days: Natural,
    observation_shift: bool,
}

impl MakeOis {
    /// Starts a builder for an OIS of `swap_tenor` on `overnight_index`
    /// (`makeois.cpp:32`).
    ///
    /// `fixed_rate` is the C++ `Null<Rate>()`-defaulted fixed rate: `Some(r)`
    /// fixes the leg at `r`, `None` fills it with the fair rate at build time
    /// (`makeois.cpp:135-159`). `forward_start` is the C++ `0*Days`-defaulted
    /// forward start. `settings` carries the evaluation date (D5).
    pub fn new(
        swap_tenor: Period,
        overnight_index: Shared<OvernightIndex>,
        fixed_rate: Option<Rate>,
        forward_start: Period,
        settings: Shared<Settings<Date>>,
    ) -> MakeOis {
        MakeOis {
            swap_tenor,
            overnight_index,
            fixed_rate,
            forward_start,
            settings,
            effective_date: None,
            swap_type: SwapType::Payer,
            nominal: 1.0,
            overnight_spread: 0.0,
            payment_lag: 0,
            payment_adjustment: BusinessDayConvention::Following,
            averaging_method: RateAveraging::Compound,
            fixed_day_count: None,
            discounting_curve: None,
            settlement_days: None,
            termination_date: None,
            payment_frequency: Frequency::Annual,
            payment_calendar: None,
            schedule_convention: BusinessDayConvention::ModifiedFollowing,
            termination_date_convention: BusinessDayConvention::ModifiedFollowing,
            rule: DateGeneration::Backward,
            end_of_month: None,
            telescopic_value_dates: false,
            lookback_days: None,
            lockout_days: 0,
            observation_shift: false,
        }
    }

    /// Sets the fixed-leg day count, overriding the default (the overnight
    /// index's own day count) (`makeois.hpp` `withFixedLegDayCount`).
    pub fn with_fixed_leg_day_count(mut self, day_count: DayCounter) -> MakeOis {
        self.fixed_day_count = Some(day_count);
        self
    }

    /// Sets the swap's start date explicitly, bypassing the settlement-days
    /// dispatch (`makeois.cpp:205`).
    pub fn with_effective_date(mut self, effective_date: Date) -> MakeOis {
        self.effective_date = Some(effective_date);
        self
    }

    /// Sets the spread over the overnight index (`makeois.cpp:344`).
    pub fn with_overnight_leg_spread(mut self, spread: Spread) -> MakeOis {
        self.overnight_spread = spread;
        self
    }

    /// Sets the nominal shared by both legs (`makeois.cpp:195`).
    pub fn with_nominal(mut self, nominal: Real) -> MakeOis {
        self.nominal = nominal;
        self
    }

    /// Sets the business days between an overnight coupon's accrual end and its
    /// payment (`makeois.cpp:236`).
    pub fn with_payment_lag(mut self, payment_lag: Integer) -> MakeOis {
        self.payment_lag = payment_lag;
        self
    }

    /// Prices the swap on `discounting_term_structure` rather than the index's
    /// forwarding curve (`makeois.cpp:274`).
    pub fn with_discounting_term_structure(
        mut self,
        discounting_term_structure: Handle<dyn YieldTermStructure>,
    ) -> MakeOis {
        self.discounting_curve = Some(discounting_term_structure);
        self
    }

    /// Sets whether the overnight leg compounds or averages its fixings
    /// (`makeois.cpp:354`).
    pub fn with_averaging_method(mut self, averaging_method: RateAveraging) -> MakeOis {
        self.averaging_method = averaging_method;
        self
    }

    /// Sets the settlement days used to derive the start date, overriding the
    /// index-family default (`makeois.cpp:213`). Ignored when an explicit
    /// effective date is set.
    pub fn with_settlement_days(mut self, settlement_days: Natural) -> MakeOis {
        self.settlement_days = Some(settlement_days);
        self
    }

    /// Sets the swap's termination date explicitly, overriding the tenor-derived
    /// end date (`makeois.cpp:401`).
    pub fn with_termination_date(mut self, termination_date: Date) -> MakeOis {
        self.termination_date = Some(termination_date);
        self
    }

    /// Sets the schedule frequency shared by both legs (`makeois.cpp:203`, which
    /// dispatches `withPaymentFrequency` onto both leg frequencies).
    pub fn with_payment_frequency(mut self, payment_frequency: Frequency) -> MakeOis {
        self.payment_frequency = payment_frequency;
        self
    }

    /// Sets the business-day convention applied when adjusting the coupon
    /// payment dates (`makeois.cpp:230`).
    pub fn with_payment_adjustment(mut self, payment_adjustment: BusinessDayConvention) -> MakeOis {
        self.payment_adjustment = payment_adjustment;
        self
    }

    /// Sets the calendar the coupon payment dates are adjusted on
    /// (`makeois.cpp:239`); empty (`None`) falls back to the schedule calendar.
    pub fn with_payment_calendar(mut self, payment_calendar: Calendar) -> MakeOis {
        self.payment_calendar = Some(payment_calendar);
        self
    }

    /// Sets the date-generation rule shared by both legs (`makeois.cpp:258`,
    /// which dispatches `withRule` onto both leg rules).
    pub fn with_rule(mut self, rule: DateGeneration) -> MakeOis {
        self.rule = rule;
        self
    }

    /// Sets the schedule roll convention shared by both legs (`makeois.cpp:290`,
    /// which dispatches `withConvention` onto both leg conventions).
    pub fn with_convention(mut self, convention: BusinessDayConvention) -> MakeOis {
        self.schedule_convention = convention;
        self
    }

    /// Sets the termination-date convention shared by both legs
    /// (`makeois.cpp:303`, which dispatches onto both leg termination
    /// conventions).
    pub fn with_termination_date_convention(
        mut self,
        convention: BusinessDayConvention,
    ) -> MakeOis {
        self.termination_date_convention = convention;
        self
    }

    /// Sets the end-of-month flag shared by both legs, overriding the default
    /// (derived from the start date) (`makeois.cpp:319`).
    pub fn with_end_of_month(mut self, end_of_month: bool) -> MakeOis {
        self.end_of_month = Some(end_of_month);
        self
    }

    /// Sets whether the overnight leg uses telescopic value dates
    /// (`makeois.cpp:347`).
    ///
    /// Telescopic value dates are unported (deferred with the
    /// [`OvernightLeg`](crate::cashflows::OvernightLeg), #328/#329), so this knob
    /// accepts only the benign default `false`. A `true` value is rejected at
    /// [`build`](Self::build) rather than accepted and silently ignored.
    pub fn with_telescopic_value_dates(mut self, telescopic_value_dates: bool) -> MakeOis {
        self.telescopic_value_dates = telescopic_value_dates;
        self
    }

    /// Sets the overnight-leg lookback days (`makeois.cpp:363`).
    ///
    /// Lookback is unported (the machinery is deferred with the overnight leg),
    /// so this knob accepts only the benign unset default `None`; a `Some`
    /// value is rejected at [`build`](Self::build). The C++ default is the unset
    /// sentinel `Null<Natural>()`, represented here as `None`.
    pub fn with_lookback_days(mut self, lookback_days: Option<Natural>) -> MakeOis {
        self.lookback_days = lookback_days;
        self
    }

    /// Sets the overnight-leg lockout days (`makeois.cpp:368`).
    ///
    /// Lockout is unported, so this knob accepts only the benign default `0`; a
    /// nonzero value is rejected at [`build`](Self::build).
    pub fn with_lockout_days(mut self, lockout_days: Natural) -> MakeOis {
        self.lockout_days = lockout_days;
        self
    }

    /// Sets the overnight-leg observation-shift flag (`makeois.cpp:373`).
    ///
    /// Observation shift is unported, so this knob accepts only the benign
    /// default `false`; a `true` value is rejected at [`build`](Self::build).
    pub fn with_observation_shift(mut self, observation_shift: bool) -> MakeOis {
        self.observation_shift = observation_shift;
        self
    }

    /// Builds the priced swap (C++ `operator OvernightIndexedSwap()` /
    /// `operator shared_ptr<OvernightIndexedSwap>()`, `makeois.cpp:42/47`).
    ///
    /// Derives the start date (explicit effective date or settlement-days
    /// dispatch), the default end-of-month flag, the end date, the two annual
    /// schedules and the fixed rate (given or fair-rate-filled), then attaches a
    /// [`DiscountingSwapEngine`].
    ///
    /// # Errors
    ///
    /// Returns an error when the start date must be derived but no evaluation
    /// date is set, and propagates the swap construction and (for a fair-rate
    /// fill) the pricing.
    pub fn build(self) -> QlResult<OvernightIndexedSwap> {
        if self.telescopic_value_dates {
            crate::fail!(
                "MakeOIS: telescopic value dates are not ported (deferred with the overnight leg); \
                 only the default false is accepted"
            );
        }
        if self.lookback_days.is_some() {
            crate::fail!(
                "MakeOIS: lookback days are not ported (deferred with the overnight leg); \
                 only the unset default is accepted"
            );
        }
        if self.lockout_days != 0 {
            crate::fail!(
                "MakeOIS: lockout days are not ported (deferred with the overnight leg); \
                 only the default 0 is accepted"
            );
        }
        if self.observation_shift {
            crate::fail!(
                "MakeOIS: observation shift is not ported (deferred with the overnight leg); \
                 only the default false is accepted"
            );
        }

        let calendar = self.overnight_index.fixing_calendar();

        let start_date = match self.effective_date {
            Some(effective_date) => effective_date,
            None => {
                let settlement_days = self
                    .settlement_days
                    .unwrap_or_else(|| default_settlement_days(self.overnight_index.family_name()));
                let ref_date = match self.settings.evaluation_date() {
                    Some(today) => calendar.adjust(today, BusinessDayConvention::Following),
                    None => crate::fail!(
                        "no evaluation date set: MakeOIS needs a reference date to derive the start date"
                    ),
                };
                let spot_date = calendar.advance(
                    ref_date,
                    settlement_days as Integer,
                    TimeUnit::Days,
                    BusinessDayConvention::Following,
                    false,
                );
                let start = spot_date + self.forward_start;
                if self.forward_start.length() < 0 {
                    calendar.adjust(start, BusinessDayConvention::Preceding)
                } else {
                    calendar.adjust(start, BusinessDayConvention::Following)
                }
            }
        };

        let start_is_end_of_month = calendar.is_end_of_month(start_date);
        let end_of_month = self.end_of_month.unwrap_or(start_is_end_of_month);

        let end_date = match self.termination_date {
            Some(termination_date) => termination_date,
            None => {
                let mut end = start_date + self.swap_tenor;
                if end_of_month && allows_end_of_month(self.swap_tenor) && start_is_end_of_month {
                    end = calendar.end_of_month(end);
                }
                end
            }
        };

        let schedule_tenor = Period::try_from(self.payment_frequency)
            .expect("a swap's payment frequency maps to a valid period");
        let schedule_calendar = calendar.clone();
        let schedule_convention = self.schedule_convention;
        let termination_date_convention = self.termination_date_convention;
        let rule = self.rule;
        let make_schedule = || {
            Schedule::new(
                start_date,
                end_date,
                schedule_tenor,
                schedule_calendar.clone(),
                schedule_convention,
                termination_date_convention,
                rule,
                end_of_month,
                Date::null(),
                Date::null(),
            )
        };
        let fixed_day_count = self
            .fixed_day_count
            .clone()
            .unwrap_or_else(|| self.overnight_index.day_counter().clone());

        let used_fixed_rate = match self.fixed_rate {
            Some(fixed_rate) => fixed_rate,
            None => {
                let mut temp = self.assemble(
                    0.0,
                    make_schedule(),
                    make_schedule(),
                    fixed_day_count.clone(),
                )?;
                temp.fixed_vs_floating_mut().fair_rate()?
            }
        };

        self.assemble(
            used_fixed_rate,
            make_schedule(),
            make_schedule(),
            fixed_day_count,
        )
    }

    /// Assembles an [`OvernightIndexedSwap`] over the two schedules at
    /// `fixed_rate` and attaches the discounting engine (`makeois.cpp:161-181`).
    fn assemble(
        &self,
        fixed_rate: Rate,
        fixed_schedule: Schedule,
        overnight_schedule: Schedule,
        fixed_day_count: DayCounter,
    ) -> QlResult<OvernightIndexedSwap> {
        let mut swap = OvernightIndexedSwap::with_nominal(
            self.swap_type,
            self.nominal,
            fixed_schedule,
            fixed_rate,
            fixed_day_count,
            overnight_schedule,
            Shared::clone(&self.overnight_index),
            self.overnight_spread,
            self.payment_lag,
            self.payment_adjustment,
            self.payment_calendar.clone(),
            self.averaging_method,
            Shared::clone(&self.settings),
        )?;

        let discount_curve = match &self.discounting_curve {
            Some(curve) => curve.clone(),
            None => self.overnight_index.forwarding_term_structure().clone(),
        };
        let engine = shared_mut(DiscountingSwapEngine::new(
            discount_curve,
            Some(false),
            None,
            None,
            Shared::clone(&self.settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

        Ok(swap)
    }
}

/// The default settlement days for an overnight index, keyed on family name
/// (`makeois.cpp:59-69`): Sonia 0, Corra 1, else 2. The comparison is
/// case-insensitive because QuantLib's family names vary in casing: `Sonia`
/// is mixed case (`sonia.cpp:28`) while `CORRA` is uppercase (`corra.cpp:26`).
fn default_settlement_days(family_name: &str) -> Natural {
    if family_name.eq_ignore_ascii_case("sonia") {
        0
    } else if family_name.eq_ignore_ascii_case("corra") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    //! The `MakeOIS` oracles from `test-suite/overnightindexedswap.cpp`, whose
    //! `CommonVars` fixture is transcribed from source (`:180-206`):
    //! `today = 5 February 2009`, `settlementDays = 2`, `nominal = 100`, an
    //! [`Estr`] index on the flat `estrTermStructure`, `settlement =
    //! TARGET.advance(today, 2 Days, Following)`, and the swap built through
    //! `MakeOIS(length, estrIndex, fixedRate, 0*Days)` with an explicit effective
    //! date of `settlement` (`makeSwap`, `:154-165`). No Estr fixings are
    //! pre-loaded: every overnight fixing is on or after `settlement >= today`, so
    //! all are forecast from the curve.
    //!
    //! The telescopic-value-dates assertions (each oracle asserts the identical
    //! number for a telescopic `swap2`) are omitted because telescopic value dates
    //! are deferred with the [`OvernightLeg`](crate::cashflows::OvernightLeg)
    //! (#328/#329); only the non-telescopic swap is reproduced.

    use super::*;
    use crate::indexes::ibor::Estr;
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;

    const NOMINAL: Real = 100.0;
    const SETTLEMENT_DAYS: Integer = 2;

    fn today() -> Date {
        Date::new(5, Month::February, 2009)
    }

    fn settings_at(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn settlement(settings: &Shared<Settings<Date>>) -> Date {
        Target::new().advance(
            settings.evaluation_date().unwrap(),
            SETTLEMENT_DAYS,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )
    }

    /// A flat curve wrapped in a handle, and an [`Estr`] index forecasting off it.
    fn estr_on(
        curve: Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> Shared<OvernightIndex> {
        shared(Estr::new(curve, Shared::clone(settings)))
    }

    /// `makeSwap(length, fixedRate, spread, false)` (`overnightindexedswap.cpp:154`):
    /// the effective date is `settlement`, the discounting curve is the index's.
    fn make_swap(
        length: Period,
        fixed_rate: Rate,
        spread: Spread,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<OvernightIndex>,
        settlement: Date,
        settings: &Shared<Settings<Date>>,
    ) -> OvernightIndexedSwap {
        MakeOis::new(
            length,
            index,
            Some(fixed_rate),
            Period::new(0, TimeUnit::Days),
            Shared::clone(settings),
        )
        .with_effective_date(settlement)
        .with_overnight_leg_spread(spread)
        .with_nominal(NOMINAL)
        .with_payment_lag(0)
        .with_discounting_term_structure(curve)
        .with_averaging_method(RateAveraging::Compound)
        .build()
        .unwrap()
    }

    /// `testCachedValue` (`:367`): a one-year Estr swap on a flat 5% curve struck
    /// at `exp(0.05) - 1` reproduces the cached NPV within 1e-11.
    #[test]
    fn cached_value() {
        let settings = settings_at(today());
        let settlement = settlement(&settings);
        let flat = 0.05;
        let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
            settlement,
            flat,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
            as Shared<dyn YieldTermStructure>);
        let index = estr_on(curve.clone(), &settings);
        let fixed_rate = flat.exp() - 1.0;

        let mut swap = make_swap(
            Period::new(1, TimeUnit::Years),
            fixed_rate,
            0.0,
            curve,
            index,
            settlement,
            &settings,
        );

        let cached_npv = 0.001730450147;
        assert!(
            (swap.npv().unwrap() - cached_npv).abs() < 1.0e-11,
            "cached NPV: got {}, expected {cached_npv}",
            swap.npv().unwrap()
        );
    }

    /// The `CommonVars` discounting curve (`overnightindexedswap.cpp:204`): flat
    /// 5% anchored at `today` on Actual/365 Fixed, both forecasting and
    /// discounting the same handle.
    fn common_curve() -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today(),
            0.05,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    const LENGTHS_YEARS: [Integer; 5] = [1, 2, 5, 10, 20];

    /// `testFairRate` (`:284`), non-telescopic branch: for each length and spread
    /// the swap rebuilt at its own fair rate prices to zero (`:305-311`). The
    /// telescopic `swap2` and its equality assertion are omitted (deferred leg).
    #[test]
    fn fair_rate() {
        let settings = settings_at(today());
        let settlement = settlement(&settings);
        let spreads = [-0.001, -0.01, 0.0, 0.01, 0.001];

        for years in LENGTHS_YEARS {
            for spread in spreads {
                let length = Period::new(years, TimeUnit::Years);
                let mut priced = make_swap(
                    length,
                    0.0,
                    spread,
                    common_curve(),
                    estr_on(common_curve(), &settings),
                    settlement,
                    &settings,
                );
                let fair = priced.fixed_vs_floating_mut().fair_rate().unwrap();

                let mut at_fair = make_swap(
                    length,
                    fair,
                    spread,
                    common_curve(),
                    estr_on(common_curve(), &settings),
                    settlement,
                    &settings,
                );
                assert!(
                    at_fair.npv().unwrap().abs() < 1.0e-10,
                    "{years}Y spread {spread}: NPV at fair rate {fair} is {}",
                    at_fair.npv().unwrap()
                );
            }
        }
    }

    /// `testFairSpread` (`:325`), non-telescopic branch: for each length and fixed
    /// rate the swap rebuilt at its own fair spread prices to zero (`:346-352`).
    /// The telescopic `swap2` and its equality assertion are omitted.
    #[test]
    fn fair_spread() {
        let settings = settings_at(today());
        let settlement = settlement(&settings);
        let rates = [0.04, 0.05, 0.06, 0.07];

        for years in LENGTHS_YEARS {
            for rate in rates {
                let length = Period::new(years, TimeUnit::Years);
                let mut priced = make_swap(
                    length,
                    rate,
                    0.0,
                    common_curve(),
                    estr_on(common_curve(), &settings),
                    settlement,
                    &settings,
                );
                let fair = priced.fixed_vs_floating_mut().fair_spread().unwrap();

                let mut at_fair = make_swap(
                    length,
                    rate,
                    fair,
                    common_curve(),
                    estr_on(common_curve(), &settings),
                    settlement,
                    &settings,
                );
                assert!(
                    at_fair.npv().unwrap().abs() < 1.0e-10,
                    "{years}Y rate {rate}: NPV at fair spread {fair} is {}",
                    at_fair.npv().unwrap()
                );
            }
        }
    }

    /// `withFixedLegDayCount` overrides the index-derived default: an OIS whose
    /// fixed leg accrues on Thirty360 rather than the Estr Actual360 prices to a
    /// different fair rate, since the fixed annuity changes with the day count.
    #[test]
    fn with_fixed_leg_day_count_changes_the_fixed_accrual() {
        use crate::time::daycounters::thirty360::{Convention, Thirty360};

        let settings = settings_at(today());
        let settlement = settlement(&settings);
        let length = Period::new(5, TimeUnit::Years);

        let default_day_count = MakeOis::new(
            length,
            estr_on(common_curve(), &settings),
            None,
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_effective_date(settlement)
        .with_nominal(NOMINAL)
        .build()
        .unwrap()
        .fixed_vs_floating_mut()
        .fair_rate()
        .unwrap();

        let thirty360 = MakeOis::new(
            length,
            estr_on(common_curve(), &settings),
            None,
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_effective_date(settlement)
        .with_nominal(NOMINAL)
        .with_fixed_leg_day_count(Thirty360::with_convention(Convention::BondBasis))
        .build()
        .unwrap()
        .fixed_vs_floating_mut()
        .fair_rate()
        .unwrap();

        assert!(
            (default_day_count - thirty360).abs() > 1.0e-6,
            "fixed-leg day count must change the fair rate: {default_day_count} vs {thirty360}"
        );
    }

    /// The family-name settlement-days dispatch (`makeois.cpp:59-69`): `Sonia` 0,
    /// `Corra` 1, else 2 - against the VERBATIM family names the C++ concretes
    /// carry: mixed-case `"Sonia"` (`sonia.cpp:28`) and uppercase `"CORRA"`
    /// (`corra.cpp:26`).
    #[test]
    fn settlement_days_dispatch_by_family() {
        assert_eq!(default_settlement_days("Sonia"), 0);
        assert_eq!(default_settlement_days("SONIA"), 0);
        assert_eq!(default_settlement_days("CORRA"), 1);
        assert_eq!(default_settlement_days("Corra"), 1);
        assert_eq!(default_settlement_days("ESTR"), 2);
        assert_eq!(default_settlement_days("SOFR"), 2);
        assert_eq!(default_settlement_days("anything else"), 2);
    }

    /// Without an explicit effective date, the start date is derived by the
    /// dispatch (`makeois.cpp:57-82`): for `Estr` (family `"ESTR"`, so 2 days) the
    /// derived start equals `settlement = TARGET.advance(today, 2 Days,
    /// Following)`, the same date the oracles set explicitly.
    #[test]
    fn derived_start_date_matches_settlement() {
        let settings = settings_at(today());
        let settlement = settlement(&settings);
        let swap = MakeOis::new(
            Period::new(1, TimeUnit::Years),
            estr_on(common_curve(), &settings),
            Some(0.03),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_nominal(NOMINAL)
        .build()
        .unwrap();

        assert_eq!(swap.overnight_schedule().start_date(), settlement);
    }

    /// The four #262-safe guards reject a non-default value at build time rather
    /// than accepting and silently ignoring the unported feature. The benign
    /// default of each still builds (the `initialize_dates` chain passes them).
    #[test]
    fn unported_knobs_reject_non_default_values() {
        let settings = settings_at(today());
        let settlement = settlement(&settings);

        let base = |settings: &Shared<Settings<Date>>| {
            MakeOis::new(
                Period::new(1, TimeUnit::Years),
                estr_on(common_curve(), settings),
                Some(0.03),
                Period::new(0, TimeUnit::Days),
                Shared::clone(settings),
            )
            .with_effective_date(settlement)
            .with_nominal(NOMINAL)
        };

        assert!(
            base(&settings)
                .with_telescopic_value_dates(true)
                .build()
                .is_err(),
            "telescopic value dates must be rejected"
        );
        assert!(
            base(&settings).with_lookback_days(Some(5)).build().is_err(),
            "a set lookback must be rejected"
        );
        assert!(
            base(&settings).with_lockout_days(5).build().is_err(),
            "nonzero lockout must be rejected"
        );
        assert!(
            base(&settings)
                .with_observation_shift(true)
                .build()
                .is_err(),
            "observation shift must be rejected"
        );

        assert!(
            base(&settings)
                .with_telescopic_value_dates(false)
                .with_lookback_days(None)
                .with_lockout_days(0)
                .with_observation_shift(false)
                .build()
                .is_ok(),
            "the benign defaults must still build"
        );
    }

    /// `with_settlement_days` overrides the family default: setting 5 days moves
    /// the derived start date past the 2-day `Estr` dispatch default.
    #[test]
    fn settlement_days_override_moves_the_start_date() {
        let settings = settings_at(today());
        let default_start = MakeOis::new(
            Period::new(1, TimeUnit::Years),
            estr_on(common_curve(), &settings),
            Some(0.03),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_nominal(NOMINAL)
        .build()
        .unwrap()
        .overnight_schedule()
        .start_date();

        let overridden_start = MakeOis::new(
            Period::new(1, TimeUnit::Years),
            estr_on(common_curve(), &settings),
            Some(0.03),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_nominal(NOMINAL)
        .with_settlement_days(5)
        .build()
        .unwrap()
        .overnight_schedule()
        .start_date();

        assert!(
            overridden_start > default_start,
            "a larger settlement-days override starts later: {overridden_start} vs {default_start}"
        );
    }
}
