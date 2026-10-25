//! CPI quoted-price surface engine, adapted from QuantLib revision
//! 9863b578af0caa4cecabf697196533e84a8308b6,
//! ql/experimental/inflation/cpicapfloorengines.cpp.
//! Copyright (C) 2011 Chris Kenyon. QuantLib license.

use crate::cashflows::cpicoupon::checked_cpi_shift;
use crate::cashflows::cpidates::{checked_cpi_adjust as adjusted, checked_cpi_day as checked_day};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, InflationIndex, inflation_period};
use crate::instrument::InstrumentResults;
use crate::instruments::cpicapfloor::{CpiCapFloorArguments, lag_months};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::shared::Shared;
use crate::termstructures::inflation::cpicapfloortermpricesurface::CpiCapFloorTermPriceSurface;
use crate::time::{period::Period, timeunit::TimeUnit};
use crate::{fail, require};

/// Interpolates a frozen CPI price surface without rescaling by instrument
/// nominal, base CPI or start date. Rebuild the surface for updated market data.
pub struct InterpolatingCpiCapFloorEngine {
    base: GenericEngine<CpiCapFloorArguments, InstrumentResults>,
    surface: Shared<CpiCapFloorTermPriceSurface>,
}
impl InterpolatingCpiCapFloorEngine {
    /// Retains the immutable surface used by this engine.
    pub fn new(surface: Shared<CpiCapFloorTermPriceSurface>) -> Self {
        Self {
            base: GenericEngine::new(
                CpiCapFloorArguments::default(),
                InstrumentResults::default(),
            ),
            surface,
        }
    }
}
impl AsObservable for InterpolatingCpiCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl PricingEngine for InterpolatingCpiCapFloorEngine {
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
        let Some(a) = &self.base.arguments().config else {
            fail!("CPI cap/floor arguments not set")
        };
        let months = lag_months(a.observation_lag)? - lag_months(self.surface.observation_lag())?;
        require!(months >= 0, "lag difference must be non-negative");
        let diff = Period::new(months, TimeUnit::Months);
        let effective = checked_cpi_shift(
            adjusted(&a.payment_calendar, a.maturity, a.payment_convention)?,
            diff,
        )?;
        let (start, end) = inflation_period(effective, a.index.frequency())?;
        let read = |date| match a.option_type {
            OptionType::Call => self.surface.cap_price(date, a.strike),
            OptionType::Put => self.surface.floor_price(date, a.strike),
        };
        let first = read(start)?;
        let price = match a.interpolation {
            CpiInterpolationType::Flat => first,
            CpiInterpolationType::Linear => {
                let next = checked_day(end, 1)?;
                first
                    + (read(next)? - first) * f64::from(effective - start) / f64::from(next - start)
            }
        };
        require!(price.is_finite(), "non-finite CPI cap/floor price");
        self.base.results_mut().value = Some(price);
        Ok(())
    }
}
