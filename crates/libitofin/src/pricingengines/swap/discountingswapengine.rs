//! Discounting swap pricing engine.
//!
//! Port of `ql/pricingengines/swap/discountingswapengine.{hpp,cpp}`:
//! [`DiscountingSwapEngine`] discounts each of a swap's legs over a
//! [`YieldTermStructure`] into the [`SwapResults`] the base
//! [`Swap`](crate::instruments::Swap) reads its leg NPVs, leg BPS and framing
//! discount factors from. It is a `Swap::engine` (`discountingswapengine.hpp:39`),
//! so it prices any swap, and reuses the combined [`CashFlows::npvbps`] pass
//! (#250) per leg. Changes to the discount-curve handle invalidate the attached
//! swap through the usual observable chain.
//!
//! Deviations, documented per D5/D10:
//! - The C++ global `Settings::instance()` becomes an explicit [`Settings`]
//!   handle the engine is built with; it drives the `includeReferenceDateEvents`
//!   fall back for the reference-date flow decision when the optional
//!   `include_settlement_date_flows` ctor argument is unset.
//! - The C++ `Date()` "unset" sentinel for the settlement and NPV dates becomes
//!   [`Option<Date>`]: `None` falls back to the curve reference date.
//! - A leg date before the curve reference date has no discount; the C++
//!   `Null<DiscountFactor>()` sentinel becomes [`Real::null`].
//! - A leg-level pricing failure is prefixed with the leg's index rather than
//!   the C++ ordinal (`io::ordinal`, not ported).

use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::instruments::{SwapArguments, SwapEngine, SwapResults};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::DiscountFactor;
use crate::utilities::null::Null;
use crate::{fail, handle::Handle, require};

/// Discounting engine for swaps.
///
/// Discounts each leg's future cash flows to the discount curve's reference
/// date, filling the per-leg NPV and BPS the base swap prices from.
pub struct DiscountingSwapEngine {
    base: SwapEngine,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    settlement_date: Option<Date>,
    npv_date: Option<Date>,
    settings: Shared<Settings<Date>>,
}

impl DiscountingSwapEngine {
    /// Builds the engine over a discount-curve handle it registers with.
    ///
    /// `include_settlement_date_flows` overrides, when set, the settings'
    /// `include_reference_date_events` flag for the reference-date flow
    /// decision (the C++ `includeSettlementDateFlows` optional). `settlement_date`
    /// and `npv_date` default, when `None`, to the curve reference date (the C++
    /// `Date()` sentinel).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
        settings: Shared<Settings<Date>>,
    ) -> DiscountingSwapEngine {
        let base = SwapEngine::new(SwapArguments::default(), SwapResults::default());
        discount_curve.register_observer(&base.observer());
        DiscountingSwapEngine {
            base,
            discount_curve,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
            settings,
        }
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for DiscountingSwapEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for DiscountingSwapEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        require!(
            !self.discount_curve.is_empty(),
            "discounting term structure handle is empty"
        );
        let curve = self.discount_curve.current_link()?;
        let ref_date = curve.reference_date()?;

        let settlement_date = match self.settlement_date {
            None => ref_date,
            Some(date) => {
                require!(
                    date >= ref_date,
                    "settlement date ({date}) before discount curve reference date ({ref_date})"
                );
                date
            }
        };

        let valuation_date = match self.npv_date {
            None => ref_date,
            Some(date) => {
                require!(
                    date >= ref_date,
                    "npv date ({date}) before discount curve reference date ({ref_date})"
                );
                date
            }
        };
        let npv_date_discount = curve.discount_date(valuation_date, false)?;

        let include_ref_date_flows = self
            .include_settlement_date_flows
            .unwrap_or_else(|| self.settings.include_reference_date_events());

        let arguments = self.base.arguments();
        let n = arguments.legs.len();
        let mut leg_npv = Vec::with_capacity(n);
        let mut leg_bps = Vec::with_capacity(n);
        let mut start_discounts = Vec::with_capacity(n);
        let mut end_discounts = Vec::with_capacity(n);
        let mut value = 0.0;

        for (i, leg) in arguments.legs.iter().enumerate() {
            let (npv, bps) = match CashFlows::npvbps(
                leg,
                &*curve,
                &self.settings,
                Some(include_ref_date_flows),
                Some(settlement_date),
                Some(valuation_date),
            ) {
                Ok(pair) => pair,
                Err(e) => fail!("leg #{}: {}", i + 1, e.message()),
            };
            let npv = npv * arguments.payer[i];
            let bps = bps * arguments.payer[i];

            if leg.is_empty() {
                start_discounts.push(DiscountFactor::null());
                end_discounts.push(DiscountFactor::null());
            } else {
                let d1 = CashFlows::start_date(leg)?;
                start_discounts.push(if d1 >= ref_date {
                    curve.discount_date(d1, false)?
                } else {
                    DiscountFactor::null()
                });
                let d2 = CashFlows::maturity_date(leg)?;
                end_discounts.push(if d2 >= ref_date {
                    curve.discount_date(d2, false)?
                } else {
                    DiscountFactor::null()
                });
            }

            leg_npv.push(npv);
            leg_bps.push(bps);
            value += npv;
        }

        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.instrument.error_estimate = None;
        results.instrument.valuation_date = Some(valuation_date);
        results.npv_date_discount = Some(npv_date_discount);
        results.leg_npv = leg_npv;
        results.leg_bps = leg_bps;
        results.start_discounts = start_discounts;
        results.end_discounts = end_discounts;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! The swap's numeric oracle: `swap.cpp` `testCachedValue` (:289), the first
    //! swap priced end to end against a hardcoded C++ NPV, plus the mode-agnostic
    //! `testFairRate` (:107) and `testFairSpread` (:131) self-consistency checks.
    //! The fixture reproduces `swap.cpp` `CommonVars` (:52-104): a Payer swap on a
    //! nominal of 100, fixed 10Y annual Thirty360(BondBasis) versus floating
    //! semiannual Euribor 6M / Actual360, discounted on a flat 5% Actual365Fixed
    //! curve the index also forecasts off.

    use super::*;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::instrument::Instrument;
    use crate::instruments::{SwapType, VanillaSwap};
    use crate::interestrate::Compounding;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Rate, Real, Spread};

    const NOMINAL: Real = 100.0;

    /// The `swap.cpp` `CommonVars` fixture (:87-104): a TARGET calendar, a
    /// two-day settlement, a flat 5% Actual365Fixed curve fixed at settlement,
    /// and a Euribor 6M index forecasting off that same curve.
    struct Vars {
        settings: Shared<Settings<Date>>,
        calendar: Calendar,
        settlement: Date,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<IborIndex>,
    }

    impl Vars {
        fn new(today: Date, using_at_par: bool) -> Vars {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today);
            settings.set_using_at_par_coupons(using_at_par);
            let calendar = Target::new();
            let settlement = calendar.advance(
                today,
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.05,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            ))
                as Shared<dyn YieldTermStructure>);
            let index = shared(Euribor::six_months(curve.clone(), Shared::clone(&settings)));
            Vars {
                settings,
                calendar,
                settlement,
                curve,
                index,
            }
        }

        /// The `swap.cpp` `makeSwap` (:65-83): a `length`-year Payer swap priced
        /// through the discounting engine over the fixture's curve.
        fn make_swap(&self, length: Integer, fixed_rate: Rate, spread: Spread) -> VanillaSwap {
            let maturity = self.calendar.advance(
                self.settlement,
                length,
                TimeUnit::Years,
                BusinessDayConvention::ModifiedFollowing,
                false,
            );
            let fixed_schedule = MakeSchedule::new()
                .from(self.settlement)
                .to(maturity)
                .with_frequency(Frequency::Annual)
                .with_calendar(self.calendar.clone())
                .with_convention(BusinessDayConvention::Unadjusted)
                .with_termination_date_convention(BusinessDayConvention::Unadjusted)
                .forwards()
                .end_of_month(false)
                .build();
            let float_schedule = MakeSchedule::new()
                .from(self.settlement)
                .to(maturity)
                .with_frequency(Frequency::Semiannual)
                .with_calendar(self.calendar.clone())
                .with_convention(BusinessDayConvention::ModifiedFollowing)
                .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
                .forwards()
                .end_of_month(false)
                .build();
            let mut swap = VanillaSwap::new(
                SwapType::Payer,
                NOMINAL,
                fixed_schedule,
                fixed_rate,
                Thirty360::with_convention(Convention::BondBasis),
                float_schedule,
                Shared::clone(&self.index),
                spread,
                Actual360::new(),
                None,
                Shared::clone(&self.settings),
            )
            .unwrap();
            let engine = shared_mut(DiscountingSwapEngine::new(
                self.curve.clone(),
                None,
                None,
                None,
                Shared::clone(&self.settings),
            ));
            swap.base_mut()
                .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
            swap
        }
    }

    /// `swap.cpp` `testCachedValue` (:289): a 10Y Payer swap at fixed 6% and a
    /// 0.001 floating spread reproduces the cached NPV at 1e-11. The value splits
    /// on the par/indexed mode (`swap.cpp:311`): the PAR arm (default Settings)
    /// is `-5.872863313209`, the INDEXED arm `-5.872342992212`. Parameterising on
    /// the flag pins the mode split end to end.
    #[test]
    fn cached_value_reproduces_the_par_and_indexed_arms() {
        for (using_at_par, expected) in [(true, -5.872863313209), (false, -5.872342992212)] {
            let vars = Vars::new(Date::new(17, Month::June, 2002), using_at_par);
            let mut swap = vars.make_swap(10, 0.06, 0.001);

            let npv = swap.npv().unwrap();
            assert!(
                (npv - expected).abs() <= 1.0e-11,
                "par={using_at_par}: npv {npv} vs cached {expected} (error {})",
                (npv - expected).abs()
            );
        }
    }

    /// `swap.cpp` `testFairRate` (:107): a swap rebuilt at its own `fairRate()`
    /// prices to zero. Mode-agnostic self-consistency across lengths and spreads.
    #[test]
    fn a_swap_rebuilt_at_its_fair_rate_prices_to_zero() {
        let vars = Vars::new(Date::new(17, Month::June, 2002), true);
        let lengths: [Integer; 5] = [1, 2, 5, 10, 20];
        let spreads: [Spread; 5] = [-0.001, -0.01, 0.0, 0.01, 0.001];

        for length in lengths {
            for spread in spreads {
                let fair = vars
                    .make_swap(length, 0.0, spread)
                    .fixed_vs_floating_mut()
                    .fair_rate()
                    .unwrap();
                let npv = vars.make_swap(length, fair, spread).npv().unwrap();
                assert!(
                    npv.abs() <= 1.0e-10,
                    "length {length}y spread {spread}: npv {npv} not zero"
                );
            }
        }
    }

    /// `swap.cpp` `testFairSpread` (:131): a swap rebuilt at its own
    /// `fairSpread()` prices to zero. This is the only coverage of the
    /// floating-leg-BPS branch of the fair-value fallback.
    #[test]
    fn a_swap_rebuilt_at_its_fair_spread_prices_to_zero() {
        let vars = Vars::new(Date::new(17, Month::June, 2002), true);
        let lengths: [Integer; 5] = [1, 2, 5, 10, 20];
        let rates: [Rate; 4] = [0.04, 0.05, 0.06, 0.07];

        for length in lengths {
            for rate in rates {
                let fair = vars
                    .make_swap(length, rate, 0.0)
                    .fixed_vs_floating_mut()
                    .fair_spread()
                    .unwrap();
                let npv = vars.make_swap(length, rate, fair).npv().unwrap();
                assert!(
                    npv.abs() <= 1.0e-10,
                    "length {length}y rate {rate}: npv {npv} not zero"
                );
            }
        }
    }

    /// An empty discount-curve handle is rejected before any discounting, as the
    /// C++ `QL_REQUIRE(!discountCurve_.empty(), ...)` (`discountingswapengine.cpp:41`).
    #[test]
    fn an_empty_discount_curve_is_rejected() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(17, Month::June, 2002));
        let mut engine = DiscountingSwapEngine::new(Handle::empty(), None, None, None, settings);
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "discounting term structure handle is empty"
        );
    }
}
