//! Black-formula cap/floor engine.
//!
//! Port of `ql/pricingengines/capfloor/blackcapfloorengine.{hpp,cpp}`:
//! [`BlackCapFloorEngine`] prices each optionlet of a
//! [`CapFloor`](crate::instruments::CapFloor) with the Black 1976 formula over an
//! [`OptionletVolatilityStructure`], discounting on a separate
//! [`YieldTermStructure`]. It does not go through the coupon pricer: it reads the
//! forwards, strikes, gearings, nominals, accrual times and fixing dates the
//! instrument's `setup_arguments` filled and calls [`black_formula`] directly
//! (`blackcapfloorengine.cpp:77-166`).
//!
//! The engine reports `value`, the additional result `"vega"` (the sum of the
//! optionlet vegas, required by the instrument, `capfloor.cpp:104`) and
//! `"optionletsPrice"` (one price per coupon, including the zeros of coupons that
//! have already paid, so its length always equals the coupon count).
//!
//! ## Divergences from QuantLib
//!
//! - The C++ `Settings::instance()` singleton has no counterpart; the flat-vol
//!   convenience constructor threads an explicit [`Settings`] into the moving
//!   [`ConstantOptionletVolatility`] it builds (D5).
//! - Only the [`ShiftedLognormal`](VolatilityType::ShiftedLognormal) path is
//!   priced. The C++ `optionletsDelta`/`optionletsStdDev`/`optionletsAtmForward`
//!   additional results feed only the deferred delta and implied-vol tests and
//!   are not produced.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorArguments, CapFloorType};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::pricingengines::blackformula::{black_formula, black_formula_std_dev_derivative};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::volatility::{
    ConstantOptionletVolatility, OptionletVolatilityStructure, VolatilityType,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::Real;
use crate::{fail, require};
use std::any::Any;

/// Black-formula engine for caps, floors and collars.
pub struct BlackCapFloorEngine {
    base: GenericEngine<CapFloorArguments, InstrumentResults>,
    discount_curve: Handle<dyn YieldTermStructure>,
    vol: Handle<dyn OptionletVolatilityStructure>,
    displacement: Real,
}

impl BlackCapFloorEngine {
    /// Builds the engine over a discount curve and an optionlet volatility
    /// surface (the C++ `Handle<OptionletVolatilityStructure>` constructor).
    ///
    /// The surface must be stripped with the shifted-lognormal model. A given
    /// `displacement` must equal the surface's own; `None` adopts the surface's
    /// displacement (`blackcapfloorengine.cpp:56-75`).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn OptionletVolatilityStructure>,
        displacement: Option<Real>,
    ) -> QlResult<BlackCapFloorEngine> {
        let surface = vol.current_link()?;
        require!(
            surface.volatility_type() == VolatilityType::ShiftedLognormal,
            "BlackCapFloorEngine needs a shifted-lognormal optionlet surface"
        );
        let displacement = match displacement {
            Some(displacement) => {
                require!(
                    surface.displacement() == displacement,
                    "displacement ({displacement}) differs from the surface's ({})",
                    surface.displacement()
                );
                displacement
            }
            None => surface.displacement(),
        };
        drop(surface);

        let base = GenericEngine::new(CapFloorArguments::default(), InstrumentResults::default());
        discount_curve.register_observer(&base.observer());
        vol.register_observer(&base.observer());
        Ok(BlackCapFloorEngine {
            base,
            discount_curve,
            vol,
            displacement,
        })
    }

    /// Builds the engine over a flat volatility quote, wrapping it in a moving
    /// [`ConstantOptionletVolatility`] on a null calendar (the C++
    /// `Handle<Quote>` constructor, `blackcapfloorengine.cpp:44-54`).
    pub fn with_flat_vol(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn Quote>,
        day_counter: DayCounter,
        displacement: Real,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<BlackCapFloorEngine> {
        let surface = ConstantOptionletVolatility::moving_with_quote(
            0,
            NullCalendar::new(),
            BusinessDayConvention::Following,
            vol,
            day_counter,
            VolatilityType::ShiftedLognormal,
            displacement,
            settings,
        );
        let vol = Handle::new(shared(surface) as Shared<dyn OptionletVolatilityStructure>);
        BlackCapFloorEngine::new(discount_curve, vol, None)
    }

    /// The lognormal shift applied to forwards and strikes.
    pub fn displacement(&self) -> Real {
        self.displacement
    }
}

impl AsObservable for BlackCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BlackCapFloorEngine {
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
        let discount = self.discount_curve.current_link()?;
        let surface = self.vol.current_link()?;
        let today = surface.reference_date()?;
        let settlement = discount.reference_date()?;
        let displacement = self.displacement;

        let arguments = self.base.arguments();
        let cap_floor_type = match arguments.cap_floor_type {
            Some(cap_floor_type) => cap_floor_type,
            None => fail!("cap/floor type not set"),
        };
        let has_cap = matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar);
        let has_floor = matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar);

        let n = arguments.end_dates.len();
        let mut values = Vec::with_capacity(n);
        let mut value = 0.0;
        let mut vega = 0.0;

        for i in 0..n {
            let payment_date = arguments.end_dates[i];
            // Settlement/npv-date handling is not modelled; expired caplets are
            // simply discarded (`blackcapfloorengine.cpp:92-95`), but still keep a
            // zero entry so `optionletsPrice` spans every coupon.
            if payment_date <= settlement {
                values.push(0.0);
                continue;
            }
            let d = discount.discount_date(payment_date, false)?;
            let accrual_factor =
                arguments.nominals[i] * arguments.gearings[i] * arguments.accrual_times[i];
            let discounted_accrual = d * accrual_factor;
            let Some(forward) = arguments.forwards[i] else {
                values.push(0.0);
                continue;
            };

            let fixing_date = arguments.fixing_dates[i];
            let sqrt_time = if fixing_date > today {
                surface.time_from_reference(fixing_date)?.sqrt()
            } else {
                0.0
            };

            let mut optionlet_value = 0.0;
            let mut optionlet_vega = 0.0;

            if has_cap {
                let strike = arguments.cap_rates[i].expect("cap rate set for cap/collar");
                let mut std_dev = 0.0;
                if sqrt_time > 0.0 {
                    std_dev = surface
                        .black_variance_date(fixing_date, strike, false)?
                        .sqrt();
                    optionlet_vega += black_formula_std_dev_derivative(
                        strike,
                        forward,
                        std_dev,
                        discounted_accrual,
                        displacement,
                    )? * sqrt_time;
                }
                optionlet_value += black_formula(
                    OptionType::Call,
                    strike,
                    forward,
                    std_dev,
                    discounted_accrual,
                    displacement,
                )?;
            }

            if has_floor {
                let strike = arguments.floor_rates[i].expect("floor rate set for floor/collar");
                let mut std_dev = 0.0;
                let mut floorlet_vega = 0.0;
                if sqrt_time > 0.0 {
                    std_dev = surface
                        .black_variance_date(fixing_date, strike, false)?
                        .sqrt();
                    floorlet_vega = black_formula_std_dev_derivative(
                        strike,
                        forward,
                        std_dev,
                        discounted_accrual,
                        displacement,
                    )? * sqrt_time;
                }
                let floorlet = black_formula(
                    OptionType::Put,
                    strike,
                    forward,
                    std_dev,
                    discounted_accrual,
                    displacement,
                )?;
                if cap_floor_type == CapFloorType::Floor {
                    optionlet_value = floorlet;
                    optionlet_vega = floorlet_vega;
                } else {
                    // A collar is long the cap and short the floor.
                    optionlet_value -= floorlet;
                    optionlet_vega -= floorlet_vega;
                }
            }

            values.push(optionlet_value);
            value += optionlet_value;
            vega += optionlet_vega;
        }

        drop(discount);
        drop(surface);

        let results = self.base.results_mut();
        results.value = Some(value);
        results.error_estimate = None;
        results.valuation_date = None;
        results
            .additional_results
            .insert("vega".to_string(), shared(vega) as Shared<dyn Any>);
        results.additional_results.insert(
            "optionletsPrice".to_string(),
            shared(values) as Shared<dyn Any>,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::{IborCoupon, IborLeg};
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::instrument::Instrument;
    use crate::instruments::CapFloor;
    use crate::instruments::{SwapType, VanillaSwap};
    use crate::interestrate::Compounding;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::quotes::make_quote_handle;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::{MakeSchedule, Schedule};
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Rate, Volatility};

    /// The `capfloor.cpp` `CommonVars` fixture, retargeted to the cached test's
    /// dates (`testCachedValue:542`): today 14-Mar-2002, settlement 18-Mar-2002,
    /// a flat 5% Actual360 curve at settlement, a Euribor 6M index forecasting
    /// off it, and a semiannual leg on a nominal of 100.
    struct Vars {
        settings: Shared<Settings<Date>>,
        calendar: Calendar,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<IborIndex>,
    }

    impl Vars {
        fn new(using_at_par: bool) -> Vars {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(Date::new(14, Month::March, 2002));
            settings.set_using_at_par_coupons(using_at_par);
            let settlement = Date::new(18, Month::March, 2002);
            let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.05,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            ))
                as Shared<dyn YieldTermStructure>);
            let index = shared(Euribor::six_months(curve.clone(), Shared::clone(&settings)));
            Vars {
                settings,
                calendar: Target::new(),
                curve,
                index,
            }
        }

        /// The `length`-year semiannual ModifiedFollowing schedule from `start`.
        fn schedule(&self, start: Date, length: Integer) -> Schedule {
            let end = self.calendar.advance(
                start,
                length,
                TimeUnit::Years,
                BusinessDayConvention::ModifiedFollowing,
                false,
            );
            MakeSchedule::new()
                .from(start)
                .to(end)
                .with_frequency(Frequency::Semiannual)
                .with_calendar(self.calendar.clone())
                .with_convention(BusinessDayConvention::ModifiedFollowing)
                .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
                .forwards()
                .end_of_month(false)
                .build()
        }

        /// `CommonVars::makeLeg` (`capfloor.cpp:78`): a leg on the index day
        /// counter, ModifiedFollowing, two fixing days.
        fn make_leg(&self, start: Date, length: Integer) -> Vec<Shared<IborCoupon>> {
            IborLeg::new(self.schedule(start, length), Shared::clone(&self.index))
                .with_notional(100.0)
                .with_payment_day_counter(self.index.day_counter().clone())
                .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
                .with_fixing_days(2)
                .coupons()
                .unwrap()
        }

        /// The `capfloor.cpp` parity/ATM swap: a Payer swap fixed against the
        /// same index on one shared schedule, priced with the discounting engine.
        fn make_swap(&self, start: Date, length: Integer, fixed_rate: Rate) -> VanillaSwap {
            let schedule = self.schedule(start, length);
            let mut swap = VanillaSwap::new(
                SwapType::Payer,
                100.0,
                schedule.clone(),
                fixed_rate,
                self.index.day_counter().clone(),
                schedule,
                Shared::clone(&self.index),
                0.0,
                self.index.day_counter().clone(),
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

        /// `CommonVars::makeEngine` (`capfloor.cpp:90`): the Black engine over a
        /// flat volatility quote.
        fn engine(&self, volatility: Volatility) -> SharedMut<dyn PricingEngine> {
            let vol = make_quote_handle(volatility).handle();
            let engine = BlackCapFloorEngine::with_flat_vol(
                self.curve.clone(),
                vol,
                Actual365Fixed::new(),
                0.0,
                Shared::clone(&self.settings),
            )
            .unwrap();
            shared_mut(engine) as SharedMut<dyn PricingEngine>
        }

        fn start_date(&self) -> Date {
            self.curve.current_link().unwrap().reference_date().unwrap()
        }
    }

    /// `capfloor.cpp` `testCachedValue` (`:536`): a 20Y cap at 0.07 and floor at
    /// 0.03, vol 0.20, reproduce the cached NPVs at 1e-11. The value splits on
    /// the par/indexed convention (`:554`): the par arm (default `Settings`) is
    /// cap 6.87570026732 / floor 2.65812927959, the indexed arm cap
    /// 6.87630307745 / floor 2.65796764715. Parameterising on the flag pins both.
    #[test]
    fn cached_value_reproduces_the_par_and_indexed_arms() {
        for (using_at_par, expected_cap, expected_floor) in [
            (true, 6.87570026732, 2.65812927959),
            (false, 6.87630307745, 2.65796764715),
        ] {
            let vars = Vars::new(using_at_par);
            let start = vars.start_date();
            let leg = vars.make_leg(start, 20);

            let mut cap =
                CapFloor::cap(leg.clone(), vec![0.07], Shared::clone(&vars.settings)).unwrap();
            cap.base_mut().set_pricing_engine(vars.engine(0.20));
            let mut floor =
                CapFloor::floor(leg, vec![0.03], Shared::clone(&vars.settings)).unwrap();
            floor.base_mut().set_pricing_engine(vars.engine(0.20));

            let cap_npv = cap.npv().unwrap();
            let floor_npv = floor.npv().unwrap();
            assert!(
                (cap_npv - expected_cap).abs() <= 1.0e-11,
                "par={using_at_par}: cap {cap_npv} vs cached {expected_cap} (error {})",
                (cap_npv - expected_cap).abs()
            );
            assert!(
                (floor_npv - expected_floor).abs() <= 1.0e-11,
                "par={using_at_par}: floor {floor_npv} vs cached {expected_floor} (error {})",
                (floor_npv - expected_floor).abs()
            );
        }
    }

    /// Builds a cap (or floor) over `leg`, priced with the flat-vol engine.
    fn priced(
        vars: &Vars,
        leg: &[Shared<IborCoupon>],
        is_cap: bool,
        strike: Rate,
        vol: Volatility,
    ) -> CapFloor {
        let mut cf = if is_cap {
            CapFloor::cap(leg.to_vec(), vec![strike], Shared::clone(&vars.settings))
        } else {
            CapFloor::floor(leg.to_vec(), vec![strike], Shared::clone(&vars.settings))
        }
        .unwrap();
        cf.base_mut().set_pricing_engine(vars.engine(vol));
        cf
    }

    /// `capfloor.cpp:254`: the engine strike is de-spread `(strike - spread)/gearing`.
    /// A leg with a nonzero spread and gearing != 1 pins it through the arguments.
    #[test]
    fn setup_arguments_despreads_the_strike_by_spread_and_gearing() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        let coupons = IborLeg::new(vars.schedule(start, 2), Shared::clone(&vars.index))
            .with_notional(100.0)
            .with_payment_day_counter(vars.index.day_counter().clone())
            .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
            .with_fixing_days(2)
            .with_gearing(2.0)
            .with_spread(0.01)
            .coupons()
            .unwrap();
        let cap = CapFloor::cap(coupons, vec![0.07], Shared::clone(&vars.settings)).unwrap();

        let mut args = CapFloorArguments::default();
        cap.setup_arguments(&mut args).unwrap();
        assert!(!args.cap_rates.is_empty());
        for strike in &args.cap_rates {
            assert!((strike.expect("cap strike") - (0.07 - 0.01) / 2.0).abs() < 1e-15);
        }
        assert!(args.floor_rates.iter().all(Option::is_none));
    }

    /// `testCachedValueFromOptionLets` (`:580`): the `optionletsPrice` result has
    /// one entry per coupon (40 for a 20Y semiannual leg) and sums to the cached
    /// cap/floor NPV at 1e-11, on both par and indexed arms.
    #[test]
    fn cached_value_equals_the_sum_of_optionlet_prices() {
        for (using_at_par, expected_cap, expected_floor) in [
            (true, 6.87570026732, 2.65812927959),
            (false, 6.87630307745, 2.65796764715),
        ] {
            let vars = Vars::new(using_at_par);
            let start = vars.start_date();
            let leg = vars.make_leg(start, 20);
            let mut cap = priced(&vars, &leg, true, 0.07, 0.20);
            let mut floor = priced(&vars, &leg, false, 0.03, 0.20);

            let cap_prices = cap.result::<Vec<Real>>("optionletsPrice").unwrap();
            let floor_prices = floor.result::<Vec<Real>>("optionletsPrice").unwrap();
            assert_eq!(cap_prices.len(), 40);
            assert_eq!(floor_prices.len(), 40);

            let cap_sum: Real = cap_prices.iter().sum();
            let floor_sum: Real = floor_prices.iter().sum();
            assert!(
                (cap_sum - expected_cap).abs() <= 1.0e-11,
                "par={using_at_par}: cap sum {cap_sum} vs {expected_cap}"
            );
            assert!(
                (floor_sum - expected_floor).abs() <= 1.0e-11,
                "par={using_at_par}: floor sum {floor_sum} vs {expected_floor}"
            );
        }
    }

    /// `testStrikeDependency` (`:196`): a cap's NPV falls with the strike, a
    /// floor's rises.
    #[test]
    fn cap_npv_falls_and_floor_npv_rises_with_the_strike() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        for length in [1, 5, 10, 20] {
            let leg = vars.make_leg(start, length);
            let mut caps = Vec::new();
            let mut floors = Vec::new();
            for strike in [0.03, 0.04, 0.05, 0.06, 0.07] {
                caps.push(priced(&vars, &leg, true, strike, 0.20).npv().unwrap());
                floors.push(priced(&vars, &leg, false, strike, 0.20).npv().unwrap());
            }
            for pair in caps.windows(2) {
                assert!(
                    pair[0] >= pair[1],
                    "cap NPV increased with strike: {pair:?}"
                );
            }
            for pair in floors.windows(2) {
                assert!(
                    pair[0] <= pair[1],
                    "floor NPV decreased with strike: {pair:?}"
                );
            }
        }
    }

    /// `testVega` (`:147`): the `"vega"` result matches a central finite
    /// difference of the NPV in the volatility, to 0.5% relative.
    #[test]
    fn analytical_vega_matches_a_finite_difference() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        let shift = 1.0e-8;
        for length in [1, 5, 10, 20] {
            let leg = vars.make_leg(start, length);
            for strike in [0.03, 0.05, 0.07] {
                for is_cap in [true, false] {
                    let analytical = priced(&vars, &leg, is_cap, strike, 0.20)
                        .result::<Real>("vega")
                        .unwrap();
                    let up = priced(&vars, &leg, is_cap, strike, 0.20 + shift)
                        .npv()
                        .unwrap();
                    let down = priced(&vars, &leg, is_cap, strike, 0.20 - shift)
                        .npv()
                        .unwrap();
                    let numerical = (up - down) / (2.0 * shift);
                    if numerical > 1.0e-4 {
                        let discrepancy = (numerical - analytical).abs() / numerical;
                        assert!(
                            discrepancy <= 0.005,
                            "vega {length}y strike {strike} cap={is_cap}: \
                             analytical {analytical} vs numerical {numerical}"
                        );
                    }
                }
            }
        }
    }

    /// `testParity` (`:354`): a cap minus a floor at the same strike equals a
    /// payer swap fixed at that strike.
    #[test]
    fn cap_minus_floor_equals_the_underlying_swap() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        for length in [1, 5, 10, 20] {
            let leg = vars.make_leg(start, length);
            for strike in [0.0, 0.03, 0.05, 0.07] {
                let cap = priced(&vars, &leg, true, strike, 0.20).npv().unwrap();
                let floor = priced(&vars, &leg, false, strike, 0.20).npv().unwrap();
                let swap = vars.make_swap(start, length, strike).npv().unwrap();
                assert!(
                    ((cap - floor) - swap).abs() <= 1.0e-10,
                    "parity {length}y strike {strike}: cap-floor {} vs swap {swap}",
                    cap - floor
                );
            }
        }
    }

    /// `testATMRate` (`:397`): the cap and floor share an ATM rate, and a swap
    /// struck at it prices to zero.
    #[test]
    fn a_swap_struck_at_the_atm_rate_prices_to_zero() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        let curve = vars.curve.current_link().unwrap();
        for length in [1, 5, 10, 20] {
            let leg = vars.make_leg(start, length);
            let cap =
                CapFloor::cap(leg.clone(), vec![0.05], Shared::clone(&vars.settings)).unwrap();
            let floor = CapFloor::floor(leg, vec![0.05], Shared::clone(&vars.settings)).unwrap();
            let cap_atm = cap.atm_rate(&*curve).unwrap();
            let floor_atm = floor.atm_rate(&*curve).unwrap();
            assert!((cap_atm - floor_atm).abs() <= 1.0e-10);
            let swap = vars.make_swap(start, length, floor_atm).npv().unwrap();
            assert!(swap.abs() <= 1.0e-10, "atm swap {length}y npv {swap}");
        }
    }

    /// `testConsistency` (`:249`), the collar identity only: a collar equals the
    /// cap minus the floor at the same strikes.
    #[test]
    fn a_collar_equals_the_cap_minus_the_floor() {
        let vars = Vars::new(true);
        let start = vars.start_date();
        for length in [1, 5, 10, 20] {
            let leg = vars.make_leg(start, length);
            for (cap_rate, floor_rate) in [(0.05, 0.03), (0.06, 0.04), (0.07, 0.03)] {
                let cap = priced(&vars, &leg, true, cap_rate, 0.20).npv().unwrap();
                let floor = priced(&vars, &leg, false, floor_rate, 0.20).npv().unwrap();
                let mut collar = CapFloor::collar(
                    leg.clone(),
                    vec![cap_rate],
                    vec![floor_rate],
                    Shared::clone(&vars.settings),
                )
                .unwrap();
                collar.base_mut().set_pricing_engine(vars.engine(0.20));
                assert!(
                    ((cap - floor) - collar.npv().unwrap()).abs() <= 1.0e-10,
                    "collar {length}y cap {cap_rate} floor {floor_rate}"
                );
            }
        }
    }
}
