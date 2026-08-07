//! Integral credit-default-swap pricing engine.
//!
//! Port of `ql/pricingengines/credit/integralcdsengine.{hpp,cpp}`:
//! [`IntegralCdsEngine`] is a near-twin of
//! [`MidPointCdsEngine`](super::MidPointCdsEngine) that replaces the single
//! mid-point default approximation with a numerical integration over a grid of
//! a fixed step (`integralcdsengine.cpp:105-139`). Everything else is the same
//! code in C++ (`.cpp:46-71` and `:142-195` against `midpointcdsengine.cpp:43-68`
//! and `:132-185`), so the deviations documented on
//! [`midpointcdsengine`](super::midpointcdsengine) apply here unchanged. Two
//! things are this engine's own. A null step is rejected in `calculate()`
//! rather than in the constructor, where C++ puts it (`.cpp:44-45`); QuantLib's
//! default-constructed `Period()` is the `0 Days` [`Period::default`] builds.
//! And the grid probabilities come from `defaultProbability(Date)` at each grid
//! point (`.cpp:108`, `:116`) rather than from the between-dates form the
//! mid-point engine uses, so a grid point before the curve's reference date is
//! an error rather than a clamp - as it is in C++, through `checkRange`.

use crate::cashflow::CashFlow;
use crate::errors::QlResult;
use crate::event::Event;
use crate::instruments::{CdsArguments, CdsEngine, CdsResults, ProtectionSide};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real};
use crate::{fail, handle::Handle, require};

/// The one-basis-point move sensitivities are quoted against (`integralcdsengine.cpp:179`).
const BASIS_POINT: Rate = 1.0e-4;

/// Integral engine for credit-default swaps: each live premium period is walked
/// on a grid of `integration_step`, accumulating the accrual and the protection
/// payment against the default probability gained over each step.
pub struct IntegralCdsEngine {
    base: CdsEngine,
    integration_step: Period,
    probability: Handle<dyn DefaultProbabilityTermStructure>,
    recovery_rate: Real,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    settings: Shared<Settings<Date>>,
}

impl IntegralCdsEngine {
    /// Builds the engine over the two curve handles it registers with
    /// (`integralcdsengine.cpp:31-41`). `integration_step` is stored as given
    /// and checked when the engine prices, not here.
    pub fn new(
        integration_step: Period,
        probability: Handle<dyn DefaultProbabilityTermStructure>,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settings: Shared<Settings<Date>>,
    ) -> IntegralCdsEngine {
        let base = CdsEngine::new(CdsArguments::default(), CdsResults::default());
        probability.register_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        IntegralCdsEngine {
            base,
            integration_step,
            probability,
            recovery_rate,
            discount_curve,
            include_settlement_date_flows,
            settings,
        }
    }
}

impl AsObservable for IntegralCdsEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for IntegralCdsEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `integralcdsengine.cpp:43-195`.
    fn calculate(&mut self) -> QlResult<()> {
        let step = self.integration_step;
        require!(step != Period::default(), "null period set");
        require!(
            !self.discount_curve.is_empty(),
            "no discount term structure set"
        );
        require!(
            !self.probability.is_empty(),
            "no probability term structure set"
        );
        let discount = self.discount_curve.current_link()?;
        let probability = self.probability.current_link()?;

        let Some(today) = self.settings.evaluation_date() else {
            fail!("no evaluation date set: the integral CDS engine needs today's date");
        };
        let settlement_date = discount.reference_date()?;

        let arguments = self.base.arguments();
        let (Some(side), Some(notional), Some(spread)) =
            (arguments.side, arguments.notional, arguments.spread)
        else {
            fail!("side, notional or spread not set");
        };
        let Some(claim) = arguments.claim.as_ref() else {
            fail!("claim not set");
        };
        let Some(protection_start) = arguments.protection_start else {
            fail!("protection start date not set");
        };
        let Some(upfront_payment) = arguments.upfront_payment.as_ref() else {
            fail!("upfront payment not set");
        };

        let mut upfront_pvo1 = 0.0;
        let mut upfront_npv = 0.0;
        if !upfront_payment.has_occurred(
            &self.settings,
            Some(settlement_date),
            self.include_settlement_date_flows,
        )? {
            upfront_pvo1 = discount.discount_date(upfront_payment.date(), false)?;
            upfront_npv = upfront_pvo1 * upfront_payment.amount()?;
        }

        let mut accrual_rebate_npv = 0.0;
        if let Some(rebate) = arguments.accrual_rebate.as_ref()
            && !rebate.has_occurred(
                &self.settings,
                Some(settlement_date),
                self.include_settlement_date_flows,
            )?
        {
            accrual_rebate_npv = discount.discount_date(rebate.date(), false)? * rebate.amount()?;
        }

        let mut coupon_leg_npv = 0.0;
        let mut default_leg_npv = 0.0;
        for (i, flow) in arguments.leg.iter().enumerate() {
            if flow.has_occurred(
                &self.settings,
                Some(settlement_date),
                self.include_settlement_date_flows,
            )? {
                continue;
            }
            let Some(coupon) = flow.as_coupon() else {
                fail!("premium leg flow #{} is not a coupon", i + 1);
            };

            let payment_date = flow.date();
            let end_date = coupon.accrual_end_date();
            let start_date = if i == 0 {
                protection_start
            } else {
                coupon.accrual_start_date()
            };
            let effective_start_date = if start_date <= today && today <= end_date {
                today
            } else {
                start_date
            };
            let coupon_amount = coupon.amount()?;
            let end_discount = discount.discount_date(payment_date, false)?;
            let survival = probability.survival_probability_date(payment_date, false)?;
            coupon_leg_npv += survival * coupon_amount * end_discount;

            let mut d0 = effective_start_date;
            let mut d1 = std::cmp::min(d0 + step, end_date);
            let mut p0 = probability.default_probability_date(d0, false)?;
            loop {
                let discount_factor = if arguments.pays_at_default_time {
                    discount.discount_date(d1, false)?
                } else {
                    end_discount
                };
                let p1 = probability.default_probability_date(d1, false)?;
                let default = p1 - p0;

                if arguments.settles_accrual {
                    let accrual = if arguments.pays_at_default_time {
                        coupon.accrued_amount(d1)?
                    } else {
                        coupon_amount
                    };
                    coupon_leg_npv += accrual * discount_factor * default;
                }
                let claim_amount = claim.amount(&d1, notional, self.recovery_rate);
                default_leg_npv += claim_amount * discount_factor * default;
                p0 = p1;
                d0 = d1;
                d1 = std::cmp::min(d0 + step, end_date);
                if d0 >= end_date {
                    break;
                }
            }
        }

        let mut upfront_sign = 1.0;
        match side {
            ProtectionSide::Seller => {
                default_leg_npv *= -1.0;
                accrual_rebate_npv *= -1.0;
            }
            ProtectionSide::Buyer => {
                coupon_leg_npv *= -1.0;
                upfront_npv *= -1.0;
                upfront_sign = -1.0;
            }
        }

        let fair_spread = if coupon_leg_npv != 0.0 {
            Some(-default_leg_npv * spread / (coupon_leg_npv + accrual_rebate_npv))
        } else {
            None
        };
        let fair_upfront = if upfront_pvo1 > 0.0 {
            Some(
                -upfront_sign * (default_leg_npv + coupon_leg_npv + accrual_rebate_npv)
                    / (upfront_pvo1 * notional),
            )
        } else {
            None
        };
        let coupon_leg_bps = if spread != 0.0 {
            Some(coupon_leg_npv * BASIS_POINT / spread)
        } else {
            None
        };
        let upfront_bps = match arguments.upfront {
            Some(upfront) if upfront != 0.0 => Some(upfront_npv * BASIS_POINT / upfront),
            _ => None,
        };

        let results = self.base.results_mut();
        results.instrument.value =
            Some(default_leg_npv + coupon_leg_npv + upfront_npv + accrual_rebate_npv);
        results.instrument.error_estimate = None;
        results.coupon_leg_npv = Some(coupon_leg_npv);
        results.default_leg_npv = Some(default_leg_npv);
        results.upfront_npv = Some(upfront_npv);
        results.accrual_rebate_npv = Some(accrual_rebate_npv);
        results.fair_spread = fair_spread;
        results.fair_upfront = fair_upfront;
        results.coupon_leg_bps = coupon_leg_bps;
        results.upfront_bps = upfront_bps;
        Ok(())
    }
}

#[cfg(test)]
mod oracle {
    //! The numeric oracle: the integral arm of `testCachedValue`
    //! (`creditdefaultswap.cpp:119-165`), which reprices on a daily and on a
    //! weekly grid the ten-year contract the mid-point arm caches at `:98-99`,
    //! requiring the NPV back within `notional*1e-5*10` and the fair spread
    //! within `1e-5`. The fixture is the mid-point arm's, and the deviations
    //! documented on that oracle (`midpointcdsengine.rs`) carry over. A band
    //! that wide is not on its own evidence that the grid is walked, so two
    //! assertions beyond the C++ pin the mechanism: the daily result must
    //! differ from the cached mid-point value, which excludes an engine that
    //! collapsed onto the mid-point path, and the two grids must differ from
    //! each other, which excludes one that ignores its step.

    use super::*;
    use crate::instrument::Instrument;
    use crate::instruments::CreditDefaultSwap;
    use crate::interestrate::Compounding;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::FlatForward;
    use crate::time::{
        businessdayconvention::BusinessDayConvention, calendar::Calendar,
        calendars::target::Target, date::Month, dategenerationrule::DateGeneration,
        daycounters::actual360::Actual360, frequency::Frequency, schedule::Schedule,
        timeunit::TimeUnit,
    };

    const NOTIONAL: Real = 10_000.0;
    const CACHED_NPV: Real = 295.015_339_8;
    const CACHED_FAIR_RATE: Rate = 0.007_517_539_081;

    /// `creditdefaultswap.cpp:61-95`, priced on a grid of `step`.
    fn priced(step: Period) -> CreditDefaultSwap {
        let today = Date::new(9, Month::June, 2006);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let calendar: Calendar = Target::new();
        let following = BusinessDayConvention::Following;
        let convention = BusinessDayConvention::ModifiedFollowing;

        let hazard_rate = Handle::new(shared(SimpleQuote::new(0.01234)) as Shared<dyn Quote>);
        let probability = Handle::new(shared(FlatHazardRate::moving(
            0,
            calendar.clone(),
            hazard_rate,
            Actual360::new(),
            Shared::clone(&settings),
        )) as Shared<dyn DefaultProbabilityTermStructure>);
        let discount = Handle::new(shared(FlatForward::with_rate(
            today,
            0.06,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let issue_date = calendar.advance(today, -1, TimeUnit::Years, following, false);
        let maturity = calendar.advance(issue_date, 10, TimeUnit::Years, following, false);
        let schedule = Schedule::new(
            issue_date,
            maturity,
            Period::try_from(Frequency::Semiannual).unwrap(),
            calendar,
            convention,
            convention,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let mut cds = CreditDefaultSwap::new(
            ProtectionSide::Seller,
            NOTIONAL,
            0.0120,
            schedule,
            convention,
            Actual360::new(),
            true,
            true,
            Shared::clone(&settings),
        )
        .unwrap();
        let engine = IntegralCdsEngine::new(step, probability, 0.4, discount, None, settings);
        cds.base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        cds
    }

    /// `creditdefaultswap.cpp:119-165`.
    #[test]
    fn both_integration_steps_reproduce_the_cached_value() {
        let mut daily = priced(Period::new(1, TimeUnit::Days));
        let mut weekly = priced(Period::new(1, TimeUnit::Weeks));
        let daily_npv = daily.npv().unwrap();
        let weekly_npv = weekly.npv().unwrap();
        for (grid, npv, fair_rate) in [
            ("1 day", daily_npv, daily.fair_spread().unwrap()),
            ("1 week", weekly_npv, weekly.fair_spread().unwrap()),
        ] {
            assert!(
                (npv - CACHED_NPV).abs() <= NOTIONAL * 1.0e-5 * 10.0,
                "the integral engine on a {grid} grid priced the cached contract at {npv}, not {CACHED_NPV}"
            );
            assert!(
                (fair_rate - CACHED_FAIR_RATE).abs() <= 1.0e-5,
                "the integral engine on a {grid} grid quoted a fair spread of {fair_rate}, not {CACHED_FAIR_RATE}"
            );
        }

        assert!(
            (daily_npv - CACHED_NPV).abs() > 1.0e-9,
            "a daily grid reproduced {CACHED_NPV} exactly: the engine collapsed onto the mid-point"
        );
        assert!(
            daily_npv != weekly_npv,
            "the daily and weekly grids both priced {daily_npv}: the engine ignores its step"
        );
    }

    /// `integralcdsengine.cpp:44-45`: rejected when pricing, not when built.
    #[test]
    fn a_null_integration_step_is_rejected() {
        assert_eq!(
            priced(Period::default()).npv().unwrap_err().message(),
            "null period set"
        );
    }
}
