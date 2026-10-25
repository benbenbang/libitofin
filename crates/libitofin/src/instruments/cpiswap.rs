//! Plain CPI-scaled fixed coupons exchanged against Ibor coupons.
//!
//! Adapted from QuantLib revision 9863b578af0caa4cecabf697196533e84a8308b6,
//! ql/instruments/cpiswap.{hpp,cpp}. Copyright (C) 2007, 2009, 2011 Chris
//! Kenyon; Copyright (C) 2009 StatPro Italia srl. See QUANTLIB_LICENSE.txt.
use crate::cashflow::Leg;
use crate::cashflows::{CpiLeg, IborLeg, SimpleCashFlow};
use crate::errors::QlResult;
use crate::indexes::inflationindex::CpiInterpolationType;
use crate::indexes::{IborIndex, Index, InterestRateIndex, ZeroInflationIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::{Swap, SwapType};
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::{
    businessdayconvention::BusinessDayConvention, date::Date, daycounter::DayCounter,
    period::Period, schedule::Schedule, timeunit::TimeUnit,
};
use crate::types::Real;

#[path = "cpiswap_dates.rs"]
mod dates;

/// A CPI leg against a plain Ibor-plus-spread leg.
///
/// Native `Payer` pays the Ibor leg and receives CPI, not vice versa. Both
/// schedules must contain coupons. Nominals are constant but can differ.
/// `subtract_inflation_nominal` subtracts the CPI nominal from both terminal
/// exchanges. There is no initial exchange. Existing generic swap engines apply.
pub struct CpiSwap {
    swap: Swap,
    spread: Real,
    settings: Shared<Settings<Date>>,
    settings_flags: (bool, Option<bool>, bool, bool),
}

impl CpiSwap {
    /// Builds plain CPI and Ibor coupon legs with terminal exchanges.
    ///
    /// Ibor fixing days use the index default. Single-date zero-coupon schedules,
    /// optionality, overnight and day-tenor indexes are outside this slice.
    /// # Errors
    /// Rejects non-finite inputs, non-positive nominals/base CPI, malformed
    /// schedules, different Settings, and invalid cashflow construction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        nominal: Real,
        inflation_nominal: Real,
        subtract_inflation_nominal: bool,
        spread: Real,
        float_day_count: DayCounter,
        float_schedule: Schedule,
        float_roll: BusinessDayConvention,
        float_index: Shared<IborIndex>,
        fixed_rate: Real,
        base_cpi: Real,
        fixed_day_count: DayCounter,
        fixed_schedule: Schedule,
        fixed_roll: BusinessDayConvention,
        observation_lag: Period,
        fixed_index: Shared<ZeroInflationIndex>,
        interpolation: CpiInterpolationType,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        crate::require!(
            nominal.is_finite()
                && nominal > 0.0
                && inflation_nominal.is_finite()
                && inflation_nominal > 0.0,
            "nominals must be finite and positive"
        );
        crate::require!(
            spread.is_finite() && fixed_rate.is_finite(),
            "rates must be finite"
        );
        crate::require!(
            base_cpi.is_finite() && base_cpi > 0.0,
            "base CPI must be finite and positive"
        );
        crate::require!(
            std::ptr::eq(Index::settings(&*float_index), &*settings)
                && std::ptr::eq(Index::settings(&*fixed_index), &*settings),
            "indexes must share swap settings"
        );
        crate::require!(
            float_index.tenor().length() > 0
                && matches!(
                    float_index.tenor().units(),
                    TimeUnit::Weeks | TimeUnit::Months | TimeUnit::Years
                ),
            "day-tenor and overnight indexes are not supported"
        );
        crate::require!(
            i32::try_from(float_index.fixing_days()).is_ok(),
            "index fixing days overflow"
        );
        for schedule in [&float_schedule, &fixed_schedule] {
            crate::require!(
                schedule.len() >= 2,
                "schedule must contain at least two dates"
            );
            crate::require!(
                schedule
                    .dates()
                    .iter()
                    .all(|d| *d >= Date::min_date() && *d <= Date::max_date()),
                "schedule dates must be valid and non-null"
            );
            crate::require!(
                schedule.dates().windows(2).all(|pair| pair[0] < pair[1]),
                "schedule dates must increase"
            );
        }
        dates::schedule(&float_schedule, float_roll)?;
        dates::schedule(&fixed_schedule, fixed_roll)?;
        let mut floating: Leg = IborLeg::new(float_schedule, float_index)
            .with_notional(nominal)
            .with_spread(spread)
            .with_payment_day_counter(float_day_count)
            .with_payment_adjustment(float_roll)
            .coupons()?
            .into_iter()
            .map(|coupon| {
                shared(dates::CheckedIborCoupon(coupon)) as Shared<dyn crate::cashflow::CashFlow>
            })
            .collect();
        if !subtract_inflation_nominal || (nominal - inflation_nominal).abs() > 0.00001 {
            let payment = floating
                .last()
                .ok_or_else(|| crate::errors::QlError::new("empty floating leg", file!(), line!()))?
                .date();
            let amount = if subtract_inflation_nominal {
                nominal - inflation_nominal
            } else {
                nominal
            };
            floating.push(shared(SimpleCashFlow::new(amount, payment)?));
        }
        let cpi = CpiLeg::new(fixed_schedule, fixed_index, base_cpi, observation_lag)
            .with_notional(inflation_nominal)
            .with_fixed_rate(fixed_rate)
            .with_payment_day_counter(fixed_day_count)
            .with_payment_adjustment(fixed_roll)
            .with_observation_interpolation(interpolation)
            .with_subtract_inflation_nominal(subtract_inflation_nominal)
            .build()?;
        let pay_cpi = swap_type == SwapType::Receiver;
        let swap = Swap::new(
            vec![cpi, floating],
            vec![pay_cpi, !pay_cpi],
            settings.clone(),
        )?;
        Ok(Self {
            swap,
            spread,
            settings_flags: Self::flags(&settings),
            settings,
        })
    }

    fn flags(settings: &Settings<Date>) -> (bool, Option<bool>, bool, bool) {
        (
            settings.include_reference_date_events(),
            settings.include_todays_cash_flows(),
            settings.enforces_todays_historic_fixings(),
            settings.using_at_par_coupons(),
        )
    }
    /// Embedded generic swap with immutable legs ordered CPI, Ibor.
    pub fn swap(&self) -> &Swap {
        &self.swap
    }
    /// CPI-scaled coupons followed by the indexed terminal cashflow.
    pub fn cpi_leg(&self) -> &Leg {
        &self.swap.legs()[0]
    }
    /// Ibor coupons followed by any terminal nominal exchange.
    pub fn float_leg(&self) -> &Leg {
        &self.swap.legs()[1]
    }
    fn leg_measure(&mut self, leg: usize, bps: bool) -> QlResult<Real> {
        self.calculate()?;
        let value = if bps {
            self.swap.leg_bps(leg)?
        } else {
            self.swap.leg_npv(leg)?
        };
        crate::require!(value.is_finite(), "leg result must be finite");
        Ok(value)
    }
    /// Signed CPI-leg present value, including indexed principal.
    /// # Errors
    /// Propagates engine, fixing, curve and non-finite errors.
    pub fn fixed_leg_npv(&mut self) -> QlResult<Real> {
        self.leg_measure(0, false)
    }
    /// Signed Ibor-leg present value, including any nominal exchange.
    /// # Errors
    /// Propagates valuation errors.
    pub fn float_leg_npv(&mut self) -> QlResult<Real> {
        self.leg_measure(1, false)
    }
    /// Signed one-basis-point sensitivity to Ibor spread.
    /// # Errors
    /// Propagates valuation errors.
    pub fn float_leg_bps(&mut self) -> QlResult<Real> {
        self.leg_measure(1, true)
    }
    /// Absolute Ibor spread that zeroes NPV with the CPI leg unchanged.
    /// # Errors
    /// Rejects zero/non-finite sensitivity and propagates valuation errors.
    pub fn fair_spread(&mut self) -> QlResult<Real> {
        let npv = self.npv()?;
        let sensitivity = self.float_leg_bps()? / 1e-4;
        crate::require!(
            sensitivity.is_finite() && sensitivity != 0.0,
            "null or non-finite spread sensitivity"
        );
        let value = self.spread - npv / sensitivity;
        crate::require!(value.is_finite(), "fair spread must be finite");
        Ok(value)
    }
}

impl Instrument for CpiSwap {
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
    fn setup_arguments(&self, args: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(args)
    }
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)
    }
    fn calculate(&mut self) -> QlResult<()> {
        let today = self.settings.evaluation_date().ok_or_else(|| {
            crate::errors::QlError::new("evaluation date is unset", file!(), line!())
        })?;
        crate::require!(
            today >= Date::min_date() && today <= Date::max_date(),
            "evaluation date must be valid and non-null"
        );
        let flags = Self::flags(&self.settings);
        if flags != self.settings_flags {
            self.swap.base().invalidate_silently();
            self.settings_flags = flags;
        }
        self.swap.calculate()?;
        crate::require!(self.swap.npv()?.is_finite(), "NPV must be finite");
        Ok(())
    }
}
