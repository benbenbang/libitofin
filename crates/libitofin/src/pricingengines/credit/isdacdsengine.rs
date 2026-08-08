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
//!
//! ## Unported (#796, #797)
//!
//! The leg integrations (`isdacdsengine.cpp:159-286`) and the results tail
//! (`:288-310`) are not written yet: [`IsdaCdsEngine::calculate`] validates its
//! inputs, builds what those kernels run on, and then reports #796 rather than
//! returning a price - an engine answering `0.0` here could not be told apart
//! from a contract genuinely worth nothing.

use crate::errors::QlResult;
use crate::instruments::{CdsArguments, CdsEngine, CdsResults, FaceValueClaim};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::types::Real;
use crate::{fail, handle::Handle, require};

use super::isda_node_grid;

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
        })
    }
}

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
/// written against them without reshaping the engine. Every field is read by the
/// integrations of #796; the scaffold only builds it.
#[allow(dead_code)]
struct IsdaContext {
    discount: Shared<dyn YieldTermStructure>,
    probability: Shared<dyn DefaultProbabilityTermStructure>,
    eval_date: Date,
    effective_protection_start: Date,
    nodes: Vec<Date>,
    n_fix: Real,
    recovery_rate: Real,
    include_settlement_date_flows: Option<bool>,
    accrual_bias: AccrualBias,
    forwards_in_coupon_period: ForwardsInCouponPeriod,
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

    /// `isdacdsengine.cpp:52-310`, of which only the validation and the setup
    /// (`:54-157`) are ported.
    fn calculate(&mut self) -> QlResult<()> {
        let _context = self.validated()?;
        fail!("the ISDA CDS engine values no legs yet: its integrations are #796")
    }
}
