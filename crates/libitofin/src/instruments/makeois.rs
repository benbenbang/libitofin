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
//! The builder exposes only the overrides the oracles use:
//! [`with_effective_date`](MakeOis::with_effective_date),
//! [`with_overnight_leg_spread`](MakeOis::with_overnight_leg_spread),
//! [`with_nominal`](MakeOis::with_nominal),
//! [`with_payment_lag`](MakeOis::with_payment_lag),
//! [`with_discounting_term_structure`](MakeOis::with_discounting_term_structure)
//! and [`with_averaging_method`](MakeOis::with_averaging_method). The swap tenor,
//! overnight index, optional fixed rate and forward start are the constructor
//! arguments (`makeois.hpp:40`).
//!
//! ## Deferred knobs
//!
//! Every other `with*` on `makeois.hpp` is deferred, defaulting to the C++
//! default (`makeois.hpp:96-137`):
//!
//! - swap type (`receiveFixed` / `withType`): defaults to `Payer`;
//! - `withSettlementDays`: the settlement-days dispatch feeds the start date only
//!   when no effective date is set (which the oracles always set); it defaults by
//!   index family (see below) and cannot be overridden;
//! - `withTerminationDate`, `withRule` / leg variants, `withPaymentFrequency` /
//!   leg variants, `withPaymentAdjustment`, `withPaymentCalendar` / `withCalendar`
//!   / leg variants, `withConvention` / termination / leg variants,
//!   `withEndOfMonth` / leg / maturity variants, `withFixedLegDayCount`: the
//!   schedules use the C++ defaults (annual `Backward`, `ModifiedFollowing`,
//!   default end-of-month, fixed day count from the index);
//! - `withTelescopicValueDates`: telescopic value dates are deferred with the
//!   [`OvernightLeg`](crate::cashflows::OvernightLeg) (#328/#329), so the knob is
//!   omitted rather than accepted and ignored. The cached NPV is identical for
//!   telescopic and non-telescopic value dates by construction (C++ asserts the
//!   same number for both, `overnightindexedswap.cpp:382/386`), so only the
//!   non-telescopic swap is reproduced;
//! - `withLookbackDays`, `withLockoutDays`, `withObservationShift`: deferred with
//!   the leg;
//! - `withPricingEngine`: the engine is always the
//!   [`DiscountingSwapEngine`] over the discounting curve (set) or the index's
//!   forwarding curve (default), matching `makeois.cpp:145-156/172-180`.
//!
//! ## Settlement-days dispatch onto the newtype
//!
//! C++ dispatches the default settlement days by dynamic type
//! (`makeois.cpp:59-69`): `Sonia` 0, `Corra` 1, else 2. The port's
//! [`OvernightIndex`] is a newtype with no subtype, so the dispatch keys on the
//! index family name instead: `"SONIA"` 0, `"CORRA"` 1, else 2. Only `Estr`
//! (family `"ESTR"`, so 2) is ported today; the mapping is future-proof for when
//! the other overnight indexes land.

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
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
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
    discounting_curve: Option<Handle<dyn YieldTermStructure>>,
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
            discounting_curve: None,
        }
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
        let calendar = self.overnight_index.fixing_calendar();

        let start_date = match self.effective_date {
            Some(effective_date) => effective_date,
            None => {
                let settlement_days = default_settlement_days(self.overnight_index.family_name());
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

        let end_of_month = calendar.is_end_of_month(start_date);

        let mut end_date = start_date + self.swap_tenor;
        if end_of_month && allows_end_of_month(self.swap_tenor) {
            end_date = calendar.end_of_month(end_date);
        }

        let schedule_tenor = Period::new(1, TimeUnit::Years);
        let make_schedule = || {
            Schedule::new(
                start_date,
                end_date,
                schedule_tenor,
                calendar.clone(),
                BusinessDayConvention::ModifiedFollowing,
                BusinessDayConvention::ModifiedFollowing,
                DateGeneration::Backward,
                end_of_month,
                Date::null(),
                Date::null(),
            )
        };
        let fixed_day_count = self.overnight_index.day_counter().clone();

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
            None,
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
/// (`makeois.cpp:59-69`): `"SONIA"` 0, `"CORRA"` 1, else 2.
fn default_settlement_days(family_name: &str) -> Natural {
    match family_name {
        "SONIA" => 0,
        "CORRA" => 1,
        _ => 2,
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

    /// The family-name settlement-days dispatch (`makeois.cpp:59-69`): `Sonia` 0,
    /// `Corra` 1, else 2.
    #[test]
    fn settlement_days_dispatch_by_family() {
        assert_eq!(default_settlement_days("SONIA"), 0);
        assert_eq!(default_settlement_days("CORRA"), 1);
        assert_eq!(default_settlement_days("ESTR"), 2);
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
}
