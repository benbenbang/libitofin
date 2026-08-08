//! ISDA credit-default-swap pricing engine.
//!
//! Port of `ql/pricingengines/credit/isdacdsengine.{hpp,cpp}`: [`IsdaCdsEngine`]
//! prices a [`CreditDefaultSwap`](crate::instruments::CreditDefaultSwap) the way
//! the ISDA standard model does, integrating both legs over the pillar dates of
//! the two curves it is built with rather than over the premium schedule alone.
//! Three flags - [`NumericalFix`], [`AccrualBias`] and
//! [`ForwardsInCouponPeriod`] - select which of that model's known
//! approximations the engine reproduces, so that it can be graded against the
//! standard model's C code and not only against the theory. They are documented
//! against the two references named at `isdacdsengine.hpp:36-47`: [1] OpenGamma's
//! note on the ISDA model and [2] Markit's proposed numerical fix.
//!
//! The model is specified against curves of a fixed shape, so the engine refuses
//! anything else outright (`isdacdsengine.cpp:62-98`).
//!
//! Deviations, documented per D5/D10:
//! - The C++ global `Settings::instance()` (`isdacdsengine.cpp:70`) becomes an
//!   explicit [`Settings`] handle the engine is built with, as for
//!   [`MidPointCdsEngine`](super::MidPointCdsEngine); an unset evaluation date is
//!   an `Err` rather than a system-clock fall back.
//! - The three range checks on the flags (`isdacdsengine.cpp:54-60`) have no
//!   counterpart: they reject a `NumericalFix` that is neither `None` nor
//!   `Taylor`, which a C++ enum permits and a Rust one does not.
//! - The C++ `dynamic_pointer_cast<FixedRateCoupon>` on each premium flow
//!   (`isdacdsengine.cpp:205`) becomes
//!   [`CashFlow::as_coupon`](crate::cashflow::CashFlow::as_coupon), as it does for
//!   [`MidPointCdsEngine`](super::MidPointCdsEngine): C++ dereferences the
//!   resulting null pointer when a flow is not a coupon and the port reports it
//!   (D4). The narrowing stops at `dyn Coupon` rather than at the fixed-rate
//!   coupon, which is every member the kernel reads; a floating-rate coupon in
//!   the leg would therefore price here where C++ is undefined.
//! - The C++ `default:` arm of the side switch (`isdacdsengine.cpp:322-323`) has
//!   no counterpart: [`ProtectionSide`] has exactly the two variants, so there
//!   is no unknown side to reject.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::{Event, event_has_occurred};
use crate::instruments::{
    CdsArguments, CdsEngine, CdsResults, Claim, FaceValueClaim, ProtectionSide,
};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual360::Actual360;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::types::{Rate, Real};
use crate::{fail, handle::Handle, require};

use super::isda_node_grid;

/// The one-basis-point move the leg sensitivities are quoted against
/// (`isdacdsengine.cpp:349`), as for
/// [`MidPointCdsEngine`](super::MidPointCdsEngine).
const BASIS_POINT: Rate = 1.0e-4;

/// How the engine keeps the integrands' `f_i + h_i` denominators away from zero
/// (`isdacdsengine.hpp:66-70`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalFix {
    /// No fix: `10^-50` is added to the denominators instead ([1] footnote 26).
    /// C++ spells this variant `None` (`hpp:67`); it is renamed here to keep
    /// clear of [`Option::None`], which every use site has in scope.
    NoFix,
    /// A Taylor expansion replaces the quotient once `f_i + h_i < 10^-4` ([2]).
    Taylor,
}

/// Whether the premium leg carries the standard model's half-day accrual bias
/// (`isdacdsengine.hpp:72-76`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccrualBias {
    /// The second, erroneous term of [1] formula (50) is included, as the
    /// standard model's C code does before version 1.8.2.
    HalfDayBias,
    /// It is left out, as from 1.8.2 on.
    NoBias,
}

/// How the engine treats forward rates inside a coupon period
/// (`isdacdsengine.hpp:78-83`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardsInCouponPeriod {
    /// The second, erroneous term of [1] formula (52) is included.
    Flat,
    /// It is left out, which with [`AccrualBias::NoBias`] is the theoretically
    /// correct setting (`isdacdsengine.hpp:59-61`).
    Piecewise,
}

/// ISDA standard-model engine for credit-default swaps.
///
/// The client is responsible for supplying curves built to the ISDA
/// specification; the engine checks the properties it can and refuses the rest
/// (`isdacdsengine.hpp:85-96`).
pub struct IsdaCdsEngine {
    base: CdsEngine,
    probability: Handle<dyn DefaultProbabilityTermStructure>,
    recovery_rate: Real,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    numerical_fix: NumericalFix,
    accrual_bias: AccrualBias,
    forwards_in_coupon_period: ForwardsInCouponPeriod,
    settings: Shared<Settings<Date>>,
}

impl IsdaCdsEngine {
    /// Builds the engine over the two curve handles it registers with
    /// (`isdacdsengine.cpp:36-50`), on the C++ default flags `Taylor` /
    /// `HalfDayBias` / `Piecewise` (`isdacdsengine.hpp:101-103`).
    ///
    /// The arguments are [`MidPointCdsEngine::new`](super::MidPointCdsEngine::new)'s,
    /// so the two engines are interchangeable at a call site that prices on
    /// either.
    pub fn new(
        probability: Handle<dyn DefaultProbabilityTermStructure>,
        recovery_rate: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settings: Shared<Settings<Date>>,
    ) -> IsdaCdsEngine {
        let base = CdsEngine::new(CdsArguments::default(), CdsResults::default());
        probability.register_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        IsdaCdsEngine {
            base,
            probability,
            recovery_rate,
            discount_curve,
            include_settlement_date_flows,
            numerical_fix: NumericalFix::Taylor,
            accrual_bias: AccrualBias::HalfDayBias,
            forwards_in_coupon_period: ForwardsInCouponPeriod::Piecewise,
            settings,
        }
    }

    /// Chooses the three fidelity flags, which the C++ constructor takes as
    /// trailing defaulted arguments (`isdacdsengine.hpp:98-104`).
    pub fn with_fidelity(
        mut self,
        numerical_fix: NumericalFix,
        accrual_bias: AccrualBias,
        forwards_in_coupon_period: ForwardsInCouponPeriod,
    ) -> IsdaCdsEngine {
        self.numerical_fix = numerical_fix;
        self.accrual_bias = accrual_bias;
        self.forwards_in_coupon_period = forwards_in_coupon_period;
        self
    }

    /// `isdacdsengine.cpp:62-157`: the ISDA-compatibility checks, then the
    /// integration grid and the constants the leg kernels run on.
    fn validated(&self) -> QlResult<IsdaContext> {
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
        require_act_365_fixed(discount.day_counter(), "yield")?;
        require_act_365_fixed(probability.day_counter(), "probability")?;

        let Some(eval_date) = self.settings.evaluation_date() else {
            fail!("no evaluation date set: the ISDA CDS engine needs today's date");
        };
        let reference = discount.reference_date()?;
        require!(
            reference == eval_date,
            "yield term structure reference date ({reference}) should be evaluation date ({eval_date})"
        );
        let reference = probability.reference_date()?;
        require!(
            reference == eval_date,
            "probability term structure reference date ({reference}) should be evaluation date ({eval_date})"
        );

        let arguments = self.base.arguments();
        require!(
            arguments.settles_accrual,
            "ISDA engine not compatible with non accrual paying CDS"
        );
        require!(
            arguments.pays_at_default_time,
            "ISDA engine not compatible with end period payment"
        );
        let Some(claim) = arguments.claim.as_ref() else {
            fail!("claim not set");
        };
        require!(
            claim.as_any().is_some_and(|any| any.is::<FaceValueClaim>()),
            "ISDA engine not compatible with non face value claim"
        );
        let (Some(maturity), Some(start)) = (arguments.maturity, arguments.protection_start) else {
            fail!("maturity or protection start date not set");
        };

        Ok(IsdaContext {
            discount,
            probability,
            eval_date,
            effective_protection_start: start.max(eval_date + 1),
            nodes: isda_node_grid(&self.discount_curve, &self.probability, maturity)?,
            n_fix: if self.numerical_fix == NumericalFix::NoFix {
                1.0e-50
            } else {
                0.0
            },
            recovery_rate: self.recovery_rate,
            include_settlement_date_flows: self.include_settlement_date_flows,
            accrual_bias: self.accrual_bias,
            forwards_in_coupon_period: self.forwards_in_coupon_period,
            maturity,
        })
    }

    /// `isdacdsengine.cpp:159-197`: the protection leg, integrated over the
    /// grid nodes from the day before the effective protection start out to the
    /// maturity, and scaled by what a default would claim.
    ///
    /// The value is positive here, whatever the C++ comment at `:159` says:
    /// `h_hat` is the fall in the log survival probability over a step and so is
    /// non-negative, as is the `N (1 - R)` it ends up scaled by. The sign the
    /// protection side gives the leg belongs to the results tail (`:311-324`),
    /// exactly as it does for
    /// [`MidPointCdsEngine`](super::MidPointCdsEngine) (`midpointcdsengine.cpp:143`).
    ///
    /// C++ rolls a `d1` into `d0` alongside `P0` and `Q0` (`:192-194`) that
    /// nothing reads after the two curve lookups it seeds (`:163-164`); the port
    /// keeps the lookups and drops the date.
    ///
    /// The claim is asked for its amount on the null date, as in C++ (`:196`).
    /// The engine settles face-value claims alone (`:97-98`) and those ignore
    /// the date, so nothing is lost by having no default date to offer.
    fn protection_leg_npv(
        &self,
        context: &IsdaContext,
        claim: &dyn Claim,
        notional: Real,
    ) -> QlResult<Real> {
        let opening = context.effective_protection_start - 1;
        let mut p0 = context.discount.discount_date(opening, false)?;
        let mut q0 = context
            .probability
            .survival_probability_date(opening, false)?;
        let mut protection_npv = 0.0;

        let mut index = context
            .nodes
            .partition_point(|node| *node <= context.effective_protection_start);
        while index < context.nodes.len() {
            let past_maturity = context.nodes[index] > context.maturity;
            let d1 = if past_maturity {
                context.maturity
            } else {
                context.nodes[index]
            };
            let p1 = context.discount.discount_date(d1, false)?;
            let q1 = context.probability.survival_probability_date(d1, false)?;
            let f_hat = p0.ln() - p1.ln();
            let h_hat = q0.ln() - q1.ln();
            let fhphh = f_hat + h_hat;

            protection_npv +=
                if fhphh < TAYLOR_THRESHOLD && self.numerical_fix == NumericalFix::Taylor {
                    let fhphhq = fhphh * fhphh;
                    p0 * q0
                        * h_hat
                        * (1.0 - 0.5 * fhphh + 1.0 / 6.0 * fhphhq - 1.0 / 24.0 * fhphhq * fhphh
                            + 1.0 / 120.0 * fhphhq * fhphhq)
                } else {
                    h_hat / (fhphh + context.n_fix) * (p0 * q0 - p1 * q1)
                };

            p0 = p1;
            q0 = q1;
            if past_maturity {
                break;
            }
            index += 1;
        }

        Ok(protection_npv * claim.amount(&Date::null(), notional, context.recovery_rate))
    }

    /// `isdacdsengine.cpp:201-287`: the premium leg - each live coupon carried
    /// by the survival to the day before it pays, plus what a default part way
    /// through a period would accrue.
    ///
    /// The survival factor and its one-day offset (`:218`) are the standard
    /// model's, not a discounting identity: a coupon is paid only if the name
    /// survived the day before the payment date, which is a day earlier than
    /// the date the coupon is discounted from.
    ///
    /// The specification fixes the premium leg's day count as well as the
    /// curves' (`:205-211`), so the ISDA `365/360` scaling at `:282` is the
    /// conversion between the two conventions it allows.
    fn premium_leg_npv(&self, context: &IsdaContext, leg: &Leg, notional: Real) -> QlResult<Real> {
        let mut premium_npv = 0.0;
        let mut default_accrual_npv = 0.0;

        for (position, flow) in leg.iter().enumerate() {
            let Some(coupon) = flow.as_coupon() else {
                fail!("premium leg flow #{} is not a coupon", position + 1);
            };
            let day_counter = coupon.day_counter();
            require!(
                day_counter == Actual365Fixed::new()
                    || day_counter == Actual360::new()
                    || day_counter == Actual360::with_last_day(true),
                "ISDA engine requires a coupon day counter Act/365Fixed or Act/360 ({day_counter})"
            );

            if !flow.has_occurred(
                &self.settings,
                Some(context.effective_protection_start),
                context.include_settlement_date_flows,
            )? {
                premium_npv += coupon.amount()?
                    * context.discount.discount_date(flow.date(), false)?
                    * context
                        .probability
                        .survival_probability_date(flow.date() - 1, false)?;
            }

            if event_has_occurred(
                coupon.accrual_end_date(),
                &self.settings,
                Some(context.effective_protection_start),
                Some(false),
            )? {
                continue;
            }
            default_accrual_npv += self.default_accrual(context, coupon, flow.date())?
                * notional
                * coupon.rate()?
                * 365.0
                / 360.0;
        }

        Ok(premium_npv + default_accrual_npv)
    }

    /// `isdacdsengine.cpp:223-280`: what a default inside one coupon's period
    /// accrues, per unit of notional and of coupon rate.
    ///
    /// The period is integrated from the day before its accrual starts - or
    /// before the protection does, whichever is later - to the day before it
    /// pays (`:225-227`), subdivided at the grid's own nodes when the engine was
    /// asked to see the forwards inside the period as piecewise (`:231-241`).
    /// Only the caller's guard on the accrual end (`:223-224`) keeps that
    /// subdivision well formed: it holds the period open past the effective
    /// protection start, which with a payment date no earlier than the accrual
    /// end puts the opening date strictly before the closing one, so the node
    /// range between them cannot run backwards.
    ///
    /// `tstart` is measured from the unclamped accrual start (`:228-230`), a day
    /// earlier than the accrual it weights, which is what the half-day bias
    /// half-corrects.
    fn default_accrual(
        &self,
        context: &IsdaContext,
        coupon: &dyn Coupon,
        payment_date: Date,
    ) -> QlResult<Real> {
        let start = coupon
            .accrual_start_date()
            .max(context.effective_protection_start)
            - 1;
        let end = payment_date - 1;
        let tstart = context
            .discount
            .time_from_reference(coupon.accrual_start_date() - 1)?
            - match context.accrual_bias {
                AccrualBias::HalfDayBias => 1.0 / 730.0,
                AccrualBias::NoBias => 0.0,
            };

        let mut local_nodes = vec![start];
        if context.forwards_in_coupon_period == ForwardsInCouponPeriod::Piecewise {
            let opening = context.nodes.partition_point(|node| *node <= start);
            let closing = context.nodes.partition_point(|node| *node < end);
            local_nodes.extend_from_slice(&context.nodes[opening..closing]);
        }
        local_nodes.push(end);

        let mut accrual = 0.0;
        let mut t0 = context.discount.time_from_reference(local_nodes[0])?;
        let mut p0 = context.discount.discount_date(local_nodes[0], false)?;
        let mut q0 = context
            .probability
            .survival_probability_date(local_nodes[0], false)?;
        for node in &local_nodes[1..] {
            let t1 = context.discount.time_from_reference(*node)?;
            let p1 = context.discount.discount_date(*node, false)?;
            let q1 = context
                .probability
                .survival_probability_date(*node, false)?;
            let f_hat = p0.ln() - p1.ln();
            let h_hat = q0.ln() - q1.ln();
            let fhphh = f_hat + h_hat;

            accrual += if fhphh < TAYLOR_THRESHOLD && self.numerical_fix == NumericalFix::Taylor {
                let fhphhq = fhphh * fhphh;
                h_hat
                    * p0
                    * q0
                    * ((t0 - tstart)
                        * (1.0 - 0.5 * fhphh + 1.0 / 6.0 * fhphhq - 1.0 / 24.0 * fhphhq * fhphh)
                        + (t1 - t0)
                            * (0.5 - 1.0 / 3.0 * fhphh + 1.0 / 8.0 * fhphhq
                                - 1.0 / 30.0 * fhphhq * fhphh))
            } else {
                (h_hat / (fhphh + context.n_fix))
                    * ((t1 - t0) * ((p0 * q0 - p1 * q1) / (fhphh + context.n_fix) - p1 * q1)
                        + (t0 - tstart) * (p0 * q0 - p1 * q1))
            };

            t0 = t1;
            p0 = p1;
            q0 = q1;
        }
        Ok(accrual)
    }
}

/// Below this the integrands' quotients are replaced by their Taylor expansions
/// (`isdacdsengine.cpp:183`, `:256`), when the engine was asked for them.
const TAYLOR_THRESHOLD: Real = 1.0e-4;

/// `isdacdsengine.cpp:77-84`: the specification fixes both curves on
/// Act/365 (Fixed), which C++ checks by comparing the day counters themselves.
/// A curve carrying none has no C++ counterpart - an absent one there is an
/// empty `DayCounter`, which compares unequal and trips this same check - and is
/// reported as `none`.
fn require_act_365_fixed(day_counter: Option<DayCounter>, curve: &str) -> QlResult<()> {
    match day_counter {
        Some(day_counter) if day_counter == Actual365Fixed::new() => Ok(()),
        Some(day_counter) => {
            fail!("{curve} term structure day counter ({day_counter}) should be Act/365(Fixed)")
        }
        None => fail!("{curve} term structure day counter (none) should be Act/365(Fixed)"),
    }
}

/// What the leg kernels run on once the compatibility checks have passed: the
/// C++ locals of `isdacdsengine.cpp:66-157`, gathered so that the kernels can be
/// written against them without reshaping the engine.
struct IsdaContext {
    discount: Shared<dyn YieldTermStructure>,
    probability: Shared<dyn DefaultProbabilityTermStructure>,
    /// The one field neither leg reads: the results tail measures the upfront
    /// payment and the accrual rebate against it (`isdacdsengine.cpp:292`,
    /// `:304`).
    eval_date: Date,
    effective_protection_start: Date,
    nodes: Vec<Date>,
    n_fix: Real,
    recovery_rate: Real,
    include_settlement_date_flows: Option<bool>,
    accrual_bias: AccrualBias,
    forwards_in_coupon_period: ForwardsInCouponPeriod,
    maturity: Date,
}

impl AsObservable for IsdaCdsEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for IsdaCdsEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `isdacdsengine.cpp:52-364`: the validation and the setup (`:54-157`),
    /// both leg integrations (`:159-287`), and the results tail (`:289-364`)
    /// that signs the legs by protection side and derives the fair quotes from
    /// them.
    ///
    /// The fair spread's guard is C++'s verbatim: it tests the coupon leg alone
    /// (`:331`) yet divides by the coupon leg plus the accrual rebate (`:333`),
    /// so a contract whose coupon leg is worth nothing reports none even where
    /// the rebate alone would carry the quotient. The mid-point engine holds the
    /// same quirk (`midpointcdsengine.cpp:152-155`).
    fn calculate(&mut self) -> QlResult<()> {
        let context = self.validated()?;
        let (side, notional, spread, upfront, upfront_payment, accrual_rebate) = {
            let arguments = self.base.arguments();
            let (Some(side), Some(notional), Some(spread)) =
                (arguments.side, arguments.notional, arguments.spread)
            else {
                fail!("side, notional or spread not set");
            };
            let Some(upfront_payment) = arguments.upfront_payment.as_ref() else {
                fail!("upfront payment not set");
            };
            (
                side,
                notional,
                spread,
                arguments.upfront,
                Shared::clone(upfront_payment),
                arguments.accrual_rebate.as_ref().map(Shared::clone),
            )
        };

        let (mut default_leg_npv, mut coupon_leg_npv) = {
            let arguments = self.base.arguments();
            let Some(claim) = arguments.claim.as_ref() else {
                fail!("claim not set");
            };
            (
                self.protection_leg_npv(&context, &**claim, notional)?,
                self.premium_leg_npv(&context, &arguments.leg, notional)?,
            )
        };

        let mut upfront_pvo1 = 0.0;
        let mut upfront_npv = 0.0;
        if !upfront_payment.has_occurred(
            &self.settings,
            Some(context.eval_date),
            context.include_settlement_date_flows,
        )? {
            upfront_pvo1 = context
                .discount
                .discount_date(upfront_payment.date(), false)?;
            if upfront_payment.amount()? != 0.0 {
                upfront_npv = upfront_pvo1 * upfront_payment.amount()?;
            }
        }

        let mut accrual_rebate_npv = 0.0;
        if let Some(rebate) = accrual_rebate.as_ref()
            && rebate.amount()? != 0.0
            && !rebate.has_occurred(
                &self.settings,
                Some(context.eval_date),
                context.include_settlement_date_flows,
            )?
        {
            accrual_rebate_npv =
                context.discount.discount_date(rebate.date(), false)? * rebate.amount()?;
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
        let upfront_sensitivity = upfront_pvo1 * notional;
        let fair_upfront = if upfront_sensitivity != 0.0 {
            Some(
                -upfront_sign * (default_leg_npv + coupon_leg_npv + accrual_rebate_npv)
                    / upfront_sensitivity,
            )
        } else {
            None
        };
        let coupon_leg_bps = if spread != 0.0 {
            Some(coupon_leg_npv * BASIS_POINT / spread)
        } else {
            None
        };
        let upfront_bps = match upfront {
            Some(upfront) if upfront != 0.0 => Some(upfront_npv * BASIS_POINT / upfront),
            _ => None,
        };

        let results = self.base.results_mut();
        results.instrument.value =
            Some(default_leg_npv + coupon_leg_npv + upfront_npv + accrual_rebate_npv);
        results.instrument.error_estimate = None;
        results.default_leg_npv = Some(default_leg_npv);
        results.coupon_leg_npv = Some(coupon_leg_npv);
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
mod tests {
    //! Oracle: the ISDA-compatibility block of `IsdaCdsEngine::calculate`
    //! (`isdacdsengine.cpp:54-98`), which is about which inputs the engine
    //! refuses and with which message.
    //!
    //! Every case starts from one fixture the engine accepts and corrupts a
    //! single dimension of it, and compares the message rather than only that an
    //! error came back: the checks run in a fixed order, so a case that broke two
    //! dimensions at once would pass on the wrong guard. The uncorrupted fixture
    //! is a case of its own and prices, which is what shows every guard above it
    //! passed.

    use super::*;
    use crate::instrument::Instrument;
    use crate::instruments::{Claim, CreditDefaultSwap, ProtectionSide};
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn act365f() -> DayCounter {
        Actual365Fixed::new()
    }

    fn discount(reference: Date, day_counter: DayCounter) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            0.03,
            day_counter,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn credit(
        reference: Date,
        day_counter: DayCounter,
    ) -> Handle<dyn DefaultProbabilityTermStructure> {
        Handle::new(
            shared(FlatHazardRate::with_rate(reference, 0.02, day_counter))
                as Shared<dyn DefaultProbabilityTermStructure>,
        )
    }

    /// A claim outside the downcast seam, standing in for the claims the ISDA
    /// model does not settle (`isdacdsengine.cpp:97-98`).
    struct WholeNotionalClaim;

    impl Claim for WholeNotionalClaim {
        fn amount(&self, _default_date: &Date, notional: Real, _recovery_rate: Real) -> Real {
            notional
        }
    }

    /// Arms an engine over the two curves with a contract the ISDA model
    /// covers, then corrupts one dimension of the arguments.
    fn armed(
        discount: Handle<dyn YieldTermStructure>,
        credit: Handle<dyn DefaultProbabilityTermStructure>,
        corrupt: impl FnOnce(&mut CdsArguments),
    ) -> IsdaCdsEngine {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let mut engine = IsdaCdsEngine::new(credit, 0.4, discount, None, Shared::clone(&settings));
        let schedule = MakeSchedule::new()
            .from(today())
            .to(Date::new(15, Month::June, 2028))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(WeekendsOnly::new())
            .build();
        let cds = CreditDefaultSwap::new(
            ProtectionSide::Seller,
            10_000_000.0,
            0.01,
            schedule,
            BusinessDayConvention::Following,
            Actual360::new(),
            true,
            true,
            settings,
        )
        .expect("the contract is well formed");
        cds.setup_arguments(engine.base.arguments_mut())
            .expect("the contract fills the arguments");
        corrupt(engine.base.arguments_mut());
        engine
    }

    /// What `calculate` reports for such an engine.
    fn refusal(
        discount: Handle<dyn YieldTermStructure>,
        credit: Handle<dyn DefaultProbabilityTermStructure>,
        corrupt: impl FnOnce(&mut CdsArguments),
    ) -> String {
        armed(discount, credit, corrupt)
            .calculate()
            .expect_err("the corrupted dimension is refused")
            .message()
            .to_string()
    }

    /// The fixture every argument-side case corrupts: both curves Act/365
    /// (Fixed) at the evaluation date, as the specification asks.
    fn compatible(corrupt: impl FnOnce(&mut CdsArguments)) -> String {
        refusal(
            discount(today(), act365f()),
            credit(today(), act365f()),
            corrupt,
        )
    }

    /// `isdacdsengine.cpp:77-84`.
    #[test]
    fn a_curve_that_does_not_count_act_365_fixed_is_refused() {
        assert_eq!(
            refusal(
                discount(today(), Actual360::new()),
                credit(today(), act365f()),
                |_| {}
            ),
            "yield term structure day counter (Actual/360) should be Act/365(Fixed)"
        );
        assert_eq!(
            refusal(
                discount(today(), act365f()),
                credit(today(), Actual360::new()),
                |_| {}
            ),
            "probability term structure day counter (Actual/360) should be Act/365(Fixed)"
        );
    }

    /// `isdacdsengine.cpp:85-92`: the date the curves are held against is the
    /// evaluation date the engine was threaded with (D5), not a clock.
    #[test]
    fn a_curve_referenced_off_the_evaluation_date_is_refused() {
        let tomorrow = today() + 1;
        assert_eq!(
            refusal(
                discount(tomorrow, act365f()),
                credit(today(), act365f()),
                |_| {}
            ),
            format!(
                "yield term structure reference date ({tomorrow}) should be evaluation date ({})",
                today()
            )
        );
        assert_eq!(
            refusal(
                discount(today(), act365f()),
                credit(tomorrow, act365f()),
                |_| {}
            ),
            format!(
                "probability term structure reference date ({tomorrow}) should be evaluation date ({})",
                today()
            )
        );
    }

    /// `isdacdsengine.cpp:93-98`: the three contract features the ISDA model
    /// does not cover, the last read through the [`Claim`] downcast seam.
    #[test]
    fn a_contract_feature_the_isda_model_does_not_cover_is_refused() {
        assert_eq!(
            compatible(|arguments| arguments.settles_accrual = false),
            "ISDA engine not compatible with non accrual paying CDS"
        );
        assert_eq!(
            compatible(|arguments| arguments.pays_at_default_time = false),
            "ISDA engine not compatible with end period payment"
        );
        assert_eq!(
            compatible(|arguments| {
                arguments.claim = Some(shared(WholeNotionalClaim) as Shared<dyn Claim>);
            }),
            "ISDA engine not compatible with non face value claim"
        );
    }

    /// The fixture every case above corrupts, left alone: it clears every guard
    /// and prices, which is what makes each of those cases a single-dimension
    /// corruption.
    #[test]
    fn a_compatible_contract_prices() {
        let mut engine = armed(
            discount(today(), act365f()),
            credit(today(), act365f()),
            |_| {},
        );
        engine.calculate().expect("the fixture clears every guard");
        assert!(engine.base.results().instrument.value.is_some());
    }

    /// What the checks leave behind for the kernels of #796: the flags the
    /// caller chose (`isdacdsengine.hpp:98-104`), the `10^-50` the no-fix
    /// variant puts into the denominators (`:157`), the protection start pushed
    /// past the evaluation date (`:100-102`) and the integration grid
    /// (`:150-156`), which two flat curves leave as the maturity alone.
    #[test]
    fn the_checks_leave_the_kernels_the_flags_and_the_grid() {
        let fixture = || {
            armed(
                discount(today(), act365f()),
                credit(today(), act365f()),
                |_| {},
            )
        };
        let defaulted = fixture().validated().expect("the fixture is compatible");
        assert_eq!(defaulted.n_fix, 0.0);
        assert_eq!(defaulted.accrual_bias, AccrualBias::HalfDayBias);
        assert_eq!(
            defaulted.forwards_in_coupon_period,
            ForwardsInCouponPeriod::Piecewise
        );
        assert_eq!(defaulted.effective_protection_start, today() + 1);
        assert_eq!(defaulted.nodes, vec![Date::new(15, Month::June, 2028)]);

        let chosen = fixture()
            .with_fidelity(
                NumericalFix::NoFix,
                AccrualBias::NoBias,
                ForwardsInCouponPeriod::Flat,
            )
            .validated()
            .expect("the fixture is compatible");
        assert_eq!(chosen.n_fix, 1.0e-50);
        assert_eq!(chosen.accrual_bias, AccrualBias::NoBias);
        assert_eq!(
            chosen.forwards_in_coupon_period,
            ForwardsInCouponPeriod::Flat
        );
    }
}

#[cfg(test)]
mod protection_leg {
    //! Oracle: the protection-leg integration (`isdacdsengine.cpp:159-199`).
    //!
    //! The grid this walks is not something a contract can be asked for, so
    //! every case grades it against a closed form instead of against a repriced
    //! contract. On curves of a flat continuous rate `r` and a flat hazard `h`,
    //! a step's exact integrand `h_hat / (f_hat + h_hat) (P0 Q0 - P1 Q1)`
    //! collapses to `h/(r+h) (P0 Q0 - P1 Q1)`, so the sum telescopes to
    //! `h/(r+h) (P0 Q0 - P_end Q_end)` whatever the nodes in between are. That
    //! value is arrived at without walking anything, which is what makes it an
    //! oracle for the walk rather than a restatement of it.
    //!
    //! The discount curve is interpolated rather than flat even though it prices
    //! flat: `isda_node_grid` takes the grid from the curves' own pillars, so
    //! two flat curves leave it as the maturity alone and nothing about the walk
    //! over it could be seen (which is what the scaffold's own grid case pins).
    //!
    //! The full numeric gate for this engine is the Markit grid of #798; these
    //! are the mechanical pins that gate cannot localise.

    use super::*;
    use crate::cashflows::SimpleCashFlow;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::InterpolatedDiscountCurve;
    use crate::time::date::{Month, SerialNumber};

    const NOTIONAL: Real = 10_000_000.0;
    const RECOVERY: Real = 0.4;

    pub(super) fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    pub(super) fn act365f() -> DayCounter {
        Actual365Fixed::new()
    }

    /// Act/365 (Fixed) time from the evaluation date, which both curves count
    /// in and which the closed form below is stated in.
    pub(super) fn years(days: SerialNumber) -> Real {
        Real::from(days) / 365.0
    }

    /// A log-linear discount curve pillared at `offsets` days from today, its
    /// factors set to `exp(-rate t)`. It prices as a flat continuous curve - log
    /// linear in the discount factor is linear in `-rate t` - while contributing
    /// those pillars to the ISDA grid.
    fn nodal_discount(rate: Real, offsets: &[SerialNumber]) -> Handle<dyn YieldTermStructure> {
        let dates = offsets.iter().map(|days| today() + *days).collect();
        let discounts = offsets
            .iter()
            .map(|days| (-rate * years(*days)).exp())
            .collect();
        Handle::new(shared(
            InterpolatedDiscountCurve::<LogLinear>::new(dates, discounts, act365f(), None)
                .expect("the pillars increase and open at a discount factor of 1"),
        ) as Shared<dyn YieldTermStructure>)
    }

    /// The protection leg the walk should sum to, in closed form. The
    /// integration opens at the day before the effective protection start, which
    /// for a protection start on or before today is today itself, where the time
    /// is zero and both curves are `1`.
    fn analytic(rate: Real, hazard: Real, end: SerialNumber) -> Real {
        let total = rate + hazard;
        hazard / total * (1.0 - (-total * years(end)).exp()) * NOTIONAL * (1.0 - RECOVERY)
    }

    /// The protection leg of a contract on those curves, read out of the priced
    /// results.
    ///
    /// The arguments are set directly rather than through a contract: the
    /// protection leg reads only four of them, and a schedule would tie the
    /// maturity to a coupon date and so hide the clamp below. The rest are the
    /// least the results tail needs, on the buyer's side, which is the one that
    /// leaves the protection leg its own sign (`isdacdsengine.cpp:311-324`).
    fn protection_leg(
        rate: Real,
        hazard: Real,
        offsets: &[SerialNumber],
        maturity: SerialNumber,
        numerical_fix: NumericalFix,
    ) -> Real {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let credit = Handle::new(
            shared(FlatHazardRate::with_rate(today(), hazard, act365f()))
                as Shared<dyn DefaultProbabilityTermStructure>,
        );
        let mut engine = IsdaCdsEngine::new(
            credit,
            RECOVERY,
            nodal_discount(rate, offsets),
            None,
            settings,
        )
        .with_fidelity(
            numerical_fix,
            AccrualBias::HalfDayBias,
            ForwardsInCouponPeriod::Piecewise,
        );

        let arguments = engine.base.arguments_mut();
        arguments.notional = Some(NOTIONAL);
        arguments.claim = Some(shared(FaceValueClaim) as Shared<dyn Claim>);
        arguments.protection_start = Some(today());
        arguments.maturity = Some(today() + maturity);
        arguments.settles_accrual = true;
        arguments.pays_at_default_time = true;
        arguments.side = Some(ProtectionSide::Buyer);
        arguments.spread = Some(0.0);
        arguments.upfront_payment = Some(shared(
            SimpleCashFlow::new(0.0, today()).expect("a flow of nothing is well formed"),
        ));

        engine.calculate().expect("the arguments are complete");
        engine
            .base
            .results()
            .default_leg_npv
            .expect("the protection leg is valued")
    }

    /// `isdacdsengine.cpp:162-195`: the walk over a grid of several pillars sums
    /// to the closed form, and does so positive - the C++ comment at `:159`
    /// notwithstanding.
    #[test]
    fn the_walk_over_the_grid_sums_to_the_closed_form() {
        let value = protection_leg(0.03, 0.02, &[0, 30, 180, 400], 400, NumericalFix::NoFix);
        let expected = analytic(0.03, 0.02, 400);

        assert!(expected > 0.0, "a leg worth nothing would be vacuous");
        assert!(
            (value - expected).abs() <= 1.0e-12 * expected,
            "the walk summed to {value} rather than to the closed form {expected}"
        );
    }

    /// `isdacdsengine.cpp:170-172`: a pillar past the maturity is integrated to
    /// the maturity instead and ends the walk, rather than overshooting it.
    #[test]
    fn a_pillar_past_the_maturity_integrates_to_the_maturity() {
        let value = protection_leg(0.03, 0.02, &[0, 30, 180, 400], 300, NumericalFix::NoFix);
        let expected = analytic(0.03, 0.02, 300);
        let overshoot = analytic(0.03, 0.02, 400);

        assert!(
            overshoot - expected > 1.0e-3 * expected,
            "a grid that stopped at the maturity anyway would be vacuous"
        );
        assert!(
            (value - expected).abs() <= 1.0e-12 * expected,
            "the walk ran to {value} rather than to the maturity's {expected}"
        );
    }

    /// `isdacdsengine.cpp:183-190`: the Taylor series and the quotient it stands
    /// in for agree, and both meet the closed form, on a grid whose every step
    /// falls under the threshold.
    ///
    /// Two-day steps at a combined rate of `0.015` put `f + h` at `8.2e-5`,
    /// under the `1e-4` the series is selected below. It is graded against the
    /// closed form rather than only against the quotient, which is what makes
    /// the coefficients load bearing: raising the `1/6` term to `1/5` moves the
    /// sum by `2e-10` relative, and the `1/2` term by `1.4e-5`.
    #[test]
    fn the_taylor_series_meets_the_quotient_and_the_closed_form() {
        let offsets = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let taylor = protection_leg(0.01, 0.005, &offsets, 8, NumericalFix::Taylor);
        let quotient = protection_leg(0.01, 0.005, &offsets, 8, NumericalFix::NoFix);
        let expected = analytic(0.01, 0.005, 8);

        assert!(expected > 0.0, "a leg worth nothing would be vacuous");
        assert!(
            (taylor - expected).abs() <= 1.0e-12 * expected,
            "the series summed to {taylor} rather than to the closed form {expected}"
        );
        assert!(
            (quotient - expected).abs() <= 1.0e-12 * expected,
            "the quotient summed to {quotient} rather than to the closed form {expected}"
        );
        assert!(
            (taylor - quotient).abs() <= 1.0e-12 * expected,
            "the two arms parted by {} at the crossover",
            taylor - quotient
        );
    }

    /// What the numerical fix is for ([1] footnote 26, [2]), and the one case in
    /// which the two arms are told apart at all: a discount rate that cancels
    /// the hazard leaves `f + h` at zero, where the quotient divides a rounding
    /// error by `10^-50` and the series returns the limit.
    ///
    /// The limit of `h/(r+h) (1 - e^{-(r+h)t})` as `r + h` goes to zero is
    /// `h t`, which is what the series must reproduce. The quotient is asserted
    /// only to be nowhere near it, since what it returns is not a number this
    /// port should be pinned to.
    #[test]
    fn the_series_returns_the_limit_the_quotient_cannot() {
        let hazard = 0.02;
        let offsets = [0, 1, 2];
        let expected = hazard * years(2) * NOTIONAL * (1.0 - RECOVERY);
        let taylor = protection_leg(-hazard, hazard, &offsets, 2, NumericalFix::Taylor);
        let quotient = protection_leg(-hazard, hazard, &offsets, 2, NumericalFix::NoFix);

        assert!(
            (taylor - expected).abs() <= 1.0e-12 * expected,
            "the series returned {taylor} rather than the limit {expected}"
        );
        assert!(
            (quotient - expected).abs() > 0.5 * expected,
            "the quotient returned {quotient}, near enough the limit {expected} that the arms \
             cannot be told apart here"
        );
    }
}

#[cfg(test)]
mod premium_leg {
    //! Oracle: the premium-leg integration (`isdacdsengine.cpp:201-287`).
    //!
    //! The two kernels of this leg have no closed form the way the protection
    //! leg does, so what is pinned here is what the Markit grid of #798 would
    //! fail on without saying where: the survival factor and its one-day offset,
    //! which discounting alone would not produce; the day counters the
    //! specification allows; and that each of the two fidelity flags reaches the
    //! number at all, since a flag read but never applied prices identically to
    //! one that was never read.
    //!
    //! The curves are the protection leg's, for the same reason: the grid the
    //! accrual subdivides is the discount curve's own pillars, and two flat
    //! curves leave it as the maturity alone, where the piecewise flag has
    //! nothing to insert and could not be seen.

    use super::protection_leg::{act365f, today, years};
    use super::*;
    use crate::cashflow::CashFlow;
    use crate::cashflows::FixedRateCoupon;
    use crate::instrument::Instrument;
    use crate::instruments::{CreditDefaultSwap, ProtectionSide};
    use crate::interestrate::{Compounding, InterestRate};
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::InterpolatedDiscountCurve;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::{Month, SerialNumber};
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    const NOTIONAL: Real = 10_000_000.0;
    const SPREAD: Real = 0.01;
    const RECOVERY: Real = 0.4;
    const HAZARD: Real = 0.02;

    /// Pillars spread through the two years the contract runs, so that every
    /// coupon period has at least one strictly inside it.
    const PILLARS: [SerialNumber; 6] = [0, 30, 100, 200, 400, 800];

    /// The forward rate over each segment between those pillars.
    const FORWARDS: [Real; 5] = [0.01, 0.05, 0.02, 0.07, 0.03];

    /// A log-linear discount curve whose forward rate changes from pillar to
    /// pillar, which is what the piecewise flag needs to be seen at all: over a
    /// segment of constant forward the accrual integral is additive, so
    /// subdividing a flat curve returns the very same number.
    pub(super) fn stepped_discount() -> Handle<dyn YieldTermStructure> {
        let mut dates = vec![today()];
        let mut discounts = vec![1.0];
        let mut log_discount: Real = 0.0;
        for (segment, forward) in FORWARDS.iter().enumerate() {
            log_discount -= forward * (years(PILLARS[segment + 1]) - years(PILLARS[segment]));
            dates.push(today() + PILLARS[segment + 1]);
            discounts.push(log_discount.exp());
        }
        Handle::new(shared(
            InterpolatedDiscountCurve::<LogLinear>::new(dates, discounts, act365f(), None)
                .expect("the pillars increase and open at a discount factor of 1"),
        ) as Shared<dyn YieldTermStructure>)
    }

    /// That curve's discount factor, read off a curve built again rather than
    /// out of the engine's own.
    pub(super) fn discount_factor(date: Date) -> Real {
        stepped_discount()
            .current_link()
            .expect("the handle is linked")
            .discount_date(date, false)
            .expect("the date is inside the curve")
    }

    /// The flat hazard curve's survival probability, by hand.
    fn survival(date: Date) -> Real {
        (-HAZARD * years(date - today())).exp()
    }

    /// An engine over the nodal curves, armed with a two-year semiannual
    /// contract counted in `day_counter`.
    fn armed(
        day_counter: DayCounter,
        accrual_bias: AccrualBias,
        forwards_in_coupon_period: ForwardsInCouponPeriod,
    ) -> IsdaCdsEngine {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let credit = Handle::new(
            shared(FlatHazardRate::with_rate(today(), HAZARD, act365f()))
                as Shared<dyn DefaultProbabilityTermStructure>,
        );
        let mut engine = IsdaCdsEngine::new(
            credit,
            RECOVERY,
            stepped_discount(),
            None,
            Shared::clone(&settings),
        )
        .with_fidelity(
            NumericalFix::Taylor,
            accrual_bias,
            forwards_in_coupon_period,
        );

        let schedule = MakeSchedule::new()
            .from(today())
            .to(Date::new(15, Month::June, 2028))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(WeekendsOnly::new())
            .build();
        let cds = CreditDefaultSwap::new(
            ProtectionSide::Seller,
            NOTIONAL,
            SPREAD,
            schedule,
            BusinessDayConvention::Following,
            day_counter,
            true,
            true,
            settings,
        )
        .expect("the contract is well formed");
        cds.setup_arguments(engine.base.arguments_mut())
            .expect("the contract fills the arguments");
        engine
    }

    /// The premium leg of a priced contract. Every fixture here is on the
    /// seller's side, which is the one that leaves the premium leg its own sign
    /// (`isdacdsengine.cpp:311-324`).
    fn coupon_leg(engine: &mut IsdaCdsEngine) -> Real {
        engine.calculate().expect("the contract prices");
        engine
            .base
            .results()
            .coupon_leg_npv
            .expect("the premium leg is valued")
    }

    /// `isdacdsengine.cpp:205-211`: the specification fixes the premium leg's
    /// day count on the three conventions it names, and refuses the rest.
    #[test]
    fn a_coupon_counted_outside_the_isda_conventions_is_refused() {
        for accepted in [act365f(), Actual360::new(), Actual360::with_last_day(true)] {
            assert!(
                armed(
                    accepted.clone(),
                    AccrualBias::HalfDayBias,
                    ForwardsInCouponPeriod::Piecewise,
                )
                .calculate()
                .is_ok(),
                "a leg counted in {accepted} should price"
            );
        }

        assert_eq!(
            armed(
                Thirty360::with_convention(Convention::BondBasis),
                AccrualBias::HalfDayBias,
                ForwardsInCouponPeriod::Piecewise,
            )
            .calculate()
            .expect_err("the day counter is outside the specification")
            .message(),
            "ISDA engine requires a coupon day counter Act/365Fixed or Act/360 \
             (30/360 (Bond Basis))"
        );
    }

    /// `isdacdsengine.cpp:214-219`: a coupon is discounted to its payment date
    /// but carried by the survival to the day *before* it.
    ///
    /// The leg is one coupon whose accrual closed before the protection opens,
    /// which the accrual guard (`:223-224`) drops, so the premium leg is that
    /// coupon alone and can be read against the curves directly. The two mutants
    /// the identity has to exclude - the survival read on the payment date, and
    /// no survival factor at all - are computed alongside it and asserted to
    /// differ, so neither could pass this fixture.
    #[test]
    fn a_coupon_is_carried_by_the_survival_to_the_day_before_it_pays() {
        let payment = today() + 30;
        let mut engine = armed(
            act365f(),
            AccrualBias::HalfDayBias,
            ForwardsInCouponPeriod::Piecewise,
        );
        let coupon = shared(FixedRateCoupon::new(
            payment,
            NOTIONAL,
            InterestRate::new(SPREAD, act365f(), Compounding::Simple, Frequency::Annual)
                .expect("a simple annual rate is well formed"),
            today() - 180,
            today() - 1,
            None,
            None,
            None,
        ));
        let amount = Coupon::amount(&*coupon).expect("the coupon accrues an amount");
        engine.base.arguments_mut().leg = vec![coupon as Shared<dyn CashFlow>];

        let discount = discount_factor(payment);
        let expected = amount * discount * survival(payment - 1);
        let on_the_payment_date = amount * discount * survival(payment);
        let without_survival = amount * discount;

        assert!(
            (expected - on_the_payment_date).abs() > 1.0e-9 * expected,
            "a fixture whose survival does not move over a day could not see the offset"
        );
        assert!(
            (expected - without_survival).abs() > 1.0e-3 * expected,
            "a fixture surviving with certainty could not see the factor at all"
        );
        assert!(
            (coupon_leg(&mut engine) - expected).abs() <= 1.0e-12 * expected,
            "the premium leg came to {} rather than to {expected}",
            coupon_leg(&mut engine)
        );
    }

    /// `isdacdsengine.cpp:228-230`: the half-day bias reaches the accrual.
    ///
    /// The two settings differ by the `1/730` of a year the biased one shifts
    /// `tstart` back by, which is worth about a day's accrual on each period, so
    /// the biased leg is worth strictly more.
    #[test]
    fn the_half_day_bias_moves_the_accrual() {
        let biased = coupon_leg(&mut armed(
            act365f(),
            AccrualBias::HalfDayBias,
            ForwardsInCouponPeriod::Piecewise,
        ));
        let unbiased = coupon_leg(&mut armed(
            act365f(),
            AccrualBias::NoBias,
            ForwardsInCouponPeriod::Piecewise,
        ));

        assert!(unbiased > 0.0, "a leg worth nothing would be vacuous");
        assert!(
            biased > unbiased,
            "the biased leg came to {biased}, not above the unbiased {unbiased}"
        );
    }

    /// `isdacdsengine.cpp:231-241`: subdividing a coupon period at the grid's
    /// own pillars reaches the accrual.
    ///
    /// The fixture's pillars fall strictly inside the coupon periods, which is
    /// what the two settings part over: with none inside, both integrate each
    /// period in one step and price identically.
    #[test]
    fn the_pillars_inside_a_coupon_period_move_the_accrual() {
        let piecewise = coupon_leg(&mut armed(
            act365f(),
            AccrualBias::NoBias,
            ForwardsInCouponPeriod::Piecewise,
        ));
        let flat = coupon_leg(&mut armed(
            act365f(),
            AccrualBias::NoBias,
            ForwardsInCouponPeriod::Flat,
        ));

        assert!(flat > 0.0, "a leg worth nothing would be vacuous");
        assert!(
            (piecewise - flat).abs() > 1.0e-12 * flat,
            "subdividing the periods left the leg at {piecewise}, apart from {flat} by nothing"
        );
    }
}

#[cfg(test)]
mod results_tail {
    //! Oracle: the results tail (`isdacdsengine.cpp:289-364`).
    //!
    //! The numeric acceptance for this engine is the Markit grid of #798; what
    //! is pinned here is the wiring that grid would fail on without saying
    //! where - which leg each protection side negates, the identity the value
    //! sums to, the two fair quotes and the sentinels the sensitivities fall
    //! back on - and the one place the port keeps a C++ quirk rather than the
    //! formula the quirk was meant to be.
    //!
    //! Every case prices one contract whose accrual opened three months before
    //! the evaluation date and whose trade date is the evaluation date itself,
    //! so that the accrual rebate is worth a quarter's coupon rather than
    //! nothing. That is what the fair-spread case needs: the divisor is the
    //! coupon leg plus the rebate, which a rebate worth nothing could not be
    //! told from the coupon leg alone.

    use super::premium_leg::{discount_factor, stepped_discount};
    use super::protection_leg::{act365f, today};
    use super::*;
    use crate::instrument::Instrument;
    use crate::instruments::{CdsTerms, CreditDefaultSwap};
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::Month;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    const NOTIONAL: Real = 10_000_000.0;
    const SPREAD: Rate = 0.01;
    const UPFRONT: Rate = 0.05;
    const RECOVERY: Real = 0.4;
    const HAZARD: Real = 0.02;

    /// A two-year semiannual contract on the premium leg's curves, quoted on an
    /// upfront and a running spread and traded today, straddling the evaluation
    /// date inside its first coupon period.
    fn armed(
        side: ProtectionSide,
        spread: Rate,
        upfront: Rate,
        upfront_date: Option<Date>,
    ) -> (IsdaCdsEngine, CreditDefaultSwap) {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let credit = Handle::new(
            shared(FlatHazardRate::with_rate(today(), HAZARD, act365f()))
                as Shared<dyn DefaultProbabilityTermStructure>,
        );
        let mut engine = IsdaCdsEngine::new(
            credit,
            RECOVERY,
            stepped_discount(),
            None,
            Shared::clone(&settings),
        );
        let schedule = MakeSchedule::new()
            .from(Date::new(15, Month::March, 2026))
            .to(Date::new(15, Month::March, 2028))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(WeekendsOnly::new())
            .build();
        let cds = CreditDefaultSwap::with_upfront_and_terms(
            side,
            NOTIONAL,
            upfront,
            spread,
            schedule,
            BusinessDayConvention::Following,
            Actual360::new(),
            CdsTerms {
                settles_accrual: true,
                pays_at_default_time: true,
                trade_date: Some(today()),
                upfront_date,
                ..CdsTerms::default()
            },
            settings,
        )
        .expect("the contract is well formed");
        cds.setup_arguments(engine.base.arguments_mut())
            .expect("the contract fills the arguments");
        (engine, cds)
    }

    /// The four signed legs a priced fixture leaves behind, in the order the
    /// value sums them (`isdacdsengine.cpp:326`).
    fn legs(results: &CdsResults) -> (Real, Real, Real, Real) {
        (
            results
                .default_leg_npv
                .expect("the protection leg is valued"),
            results.coupon_leg_npv.expect("the premium leg is valued"),
            results.upfront_npv.expect("the upfront is valued"),
            results
                .accrual_rebate_npv
                .expect("the accrual rebate is valued"),
        )
    }

    /// `isdacdsengine.cpp:326-328`: the value is the four signed legs, and the
    /// engine offers no error estimate.
    ///
    /// The sum is asserted exactly, in the order C++ adds it in: the legs are
    /// read back out of the results, so anything but the same additions would
    /// be a different number, not a rounding of this one.
    #[test]
    fn the_value_sums_the_four_signed_legs() {
        for side in [ProtectionSide::Seller, ProtectionSide::Buyer] {
            let (mut engine, _) = armed(side, SPREAD, UPFRONT, None);
            engine.calculate().expect("the contract prices");
            let results = engine.base.results();
            let (default_leg, coupon_leg, upfront, rebate) = legs(results);

            for leg in [default_leg, coupon_leg, upfront, rebate] {
                assert!(
                    leg != 0.0 && leg.is_finite(),
                    "a leg worth {leg} could not tell one sum from another"
                );
            }
            assert_eq!(
                results.instrument.value,
                Some(default_leg + coupon_leg + upfront + rebate)
            );
            assert_eq!(results.instrument.error_estimate, None);
        }
    }

    /// `isdacdsengine.cpp:311-324`: the seller negates the protection leg and
    /// the rebate, the buyer the premium leg and the upfront.
    ///
    /// Each leg is pinned on its own rather than only through the sum, which
    /// two legs swapped between the arms would leave untouched; and since the
    /// two sides negate disjoint halves of the same four, the contract is worth
    /// exactly the opposite to each of them, bit for bit.
    #[test]
    fn each_side_negates_its_own_two_legs() {
        let (mut seller, _) = armed(ProtectionSide::Seller, SPREAD, UPFRONT, None);
        seller.calculate().expect("the contract prices");
        let (mut buyer, _) = armed(ProtectionSide::Buyer, SPREAD, UPFRONT, None);
        buyer.calculate().expect("the contract prices");

        let sold = legs(seller.base.results());
        let bought = legs(buyer.base.results());

        assert!(sold.0 < 0.0 && sold.1 > 0.0 && sold.2 > 0.0 && sold.3 < 0.0);
        assert!(bought.0 > 0.0 && bought.1 < 0.0 && bought.2 < 0.0 && bought.3 > 0.0);
        assert_eq!(sold.0, -bought.0);
        assert_eq!(sold.1, -bought.1);
        assert_eq!(sold.2, -bought.2);
        assert_eq!(sold.3, -bought.3);
        assert_eq!(
            seller.base.results().instrument.value,
            buyer.base.results().instrument.value.map(|value| -value)
        );
    }

    /// `isdacdsengine.cpp:331-337`, quirk included: the guard tests the premium
    /// leg alone yet the divisor is the premium leg plus the accrual rebate.
    ///
    /// The two divisors are told apart here, which is what makes this the quirk
    /// and not a restatement: the fixture's rebate is worth about a percent of
    /// its premium leg, so dividing by the premium leg alone moves the fair
    /// spread far past any rounding. The rebate is asserted to carry that
    /// weight before the quirk is asserted at all, since a rebate worth nothing
    /// would make the two formulas the same expression.
    #[test]
    fn the_fair_spread_divides_by_the_premium_leg_and_the_rebate() {
        let (mut engine, _) = armed(ProtectionSide::Seller, SPREAD, UPFRONT, None);
        engine.calculate().expect("the contract prices");
        let results = engine.base.results();
        let (default_leg, coupon_leg, _, rebate) = legs(results);

        assert!(
            rebate.abs() > 1.0e-4 * coupon_leg.abs(),
            "a rebate of {rebate} against a premium leg of {coupon_leg} could not tell the \
             divisors apart"
        );
        let faithful = -default_leg * SPREAD / (coupon_leg + rebate);
        let premium_leg_alone = -default_leg * SPREAD / coupon_leg;

        assert_eq!(results.fair_spread, Some(faithful));
        assert!(
            (faithful - premium_leg_alone).abs() > 1.0e-6 * faithful.abs(),
            "the two divisors agreed to {faithful}, so the quirk is not pinned"
        );
    }

    /// `isdacdsengine.cpp:339-347`: the fair upfront is what the three other
    /// signed legs come to, per unit of the upfront's own sensitivity, against
    /// the sign the protection side gives that sensitivity.
    ///
    /// The sensitivity is rebuilt from the contract's own settlement date and a
    /// curve built again, rather than from the upfront NPV the same tail
    /// wrote, so the discount factor is an input to this case and not an
    /// output of it.
    #[test]
    fn the_fair_upfront_prices_the_other_three_legs() {
        for (side, upfront_sign) in [(ProtectionSide::Seller, 1.0), (ProtectionSide::Buyer, -1.0)] {
            let (mut engine, cds) = armed(side, SPREAD, UPFRONT, None);
            engine.calculate().expect("the contract prices");
            let results = engine.base.results();
            let (default_leg, coupon_leg, _, rebate) = legs(results);

            let sensitivity =
                discount_factor(Event::date(cds.upfront_payment().as_ref())) * NOTIONAL;
            assert!(
                sensitivity > 0.0,
                "an upfront still owed should carry a sensitivity"
            );
            let expected: Real = -upfront_sign * (default_leg + coupon_leg + rebate) / sensitivity;
            let fair_upfront = results.fair_upfront.expect("the fair upfront is available");

            assert!(
                expected != 0.0 && expected.is_finite(),
                "a fair upfront of {expected} would be vacuous"
            );
            assert!(
                (fair_upfront - expected).abs() <= 1.0e-12 * expected.abs(),
                "the fair upfront came to {fair_upfront} rather than to {expected}"
            );
        }
    }

    /// `isdacdsengine.cpp:341-346`: an upfront the cash settlement date has
    /// already seen leaves no sensitivity, and so no fair upfront.
    ///
    /// The rebate settles on that same date (`creditdefaultswap.cpp:543-546`),
    /// so it is worth nothing here too, which is what leaves the guard the only
    /// thing that could return the sentinel.
    #[test]
    fn an_upfront_already_settled_prices_no_fair_upfront() {
        let (mut engine, _) = armed(ProtectionSide::Seller, SPREAD, UPFRONT, Some(today() - 1));
        engine.calculate().expect("the contract prices");
        let results = engine.base.results();

        assert_eq!(results.upfront_npv, Some(0.0));
        assert_eq!(results.accrual_rebate_npv, Some(0.0));
        assert_eq!(results.fair_upfront, None);
        assert!(results.fair_spread.is_some());
    }

    /// `isdacdsengine.cpp:349-364`: each sensitivity is a basis point of its own
    /// leg per unit of its own quote, and reports the sentinel where that quote
    /// is nothing to divide by.
    #[test]
    fn the_sensitivities_follow_the_quotes_they_divide_by() {
        let (mut quoted, _) = armed(ProtectionSide::Seller, SPREAD, UPFRONT, None);
        quoted.calculate().expect("the contract prices");
        let results = quoted.base.results();
        let (_, coupon_leg, upfront, _) = legs(results);

        assert_eq!(results.coupon_leg_bps, Some(coupon_leg * 1.0e-4 / SPREAD));
        assert_eq!(results.upfront_bps, Some(upfront * 1.0e-4 / UPFRONT));

        let (mut unquoted, _) = armed(ProtectionSide::Seller, 0.0, 0.0, None);
        unquoted.calculate().expect("the contract prices");
        let results = unquoted.base.results();

        assert_eq!(results.coupon_leg_npv, Some(0.0));
        assert_eq!(results.coupon_leg_bps, None);
        assert_eq!(results.upfront_bps, None);
        assert_eq!(results.fair_spread, None);
    }
}

/// The ISDA/Markit acceptance oracle: the 20-case upfront grid
/// (`creditdefaultswap.cpp:567-722`) and the two single-record reconciliations
/// (`:759-861` and `:863-960`), all priced through the contract's own
/// [`implied_hazard_rate`](crate::instruments::CreditDefaultSwap::implied_hazard_rate)
/// rather than a hazard rate handed in.
#[cfg(test)]
mod markit_oracle {
    use super::*;
    use crate::currency::Currency;
    use crate::event::Event;
    use crate::indexes::iborindex::IborIndex;
    use crate::instrument::Instrument;
    use crate::instruments::{MakeCreditDefaultSwap, PricingModel, ProtectionSide};
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::pricingengine::PricingEngine;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::{DepositRateHelper, PiecewiseYieldCurve, SwapRateHelper};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Integer;

    const NOTIONAL: Real = 10_000_000.0;

    /// Boost's `BOOST_CHECK_CLOSE`, which `QL_CHECK_CLOSE` forwards to: the
    /// tolerance is a *percentage*, and the strong form requires the relative
    /// difference to clear it against both operands. A `tolerance` of 1e-6 is
    /// therefore a relative 1e-8, not a relative 1e-6.
    fn assert_close(actual: Real, expected: Real, tolerance: Real, what: &str) {
        let fraction = tolerance / 100.0;
        let difference = (actual - expected).abs();
        assert!(
            difference <= fraction * actual.abs() && difference <= fraction * expected.abs(),
            "{what}: {actual} is not within {tolerance}% of {expected} \
             (relative {})",
            difference / expected.abs()
        );
    }

    /// The ISDA-convention forecasting index the C++ fixture builds inline
    /// (`creditdefaultswap.cpp:616-618` and `:796-798`).
    ///
    /// The deposit helpers take one of these too: C++ reaches the same
    /// conventions through `DepositRateHelper`'s explicit-convention
    /// constructor, which synthesises exactly this index internally
    /// (`ratehelpers.cpp:41-50`), while the port's helper takes the index
    /// outright. The forwarding handle is left empty because both helpers'
    /// builders re-point their own clone of the index at the curve being
    /// bootstrapped.
    ///
    /// C++ leaves that synthesised index an empty `Currency()`, which has no
    /// counterpart here; the currency the fixture passes is inert, since the
    /// swap helper is given its fixed-leg frequency and day counter outright
    /// and so never reaches the currency defaults.
    fn isda_ibor(
        tenor: Period,
        currency: Currency,
        settings: &Shared<Settings<Date>>,
    ) -> IborIndex {
        IborIndex::new(
            "IsdaIbor".to_string(),
            tenor,
            2,
            currency,
            WeekendsOnly::new(),
            BusinessDayConvention::ModifiedFollowing,
            false,
            Actual360::new(),
            Handle::empty(),
            Shared::clone(settings),
        )
    }

    /// The ISDA-compliant discount curve: deposits in months, swaps in years,
    /// bootstrapped log-linearly on discount factors over Act/365F
    /// (`creditdefaultswap.cpp:628-632`).
    ///
    /// C++ builds it with a moving reference date at zero settlement days; the
    /// port's piecewise curve takes the reference date outright, and the caller
    /// passes the evaluation date, which is what zero settlement days on a
    /// business day resolve to.
    fn isda_curve(
        reference: Date,
        deposits: &[(Integer, Real)],
        swaps: &[(Integer, Real)],
        float_tenor: Period,
        fixed_frequency: Frequency,
        currency: Currency,
        settings: &Shared<Settings<Date>>,
    ) -> Handle<dyn YieldTermStructure> {
        let mut helpers: Vec<Shared<dyn RateHelper>> = Vec::new();
        for (months, quote) in deposits {
            let index = isda_ibor(
                Period::new(*months, TimeUnit::Months),
                currency.clone(),
                settings,
            );
            helpers.push(DepositRateHelper::from_rate(*quote, &index) as Shared<dyn RateHelper>);
        }
        let float_index = isda_ibor(float_tenor, currency, settings);
        for (years, quote) in swaps {
            helpers.push(SwapRateHelper::from_rate(
                *quote,
                Period::new(*years, TimeUnit::Years),
                WeekendsOnly::new(),
                fixed_frequency,
                BusinessDayConvention::ModifiedFollowing,
                Thirty360::with_convention(Convention::BondBasis),
                &float_index,
            ) as Shared<dyn RateHelper>);
        }
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            reference,
            helpers,
            Actual365Fixed::new(),
            LogLinear,
        )
        .expect("the ISDA rate helpers bootstrap");
        Handle::new(curve as Shared<dyn YieldTermStructure>)
    }

    /// The engine the fixture prices on, over the flat hazard rate `h` and the
    /// three fidelity flags C++ spells out (`creditdefaultswap.cpp:686-688`).
    fn isda_engine(
        hazard_rate: Rate,
        recovery: Real,
        discount: &Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> SharedMut<dyn PricingEngine> {
        let probability = Handle::new(shared(FlatHazardRate::moving_with_rate(
            0,
            WeekendsOnly::new(),
            hazard_rate,
            Actual365Fixed::new(),
            Shared::clone(settings),
        )) as Shared<dyn DefaultProbabilityTermStructure>);
        shared_mut(
            IsdaCdsEngine::new(
                probability,
                recovery,
                discount.clone(),
                None,
                Shared::clone(settings),
            )
            .with_fidelity(
                NumericalFix::Taylor,
                AccrualBias::HalfDayBias,
                ForwardsInCouponPeriod::Piecewise,
            ),
        ) as SharedMut<dyn PricingEngine>
    }

    /// `creditdefaultswap.cpp:583-588`: the Markit deposit quotes, in months.
    const USD_DEPOSITS: [(Integer, Real); 6] = [
        (1, 0.003081),
        (2, 0.005525),
        (3, 0.007163),
        (6, 0.012413),
        (9, 0.014),
        (12, 0.015488),
    ];

    /// `creditdefaultswap.cpp:598-611`: the Markit swap quotes, in years.
    const USD_SWAPS: [(Integer, Real); 14] = [
        (2, 0.011907),
        (3, 0.01699),
        (4, 0.021198),
        (5, 0.02444),
        (6, 0.026937),
        (7, 0.028967),
        (8, 0.030504),
        (9, 0.031719),
        (10, 0.03279),
        (12, 0.034535),
        (15, 0.036217),
        (20, 0.036981),
        (25, 0.037246),
        (30, 0.037605),
    ];

    /// `creditdefaultswap.cpp:643-664`: the ISDA-model upfronts on a ten-million
    /// notional, in the term-date / spread / recovery order the loop visits.
    const MARKIT_VALUES: [Real; 20] = [
        -97798.29358,
        -97776.11889,
        914971.5977,
        894985.6298,
        -186921.3594,
        -186839.8148,
        1646623.672,
        1579803.626,
        -274298.9203,
        -274122.4725,
        2279730.93,
        2147972.527,
        -592420.2297,
        -591571.2294,
        3993550.206,
        3545843.418,
        -797501.1422,
        -795915.9787,
        4702034.688,
        4042340.999,
    ];

    /// `creditdefaultswap.cpp:567-722`: 20 conventional trades reprice to the
    /// Markit upfronts, and both sides of each are worth nothing once the fair
    /// upfront is paid.
    ///
    /// Each case implies a flat hazard rate off a quoted trade through the ISDA
    /// branch of
    /// [`implied_hazard_rate`](crate::instruments::CreditDefaultSwap::implied_hazard_rate),
    /// then prices a 1% conventional trade of the same maturity on that rate.
    ///
    /// ## Tolerance
    ///
    /// C++ picks its tolerance off `IborCoupon::Settings::usingAtParCoupons()`
    /// (`creditdefaultswap.cpp:666-669`): 1e-6 with at-par coupons, 1e-3 with
    /// indexed ones, because indexed coupons leave the bootstrapped risk-free
    /// curve "a bit off". The port threads that flag on
    /// [`Settings`](crate::settings::Settings::using_at_par_coupons) instead of
    /// a singleton, defaulting to at-par, and the fixture leaves it there - so
    /// the at-par arm is the live one and 1e-6 is the tolerance pinned. All 20
    /// cases clear it with room: the worst relative error is 1.2e-9 against the
    /// 1e-8 that a 1e-6 *percentage* allows, so no case is excluded.
    ///
    /// The tight arm is load-bearing rather than merely available. Corrupting
    /// the premium leg's survival offset (`isdacdsengine.cpp:218`) moves case 0
    /// by a relative 4.0e-6, which the loose 1e-3 arm - a relative 1e-5 - would
    /// have let through.
    #[test]
    fn the_markit_grid_reproduces_the_isda_upfronts() {
        const TOLERANCE: Real = 1.0e-6;
        let settings = shared(Settings::<Date>::new());
        let trade_date = Date::new(21, Month::May, 2009);
        settings.set_evaluation_date(trade_date);
        let discount = isda_curve(
            trade_date,
            &USD_DEPOSITS,
            &USD_SWAPS,
            Period::new(3, TimeUnit::Months),
            Frequency::Semiannual,
            Currency::usd(),
            &settings,
        );

        let term_dates = [
            Date::new(20, Month::June, 2010),
            Date::new(20, Month::June, 2011),
            Date::new(20, Month::June, 2012),
            Date::new(20, Month::June, 2016),
            Date::new(20, Month::June, 2019),
        ];
        let mut case = 0;
        for term_date in term_dates {
            for spread in [0.001, 0.1] {
                for recovery in [0.2, 0.4] {
                    let trade = |running: Rate| {
                        MakeCreditDefaultSwap::from_term_date(
                            term_date,
                            running,
                            Shared::clone(&settings),
                        )
                        .with_nominal(NOTIONAL)
                    };
                    let hazard_rate = trade(spread)
                        .build()
                        .expect("the quoted trade builds")
                        .implied_hazard_rate(
                            0.0,
                            &discount,
                            Actual365Fixed::new(),
                            recovery,
                            1.0e-10,
                            PricingModel::Isda,
                        )
                        .expect("the quoted trade inverts on the ISDA engine");
                    let engine = isda_engine(hazard_rate, recovery, &discount, &settings);

                    let mut conventional = trade(0.01).build().expect("the trade builds");
                    conventional
                        .base_mut()
                        .set_pricing_engine(SharedMut::clone(&engine));
                    let fair_upfront = conventional.fair_upfront().expect("the trade prices");
                    assert_close(
                        conventional.notional() * fair_upfront,
                        MARKIT_VALUES[case],
                        TOLERANCE,
                        &format!("case {case} ({term_date}, spread {spread}, recovery {recovery})"),
                    );

                    for side in [ProtectionSide::Buyer, ProtectionSide::Seller] {
                        let mut at_fair = trade(0.01)
                            .with_upfront_rate(fair_upfront)
                            .with_side(side)
                            .build()
                            .expect("the trade builds");
                        at_fair
                            .base_mut()
                            .set_pricing_engine(SharedMut::clone(&engine));
                        let npv = at_fair.npv().expect("the trade prices");
                        assert!(
                            npv.abs() <= TOLERANCE,
                            "case {case} {side:?} is worth {npv} at its own fair upfront"
                        );
                    }
                    case += 1;
                }
            }
        }
    }

    /// `creditdefaultswap.cpp:770-771` and `:875-876`: the EUR deposit quotes,
    /// in months. Both reconciliation cases build the same curve.
    const EUR_DEPOSITS: [(Integer, Real); 4] = [
        (1, -0.0056),
        (3, -0.005440),
        (6, -0.005190),
        (12, -0.004930),
    ];

    /// `creditdefaultswap.cpp:781-793` and `:886-898`: the EUR swap quotes, in
    /// years.
    const EUR_SWAPS: [(Integer, Real); 13] = [
        (2, -0.004820),
        (3, -0.004420),
        (4, -0.003990),
        (5, -0.003520),
        (6, -0.002970),
        (7, -0.002370),
        (8, -0.001760),
        (9, -0.001140),
        (10, -0.000540),
        (12, 0.000570),
        (15, 0.001880),
        (20, 0.002940),
        (30, 0.002820),
    ];

    /// `creditdefaultswap.cpp:765` and `:869`: today, on both cases.
    fn reconcile_value_date() -> Date {
        Date::new(26, Month::July, 2021)
    }

    const RECONCILE_NOMINAL: Real = 1.0e6;
    const RECONCILE_RECOVERY: Real = 0.4;
    const RECONCILE_TOLERANCE: Real = 1.0e-3;
    const CONVENTIONAL_SPREAD: Rate = 0.006713;

    /// The EUR curve of both reconciliation cases, and the engine over the flat
    /// hazard rate implied off a trade quoted at the conventional spread
    /// (`creditdefaultswap.cpp:764-826`, which `:868-931` repeats verbatim).
    ///
    /// The evaluation date is set before the helpers are built, since they date
    /// themselves off it.
    fn reconcile_engine(settings: &Shared<Settings<Date>>) -> (SharedMut<dyn PricingEngine>, Date) {
        let value_date = reconcile_value_date();
        settings.set_evaluation_date(value_date);
        let discount = isda_curve(
            value_date,
            &EUR_DEPOSITS,
            &EUR_SWAPS,
            Period::new(6, TimeUnit::Months),
            Frequency::Annual,
            Currency::eur(),
            settings,
        );
        let maturity = Date::new(20, Month::June, 2026);
        let hazard_rate = MakeCreditDefaultSwap::from_term_date(
            maturity,
            CONVENTIONAL_SPREAD,
            Shared::clone(settings),
        )
        .with_nominal(RECONCILE_NOMINAL)
        .build()
        .expect("the quoted trade builds")
        .implied_hazard_rate(
            0.0,
            &discount,
            Actual365Fixed::new(),
            RECONCILE_RECOVERY,
            1.0e-10,
            PricingModel::Isda,
        )
        .expect("the quoted trade inverts on the ISDA engine");
        (
            isda_engine(hazard_rate, RECONCILE_RECOVERY, &discount, settings),
            maturity,
        )
    }

    /// `creditdefaultswap.cpp:759-861`: one Markit record traded today
    /// reconciles - its value, its upfront, and the thousand of accrual it
    /// rebates, read both off the legs and off the rebate flow itself.
    ///
    /// `df` is C++'s own: the ratio of the upfront to the value, which carries
    /// the discount to cash settlement, and which the second and third
    /// assertions then divide back out.
    #[test]
    fn a_traded_today_record_reconciles_with_its_accrual_rebate() {
        const MARKIT_VALUE: Real = -16070.7;
        const EXPECTED_ACCRUAL: Real = 1000.0;
        let settings = shared(Settings::<Date>::new());
        let (engine, maturity) = reconcile_engine(&settings);

        let mut conventional =
            MakeCreditDefaultSwap::from_term_date(maturity, 0.01, Shared::clone(&settings))
                .with_nominal(RECONCILE_NOMINAL)
                .build()
                .expect("the conventional trade builds");
        conventional.base_mut().set_pricing_engine(engine);

        let npv = conventional.npv().expect("the trade prices");
        let calculated_upfront =
            conventional.notional() * conventional.fair_upfront().expect("the trade prices");
        let df = calculated_upfront / npv;
        let derived_accrual = df
            * (npv
                - conventional.default_leg_npv().expect("the trade prices")
                - conventional.coupon_leg_npv().expect("the trade prices"));
        let rebate = conventional
            .accrual_rebate()
            .expect("a rebating trade carries the flow");
        let calculated_accrual = rebate.amount().expect("the rebate is a known amount");
        let settlement_date = Event::date(rebate.as_ref());

        assert_close(npv, MARKIT_VALUE, RECONCILE_TOLERANCE, "the value");
        assert_close(
            calculated_upfront,
            df * MARKIT_VALUE,
            RECONCILE_TOLERANCE,
            "the upfront",
        );
        assert_close(
            derived_accrual,
            EXPECTED_ACCRUAL,
            RECONCILE_TOLERANCE,
            "the accrual derived from the legs",
        );
        assert_close(
            calculated_accrual,
            EXPECTED_ACCRUAL,
            RECONCILE_TOLERANCE,
            "the accrual on the rebate flow",
        );
        assert_eq!(
            settlement_date,
            WeekendsOnly::new().advance(
                reconcile_value_date(),
                3,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            )
        );
    }

    /// `creditdefaultswap.cpp:863-960`: the same record traded two years ago
    /// settled its rebate long before today, so the value drops by exactly the
    /// thousand the rebate carried and the legs account for all of what is left.
    #[test]
    fn a_record_traded_in_the_past_reconciles_without_its_accrual_rebate() {
        const MARKIT_VALUE: Real = -17070.77;
        const EXPECTED_ACCRUAL: Real = 0.0;
        let settings = shared(Settings::<Date>::new());
        let (engine, maturity) = reconcile_engine(&settings);

        let mut conventional =
            MakeCreditDefaultSwap::from_term_date(maturity, 0.01, Shared::clone(&settings))
                .with_nominal(RECONCILE_NOMINAL)
                .with_trade_date(Date::new(20, Month::July, 2019))
                .build()
                .expect("the conventional trade builds");
        conventional.base_mut().set_pricing_engine(engine);

        let npv = conventional.npv().expect("the trade prices");
        let calculated_accrual = npv
            - conventional.default_leg_npv().expect("the trade prices")
            - conventional.coupon_leg_npv().expect("the trade prices");

        assert_close(npv, MARKIT_VALUE, RECONCILE_TOLERANCE, "the value");
        assert_close(
            calculated_accrual,
            EXPECTED_ACCRUAL,
            RECONCILE_TOLERANCE,
            "the accrual derived from the legs",
        );
    }
}

/// The oracle the Python `IsdaCdsEngine` facade (#812) is pinned against: a
/// three-year contract on two flat Act/365F curves referenced at the evaluation
/// date, priced through the instrument the way a caller prices one.
///
/// The fixture lives here rather than only in the binding because the pinned
/// number has to come out of the constructor the facade wraps.
/// `crates/itofin-py/tests/test_isda_cds.py` rebuilds it byte-for-byte - the
/// same dates, curves, schedule and contract terms - and asserts the same three
/// literals, so a facade that reached a different engine, or dropped one of the
/// arguments it forwards, shows up there as a mismatch rather than as a
/// plausible number nothing grades.
///
/// The schedule is built through the same builder calls `PySchedule::new`
/// (`itofin-py/src/time.rs:528-536`) makes, including the rule and the
/// termination convention the refusal fixture leaves defaulted, and its shape
/// is asserted alongside the value so a drift in the dates is told apart from a
/// drift in the pricing. Two flat curves leave the integration grid as the
/// maturity alone (`isdanodegrid.rs:122-124`), so this is a single-node ISDA
/// reprice - still the engine's own kernels, and still distinct from the
/// mid-point engine's period-by-period integration.
#[cfg(test)]
mod binding_oracle {
    use super::*;
    use crate::instrument::Instrument;
    use crate::instruments::{CreditDefaultSwap, ProtectionSide};
    use crate::interestrate::Compounding;
    use crate::pricingengine::PricingEngine;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    const NOTIONAL: Real = 10_000_000.0;
    const SPREAD: Rate = 0.01;
    const RECOVERY: Real = 0.4;
    const HAZARD: Real = 0.02;
    const DISCOUNT_RATE: Rate = 0.03;

    /// What the fixture prices to, recorded from this test's own run to the
    /// precision an `f64` round-trips at. `test_isda_cds.py` carries the same
    /// three numbers.
    const NPV: Real = -52927.18294373818;
    const COUPON_LEG_NPV: Real = 281656.6267407311;
    const DEFAULT_LEG_NPV: Real = -334583.8096844693;

    /// Wide enough to survive a platform's `exp` differing in its last bit: the
    /// values run to `10^5`, so this is a relative `2 * 10^-13`, while the
    /// literals were recorded on one machine and are asserted on whichever one
    /// CI happens to run. Still some nine orders of magnitude below anything a
    /// mis-wired engine, curve or contract term would move the price by.
    const TOLERANCE: Real = 1.0e-8;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn maturity() -> Date {
        Date::new(15, Month::June, 2029)
    }

    /// The contract of the fixture, with the ISDA engine already installed.
    fn priced() -> CreditDefaultSwap {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let discount = Handle::new(shared(FlatForward::with_rate(
            today(),
            DISCOUNT_RATE,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let credit = Handle::new(shared(FlatHazardRate::with_rate(
            today(),
            HAZARD,
            Actual365Fixed::new(),
        )) as Shared<dyn DefaultProbabilityTermStructure>);
        let schedule = MakeSchedule::new()
            .from(today())
            .to(maturity())
            .with_frequency(Frequency::Quarterly)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .with_termination_date_convention(BusinessDayConvention::Following)
            .with_rule(DateGeneration::Forward)
            .build();
        assert_eq!(schedule.dates().len(), 13);
        assert_eq!(schedule.dates()[0], today());
        assert_eq!(schedule.dates()[12], maturity());
        let mut cds = CreditDefaultSwap::new(
            ProtectionSide::Seller,
            NOTIONAL,
            SPREAD,
            schedule,
            BusinessDayConvention::Following,
            Actual360::new(),
            true,
            true,
            Shared::clone(&settings),
        )
        .expect("the contract is well formed");
        let engine = shared_mut(IsdaCdsEngine::new(
            credit, RECOVERY, discount, None, settings,
        )) as SharedMut<dyn PricingEngine>;
        cds.base_mut().set_pricing_engine(engine);
        cds
    }

    /// The value and its two legs, which the Python oracle asserts to the same
    /// three literals.
    ///
    /// The legs are pinned alongside the value so that a Python-side mismatch
    /// says which half of the price moved rather than only that the total did.
    #[test]
    fn the_flat_curve_fixture_prices_to_the_pinned_value() {
        let mut cds = priced();
        assert!((cds.npv().expect("the contract prices") - NPV).abs() <= TOLERANCE);
        assert!(
            (cds.coupon_leg_npv().expect("the premium leg is valued") - COUPON_LEG_NPV).abs()
                <= TOLERANCE
        );
        assert!(
            (cds.default_leg_npv().expect("the protection leg is valued") - DEFAULT_LEG_NPV).abs()
                <= TOLERANCE
        );
    }
}
