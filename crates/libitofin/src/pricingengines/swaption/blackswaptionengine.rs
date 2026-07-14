//! Black-formula swaption engine.
//!
//! Port of `ql/pricingengines/swaption/blackswaptionengine.{hpp}`:
//! [`BlackSwaptionEngine`] prices a [`Swaption`](crate::instruments::Swaption)
//! as a Black 1976 option on the underlying swap's fair rate, using a
//! [`SwaptionVolatilityStructure`] for the variance and shift and the swap's
//! annuity for the discounting (`blackswaptionengine.hpp:220-327`). It installs
//! a [`DiscountingSwapEngine`] on the swap, reads its `valuationDate`,
//! `fairRate` and leg BPS, and feeds those into [`black_formula`].
//!
//! Only the shifted-lognormal (`Black76`) instantiation of the C++ `Spec`
//! template is ported here; the Bachelier (`Normal`) engine and the swaption
//! delta oracle land with #365.
//!
//! ## Divergences from QuantLib
//!
//! - **The C++ engine mutates its own argument.** It installs the discounting
//!   engine on the swap it was handed, wrapped in
//!   `ObservableSettings::instance().disableUpdates()` /
//!   `enableUpdates()` (`blackswaptionengine.hpp:247-249`) so the swaption
//!   observing that swap is not invalidated mid-calculation. `ObservableSettings`
//!   is a global singleton with no Rust counterpart (deliberately not ported,
//!   D5). The port expresses the same suppression through
//!   [`InstrumentBase::set_pricing_engine_silent`](crate::instrument::InstrumentBase::set_pricing_engine_silent):
//!   a targeted non-broadcasting install that invalidates the swap locally (so
//!   it reprices on the engine's discount curve) without notifying the swaption.
//!   The swap is borrowed mutably only to install and price; no notification
//!   runs while that borrow is live, honouring the D1 re-entrancy rule.
//! - **No `firstCoupon` downcast.** C++ `dynamic_pointer_cast<FixedRateCoupon>`
//!   (`:236`) only calls `accrualStartDate()` and `dayCounter()` on the result,
//!   both `Coupon`-level, so the port reads them through
//!   [`CashFlow::as_coupon`](crate::cashflow::CashFlow::as_coupon).
//! - **`CashAnnuityModel` has no default.** The C++ header defaults to
//!   [`DiscountCurve`](CashAnnuityModel::DiscountCurve) (`:143`) but every
//!   ported test passes [`SwapRate`](CashAnnuityModel::SwapRate)
//!   (`swaption.cpp:85`), so every test takes the `valuation_date` branch and
//!   the `DiscountCurve` branch (first coupon accrual start) is UNPINNED by the
//!   oracle. It is ported but untested.
//! - **The `Cash && CollateralizedCashPrice` annuity arm** has no oracle in the
//!   swaption test file (only the delta helper at `:1057`, pinned by #365). It
//!   is ported but unexercised here.
//! - Only the `Handle<Quote>` and `Handle<SwaptionVolatilityStructure>`
//!   constructors are ported; the flat-`Volatility` convenience constructor
//!   (`:57`) is subsumed by [`with_flat_vol`](BlackSwaptionEngine::with_flat_vol)
//!   taking a quote handle, matching how the tests build the engine.

use std::any::Any;

use crate::cashflow::Leg;
use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::handle::Handle;
use crate::instrument::{Instrument, InstrumentResults};
use crate::instruments::{
    SettlementMethod, SettlementType, SwapType, SwaptionArguments, SwaptionEngine,
};
use crate::interestrate::{Compounding, InterestRate};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::DiscountingSwapEngine;
use crate::pricingengines::blackformula::{
    black_formula, black_formula_forward_derivative, black_formula_std_dev_derivative,
};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{
    ConstantSwaptionVolatility, SwaptionVolatilityStructure, VolatilityType,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::Real;
use crate::{fail, require};

/// One basis point, the unit the fixed-leg BPS annuity divides by
/// (`blackswaptionengine.hpp:222`).
const BASIS_POINT: Real = 1.0e-4;

/// How the cash-settled par-yield annuity picks its discount date
/// (`BlackStyleSwaptionEngine::CashAnnuityModel`, `blackswaptionengine.hpp:56`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CashAnnuityModel {
    /// Discount at the swap's valuation date (the fixture's choice,
    /// `swaption.cpp:85`).
    SwapRate,
    /// Discount at the first fixed coupon's accrual start date (the C++ header
    /// default; unpinned by the ported oracle).
    DiscountCurve,
}

/// Shifted-lognormal Black-formula swaption engine
/// (`blackswaptionengine.hpp:136`).
pub struct BlackSwaptionEngine {
    base: SwaptionEngine,
    discount_curve: Handle<dyn YieldTermStructure>,
    vol: Handle<dyn SwaptionVolatilityStructure>,
    model: CashAnnuityModel,
    settings: Shared<Settings<Date>>,
}

impl BlackSwaptionEngine {
    /// Builds the engine over a discount curve and a swaption volatility surface
    /// (the C++ `Handle<SwaptionVolatilityStructure>` constructor,
    /// `blackswaptionengine.hpp:149`).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn SwaptionVolatilityStructure>,
        model: CashAnnuityModel,
        settings: Shared<Settings<Date>>,
    ) -> BlackSwaptionEngine {
        let base = SwaptionEngine::new(SwaptionArguments::default(), InstrumentResults::default());
        discount_curve.register_observer(&base.observer());
        vol.register_observer(&base.observer());
        BlackSwaptionEngine {
            base,
            discount_curve,
            vol,
            model,
            settings,
        }
    }

    /// Builds the engine over a flat volatility quote, wrapping it in a moving
    /// [`ConstantSwaptionVolatility`] on a null calendar (the C++ `Handle<Quote>`
    /// constructor, `blackswaptionengine.hpp:144` / `:189`).
    pub fn with_flat_vol(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn Quote>,
        day_counter: DayCounter,
        displacement: Real,
        model: CashAnnuityModel,
        settings: Shared<Settings<Date>>,
    ) -> BlackSwaptionEngine {
        let surface = ConstantSwaptionVolatility::moving_with_quote(
            0,
            NullCalendar::new(),
            BusinessDayConvention::Following,
            vol,
            day_counter,
            VolatilityType::ShiftedLognormal,
            displacement,
            Shared::clone(&settings),
        );
        let vol = Handle::new(shared(surface) as Shared<dyn SwaptionVolatilityStructure>);
        BlackSwaptionEngine::new(discount_curve, vol, model, settings)
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for BlackSwaptionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BlackSwaptionEngine {
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
        // Read the swaption arguments, then release the borrow before pricing.
        let (exercise, swap, settlement_type, settlement_method) = {
            let arguments = self.base.arguments();
            let Some(exercise) = arguments.exercise.as_ref() else {
                fail!("exercise not set");
            };
            let Some(swap) = arguments.swap.as_ref() else {
                fail!("swap not set");
            };
            (
                Shared::clone(exercise),
                SharedMut::clone(swap),
                arguments.settlement_type,
                arguments.settlement_method,
            )
        };

        require!(
            exercise.exercise_type() == ExerciseType::European,
            "not a European option"
        );
        let exercise_date = exercise.dates()[0];

        let discount = self.discount_curve.current_link()?;
        let vol = self.vol.current_link()?;

        // Install a discounting engine on the swap and price it. The install is
        // silent so the swaption observing this swap is not invalidated
        // mid-calculation (the C++ disableUpdates() guard, expressed per D5).
        let discounting_engine = shared_mut(DiscountingSwapEngine::new(
            self.discount_curve.clone(),
            Some(false),
            None,
            None,
            Shared::clone(&self.settings),
        )) as SharedMut<dyn PricingEngine>;

        let (
            valuation_date,
            atm_forward,
            fixed_rate,
            swap_type,
            fixed_leg_bps,
            floating_leg_bps,
            spread,
            fixed_leg,
            first_accrual_start,
            first_day_count,
            fixed_frequency,
            floating_front,
            floating_back,
        ) = {
            let mut swap_ref = swap.borrow_mut();

            let (first_accrual_start, first_day_count) = {
                let leg = swap_ref.fixed_leg();
                let Some(first) = leg.first() else {
                    fail!("swap has no fixed leg");
                };
                let Some(coupon) = first.as_coupon() else {
                    fail!("first fixed cash flow is not a coupon");
                };
                (coupon.accrual_start_date(), coupon.day_counter())
            };
            require!(
                first_accrual_start >= exercise_date,
                "swap start ({first_accrual_start}) before exercise date ({exercise_date}) \
                 not supported in Black swaption engine"
            );

            let fixed_rate = swap_ref.fixed_rate();
            let spread = swap_ref.spread();
            let swap_type = swap_ref.swap_type();
            let fixed_leg: Leg = swap_ref.fixed_leg().to_vec();
            let fixed_frequency = if swap_ref.fixed_schedule().has_tenor() {
                swap_ref.fixed_schedule().tenor().frequency()
            } else {
                Frequency::Annual
            };
            let (floating_front, floating_back) = {
                let dates = swap_ref.floating_schedule().dates();
                (dates[0], dates[dates.len() - 1])
            };

            swap_ref
                .base_mut()
                .set_pricing_engine_silent(discounting_engine);
            let valuation_date = swap_ref.valuation_date()?;
            let atm_forward = swap_ref.fair_rate()?;
            let fixed_leg_bps = swap_ref.fixed_leg_bps()?;
            let floating_leg_bps = swap_ref.floating_leg_bps()?;

            (
                valuation_date,
                atm_forward,
                fixed_rate,
                swap_type,
                fixed_leg_bps,
                floating_leg_bps,
                spread,
                fixed_leg,
                first_accrual_start,
                first_day_count,
                fixed_frequency,
                floating_front,
                floating_back,
            )
        };

        // Volatilities are quoted for zero-spreaded swaps, so a floating-leg
        // spread is removed with a matching correction on the fixed rate
        // (`blackswaptionengine.hpp:253-265`).
        let mut strike = fixed_rate;
        let mut atm_forward = atm_forward;
        let spread_correction = if spread != 0.0 {
            let correction = spread * (floating_leg_bps / fixed_leg_bps).abs();
            strike -= correction;
            atm_forward -= correction;
            correction
        } else {
            0.0
        };

        let annuity = match (settlement_type, settlement_method) {
            (SettlementType::Physical, _)
            | (SettlementType::Cash, SettlementMethod::CollateralizedCashPrice) => {
                fixed_leg_bps.abs() / BASIS_POINT
            }
            (SettlementType::Cash, SettlementMethod::ParYieldCurve) => {
                // The cash settlement date is assumed equal to the swap start.
                let discount_date = match self.model {
                    CashAnnuityModel::DiscountCurve => first_accrual_start,
                    CashAnnuityModel::SwapRate => valuation_date,
                };
                let yield_rate = InterestRate::new(
                    atm_forward,
                    first_day_count,
                    Compounding::Compounded,
                    fixed_frequency,
                )?;
                // Positional C++ call: discountDate lands in the settlement-date
                // slot; the npv date defaults to it (`blackswaptionengine.hpp:288`).
                let fixed_leg_cash_bps = CashFlows::bps_at_yield(
                    &fixed_leg,
                    &yield_rate,
                    &self.settings,
                    Some(false),
                    Some(discount_date),
                    None,
                )?;
                (fixed_leg_cash_bps / BASIS_POINT).abs()
                    * discount.discount_date(discount_date, false)?
            }
            _ => fail!("invalid (settlementType, settlementMethod) pair"),
        };

        // swapLength is rounded to whole months, so it is floored at 1/12 to
        // ensure a variance and shift can be read (`blackswaptionengine.hpp:303-305`).
        let swap_length = vol
            .swap_length(floating_front, floating_back)?
            .max(1.0 / 12.0);
        let variance = vol.black_variance(exercise_date, swap_length, strike, false)?;
        let displacement = if vol.volatility_type() == VolatilityType::ShiftedLognormal {
            vol.shift(exercise_date, swap_length, false)?
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let option_type = if swap_type == SwapType::Payer {
            OptionType::Call
        } else {
            OptionType::Put
        };
        let value = black_formula(
            option_type,
            strike,
            atm_forward,
            std_dev,
            annuity,
            displacement,
        )?;

        let exercise_time = vol.time_from_reference(exercise_date)?;
        let vega = exercise_time.sqrt()
            * black_formula_std_dev_derivative(
                strike,
                atm_forward,
                std_dev,
                annuity,
                displacement,
            )?;
        let delta = black_formula_forward_derivative(
            option_type,
            strike,
            atm_forward,
            std_dev,
            annuity,
            displacement,
        )?;
        let implied_volatility = std_dev / exercise_time.sqrt();
        let forward_price = value / discount.discount_date(exercise_date, false)?;

        drop(vol);
        drop(discount);

        let results = self.base.results_mut();
        results.value = Some(value);
        results.error_estimate = None;
        results.valuation_date = Some(valuation_date);
        let extras = &mut results.additional_results;
        extras.insert(
            "spreadCorrection".into(),
            shared(spread_correction) as Shared<dyn Any>,
        );
        extras.insert("strike".into(), shared(strike) as Shared<dyn Any>);
        extras.insert("atmForward".into(), shared(atm_forward) as Shared<dyn Any>);
        extras.insert("annuity".into(), shared(annuity) as Shared<dyn Any>);
        extras.insert("swapLength".into(), shared(swap_length) as Shared<dyn Any>);
        extras.insert("stdDev".into(), shared(std_dev) as Shared<dyn Any>);
        extras.insert("vega".into(), shared(vega) as Shared<dyn Any>);
        extras.insert("delta".into(), shared(delta) as Shared<dyn Any>);
        extras.insert(
            "timeToExpiry".into(),
            shared(exercise_time) as Shared<dyn Any>,
        );
        extras.insert(
            "impliedVolatility".into(),
            shared(implied_volatility) as Shared<dyn Any>,
        );
        extras.insert(
            "forwardPrice".into(),
            shared(forward_price) as Shared<dyn Any>,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! `test-suite/swaption.cpp`'s Black-engine oracle. The fixture reproduces
    //! `CommonVars` (`:60-143`): settlement two TARGET days out, a flat 5%
    //! Actual365Fixed curve fixed at settlement that both forecasts the Euribor
    //! 6M index and discounts the swaptions, a nominal of one million and an
    //! annual Thirty360 BondBasis fixed leg. `makeSwaption` (`:79`) builds the
    //! Black engine over a flat vol quote with the `SwapRate` cash-annuity model.

    use super::*;
    use crate::exercise::EuropeanExercise;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::{Eonia, Euribor};
    use crate::instrument::Instrument;
    use crate::instruments::{
        FixedVsFloatingSwap, MakeOis, MakeVanillaSwap, Swaption, VanillaSwap,
    };
    use crate::quotes::make_quote_handle;
    use crate::shared::shared;
    use crate::termstructures::yields::{FlatForward, ZeroSpreadedTermStructure};
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::period::Period;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Rate, Spread, Volatility};

    const SETTLEMENT_DAYS: Integer = 2;

    /// The `swaption.cpp` `CommonVars` fixture, parameterised on the evaluation
    /// date and the par/indexed coupon flag.
    struct Vars {
        settings: Shared<Settings<Date>>,
        calendar: Calendar,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<IborIndex>,
        today: Date,
        settlement: Date,
    }

    impl Vars {
        fn new(today: Date, using_at_par: bool) -> Vars {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today);
            settings.set_using_at_par_coupons(using_at_par);
            let calendar = Target::new();
            let settlement = calendar.advance(
                today,
                SETTLEMENT_DAYS,
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
                curve,
                index,
                today,
                settlement,
            }
        }

        fn fixed_day_count() -> DayCounter {
            Thirty360::with_convention(Convention::BondBasis)
        }

        fn years(&self, from: Date, n: Integer) -> Date {
            self.calendar.advance(
                from,
                n,
                TimeUnit::Years,
                BusinessDayConvention::Following,
                false,
            )
        }

        fn spot(&self, from: Date) -> Date {
            self.calendar.advance(
                from,
                SETTLEMENT_DAYS,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            )
        }

        /// `makeSwaption` (`swaption.cpp:79`): the Black engine over a flat vol
        /// quote on the fixture curve, with the `SwapRate` annuity model.
        fn make_swaption(
            &self,
            swap: FixedVsFloatingSwap,
            exercise_date: Date,
            volatility: Volatility,
            settlement_type: SettlementType,
            settlement_method: SettlementMethod,
        ) -> Swaption {
            let engine = shared_mut(BlackSwaptionEngine::with_flat_vol(
                self.curve.clone(),
                make_quote_handle(volatility).handle(),
                Actual365Fixed::new(),
                0.0,
                CashAnnuityModel::SwapRate,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>;
            let mut swaption = Swaption::new(
                shared_mut(swap),
                shared(EuropeanExercise::new(exercise_date))
                    as Shared<dyn crate::exercise::Exercise>,
                settlement_type,
                settlement_method,
                Shared::clone(&self.settings),
            );
            swaption.base_mut().set_pricing_engine(engine);
            swaption
        }

        /// A vanilla swap with an explicit type and floating-leg spread, the
        /// shape the monotonicity tests need (`MakeVanillaSwap` cannot yet set
        /// either). The fixed leg is annual Thirty360 BondBasis, the floating
        /// leg semiannual Euribor 6M, both on the TARGET calendar - the same
        /// conventions `MakeVanillaSwap` derives in the fixture. The nominal is
        /// one, matching the fixture's `MakeVanillaSwap` default.
        fn make_vanilla(
            &self,
            start: Date,
            length: Integer,
            fixed_rate: Rate,
            spread: Spread,
            swap_type: SwapType,
        ) -> VanillaSwap {
            let maturity = start + Period::new(length, TimeUnit::Years);
            let schedule = |frequency| {
                MakeSchedule::new()
                    .from(start)
                    .to(maturity)
                    .with_frequency(frequency)
                    .with_calendar(self.calendar.clone())
                    .with_convention(BusinessDayConvention::ModifiedFollowing)
                    .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
                    .forwards()
                    .end_of_month(false)
                    .build()
            };
            VanillaSwap::new(
                swap_type,
                1.0,
                schedule(Frequency::Annual),
                fixed_rate,
                Thirty360::with_convention(Convention::BondBasis),
                schedule(Frequency::Semiannual),
                Shared::clone(&self.index),
                spread,
                Actual360::new(),
                None,
                Shared::clone(&self.settings),
            )
            .unwrap()
        }

        /// The signed `floatingLegBPS / fixedLegBPS` a swap reports once priced
        /// on the fixture curve - the ratio `testSpreadTreatment` uses to build
        /// the equivalent zero-spread swap (`swaption.cpp:373`). Priced with the
        /// same `includeSettlementDateFlows = false` the Black engine's internal
        /// discounting engine uses.
        fn leg_bps_ratio(
            &self,
            start: Date,
            length: Integer,
            fixed_rate: Rate,
            spread: Spread,
            swap_type: SwapType,
        ) -> Real {
            let mut swap = self.make_vanilla(start, length, fixed_rate, spread, swap_type);
            let engine = shared_mut(DiscountingSwapEngine::new(
                self.curve.clone(),
                Some(false),
                None,
                None,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>;
            swap.base_mut().set_pricing_engine(engine);
            let base = swap.fixed_vs_floating_mut();
            base.floating_leg_bps().unwrap() / base.fixed_leg_bps().unwrap()
        }
    }

    const EXERCISES: [Integer; 3] = [1, 5, 10];
    const LENGTHS: [Integer; 4] = [1, 5, 10, 20];

    /// `testStrikeDependency` (`swaption.cpp:171-262`): a payer swaption's NPV
    /// falls with the strike, a receiver's rises, for both physical and cash
    /// (par-yield) settlement.
    #[test]
    fn npv_is_monotone_in_the_strike() {
        let strikes = [0.03, 0.04, 0.05, 0.06, 0.07];
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        for exercise in EXERCISES {
            let exercise_date = vars.years(vars.today, exercise);
            let start_date = vars.spot(exercise_date);
            for length in LENGTHS {
                for swap_type in [SwapType::Payer, SwapType::Receiver] {
                    for (settlement_type, settlement_method) in [
                        (SettlementType::Physical, SettlementMethod::PhysicalOTC),
                        (SettlementType::Cash, SettlementMethod::ParYieldCurve),
                    ] {
                        let values: Vec<Real> = strikes
                            .iter()
                            .map(|&strike| {
                                let swap = vars
                                    .make_vanilla(start_date, length, strike, 0.0, swap_type)
                                    .into_fixed_vs_floating();
                                vars.make_swaption(
                                    swap,
                                    exercise_date,
                                    0.20,
                                    settlement_type,
                                    settlement_method,
                                )
                                .npv()
                                .unwrap()
                            })
                            .collect();
                        for pair in values.windows(2) {
                            match swap_type {
                                SwapType::Payer => assert!(
                                    pair[0] >= pair[1],
                                    "payer NPV rose with strike ({exercise}y/{length}y): {pair:?}"
                                ),
                                SwapType::Receiver => assert!(
                                    pair[0] <= pair[1],
                                    "receiver NPV fell with strike ({exercise}y/{length}y): {pair:?}"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    /// `testSpreadDependency` (`swaption.cpp:264-348`): a payer swaption's NPV
    /// rises with the floating-leg spread, a receiver's falls, for both physical
    /// and cash settlement.
    #[test]
    fn npv_is_monotone_in_the_spread() {
        let spreads = [-0.002, -0.001, 0.0, 0.001, 0.002];
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        for exercise in EXERCISES {
            let exercise_date = vars.years(vars.today, exercise);
            let start_date = vars.spot(exercise_date);
            for length in LENGTHS {
                for swap_type in [SwapType::Payer, SwapType::Receiver] {
                    for (settlement_type, settlement_method) in [
                        (SettlementType::Physical, SettlementMethod::PhysicalOTC),
                        (SettlementType::Cash, SettlementMethod::ParYieldCurve),
                    ] {
                        let values: Vec<Real> = spreads
                            .iter()
                            .map(|&spread| {
                                let swap = vars
                                    .make_vanilla(start_date, length, 0.06, spread, swap_type)
                                    .into_fixed_vs_floating();
                                vars.make_swaption(
                                    swap,
                                    exercise_date,
                                    0.20,
                                    settlement_type,
                                    settlement_method,
                                )
                                .npv()
                                .unwrap()
                            })
                            .collect();
                        for pair in values.windows(2) {
                            match swap_type {
                                SwapType::Payer => assert!(
                                    pair[0] <= pair[1],
                                    "payer NPV fell with spread ({exercise}y/{length}y): {pair:?}"
                                ),
                                SwapType::Receiver => assert!(
                                    pair[0] >= pair[1],
                                    "receiver NPV rose with spread ({exercise}y/{length}y): {pair:?}"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    /// `testSpreadTreatment` (`swaption.cpp:350-409`): a swaption on a swap with
    /// a floating-leg spread equals a swaption on a zero-spread swap whose fixed
    /// rate is shifted by `spread * floatingLegBPS / fixedLegBPS`, to 1e-6, for
    /// both physical and cash settlement.
    #[test]
    fn a_spread_swaption_equals_its_zero_spread_equivalent() {
        let spreads = [-0.002, -0.001, 0.0, 0.001, 0.002];
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        for exercise in EXERCISES {
            let exercise_date = vars.years(vars.today, exercise);
            let start_date = vars.spot(exercise_date);
            for length in LENGTHS {
                for swap_type in [SwapType::Payer, SwapType::Receiver] {
                    for spread in spreads {
                        let correction = spread
                            * vars.leg_bps_ratio(start_date, length, 0.06, spread, swap_type);
                        for (settlement_type, settlement_method) in [
                            (SettlementType::Physical, SettlementMethod::PhysicalOTC),
                            (SettlementType::Cash, SettlementMethod::ParYieldCurve),
                        ] {
                            let original = vars
                                .make_vanilla(start_date, length, 0.06, spread, swap_type)
                                .into_fixed_vs_floating();
                            let equivalent = vars
                                .make_vanilla(start_date, length, 0.06 + correction, 0.0, swap_type)
                                .into_fixed_vs_floating();
                            let original_npv = vars
                                .make_swaption(
                                    original,
                                    exercise_date,
                                    0.20,
                                    settlement_type,
                                    settlement_method,
                                )
                                .npv()
                                .unwrap();
                            let equivalent_npv = vars
                                .make_swaption(
                                    equivalent,
                                    exercise_date,
                                    0.20,
                                    settlement_type,
                                    settlement_method,
                                )
                                .npv()
                                .unwrap();
                            assert!(
                                (original_npv - equivalent_npv).abs() <= 1.0e-6,
                                "spread treatment ({exercise}y/{length}y spread {spread}): \
                                 {original_npv} vs {equivalent_npv}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// `testCachedValue` Arm A (`swaption.cpp:411-441`): a 10Y Euribor 6M payer
    /// swaption struck at 6%, exercise 5Y out, vol 0.20, reproduces the cached
    /// NPV at 1e-12. The value splits on the par/indexed convention (`:435`): the
    /// par arm (default `Settings`) is `0.036418158579`, the indexed arm
    /// `0.036421429684`. Parameterising on the flag pins both.
    #[test]
    fn cached_value_reproduces_the_par_and_indexed_arms() {
        for (using_at_par, expected) in [(true, 0.036418158579), (false, 0.036421429684)] {
            let vars = Vars::new(Date::new(13, Month::March, 2002), using_at_par);
            let exercise_date = vars.years(vars.settlement, 5);
            let start_date = vars.spot(exercise_date);
            let swap = MakeVanillaSwap::new(
                Period::new(10, TimeUnit::Years),
                Shared::clone(&vars.index),
                Some(0.06),
                Period::new(0, TimeUnit::Days),
                Shared::clone(&vars.settings),
            )
            .with_effective_date(start_date)
            .with_fixed_leg_tenor(Period::new(1, TimeUnit::Years))
            .with_fixed_leg_day_count(Vars::fixed_day_count())
            .build()
            .unwrap()
            .into_fixed_vs_floating();

            let mut swaption = vars.make_swaption(
                swap,
                exercise_date,
                0.20,
                SettlementType::Physical,
                SettlementMethod::PhysicalOTC,
            );

            let npv = swaption.npv().unwrap();
            assert!(
                (npv - expected).abs() <= 1.0e-12,
                "par={using_at_par}: npv {npv} vs cached {expected} (error {})",
                (npv - expected).abs()
            );
        }
    }

    /// `testBlackEngineCaching` (`swaption.cpp:147-169`): the swaption is not
    /// calculated before `NPV()` and is calculated after. This pins the D5
    /// re-entrancy fix: the engine installs a discounting engine on the swap the
    /// swaption observes, and that internal mutation must not invalidate the
    /// swaption mid-calculation.
    #[test]
    fn black_engine_leaves_the_swaption_calculated_after_npv() {
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        let exercise_date = vars.years(vars.today, 1);
        let start_date = vars.spot(exercise_date);
        let swap = MakeVanillaSwap::new(
            Period::new(1, TimeUnit::Years),
            Shared::clone(&vars.index),
            Some(0.03),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&vars.settings),
        )
        .with_effective_date(start_date)
        .with_fixed_leg_tenor(Period::new(1, TimeUnit::Years))
        .with_fixed_leg_day_count(Vars::fixed_day_count())
        .build()
        .unwrap()
        .into_fixed_vs_floating();

        let mut swaption = vars.make_swaption(
            swap,
            exercise_date,
            0.12,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
        );

        assert!(!swaption.base().is_calculated());
        swaption.npv().unwrap();
        assert!(
            swaption.base().is_calculated(),
            "the Black engine must leave the swaption calculated after NPV"
        );
    }

    /// `testCachedValue` Arm B (`swaption.cpp:443-458`): a 10Y OIS payer swaption
    /// on an Eonia index reproduces the cached NPV `0.014101075767` at 1e-12, a
    /// flat literal (no par/indexed split, the overnight leg has none). The Eonia
    /// forecasts off the flat 5% curve spread by -0.01
    /// (`ZeroSpreadedTermStructure`, `:131-135`), while the swaption discounts on
    /// the flat 5% curve; the OIS fixed leg is Thirty360 BondBasis (the fixture's
    /// `withFixedLegDayCount`, which the index default cannot express).
    #[test]
    fn cached_value_reproduces_the_ois_swaption_arm() {
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        let spreaded: Handle<dyn YieldTermStructure> = Handle::new(shared(
            ZeroSpreadedTermStructure::new(vars.curve.clone(), make_quote_handle(-0.01).handle()),
        )
            as Shared<dyn YieldTermStructure>);
        let ois_index = shared(Eonia::new(spreaded, Shared::clone(&vars.settings)));

        let exercise_date = vars.years(vars.settlement, 5);
        let start_date = vars.spot(exercise_date);
        let ois_swap = MakeOis::new(
            Period::new(10, TimeUnit::Years),
            ois_index,
            Some(0.06),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&vars.settings),
        )
        .with_effective_date(start_date)
        .with_fixed_leg_day_count(Vars::fixed_day_count())
        .build()
        .unwrap()
        .into_fixed_vs_floating();

        let mut swaption = vars.make_swaption(
            ois_swap,
            exercise_date,
            0.20,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
        );

        let expected = 0.014101075767;
        let npv = swaption.npv().unwrap();
        assert!(
            (npv - expected).abs() <= 1.0e-12,
            "OIS swaption npv {npv} vs cached {expected} (error {})",
            (npv - expected).abs()
        );
    }

    /// `testVega` (`swaption.cpp:462-527`): the analytical `"vega"` additional
    /// result matches a central finite difference of the NPV in the volatility,
    /// to 1.5% relative, on the two settlement shapes the parallel-indexed loop
    /// visits - `(Receiver, Physical, PhysicalOTC)` at `h=0` and
    /// `(Payer, Cash, ParYieldCurve)` at `h=1` - so both the plain and the
    /// par-yield annuity branches are vega-checked.
    #[test]
    fn analytical_vega_matches_a_finite_difference() {
        let strikes = [0.03, 0.04, 0.05, 0.06, 0.07];
        let vols = [0.01, 0.20, 0.30, 0.70, 0.90];
        let shift = 1.0e-8;
        let cases = [
            (
                SwapType::Receiver,
                SettlementType::Physical,
                SettlementMethod::PhysicalOTC,
            ),
            (
                SwapType::Payer,
                SettlementType::Cash,
                SettlementMethod::ParYieldCurve,
            ),
        ];
        let vars = Vars::new(Date::new(13, Month::March, 2002), true);
        for exercise in EXERCISES {
            let exercise_date = vars.years(vars.today, exercise);
            let start_date = vars.spot(exercise_date);
            for length in LENGTHS {
                for strike in strikes {
                    for (swap_type, settlement_type, settlement_method) in cases {
                        let priced = |volatility: Volatility| {
                            let swap = vars
                                .make_vanilla(start_date, length, strike, 0.0, swap_type)
                                .into_fixed_vs_floating();
                            vars.make_swaption(
                                swap,
                                exercise_date,
                                volatility,
                                settlement_type,
                                settlement_method,
                            )
                        };
                        for vol in vols {
                            let mut swaption = priced(vol);
                            let swaption_npv = swaption.npv().unwrap();
                            let numerical = (priced(vol + shift).npv().unwrap()
                                - priced(vol - shift).npv().unwrap())
                                / (200.0 * shift);
                            // Only the relevant vega is checked (`swaption.cpp:499`).
                            if numerical / swaption_npv > 1.0e-7 {
                                let analytical = swaption.result::<Real>("vega").unwrap() / 100.0;
                                let discrepancy = (analytical - numerical).abs() / numerical;
                                assert!(
                                    discrepancy <= 0.015,
                                    "vega {exercise}y/{length}y strike {strike} vol {vol}: \
                                     analytical {analytical} vs numerical {numerical} \
                                     (discrepancy {discrepancy})"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
