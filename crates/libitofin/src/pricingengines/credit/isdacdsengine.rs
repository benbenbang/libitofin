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
//! The premium-leg integration (`isdacdsengine.cpp:201-287`) and the results
//! tail (`:288-310`) are not written yet: [`IsdaCdsEngine::calculate`] values
//! the protection leg, records it, and then reports #796 rather than returning
//! a price - an engine answering `0.0` here could not be told apart from a
//! contract genuinely worth nothing.

use crate::errors::QlResult;
use crate::instruments::{CdsArguments, CdsEngine, CdsResults, Claim, FaceValueClaim};
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
    /// protection side gives the leg belongs to the results tail (`:311-320`,
    /// #797), exactly as it does for
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
/// written against them without reshaping the engine. The fields the protection
/// leg does not read are the premium leg's (#796) and the results tail's
/// (`eval_date`, first read at `:292`, #797).
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

    /// `isdacdsengine.cpp:52-310`, of which the validation, the setup
    /// (`:54-157`) and the protection leg (`:159-199`) are ported.
    ///
    /// The protection leg is recorded before the report, so a caller that means
    /// to read it can do so through [`results`](PricingEngine::results) despite
    /// the `Err`.
    fn calculate(&mut self) -> QlResult<()> {
        let context = self.validated()?;
        let arguments = self.base.arguments();
        let Some(notional) = arguments.notional else {
            fail!("notional not set");
        };
        let Some(claim) = arguments.claim.as_ref().map(Shared::clone) else {
            fail!("claim not set");
        };

        let default_leg_npv = self.protection_leg_npv(&context, &*claim, notional)?;
        self.base.results_mut().default_leg_npv = Some(default_leg_npv);
        fail!("the ISDA CDS engine values no premium leg yet: its integration is #796")
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
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::shared::shared;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::termstructures::yields::InterpolatedDiscountCurve;
    use crate::time::date::{Month, SerialNumber};

    const NOTIONAL: Real = 10_000_000.0;
    const RECOVERY: Real = 0.4;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn act365f() -> DayCounter {
        Actual365Fixed::new()
    }

    /// Act/365 (Fixed) time from the evaluation date, which both curves count
    /// in and which the closed form below is stated in.
    fn years(days: SerialNumber) -> Real {
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

    /// The protection leg of a contract on those curves, read out of the results
    /// the engine records before reporting the premium leg it has yet to value.
    ///
    /// The arguments are set directly rather than through a contract: the
    /// protection leg reads only these four, and a schedule would tie the
    /// maturity to a coupon date and so hide the clamp below.
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

        engine
            .calculate()
            .expect_err("the premium leg is not valued yet");
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
