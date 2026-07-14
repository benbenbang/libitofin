//! Swaption instrument and its settlement conventions.
//!
//! Port of `ql/instruments/swaption.{hpp,cpp}`. A [`Swaption`] is an option to
//! enter a [`FixedVsFloatingSwap`](crate::instruments::FixedVsFloatingSwap) at a
//! given [`Exercise`] date, settled per a [`SettlementType`] /
//! [`SettlementMethod`] pair.
//!
//! ## Divergences from QuantLib
//!
//! - `Swaption : Option` passes a null payoff (`swaption.cpp:137`) and
//!   `arguments::validate` never reaches `Option::arguments`'s payoff
//!   requirement, so there is no Rust option base: the type owns an
//!   [`InstrumentBase`](crate::instrument::InstrumentBase) directly and carries
//!   only the `exercise` that `Option::arguments` contributes.
//! - The `swap_` member is `SharedMut<FixedVsFloatingSwap>`, not the immutable
//!   [`Shared`], because the engine (#361, `blackswaptionengine.hpp:248-259`)
//!   sets a pricing engine on the swap and reads its `fairRate`, `fixedLegBPS`
//!   and `floatingLegBPS`, all of which take `&mut`. C++'s
//!   `shared_ptr<FixedVsFloatingSwap>` permits that mutation; the faithful Rust
//!   shared-mutable pointer is [`SharedMut`].
//! - `Settlement::Type`, `Settlement::Method` and
//!   `Settlement::checkTypeAndMethodConsistency` become the free
//!   [`SettlementType`], [`SettlementMethod`] and
//!   [`check_type_and_method_consistency`]; the consistency check returns a
//!   [`QlResult`] rather than throwing.
//! - `impliedVolatility` (needs the unported implied-vol solver family) and the
//!   `deepUpdate` observer optimisation are deferred; the ported tests reach
//!   neither. The `MakeSwaption` builder is deferred to #363 (it needs the
//!   unported `SwapIndex`).

use std::any::Any;

use crate::errors::QlResult;
use crate::event::event_has_occurred;
use crate::exercise::Exercise;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::instruments::fixedvsfloatingswap::{FixedVsFloatingSwap, FixedVsFloatingSwapArguments};
use crate::instruments::swap::SwapType;
use crate::pricingengine::{Arguments, GenericEngine};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::{fail, require};

/// How a swaption is settled on exercise (`Settlement::Type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettlementType {
    /// The holder enters the physical swap.
    #[default]
    Physical,
    /// The swap's value is settled in cash.
    Cash,
}

/// The convention used to settle a swaption (`Settlement::Method`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettlementMethod {
    /// Physical delivery, over-the-counter.
    #[default]
    PhysicalOTC,
    /// Physical delivery, cleared.
    PhysicalCleared,
    /// Cash settled at the collateralised cash price.
    CollateralizedCashPrice,
    /// Cash settled off the par-yield curve.
    ParYieldCurve,
}

/// Checks that a settlement type and method are compatible
/// (`Settlement::checkTypeAndMethodConsistency`, `swaption.cpp:207`).
///
/// Physical settlement pairs with [`PhysicalOTC`](SettlementMethod::PhysicalOTC)
/// or [`PhysicalCleared`](SettlementMethod::PhysicalCleared); cash settlement
/// pairs with
/// [`CollateralizedCashPrice`](SettlementMethod::CollateralizedCashPrice) or
/// [`ParYieldCurve`](SettlementMethod::ParYieldCurve).
///
/// # Errors
///
/// The type and method must match.
pub fn check_type_and_method_consistency(
    settlement_type: SettlementType,
    settlement_method: SettlementMethod,
) -> QlResult<()> {
    match settlement_type {
        SettlementType::Physical => require!(
            matches!(
                settlement_method,
                SettlementMethod::PhysicalOTC | SettlementMethod::PhysicalCleared
            ),
            "invalid settlement method for physical settlement"
        ),
        SettlementType::Cash => require!(
            matches!(
                settlement_method,
                SettlementMethod::CollateralizedCashPrice | SettlementMethod::ParYieldCurve
            ),
            "invalid settlement method for cash settlement"
        ),
    }
    Ok(())
}

/// Argument bundle a `Swaption::engine` prices (`Swaption::arguments`).
///
/// The C++ `Swaption::arguments` derives from both
/// `FixedVsFloatingSwap::arguments` (the swap half) and `Option::arguments`
/// (which contributes only `exercise`, the payoff being null); here the swap
/// half is embedded and the exercise carried directly.
#[derive(Default)]
pub struct SwaptionArguments {
    /// The underlying swap's arguments.
    pub swap_arguments: FixedVsFloatingSwapArguments,
    /// The underlying swap.
    pub swap: Option<SharedMut<FixedVsFloatingSwap>>,
    /// How the swaption settles on exercise.
    pub settlement_type: SettlementType,
    /// The settlement method.
    pub settlement_method: SettlementMethod,
    /// The exercise schedule (the C++ `Option::arguments::exercise`).
    pub exercise: Option<Shared<dyn Exercise>>,
}

impl Arguments for SwaptionArguments {
    fn validate(&self) -> QlResult<()> {
        self.swap_arguments.validate()?;
        require!(self.swap.is_some(), "swap not set");
        require!(self.exercise.is_some(), "exercise not set");
        check_type_and_method_consistency(self.settlement_type, self.settlement_method)
    }
}

/// Engine base for swaptions (the C++ `Swaption::engine`).
pub type SwaptionEngine = GenericEngine<SwaptionArguments, InstrumentResults>;

/// An option to enter a [`FixedVsFloatingSwap`] at the exercise date.
///
/// Wraps the underlying swap through a [`SharedMut`] so that a pricing engine
/// (#361) can set an engine on it and read its fair rate and leg BPS.
pub struct Swaption {
    base: InstrumentBase,
    swap: SharedMut<FixedVsFloatingSwap>,
    settlement_type: SettlementType,
    settlement_method: SettlementMethod,
    exercise: Shared<dyn Exercise>,
    settings: Shared<Settings<Date>>,
}

impl Swaption {
    /// Builds a swaption over `swap`, exercisable per `exercise` and settled by
    /// the `settlement_type` / `settlement_method` pair (the C++ ctor,
    /// `swaption.cpp:133`).
    ///
    /// The swaption observes the swap (the C++ `registerWith(swap_)`) and, per
    /// D5, the `settings` evaluation date its expiry check reads. The
    /// (type, method) pair is not checked here; the consistency check runs in
    /// [`SwaptionArguments::validate`], matching C++.
    pub fn new(
        swap: SharedMut<FixedVsFloatingSwap>,
        exercise: Shared<dyn Exercise>,
        settlement_type: SettlementType,
        settlement_method: SettlementMethod,
        settings: Shared<Settings<Date>>,
    ) -> Swaption {
        let base = InstrumentBase::new();
        swap.borrow().base().register_observer(&base.observer());
        settings.register_eval_date_observer(&base.observer());
        Swaption {
            base,
            swap,
            settlement_type,
            settlement_method,
            exercise,
            settings,
        }
    }

    /// How the swaption settles on exercise (`settlementType()`).
    pub fn settlement_type(&self) -> SettlementType {
        self.settlement_type
    }

    /// The settlement method (`settlementMethod()`).
    pub fn settlement_method(&self) -> SettlementMethod {
        self.settlement_method
    }

    /// Whether the underlying swap pays or receives the fixed leg (`type()`,
    /// forwarded to the swap).
    pub fn swap_type(&self) -> SwapType {
        self.swap.borrow().swap_type()
    }

    /// The underlying swap (`underlying()`).
    pub fn underlying(&self) -> &SharedMut<FixedVsFloatingSwap> {
        &self.swap
    }

    /// The exercise schedule (`exercise()`, on the C++ `Option` base).
    pub fn exercise(&self) -> &Shared<dyn Exercise> {
        &self.exercise
    }
}

impl Instrument for Swaption {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    fn is_expired(&self) -> QlResult<bool> {
        event_has_occurred(self.exercise.last_date(), &self.settings, None, None)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(args) = (arguments as &mut dyn Any).downcast_mut::<SwaptionArguments>() else {
            fail!("wrong argument type");
        };
        self.swap
            .borrow()
            .setup_arguments(&mut args.swap_arguments)?;
        args.swap = Some(SharedMut::clone(&self.swap));
        args.settlement_type = self.settlement_type;
        args.settlement_method = self.settlement_method;
        args.exercise = Some(Shared::clone(&self.exercise));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Physical pairs with the two physical methods, cash with the two cash
    /// methods, and every cross pair is rejected (all eight combinations).
    #[test]
    fn consistency_accepts_matching_pairs_and_rejects_the_rest() {
        use SettlementMethod::{
            CollateralizedCashPrice, ParYieldCurve, PhysicalCleared, PhysicalOTC,
        };
        use SettlementType::{Cash, Physical};

        assert!(check_type_and_method_consistency(Physical, PhysicalOTC).is_ok());
        assert!(check_type_and_method_consistency(Physical, PhysicalCleared).is_ok());
        assert!(check_type_and_method_consistency(Cash, CollateralizedCashPrice).is_ok());
        assert!(check_type_and_method_consistency(Cash, ParYieldCurve).is_ok());

        assert_eq!(
            check_type_and_method_consistency(Physical, CollateralizedCashPrice)
                .unwrap_err()
                .message(),
            "invalid settlement method for physical settlement"
        );
        assert!(check_type_and_method_consistency(Physical, ParYieldCurve).is_err());
        assert_eq!(
            check_type_and_method_consistency(Cash, PhysicalOTC)
                .unwrap_err()
                .message(),
            "invalid settlement method for cash settlement"
        );
        assert!(check_type_and_method_consistency(Cash, PhysicalCleared).is_err());
    }

    use crate::cashflow::CashFlow;
    use crate::cashflows::SimpleCashFlow;
    use crate::exercise::EuropeanExercise;
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::instruments::fixedvsfloatingswap::FloatingArgumentsFn;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A two-period annual schedule the fixed leg is built over.
    fn fixed_schedule() -> crate::time::schedule::Schedule {
        MakeSchedule::new()
            .from(Date::new(7, Month::July, 2027))
            .to(Date::new(7, Month::July, 2029))
            .with_frequency(Frequency::Annual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .build()
    }

    fn euribor(settings: &Shared<Settings<Date>>) -> Shared<IborIndex> {
        shared(Euribor::three_months(
            Handle::<dyn YieldTermStructure>::empty(),
            Shared::clone(settings),
        ))
    }

    /// A one-flow stub standing in for a derived swap's floating leg, enough to
    /// build the base without a full ibor leg.
    fn floating_stub_leg() -> crate::cashflow::Leg {
        vec![
            shared(SimpleCashFlow::new(1.0, Date::new(7, Month::July, 2028)).unwrap())
                as Shared<dyn CashFlow>,
        ]
    }

    /// A payer fixed-vs-floating swap wrapped in [`SharedMut`], the shape a
    /// swaption holds.
    fn shared_swap(settings: &Shared<Settings<Date>>) -> SharedMut<FixedVsFloatingSwap> {
        let noop: FloatingArgumentsFn = Box::new(|_, _| Ok(()));
        shared_mut(
            FixedVsFloatingSwap::new(
                SwapType::Payer,
                vec![100.0],
                fixed_schedule(),
                0.05,
                Some(Actual360::new()),
                vec![100.0],
                fixed_schedule(),
                euribor(settings),
                0.001,
                Actual360::new(),
                None,
                0,
                None,
                floating_stub_leg(),
                noop,
                Shared::clone(settings),
            )
            .unwrap(),
        )
    }

    fn european(date: Date) -> Shared<dyn Exercise> {
        shared(EuropeanExercise::new(date)) as Shared<dyn Exercise>
    }

    /// `validate()` walks the C++ decision tree (`swaption.cpp:175-180`): the
    /// swap half first, then swap present, then exercise present, then the
    /// settlement consistency check.
    #[test]
    fn validate_walks_the_swap_exercise_and_consistency_checks() {
        let settings = settings_on(Date::new(7, Month::July, 2026));

        let mut args = SwaptionArguments::default();
        assert_eq!(args.validate().unwrap_err().message(), "swap not set");

        args.swap = Some(shared_swap(&settings));
        assert_eq!(args.validate().unwrap_err().message(), "exercise not set");

        args.exercise = Some(european(Date::new(7, Month::July, 2028)));
        args.settlement_type = SettlementType::Physical;
        args.settlement_method = SettlementMethod::CollateralizedCashPrice;
        assert_eq!(
            args.validate().unwrap_err().message(),
            "invalid settlement method for physical settlement"
        );

        args.settlement_method = SettlementMethod::PhysicalOTC;
        assert!(args.validate().is_ok(), "a consistent pair validates");
    }

    /// `isExpired` follows the last exercise date against the evaluation date
    /// (`detail::simple_event(exercise_->dates().back()).hasOccurred()`).
    #[test]
    fn is_expired_tracks_the_last_exercise_date() {
        let settings = settings_on(Date::new(7, Month::July, 2026));
        let swaption = Swaption::new(
            shared_swap(&settings),
            european(Date::new(7, Month::July, 2028)),
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&settings),
        );

        assert!(!swaption.is_expired().unwrap());
        settings.set_evaluation_date(Date::new(8, Month::July, 2028));
        assert!(swaption.is_expired().unwrap());
    }

    /// `setupArguments` fills the swap half through the swap's own
    /// `setupArguments` and then the swaption fields; the carried swap keeps the
    /// wrapped swap's identity.
    #[test]
    fn setup_arguments_fills_the_swap_half_and_the_swaption_fields() {
        let settings = settings_on(Date::new(7, Month::July, 2026));
        let swap = shared_swap(&settings);
        let swaption = Swaption::new(
            SharedMut::clone(&swap),
            european(Date::new(7, Month::July, 2028)),
            SettlementType::Cash,
            SettlementMethod::ParYieldCurve,
            Shared::clone(&settings),
        );

        let mut args = SwaptionArguments::default();
        swaption.setup_arguments(&mut args).unwrap();

        assert_eq!(
            args.swap_arguments.swap_type,
            Some(SwapType::Payer),
            "the swap's own setupArguments ran"
        );
        assert_eq!(
            args.swap_arguments.fixed_pay_dates.len(),
            2,
            "two annual fixed coupons filled by the swap half"
        );
        assert_eq!(args.settlement_type, SettlementType::Cash);
        assert_eq!(args.settlement_method, SettlementMethod::ParYieldCurve);
        assert_eq!(
            args.exercise.as_ref().unwrap().last_date(),
            Date::new(7, Month::July, 2028)
        );
        assert!(
            SharedMut::ptr_eq(args.swap.as_ref().unwrap(), &swap),
            "the carried swap is the wrapped swap, not a copy"
        );
    }

    /// `underlying()` and the forwarded `type()` reach the wrapped swap; the
    /// pointer identity is shared (one `Rc`, C++'s `shared_ptr`).
    #[test]
    fn underlying_shares_identity_and_forwards_type() {
        let settings = settings_on(Date::new(7, Month::July, 2026));
        let swap = shared_swap(&settings);
        let swaption = Swaption::new(
            SharedMut::clone(&swap),
            european(Date::new(7, Month::July, 2028)),
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&settings),
        );

        assert!(SharedMut::ptr_eq(swaption.underlying(), &swap));
        assert_eq!(swaption.swap_type(), SwapType::Payer);
        assert_eq!(swaption.settlement_type(), SettlementType::Physical);
        assert_eq!(swaption.settlement_method(), SettlementMethod::PhysicalOTC);
    }

    /// A bundle of the wrong type is rejected by `setupArguments`.
    #[test]
    fn setup_arguments_rejects_a_foreign_bundle() {
        let settings = settings_on(Date::new(7, Month::July, 2026));
        let swaption = Swaption::new(
            shared_swap(&settings),
            european(Date::new(7, Month::July, 2028)),
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            settings,
        );

        let mut foreign = FixedVsFloatingSwapArguments::default();
        assert_eq!(
            swaption
                .setup_arguments(&mut foreign)
                .unwrap_err()
                .message(),
            "wrong argument type"
        );
    }
}
