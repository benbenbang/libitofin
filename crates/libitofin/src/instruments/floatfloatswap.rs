//! Plain same-currency swap exchanging two constant-notional Ibor coupon legs.
//!
//! This slice has no principal exchanges, caps, floors, gearing or CMS coupons.
//! Pricing uses the existing generic swap engine contract.
use crate::cashflows::IborLeg;
use crate::errors::QlResult;
use crate::indexes::{IborIndex, InterestRateIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::Swap;
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::{
    businessdayconvention::BusinessDayConvention, date::Date, daycounter::DayCounter,
    schedule::Schedule, timeunit::TimeUnit,
};
use crate::types::{Real, Spread};

/// Pays the first floating leg and receives the second, in one currency.
///
/// Both legs have the same constant positive nominal. Signed NPV and BPS follow
/// `DiscountingSwapEngine`: paid values are negative, received values positive.
/// Queries respect the engine cache and freeze state. Non-notifying Settings flags
/// invalidate the cache silently when changed.
/// The engine must use the same Settings as the supplied indexes and swap.
pub struct FloatFloatSwap {
    swap: Swap,
    spreads: [Spread; 2],
    settings: Shared<Settings<Date>>,
    settings_flags: (bool, Option<bool>, bool, bool),
}

impl FloatFloatSwap {
    /// Builds uncapped Ibor legs without exchanging principal.
    ///
    /// An omitted payment convention uses each schedule's own convention,
    /// matching native FloatFloatSwap rather than the IborLeg default.
    /// Empty forwarding handles are accepted for later linking.
    /// # Errors
    /// Rejects non-positive/non-finite nominal, non-finite spreads, malformed
    /// schedules, different/blank currencies, mismatched Settings and day-tenor
    /// indexes. Coupon-building errors are propagated.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nominal: Real,
        schedule1: Schedule,
        index1: Shared<IborIndex>,
        spread1: Spread,
        day_count1: DayCounter,
        schedule2: Schedule,
        index2: Shared<IborIndex>,
        spread2: Spread,
        day_count2: DayCounter,
        payment_convention: Option<BusinessDayConvention>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        crate::require!(
            nominal.is_finite() && nominal > 0.0,
            "nominal must be finite and positive"
        );
        crate::require!(
            spread1.is_finite() && spread2.is_finite(),
            "spreads must be finite"
        );
        crate::require!(
            index1.currency() == index2.currency(),
            "indexes must have the same currency"
        );
        crate::require!(
            !index1.currency().code().trim().is_empty()
                && !index1.currency().name().trim().is_empty(),
            "currency must not be blank"
        );
        for index in [&index1, &index2] {
            crate::require!(
                Shared::ptr_eq(index.base().settings(), &settings),
                "indexes must share swap settings"
            );
            crate::require!(
                index.tenor().units() != TimeUnit::Days,
                "day-tenor and overnight indexes are not supported"
            );
        }
        for schedule in [&schedule1, &schedule2] {
            crate::require!(
                schedule.len() >= 2,
                "schedule must contain at least two dates"
            );
            crate::require!(
                schedule
                    .dates()
                    .iter()
                    .all(|date| *date >= Date::min_date() && *date <= Date::max_date()),
                "schedule dates must be valid and non-null"
            );
            crate::require!(
                schedule.dates().windows(2).all(|pair| pair[0] < pair[1]),
                "schedule dates must increase"
            );
        }
        let build = |schedule: Schedule, index, spread, day_count| {
            let convention =
                payment_convention.unwrap_or_else(|| schedule.business_day_convention());
            IborLeg::new(schedule, index)
                .with_notional(nominal)
                .with_spread(spread)
                .with_payment_day_counter(day_count)
                .with_payment_adjustment(convention)
                .build()
        };
        let first = build(schedule1, index1, spread1, day_count1)?;
        let second = build(schedule2, index2, spread2, day_count2)?;
        let swap = Swap::new(vec![first, second], vec![true, false], settings.clone())?;
        Ok(Self {
            swap,
            spreads: [spread1, spread2],
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

    /// The embedded swap, for inspecting its immutable coupon legs.
    pub fn swap(&self) -> &Swap {
        &self.swap
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

    /// Signed paid-leg present value.
    /// # Errors
    /// Propagates missing-engine, curve-domain, fixing and non-finite errors.
    pub fn pay_leg_npv(&mut self) -> QlResult<Real> {
        self.leg_measure(0, false)
    }
    /// Signed received-leg present value.
    /// # Errors
    /// Propagates valuation errors.
    pub fn receive_leg_npv(&mut self) -> QlResult<Real> {
        self.leg_measure(1, false)
    }
    /// Signed paid-leg sensitivity for a one-basis-point spread increase.
    /// # Errors
    /// Propagates valuation errors.
    pub fn pay_leg_bps(&mut self) -> QlResult<Real> {
        self.leg_measure(0, true)
    }
    /// Signed received-leg sensitivity for a one-basis-point spread increase.
    /// # Errors
    /// Propagates valuation errors.
    pub fn receive_leg_bps(&mut self) -> QlResult<Real> {
        self.leg_measure(1, true)
    }

    fn fair_spread(&mut self, leg: usize) -> QlResult<Spread> {
        let npv = self.npv()?;
        let annuity = self.swap.leg_bps(leg)? / 1e-4;
        crate::require!(
            annuity.is_finite() && annuity != 0.0,
            "null or non-finite spread sensitivity"
        );
        let fair = self.spreads[leg] - npv / annuity;
        crate::require!(fair.is_finite(), "fair spread must be finite");
        Ok(fair)
    }
    /// Absolute paid spread that zeroes NPV while the received spread is held fixed.
    /// # Errors
    /// Propagates valuation errors; zero/non-finite sensitivity is rejected.
    pub fn fair_pay_spread(&mut self) -> QlResult<Spread> {
        self.fair_spread(0)
    }
    /// Absolute received spread that zeroes NPV while the paid spread is held fixed.
    /// # Errors
    /// Propagates valuation errors; zero/non-finite sensitivity is rejected.
    pub fn fair_receive_spread(&mut self) -> QlResult<Spread> {
        self.fair_spread(1)
    }
}

impl Instrument for FloatFloatSwap {
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
    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(arguments)
    }
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)
    }
    fn calculate(&mut self) -> QlResult<()> {
        let date = self.settings.evaluation_date().ok_or_else(|| {
            crate::errors::QlError::new("evaluation date is unset", file!(), line!())
        })?;
        crate::require!(
            date >= Date::min_date() && date <= Date::max_date(),
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
