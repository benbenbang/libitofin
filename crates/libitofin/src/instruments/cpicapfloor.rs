//! Single-payment cumulative CPI options with quoted-price interpolation.
//!
//! Adapted from QuantLib revision 9863b578af0caa4cecabf697196533e84a8308b6,
//! ql/instruments/cpicapfloor.{hpp,cpp}. Copyright (C) 2010, 2011 Chris Kenyon;
//! Copyright (C) 2021 Ralf Konrad Eckel. QuantLib license.

use crate::cashflows::cpicoupon::checked_cpi_shift;
use crate::cashflows::cpidates::{checked_cpi_adjust as adjusted, valid_cpi_date as valid_date};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, InflationIndex, ZeroInflationIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::option::OptionType;
use crate::pricingengine::{Arguments, PricingEngine};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date, period::Period,
    timeunit::TimeUnit,
};
use crate::{fail, require};
use std::any::Any;

/// Contract terms. The interpolation engine returns prices in surface-quote
/// units: nominal, base CPI and start date are descriptors, not price multipliers.
#[derive(Clone)]
pub struct CpiCapFloorConfig {
    pub option_type: OptionType,
    pub nominal: f64,
    pub start_date: Date,
    pub base_cpi: f64,
    pub maturity: Date,
    pub fixing_calendar: Calendar,
    pub fixing_convention: BusinessDayConvention,
    pub payment_calendar: Calendar,
    pub payment_convention: BusinessDayConvention,
    pub strike: f64,
    pub index: Shared<ZeroInflationIndex>,
    pub observation_lag: Period,
    pub interpolation: CpiInterpolationType,
    pub settings: Shared<Settings<Date>>,
}

/// Arguments for the cumulative CPI price-surface engine.
#[derive(Default)]
pub struct CpiCapFloorArguments {
    pub config: Option<CpiCapFloorConfig>,
}
impl Arguments for CpiCapFloorArguments {
    fn validate(&self) -> QlResult<()> {
        require!(self.config.is_some(), "CPI cap/floor arguments not set");
        Ok(())
    }
}

/// A cumulative CPI cap or floor, not a cap/floor on individual CPI coupons.
pub struct CpiCapFloor {
    base: InstrumentBase,
    config: CpiCapFloorConfig,
}
impl CpiCapFloor {
    /// Builds a single-payment contract; evaluation-date expiry uses raw maturity.
    ///
    /// # Errors
    /// Rejects non-finite inputs, nonpositive nominal/base CPI, inverted dates,
    /// mixed settings and observation lags shorter than publication availability.
    pub fn new(a: CpiCapFloorConfig) -> QlResult<Self> {
        require!(
            a.nominal.is_finite() && a.nominal > 0.0,
            "nominal must be positive and finite"
        );
        require!(
            a.base_cpi.is_finite() && a.base_cpi > 0.0,
            "base CPI must be positive and finite"
        );
        require!(
            a.strike.is_finite() && a.strike > -1.0,
            "strike must be finite and greater than -1"
        );
        valid_date(a.start_date)?;
        valid_date(a.maturity)?;
        require!(a.maturity > a.start_date, "maturity must follow start date");
        require!(
            Shared::ptr_eq(a.index.inflation_base().settings(), &a.settings),
            "index and instrument must share settings"
        );
        require!(a.observation_lag.length() >= 0, "negative observation lag");
        let instrument_months = lag_months(a.observation_lag)?;
        let lag = lag_months(a.index.availability_lag())?;
        require!(
            match a.interpolation {
                CpiInterpolationType::Flat => instrument_months >= lag,
                CpiInterpolationType::Linear => instrument_months > lag,
            },
            "observation lag shorter than index availability"
        );
        checked_cpi_shift(a.maturity, a.observation_lag)?;
        adjusted(
            &a.fixing_calendar,
            checked_cpi_shift(a.maturity, a.observation_lag)?,
            a.fixing_convention,
        )?;
        adjusted(&a.payment_calendar, a.maturity, a.payment_convention)?;
        let base = InstrumentBase::new();
        a.settings.register_eval_date_observer(&base.observer());
        Ok(Self { base, config: a })
    }
    /// Install an engine retaining its immutable price surface.
    pub fn set_pricing_engine(&mut self, engine: SharedMut<dyn PricingEngine>) {
        self.base.set_pricing_engine(engine);
    }
    /// Fixing date adjusted after subtracting the observation lag.
    pub fn fixing_date(&self) -> QlResult<Date> {
        adjusted(
            &self.config.fixing_calendar,
            checked_cpi_shift(self.config.maturity, self.config.observation_lag)?,
            self.config.fixing_convention,
        )
    }
    /// Adjusted payment date, distinct from the raw expiry date.
    pub fn pay_date(&self) -> QlResult<Date> {
        adjusted(
            &self.config.payment_calendar,
            self.config.maturity,
            self.config.payment_convention,
        )
    }
    /// Immutable contract metadata.
    pub fn config(&self) -> &CpiCapFloorConfig {
        &self.config
    }
}
impl Instrument for CpiCapFloor {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }
    fn is_expired(&self) -> QlResult<bool> {
        let Some(today) = self.config.settings.evaluation_date() else {
            fail!("evaluation date not set")
        };
        valid_date(today)?;
        Ok(today > self.config.maturity)
    }
    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(args) = (arguments as &mut dyn Any).downcast_mut::<CpiCapFloorArguments>() else {
            fail!("wrong CPI cap/floor engine arguments")
        };
        args.config = Some(self.config.clone());
        Ok(())
    }
}

pub(crate) fn lag_months(lag: Period) -> QlResult<i32> {
    require!(lag.length() >= 0, "negative observation lag");
    if lag.length() == 0 {
        return Ok(0);
    }
    match lag.units() {
        TimeUnit::Months => Ok(lag.length()),
        TimeUnit::Years => lag.length().checked_mul(12).ok_or_else(|| {
            crate::errors::QlError::new("observation lag overflow", file!(), line!())
        }),
        _ => fail!("CPI cap/floor observation lag must use months or years"),
    }
}

#[cfg(test)]
#[path = "cpicapfloor_tests.rs"]
mod tests;
