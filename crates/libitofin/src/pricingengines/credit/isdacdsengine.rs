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

#[cfg(test)]
mod tests {
    //! Oracle: the ISDA-compatibility block of `IsdaCdsEngine::calculate`
    //! (`isdacdsengine.cpp:54-98`). Nothing prices yet, so what is pinned is
    //! which inputs the engine refuses and with which message.
    //!
    //! Every case starts from one fixture the engine accepts and corrupts a
    //! single dimension of it, and compares the message rather than only that an
    //! error came back: the checks run in a fixed order, so a case that broke two
    //! dimensions at once would pass on the wrong guard. The uncorrupted fixture
    //! is a case of its own and reaches the unported integrations, which is what
    //! shows every guard above it passed.

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
            .expect_err("no ISDA contract prices yet")
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
    /// and stops only on the integrations #796 has yet to write, which is what
    /// makes each of those cases a single-dimension corruption.
    #[test]
    fn a_compatible_contract_reaches_the_unported_integrations() {
        let message = compatible(|_| {});
        assert!(
            message.contains("#796"),
            "a compatible contract should stop on the unported integrations, not on {message}"
        );
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
