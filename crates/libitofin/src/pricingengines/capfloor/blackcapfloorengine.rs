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
