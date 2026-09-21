//! Normal-volatility pricing for caps, floors and collars.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorArguments, CapFloorType};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::pricingengines::blackformula::{
    bachelier_black_formula, bachelier_black_formula_std_dev_derivative,
};
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
use crate::{fail, require};
use std::any::Any;

/// Bachelier normal-formula engine for caps, floors and collars.
pub struct BachelierCapFloorEngine {
    base: GenericEngine<CapFloorArguments, InstrumentResults>,
    discount_curve: Handle<dyn YieldTermStructure>,
    vol: Handle<dyn OptionletVolatilityStructure>,
}

impl BachelierCapFloorEngine {
    /// Builds a normal-volatility engine over an optionlet surface.
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn OptionletVolatilityStructure>,
    ) -> QlResult<Self> {
        require!(
            vol.current_link()?.volatility_type() == VolatilityType::Normal,
            "BachelierCapFloorEngine needs a normal optionlet surface"
        );
        let base = GenericEngine::new(CapFloorArguments::default(), InstrumentResults::default());
        discount_curve.register_observer(&base.observer());
        vol.register_observer(&base.observer());
        Ok(BachelierCapFloorEngine {
            base,
            discount_curve,
            vol,
        })
    }

    /// Builds a moving normal-volatility surface from an observable quote.
    pub fn with_flat_vol(
        discount_curve: Handle<dyn YieldTermStructure>,
        vol: Handle<dyn Quote>,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<BachelierCapFloorEngine> {
        let surface = ConstantOptionletVolatility::moving_with_quote(
            0,
            NullCalendar::new(),
            BusinessDayConvention::Following,
            vol,
            day_counter,
            VolatilityType::Normal,
            0.0,
            settings,
        );
        let vol = Handle::new(shared(surface) as Shared<dyn OptionletVolatilityStructure>);
        BachelierCapFloorEngine::new(discount_curve, vol)
    }
}

impl AsObservable for BachelierCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BachelierCapFloorEngine {
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
        self.base.arguments().validate()?;
        let discount = self.discount_curve.current_link()?;
        let surface = self.vol.current_link()?;
        require!(
            surface.volatility_type() == VolatilityType::Normal,
            "normal surface required"
        );
        let today = surface.reference_date()?;
        let settlement = discount.reference_date()?;

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
            if payment_date <= settlement {
                values.push(0.0);
                continue;
            }
            let d = discount.discount_date(payment_date, false)?;
            let accrual_factor =
                arguments.nominals[i] * arguments.gearings[i] * arguments.accrual_times[i];
            let discounted_accrual = d * accrual_factor;
            let Some(forward) = arguments.forwards[i] else {
                fail!("missing forward for live cap/floor coupon");
            };

            require!(
                forward.is_finite() && discounted_accrual.is_finite(),
                "non-finite cap/floor input"
            );
            let fixing_date = arguments.fixing_dates[i];
            let sqrt_time = if fixing_date > today {
                surface.time_from_reference(fixing_date)?.sqrt()
            } else {
                0.0
            };

            require!(
                sqrt_time.is_finite() && sqrt_time >= 0.0,
                "invalid cap/floor fixing time"
            );
            let mut optionlet_value = 0.0;
            let mut optionlet_vega = 0.0;

            if has_cap {
                let strike = arguments.cap_rates[i].ok_or_else(|| {
                    crate::errors::QlError::new("cap rate missing", file!(), line!())
                })?;
                require!(strike.is_finite(), "non-finite cap/floor strike");
                let mut std_dev = 0.0;
                if sqrt_time > 0.0 {
                    let volatility = surface.volatility_date(fixing_date, strike, false)?;
                    require!(
                        volatility.is_finite() && volatility >= 0.0,
                        "normal volatility must be finite and non-negative"
                    );
                    std_dev = volatility * sqrt_time;
                    require!(std_dev.is_finite(), "non-finite normal volatility");
                    optionlet_vega += bachelier_black_formula_std_dev_derivative(
                        strike,
                        forward,
                        std_dev,
                        discounted_accrual,
                    )? * sqrt_time;
                }
                optionlet_value += bachelier_black_formula(
                    OptionType::Call,
                    strike,
                    forward,
                    std_dev,
                    discounted_accrual,
                )?;
            }

            if has_floor {
                let strike = arguments.floor_rates[i].ok_or_else(|| {
                    crate::errors::QlError::new("floor rate missing", file!(), line!())
                })?;
                require!(strike.is_finite(), "non-finite cap/floor strike");
                let mut std_dev = 0.0;
                let mut floorlet_vega = 0.0;
                if sqrt_time > 0.0 {
                    let volatility = surface.volatility_date(fixing_date, strike, false)?;
                    require!(
                        volatility.is_finite() && volatility >= 0.0,
                        "normal volatility must be finite and non-negative"
                    );
                    std_dev = volatility * sqrt_time;
                    require!(std_dev.is_finite(), "non-finite normal volatility");
                    floorlet_vega = bachelier_black_formula_std_dev_derivative(
                        strike,
                        forward,
                        std_dev,
                        discounted_accrual,
                    )? * sqrt_time;
                }
                let floorlet = bachelier_black_formula(
                    OptionType::Put,
                    strike,
                    forward,
                    std_dev,
                    discounted_accrual,
                )?;
                if cap_floor_type == CapFloorType::Floor {
                    optionlet_value = floorlet;
                    optionlet_vega = floorlet_vega;
                } else {
                    optionlet_value -= floorlet;
                    optionlet_vega -= floorlet_vega;
                }
            }

            require!(
                optionlet_value.is_finite() && optionlet_vega.is_finite(),
                "non-finite cap/floor optionlet result"
            );
            values.push(optionlet_value);
            value += optionlet_value;
            vega += optionlet_vega;
        }

        require!(
            value.is_finite() && vega.is_finite(),
            "non-finite cap/floor result"
        );
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
    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;

    #[test]
    fn same_day_intrinsic_and_malformed_input_recovery() {
        let date = Date::new(15, Month::January, 2026);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date);
        let curve = Handle::new(shared(FlatForward::with_rate(
            date,
            -0.01,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let quote = shared(SimpleQuote::new(0.01));
        let mut engine = BachelierCapFloorEngine::with_flat_vol(
            curve,
            Handle::new(quote),
            Actual365Fixed::new(),
            settings,
        )
        .unwrap();
        let args = CapFloorArguments {
            cap_floor_type: Some(CapFloorType::Cap),
            start_dates: vec![date],
            fixing_dates: vec![date],
            end_dates: vec![date + 365],
            accrual_times: vec![1.0],
            cap_rates: vec![Some(-0.02)],
            floor_rates: vec![None],
            forwards: vec![Some(-0.01)],
            gearings: vec![1.0],
            nominals: vec![100.0],
        };
        *engine.base.arguments_mut() = args;
        engine.calculate().unwrap();
        let value = engine.base.results().value.unwrap();
        assert!((value - 0.01_f64.exp()).abs() < 1e-12);
        let vega = engine.base.results().additional_results["vega"]
            .downcast_ref::<f64>()
            .unwrap();
        assert_eq!(*vega, 0.0);
        engine.base.arguments_mut().forwards[0] = None;
        assert!(engine.calculate().is_err());
        engine.base.arguments_mut().forwards[0] = Some(-0.01);
        engine.base.arguments_mut().cap_rates[0] = Some(f64::NAN);
        assert!(engine.calculate().is_err());
        engine.base.arguments_mut().cap_rates[0] = Some(-0.02);
        engine.base.arguments_mut().nominals[0] = f64::MAX;
        engine.base.arguments_mut().gearings[0] = f64::MAX;
        assert!(engine.calculate().is_err());
        engine.base.arguments_mut().nominals[0] = 100.0;
        engine.base.arguments_mut().gearings[0] = 1.0;
        engine.calculate().unwrap();
        assert_eq!(engine.base.results().value, Some(value));
    }
}
