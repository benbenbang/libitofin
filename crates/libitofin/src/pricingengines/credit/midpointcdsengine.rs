//! Mid-point credit-default-swap pricing engine.
//!
//! Port of `ql/pricingengines/credit/midpointcdsengine.{hpp,cpp}`:
//! [`MidPointCdsEngine`] prices a [`CreditDefaultSwap`] by approximating each
//! premium period's default as happening at the period's mid-point, so that a
//! period contributes its survival-weighted coupon plus its default-weighted
//! accrual and protection payment. It is a `CreditDefaultSwap::engine`
//! (`midpointcdsengine.hpp:34`), built over a default-probability curve, a
//! recovery rate and a discount curve, and registers with both curve handles.
//!
//! Both legs accumulate as positive quantities and take their sign from the
//! protection side at the end (`midpointcdsengine.cpp:80-82`).
//!
//! Deviations, documented per D5/D10:
//! - The C++ global `Settings::instance()` (`midpointcdsengine.cpp:48`) becomes
//!   an explicit [`Settings`] handle the engine is built with, as for
//!   [`DiscountingSwapEngine`](crate::pricingengines::DiscountingSwapEngine);
//!   an unset evaluation date is an `Err` rather than a system-clock fall back.
//! - The `Null<Real>`/`Null<Rate>` "not available" sentinels of the results
//!   become [`None`], matching [`CdsResults`].
//! - The C++ `dynamic_pointer_cast<FixedRateCoupon>` (`.cpp:77-78`) becomes
//!   [`CashFlow::as_coupon`]. C++ dereferences the resulting null pointer when a
//!   premium flow is not a coupon; the port reports it as an error (D4).
//! - The C++ `default:` arm of the side switch (`.cpp:143-144`) has no
//!   counterpart: [`ProtectionSide`] has exactly the two variants, so there is
//!   no unknown side to reject.
//!
//! Registering with the two curve handles is faithful to `.cpp:38-39` but is
//! not exercised by the tests here, which never relink a handle after pricing.

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
use crate::types::{Rate, Real};
use crate::{fail, handle::Handle, require};

/// The one-basis-point move the leg sensitivities are quoted against
/// (`midpointcdsengine.cpp:169`).
const BASIS_POINT: Rate = 1.0e-4;

/// Mid-point engine for credit-default swaps.
///
/// Prices each live premium period against the default probability over the
/// period, placing the default at the period's mid-point.
pub struct MidPointCdsEngine {
    base: CdsEngine,
    probability: Handle<dyn DefaultProbabilityTermStructure>,
    recovery_rate: Real,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    settings: Shared<Settings<Date>>,
}

impl MidPointCdsEngine {
    /// Builds the engine over the two curve handles it registers with
    /// (`midpointcdsengine.cpp:31-40`).
    ///
    /// `include_settlement_date_flows` overrides, when set, the settings'
    /// flags for the settlement-date flow decision (the C++
    /// `includeSettlementDateFlows` optional).
    pub fn new(
        probability: Handle<dyn DefaultProbabilityTermStructure>,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settings: Shared<Settings<Date>>,
    ) -> MidPointCdsEngine {
        let base = CdsEngine::new(CdsArguments::default(), CdsResults::default());
        probability.register_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        MidPointCdsEngine {
            base,
            probability,
            recovery_rate,
            discount_curve,
            include_settlement_date_flows,
            settings,
        }
    }

    /// The default-probability curve handle the engine prices over.
    pub fn probability(&self) -> &Handle<dyn DefaultProbabilityTermStructure> {
        &self.probability
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for MidPointCdsEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for MidPointCdsEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `midpointcdsengine.cpp:42-185`.
    fn calculate(&mut self) -> QlResult<()> {
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
            fail!("no evaluation date set: the mid-point CDS engine needs today's date");
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
            let default_date = effective_start_date + (end_date - effective_start_date) / 2;

            let survival = probability.survival_probability_date(payment_date, false)?;
            let default = probability.default_probability_between_dates(
                effective_start_date,
                end_date,
                false,
            )?;

            coupon_leg_npv +=
                survival * coupon.amount()? * discount.discount_date(payment_date, false)?;
            if arguments.settles_accrual {
                if arguments.pays_at_default_time {
                    coupon_leg_npv += default
                        * coupon.accrued_amount(default_date)?
                        * discount.discount_date(default_date, false)?;
                } else {
                    coupon_leg_npv +=
                        default * coupon.amount()? * discount.discount_date(payment_date, false)?;
                }
            }

            let claim_amount = claim.amount(&default_date, notional, self.recovery_rate);
            if arguments.pays_at_default_time {
                default_leg_npv +=
                    default * claim_amount * discount.discount_date(default_date, false)?;
            } else {
                default_leg_npv +=
                    default * claim_amount * discount.discount_date(payment_date, false)?;
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
