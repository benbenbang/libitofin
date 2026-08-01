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

#[cfg(test)]
mod tests {
    //! Mechanics only: the numeric oracle for this engine is the ported
    //! `creditdefaultswap.cpp` test case, which prices a contract end to end
    //! through it. What is pinned here is the behaviour that oracle would fail
    //! on silently: which flows the settlement-date rule drops, that the first
    //! period accrues from the protection start rather than the schedule, the
    //! sign the protection side gives each leg, and the pays-at-end arms the
    //! oracle never reaches. Every assertion is an identity or a direction, so
    //! none of them encodes a number this port produced.

    use super::*;
    use crate::cashflows::SimpleCashFlow;
    use crate::instrument::Instrument;
    use crate::instruments::{CdsTerms, CreditDefaultSwap};
    use crate::interestrate::Compounding;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::{Day, Month, Year};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    const NOTIONAL: Real = 10_000_000.0;
    const SPREAD: Rate = 0.01;
    const RECOVERY: Real = 0.4;

    /// A weekday, so that `Following` leaves the schedule's dates alone and a
    /// coupon's payment date is its accrual end.
    fn date(day: Day, month: Month, year: Year) -> Date {
        Date::new(day, month, year)
    }

    /// Flat 2% hazard and flat 3% discount, both referenced at the evaluation
    /// date, which is what makes the identities below exact: the discount
    /// factor and the survival probability are `1.0` there, and the default
    /// probability over an empty period is `0.0`.
    struct Vars {
        settings: Shared<Settings<Date>>,
        probability: Handle<dyn DefaultProbabilityTermStructure>,
        discount: Handle<dyn YieldTermStructure>,
    }

    impl Vars {
        fn new(today: Date) -> Vars {
            Vars::with_settlement(today, today)
        }

        /// The discount curve carries the settlement date the engine measures
        /// occurrence against (`midpointcdsengine.cpp:49`), which a
        /// forward-settling curve separates from the evaluation date; the
        /// credit curve stays referenced at today.
        fn with_settlement(today: Date, settlement: Date) -> Vars {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today);
            let probability = Handle::new(shared(FlatHazardRate::with_rate(
                today,
                0.02,
                Actual365Fixed::new(),
            ))
                as Shared<dyn DefaultProbabilityTermStructure>);
            let discount = Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.03,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);
            Vars {
                settings,
                probability,
                discount,
            }
        }

        fn engine(&self, include_settlement_date_flows: Option<bool>) -> MidPointCdsEngine {
            MidPointCdsEngine::new(
                self.probability.clone(),
                RECOVERY,
                self.discount.clone(),
                include_settlement_date_flows,
                Shared::clone(&self.settings),
            )
        }

        /// A semiannual contract over `[start, end]` on weekday schedule dates.
        fn contract(
            &self,
            side: ProtectionSide,
            start: Date,
            end: Date,
            terms: CdsTerms,
        ) -> CreditDefaultSwap {
            let schedule = MakeSchedule::new()
                .from(start)
                .to(end)
                .with_frequency(Frequency::Semiannual)
                .with_calendar(WeekendsOnly::new())
                .with_convention(BusinessDayConvention::Following)
                .with_termination_date_convention(BusinessDayConvention::Unadjusted)
                .backwards()
                .build();
            CreditDefaultSwap::with_terms(
                side,
                NOTIONAL,
                SPREAD,
                schedule,
                BusinessDayConvention::Following,
                Actual360::new(),
                terms,
                Shared::clone(&self.settings),
            )
            .unwrap()
        }

        fn priced(&self, mut cds: CreditDefaultSwap, include: Option<bool>) -> CreditDefaultSwap {
            cds.base_mut().set_pricing_engine(
                shared_mut(self.engine(include)) as SharedMut<dyn PricingEngine>
            );
            cds
        }

        /// Runs the engine directly, which the instrument protocol will not do
        /// for a contract whose flows have all occurred: that one is expired,
        /// and `setup_expired` answers in the engine's place (`instrument.rs:317`).
        fn direct(&self, cds: &CreditDefaultSwap) -> MidPointCdsEngine {
            let mut engine = self.engine(None);
            cds.setup_arguments(engine.base.arguments_mut()).unwrap();
            engine.calculate().unwrap();
            engine
        }
    }

    /// A contract straddling the evaluation date: the first coupon pays exactly
    /// on it, the remaining three are live.
    fn straddling(vars: &Vars, side: ProtectionSide) -> CreditDefaultSwap {
        vars.contract(
            side,
            date(15, Month::December, 2025),
            date(15, Month::December, 2027),
            CdsTerms::default(),
        )
    }

    /// `midpointcdsengine.cpp:73`: the coupon paying exactly on the settlement
    /// date is dropped when `includeSettlementDateFlows` is unset and kept when
    /// it is `true`.
    ///
    /// The gap between the two is exactly that coupon's own amount, and this is
    /// an identity rather than a fitted number: the flow pays on the curves'
    /// reference date, where the discount factor and the survival probability
    /// are both `1.0`, and its effective accrual start collapses onto the
    /// evaluation date, so the default probability over the period is `0.0` and
    /// neither the accrual nor the protection term contributes.
    #[test]
    fn the_coupon_paying_on_the_settlement_date_rides_on_the_include_flag() {
        let vars = Vars::new(date(15, Month::June, 2026));
        let excluded = vars
            .priced(straddling(&vars, ProtectionSide::Seller), None)
            .npv()
            .unwrap();
        let mut included = vars.priced(straddling(&vars, ProtectionSide::Seller), Some(true));
        let amount = included.coupons()[0].amount().unwrap();
        let included = included.npv().unwrap();

        assert!(amount > 0.0, "the dropped coupon pays nothing to detect");
        assert!(
            (included - excluded - amount).abs() <= 1.0e-8 * amount,
            "including the settlement-date coupon moved the NPV by {} rather than its amount {amount}",
            included - excluded
        );
    }

    /// `midpointcdsengine.cpp:88-89`: the first period accrues protection from
    /// the protection start, not from the schedule's first date.
    ///
    /// A protection start on the evaluation date pulls the first period's
    /// effective start six months earlier than the schedule's, so the seller
    /// carries strictly more default risk and its protection leg is worth
    /// strictly more against it. The evaluation date precedes the schedule, so
    /// the two arms genuinely differ: once it is past the first accrual date
    /// both collapse onto it.
    #[test]
    fn the_first_period_accrues_from_the_protection_start() {
        let vars = Vars::new(date(15, Month::December, 2025));
        let start = date(15, Month::June, 2026);
        let end = date(15, Month::June, 2028);

        let from_schedule = |protection_start| {
            let cds = vars.contract(
                ProtectionSide::Seller,
                start,
                end,
                CdsTerms {
                    protection_start,
                    ..CdsTerms::default()
                },
            );
            let mut cds = vars.priced(cds, None);
            cds.npv().unwrap();
            cds
        };

        let mut scheduled = from_schedule(None);
        let mut earlier = from_schedule(Some(date(15, Month::December, 2025)));
        let (scheduled_leg, earlier_leg) = (
            scheduled.default_leg_npv().unwrap(),
            earlier.default_leg_npv().unwrap(),
        );

        assert!(
            earlier_leg < scheduled_leg,
            "an earlier protection start left the seller's protection leg at {earlier_leg} rather than below {scheduled_leg}"
        );
        assert!(
            earlier.fair_upfront().is_ok(),
            "an upfront payment still due should price a fair upfront"
        );
    }

    /// `midpointcdsengine.cpp:72` and `:88-89`: the leg index the protection
    /// start overrides on is the position in the leg, counting the coupons the
    /// settlement-date rule dropped, not the first one that survived it.
    ///
    /// Once the first coupon is dropped nothing reads the protection start, so
    /// moving it must leave the price bit for bit alone. An index that counted
    /// only the surviving coupons would apply it to the second one instead, and
    /// the two arms would part. The discount curve settles a year forward of
    /// the evaluation date, which is what keeps the difference visible: with
    /// the two dates equal, the surviving coupon's effective start collapses
    /// onto today under either index and the mix-up hides
    /// (`midpointcdsengine.cpp:90-91`).
    #[test]
    fn the_protection_start_overrides_on_the_position_in_the_leg() {
        let vars = Vars::with_settlement(
            date(15, Month::December, 2025),
            date(15, Month::December, 2026),
        );
        let priced = |protection_start| {
            let cds = vars.contract(
                ProtectionSide::Seller,
                date(15, Month::June, 2026),
                date(15, Month::June, 2028),
                CdsTerms {
                    protection_start,
                    ..CdsTerms::default()
                },
            );
            vars.priced(cds, None).npv().unwrap()
        };

        let scheduled = priced(None);
        assert!(scheduled != 0.0, "a zero contract would make this vacuous");
        assert_eq!(scheduled, priced(Some(date(15, Month::December, 2025))));
    }

    /// `midpointcdsengine.cpp:132-145`: the two sides value the same contract
    /// as exact opposites.
    ///
    /// Exact, not approximate, and only because the upfront payment and the
    /// accrual rebate are both zero-amount here (`creditdefaultswap.cpp:385`,
    /// `:393`): the seller negates the rebate where the buyer does not, so a
    /// contract that actually rebated would not be symmetric.
    #[test]
    fn the_two_sides_value_a_contract_as_exact_opposites() {
        let vars = Vars::new(date(15, Month::June, 2026));
        let seller = vars
            .priced(straddling(&vars, ProtectionSide::Seller), None)
            .npv()
            .unwrap();
        let buyer = vars
            .priced(straddling(&vars, ProtectionSide::Buyer), None)
            .npv()
            .unwrap();

        assert!(seller != 0.0, "a zero contract would make this vacuous");
        assert_eq!(seller, -buyer);
    }

    /// `midpointcdsengine.cpp:106-116` and `:126-129`: the pays-at-end arms,
    /// which the numeric oracle never reaches because it pays at default time.
    ///
    /// Paying the protection at the period end rather than at the mid-point
    /// discounts it further, so it is worth less; settling the accrual at the
    /// period end pays the whole coupon rather than the half accrued to the
    /// mid-point, so the premium leg is worth more.
    #[test]
    fn paying_at_the_period_end_moves_both_legs_the_way_discounting_says() {
        let vars = Vars::new(date(15, Month::December, 2025));
        let priced = |pays_at_default_time| {
            let cds = vars.contract(
                ProtectionSide::Seller,
                date(15, Month::June, 2026),
                date(15, Month::June, 2028),
                CdsTerms {
                    pays_at_default_time,
                    ..CdsTerms::default()
                },
            );
            let mut cds = vars.priced(cds, None);
            cds.npv().unwrap();
            cds
        };

        let mut at_default = priced(true);
        let mut at_end = priced(false);

        assert!(at_default.default_leg_npv().unwrap() < 0.0);
        assert!(
            at_end.default_leg_npv().unwrap() > at_default.default_leg_npv().unwrap(),
            "protection paid at the period end should be worth less, not more"
        );
        assert!(
            at_end.coupon_leg_npv().unwrap() > at_default.coupon_leg_npv().unwrap(),
            "a whole coupon settled at the period end should beat the half accrued to the mid-point"
        );
    }

    /// `midpointcdsengine.cpp:152-167`: with every premium flow behind the
    /// settlement date the premium leg is worth nothing, and neither the fair
    /// spread nor the fair upfront is available.
    #[test]
    fn a_fully_occurred_leg_prices_to_nothing_and_quotes_neither_fair_level() {
        let vars = Vars::new(date(15, Month::December, 2027));
        let cds = straddling(&vars, ProtectionSide::Seller);
        let engine = vars.direct(&cds);
        let results = engine.base.results();

        assert_eq!(results.coupon_leg_npv, Some(0.0));
        assert_eq!(results.default_leg_npv, Some(0.0));
        assert_eq!(results.instrument.value, Some(0.0));
        assert_eq!(results.fair_spread, None);
        assert_eq!(results.fair_upfront, None);
        assert_eq!(results.coupon_leg_bps, Some(0.0));
        assert_eq!(results.upfront_bps, None);
    }

    /// A premium flow that is not a coupon has no accrual to price, which C++
    /// reaches through a null `dynamic_pointer_cast` (`.cpp:77-78`) and this
    /// port reports (D4).
    #[test]
    fn a_premium_flow_that_is_not_a_coupon_is_rejected() {
        let vars = Vars::new(date(15, Month::June, 2026));
        let cds = straddling(&vars, ProtectionSide::Seller);
        let mut engine = vars.engine(None);
        let arguments = engine.base.arguments_mut();
        cds.setup_arguments(arguments).unwrap();
        arguments.leg[1] = shared(SimpleCashFlow::new(1.0, date(15, Month::June, 2027)).unwrap())
            as Shared<dyn CashFlow>;

        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "premium leg flow #2 is not a coupon"
        );
    }

    /// `midpointcdsengine.cpp:43-46`, message for message.
    #[test]
    fn an_empty_curve_handle_is_rejected() {
        let vars = Vars::new(date(15, Month::June, 2026));
        let mut no_discount = MidPointCdsEngine::new(
            vars.probability.clone(),
            RECOVERY,
            Handle::empty(),
            None,
            Shared::clone(&vars.settings),
        );
        assert_eq!(
            no_discount.calculate().unwrap_err().message(),
            "no discount term structure set"
        );

        let mut no_probability = MidPointCdsEngine::new(
            Handle::empty(),
            RECOVERY,
            vars.discount.clone(),
            None,
            Shared::clone(&vars.settings),
        );
        assert_eq!(
            no_probability.calculate().unwrap_err().message(),
            "no probability term structure set"
        );
    }
}
