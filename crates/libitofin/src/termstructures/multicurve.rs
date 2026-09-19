//! The user-facing multi-curve assembler (`MultiCurve`,
//! `ql/termstructures/multicurve.{hpp,cpp}`): it owns a
//! [`MultiCurveBootstrap`] and the member curves that form a dependency cycle,
//! links each member's internal handle non-owningly, hands back an external
//! handle, and fans an external change out to every member.
//!
//! ## Divergence from the C++ external handle (`multicurve.cpp:60-62`)
//!
//! C++ builds the external handle from an aliasing
//! `shared_ptr(shared_from_this(), curve.get())`, so holding any external
//! handle keeps the whole `MultiCurve` and every member alive. Rust has no
//! `Rc` aliasing constructor, and both substitutes are wrong: a per-curve
//! newtype re-implementing [`YieldTermStructure`] loses virtual dispatch, and
//! an `Rc<MultiCurve>` stored on a curve closes a reference cycle. So the
//! external handle owns only its own curve; the `MultiCurve` instance's
//! lifetime is the caller's, kept as a local that outlives the curves (the
//! intended usage). Dropping the `MultiCurve` while holding only an external
//! handle drops the co-contributor curves, and the next re-solve upgrades a
//! dropped contributor's `Weak` and returns the honest D4 error
//! [`MultiCurveBootstrap::run`] raises, never a silent single-curve fallback.

use std::cell::RefCell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::math::optimization::endcriteria::EndCriteria;
use crate::patterns::observable::Observer;
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::globalbootstrap::{MultiCurveBootstrap, MultiCurveBootstrapContributor};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::Real;

/// The observer half of a [`MultiCurve`] (`MultiCurve` is itself an `Observer`
/// in C++; this port composes the observer as a separate struct, the D1
/// pattern the curves use).
///
/// It holds a `Weak` back-reference so the member curves that register it never
/// keep the `MultiCurve` alive, mirroring the divergence documented on the
/// module: the wrapper's lifetime stays the caller's.
struct MultiCurveUpdater {
    multicurve: Weak<MultiCurve>,
}

impl Observer for MultiCurveUpdater {
    fn update(&mut self) {
        if let Some(multicurve) = self.multicurve.upgrade() {
            multicurve.update();
        }
    }

    /// A re-entrant notification that finds this updater already running is a
    /// member re-broadcasting through the notification the fan-out just
    /// triggered; replaying it would only churn the same invalidations, so it
    /// is dropped rather than deferred (the members' own `updating_` guard
    /// already reaches the fixed point).
    fn defer_reentrant_update(&self) -> bool {
        false
    }
}

/// Assembles a set of curves that form a dependency cycle
/// (`MultiCurve`, `multicurve.hpp:79-104`).
///
/// It owns the [`MultiCurveBootstrap`] that joins the bootstrapped members into
/// one solve and owns the member curves themselves; the internal handles the
/// caller links their helpers through are pointed at the members non-owningly,
/// so no reference cycle forms.
pub struct MultiCurve {
    bootstrap: Shared<MultiCurveBootstrap>,
    curves: RefCell<Vec<Shared<dyn YieldTermStructure>>>,
    updater: SharedMut<MultiCurveUpdater>,
}

impl MultiCurve {
    /// The accuracy constructor (`multicurve.cpp:24-25`): the parent builds its
    /// optimizer and stopping criteria from the one number.
    pub fn new(accuracy: Real) -> Shared<MultiCurve> {
        Self::assemble(MultiCurveBootstrap::new(accuracy))
    }

    /// The override constructor (`multicurve.cpp:27-29`) minus its dropped
    /// optimizer argument: an explicit [`EndCriteria`], or `None` for the
    /// default the parent builds in [`MultiCurveBootstrap::run`].
    pub fn with_end_criteria(end_criteria: Option<EndCriteria>) -> Shared<MultiCurve> {
        Self::assemble(MultiCurveBootstrap::with_end_criteria(end_criteria))
    }

    fn assemble(bootstrap: MultiCurveBootstrap) -> Shared<MultiCurve> {
        let bootstrap = shared(bootstrap);
        Shared::new_cyclic(|weak: &Weak<MultiCurve>| MultiCurve {
            bootstrap,
            curves: RefCell::new(Vec::new()),
            updater: shared_mut(MultiCurveUpdater {
                multicurve: weak.clone(),
            }),
        })
    }

    /// Adds a bootstrapped member (`addBootstrappedCurve`,
    /// `multicurve.cpp:31-42`): the curve joins the parent's joint solve, so on
    /// the next query its `calculate` routes to [`MultiCurveBootstrap::run`]
    /// rather than solving alone.
    ///
    /// The C++ `dynamic_pointer_cast` to `MultiCurveBootstrapProvider` is
    /// replaced by the generic bound `C: MultiCurveBootstrapContributor`: the
    /// caller names the contributing type at the call site, so the cast is a
    /// compile-time coercion and the provider indirection is not needed.
    pub fn add_bootstrapped_curve<C>(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<C>,
    ) -> QlResult<Handle<dyn YieldTermStructure>>
    where
        C: MultiCurveBootstrapContributor + YieldTermStructure + 'static,
    {
        require!(
            internal.handle().is_empty(),
            "internal handle must be empty; was the curve added already?"
        );
        self.bootstrap
            .add(&(Shared::clone(&curve) as Shared<dyn MultiCurveBootstrapContributor>));
        self.add_curve(internal, curve as Shared<dyn YieldTermStructure>)
    }

    /// Adds a non-bootstrapped member (`addNonBootstrappedCurve`,
    /// `multicurve.cpp:44-51`): the curve does not join the solve but is
    /// registered as an observer of the parent, so the stacked solve's
    /// mid-iteration notify reaches its downstream cache.
    pub fn add_non_bootstrapped_curve(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<dyn YieldTermStructure>,
    ) -> QlResult<Handle<dyn YieldTermStructure>> {
        require!(
            internal.handle().is_empty(),
            "internal handle must be empty; was the curve added already?"
        );
        self.bootstrap.add_observer(&curve.updater());
        self.add_curve(internal, curve)
    }

    /// The shared tail of both adders (`addCurve`, `multicurve.cpp:53-71`):
    /// points the internal handle at the curve non-owningly, subscribes this
    /// wrapper to the curve's inputs, takes ownership of the curve, and returns
    /// an external handle.
    ///
    /// The internal handle is linked weakly (`linkTo(..., null_deleter, false)`,
    /// `multicurve.cpp:56-57`): `curves` is the only strong owner, so the
    /// handle can neither keep the curve alive nor form the notification cycle
    /// the internal handles exist to avoid. The external handle owns only the
    /// curve, per the divergence documented on the module.
    ///
    /// The `registerWithObservables(curve)` of `multicurve.cpp:70` registers
    /// with the observables *of* the curve (its inputs), not with the curve
    /// itself (`observable.hpp:132-139`), so this calls
    /// [`register_upstream`](TermStructure::register_upstream), not
    /// `curve.observable()`. This is what keeps the cycle broken: MC sits beside
    /// the members' own updaters on the shared inputs, so it hears a real input
    /// move (a quote, a relink, an eval-date change) but is never notified from
    /// inside a member's updater during the joint solve. A member's
    /// [`link_to_weak`](RelinkableHandle::link_to_weak) internal handle registers
    /// no observer on its pointee and relinks only at construction, so no
    /// steady-state notification path leads a member's updater back to MC.
    fn add_curve(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<dyn YieldTermStructure>,
    ) -> QlResult<Handle<dyn YieldTermStructure>> {
        internal.link_to_weak(Shared::downgrade(&curve));
        let observer = SharedMut::clone(&self.updater) as SharedMut<dyn Observer>;
        curve.register_upstream(&observer);
        self.curves.borrow_mut().push(Shared::clone(&curve));
        Ok(Handle::new(curve))
    }

    /// Fans an external change out to every member (`MultiCurve::update`,
    /// `multicurve.cpp:73-76`).
    ///
    /// Divergence from the C++ `for (c : curves_) c->update();`: a member's own
    /// updater may already be on the stack (its quote changed, its updater
    /// notified this wrapper, and the fan-out now reaches back to it), so a
    /// plain `borrow_mut` would panic on the live borrow where C++ simply
    /// re-enters and relies on the callee's `updating_` guard. This mirrors
    /// [`Observable::notify_observers`] instead: it snapshots the members,
    /// releases the borrow, and skips any member whose updater is already
    /// borrowed - a member mid-update is already invalidating, so skipping it
    /// reaches the same fixed point.
    fn update(&self) {
        let members = self.curves.borrow().clone();
        for member in members {
            let updater = member.updater();
            if let Ok(mut observer) = updater.try_borrow_mut() {
                observer.update();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::IborLeg;
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::instrument::Instrument;
    use crate::instruments::{ForwardRateAgreement, MakeVanillaSwap, Swap};
    use crate::interestrate::Compounding;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::patterns::observable::AsObservable;
    use crate::position::Position;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::swap::DiscountingSwapEngine;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared_mut;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::globalbootstrap::GlobalBootstrap;
    use crate::termstructures::yields::{
        DepositRateHelper, FlatForward, FraRateHelper, IborIborBasisSwapRateHelper,
        PiecewiseYieldCurve, Pillar, SwapRateHelper, ZeroSpreadedTermStructure,
    };
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Integer;

    type BootCurve = PiecewiseYieldCurve<Discount, LogLinear, GlobalBootstrap>;

    #[derive(Default)]
    struct Flag {
        fired: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.fired = true;
        }
    }

    fn env() -> (Shared<Settings<Date>>, Date, IborIndex) {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(13, Month::December, 2019);
        settings.set_evaluation_date(today);
        let index = Euribor::six_months(Handle::empty(), Shared::clone(&settings));
        (settings, today, index)
    }

    /// A minimal single-deposit curve. The wiring tests never run the joint
    /// solve, so the curve only has to build, not converge.
    fn deposit_curve(reference_date: Date, index: &IborIndex, rate: Real) -> Shared<BootCurve> {
        let helper = DepositRateHelper::from_rate(rate, index) as Shared<dyn RateHelper>;
        PiecewiseYieldCurve::with_bootstrap(
            reference_date,
            vec![helper],
            Actual365Fixed::new(),
            LogLinear,
            GlobalBootstrap::default(),
        )
        .expect("the single-deposit strip builds a curve")
    }

    fn flag_observer(flag: &SharedMut<Flag>) -> SharedMut<dyn Observer> {
        SharedMut::clone(flag) as SharedMut<dyn Observer>
    }

    #[test]
    fn adds_bootstrapped_and_non_bootstrapped_curves() {
        let (_settings, reference_date, index) = env();
        let multicurve = MultiCurve::new(1.0e-10);

        let internal_boot = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let boot = deposit_curve(reference_date, &index, 0.02);
        let boot_dyn = Shared::clone(&boot) as Shared<dyn YieldTermStructure>;
        let external_boot = multicurve
            .add_bootstrapped_curve(&internal_boot, Shared::clone(&boot))
            .expect("a bootstrapped curve is added");

        let internal_non = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let non_dyn: Shared<dyn YieldTermStructure> = deposit_curve(reference_date, &index, 0.025);
        let external_non = multicurve
            .add_non_bootstrapped_curve(&internal_non, Shared::clone(&non_dyn))
            .expect("a non-bootstrapped curve is added");

        assert!(!internal_boot.handle().is_empty());
        assert!(!internal_non.handle().is_empty());
        assert!(Shared::ptr_eq(
            &internal_boot
                .handle()
                .current_link()
                .expect("the internal handle resolves the curve"),
            &boot_dyn
        ));
        assert!(Shared::ptr_eq(
            &internal_non
                .handle()
                .current_link()
                .expect("the internal handle resolves the curve"),
            &non_dyn
        ));

        assert_eq!(
            multicurve.bootstrap.contributor_count(),
            1,
            "only the bootstrapped curve is a contributor"
        );
        assert_eq!(
            multicurve.bootstrap.observer_count(),
            1,
            "only the non-bootstrapped curve is an observer"
        );

        assert!(Shared::ptr_eq(
            &external_boot
                .current_link()
                .expect("the external handle owns its curve"),
            &boot_dyn
        ));
        assert!(Shared::ptr_eq(
            &external_non
                .current_link()
                .expect("the external handle owns its curve"),
            &non_dyn
        ));
    }

    #[test]
    fn rejects_a_reused_internal_handle() {
        let (_settings, reference_date, index) = env();
        let multicurve = MultiCurve::new(1.0e-10);
        let internal = RelinkableHandle::<dyn YieldTermStructure>::empty();

        let first = deposit_curve(reference_date, &index, 0.02);
        multicurve
            .add_bootstrapped_curve(&internal, first)
            .expect("the first add links the handle");

        let second = deposit_curve(reference_date, &index, 0.03);
        assert!(
            multicurve
                .add_bootstrapped_curve(&internal, second)
                .is_err(),
            "a second add on a linked handle must be rejected"
        );
    }

    #[test]
    fn dropping_the_multicurve_surfaces_a_dropped_contributor() {
        let (_settings, reference_date, index) = env();
        let multicurve = MultiCurve::new(1.0e-10);

        let internal_a = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let a = deposit_curve(reference_date, &index, 0.02);
        let external_a = multicurve
            .add_bootstrapped_curve(&internal_a, Shared::clone(&a))
            .expect("curve A is added");

        let internal_b = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let b = deposit_curve(reference_date, &index, 0.03);
        multicurve
            .add_bootstrapped_curve(&internal_b, Shared::clone(&b))
            .expect("curve B is added");
        drop(b);

        drop(multicurve);

        assert!(
            internal_b.handle().current_link().is_err(),
            "the weak internal handle did not keep the dropped contributor alive"
        );

        let error = a
            .calculate()
            .expect_err("a dropped contributor must surface as an error");
        assert!(
            format!("{error}").contains("dropped"),
            "the error must name the dropped contributor: {error}"
        );

        drop(external_a);
    }

    #[test]
    fn update_fans_out_to_member_curves() {
        // MultiCurve registers with each member's UPSTREAM observables (its
        // inputs), never the member's own observable, so a change reaches the
        // wrapper through an input. Build A with a helper we keep, add it, then
        // notify that helper (A's upstream): the fan-out must reach B.
        let (_settings, reference_date, index) = env();
        let multicurve = MultiCurve::new(1.0e-10);

        let helper_a = DepositRateHelper::from_rate(0.02, &index) as Shared<dyn RateHelper>;
        let a = PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
            reference_date,
            vec![Shared::clone(&helper_a)],
            Actual365Fixed::new(),
            LogLinear,
            GlobalBootstrap::default(),
        )
        .expect("curve A builds");
        let internal_a = RelinkableHandle::<dyn YieldTermStructure>::empty();
        multicurve
            .add_bootstrapped_curve(&internal_a, Shared::clone(&a))
            .expect("curve A is added");

        let internal_b = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let b = deposit_curve(reference_date, &index, 0.03);
        multicurve
            .add_bootstrapped_curve(&internal_b, Shared::clone(&b))
            .expect("curve B is added");

        let flag = shared_mut(Flag::default());
        b.observable().register_observer(&flag_observer(&flag));

        helper_a.observable().notify_observers();

        assert!(
            flag.borrow().fired,
            "MultiCurve::update did not fan an upstream change on A out to B"
        );
    }

    #[test]
    fn the_dependency_cycle_solves_without_re_entering_the_bootstrap() {
        // The real multi-curve cycle: a bootstrapped 3m curve whose swap helpers
        // discount on the ois curve, and a spreaded ois curve built over the 3m
        // internal handle. Under the pre-fix wiring (MultiCurve registered on
        // each member's OWN observable) the mid-solve observers-notify re-entered
        // MultiCurveBootstrap::run and panicked at the state borrow_mut
        // (globalbootstrap.rs:1126, the hazard globalbootstrap.rs:640-643 warns
        // of). With MultiCurve off the members' observables the query returns Ok.
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(23, Month::October, 2025);
        settings.set_evaluation_date(today);

        let intcurveois = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let intcurve3m = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let euribor3m = Euribor::three_months(intcurve3m.handle(), Shared::clone(&settings));

        let q = Handle::new(shared(SimpleQuote::new(0.03)) as Shared<dyn Quote>);
        let mut helpers3m: Vec<Shared<dyn RateHelper>> = Vec::new();
        for i in 1..=5i32 {
            helpers3m.push(SwapRateHelper::with_details(
                q.clone(),
                Period::new(i, TimeUnit::Years),
                Target::new(),
                Frequency::Annual,
                BusinessDayConvention::Following,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor3m,
                Handle::empty(),
                Period::new(0, TimeUnit::Days),
                Some(intcurveois.handle()),
                Pillar::LastRelevantDate,
            ) as Shared<dyn RateHelper>);
        }

        let ptr3m = PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
            today,
            helpers3m,
            Actual360::new(),
            LogLinear,
            GlobalBootstrap::new(Some(1.0e-10), None, Vec::new()),
        )
        .expect("the 3m curve builds");

        let multicurve = MultiCurve::new(1.0e-10);
        let curve3m = multicurve
            .add_bootstrapped_curve(&intcurve3m, ptr3m)
            .expect("adds the 3m contributor");

        let b = Handle::new(shared(SimpleQuote::new(-0.01)) as Shared<dyn Quote>);
        let ptrois = shared(ZeroSpreadedTermStructure::new(intcurve3m.handle(), b));
        let curveois = multicurve
            .add_non_bootstrapped_curve(
                &intcurveois,
                Shared::clone(&ptrois) as Shared<dyn YieldTermStructure>,
            )
            .expect("adds the ois member");

        let ois_rate = curveois
            .current_link()
            .expect("the ois handle resolves")
            .zero_rate(1.0, Compounding::Continuous, Frequency::NoFrequency, false)
            .expect("the ois zero rate solves without re-entering the bootstrap")
            .rate();
        let rate3m = curve3m
            .current_link()
            .expect("the 3m handle resolves")
            .zero_rate(1.0, Compounding::Continuous, Frequency::NoFrequency, false)
            .expect("the 3m zero rate solves")
            .rate();

        assert!(
            (ois_rate - rate3m - (-0.01)).abs() < 1.0e-10,
            "spread {} is not the -0.01 the ZeroSpreadedTermStructure adds",
            ois_rate - rate3m
        );
    }

    #[test]
    fn the_internal_weak_handle_does_not_forward() {
        let (_settings, reference_date, index) = env();
        let multicurve = MultiCurve::new(1.0e-10);

        let internal_a = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let a = deposit_curve(reference_date, &index, 0.02);
        multicurve
            .add_bootstrapped_curve(&internal_a, Shared::clone(&a))
            .expect("curve A is added");

        let weak_flag = shared_mut(Flag::default());
        internal_a
            .handle()
            .register_observer(&flag_observer(&weak_flag));

        let owning = RelinkableHandle::new(Shared::clone(&a) as Shared<dyn YieldTermStructure>);
        let owning_flag = shared_mut(Flag::default());
        owning
            .handle()
            .register_observer(&flag_observer(&owning_flag));

        a.observable().notify_observers();

        assert!(
            !weak_flag.borrow().fired,
            "the weak internal handle forwarded a notification it must not"
        );
        assert!(
            owning_flag.borrow().fired,
            "an owning handle on the same curve did not forward the notification"
        );
    }

    /// Prices an `i`-year vanilla swap at `fixed_rate` off `discount` (mirror
    /// cpp:1731-1738): the fixed leg is annual Thirty360/Following, the float
    /// leg the index's, and the discount curve is the exogenous ois handle.
    fn swap_npv(
        i: i32,
        fixed_rate: Real,
        euribor3m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
        discount: &Handle<dyn YieldTermStructure>,
    ) -> Real {
        let mut swap = MakeVanillaSwap::new(
            Period::new(i, TimeUnit::Years),
            Shared::clone(euribor3m),
            Some(fixed_rate),
            Period::new(0, TimeUnit::Days),
            Shared::clone(settings),
        )
        .with_settlement_days(euribor3m.fixing_days())
        .with_fixed_leg_day_count(Thirty360::with_convention(Convention::BondBasis))
        .with_fixed_leg_tenor(Period::new(1, TimeUnit::Years))
        .with_fixed_leg_convention(BusinessDayConvention::Following)
        .with_fixed_leg_termination_date_convention(BusinessDayConvention::Following)
        .build()
        .expect("the swap builds");
        let engine = shared_mut(DiscountingSwapEngine::new(
            discount.clone(),
            None,
            None,
            None,
            Shared::clone(settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        swap.npv().expect("the swap prices off the ois curve")
    }

    /// Ports `testMultiCurvePiecewiseYieldCurveAndSpreadedCurve`
    /// (`piecewiseyieldcurve.cpp:1686-1742`): a bootstrapped 3m curve whose swap
    /// helpers discount on a spreaded ois curve built over the 3m curve, joined
    /// in one [`MultiCurve`]. The two members form a dependency cycle the joint
    /// solve resolves; the spread and the self-repricing swaps are the oracle,
    /// and a quote bump drives the cross-member fan-out.
    ///
    /// Honest negative: the joint mechanism (two-phase set/notify/evaluate,
    /// offset slicing, parent routing, error revert) is pinned by the
    /// mock-contributor tests in `globalbootstrap.rs`; this does not re-pin it.
    /// The spread identity is additive for a `ZeroSpreadedTermStructure` and the
    /// swap NPVs are self-reprice, so both pass even under a mis-wired solve:
    /// this is an integration / self-consistency test, and the discriminating
    /// arm is the q-bump fan-out flag (a broken registration fails it). The
    /// strongest oracle (mutual basis, `testMultiCurveTwoPiecewiseYieldCurves`)
    /// is [`multicurve_two_piecewise_yield_curves_reprice`]; no C++ value pin
    /// here.
    #[test]
    fn multicurve_piecewise_and_spreaded_curve_self_reprice() {
        let calendar = Target::new();
        let settings = shared(Settings::<Date>::new());
        let today = calendar.adjust(
            Date::new(23, Month::October, 2025),
            BusinessDayConvention::Following,
        );
        settings.set_evaluation_date(today);

        let intcurveois = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let intcurve3m = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let euribor3m = shared(Euribor::three_months(
            intcurve3m.handle(),
            Shared::clone(&settings),
        ));

        let q = shared(SimpleQuote::new(0.03));
        let q_handle = Handle::new(Shared::clone(&q) as Shared<dyn Quote>);
        let b = shared(SimpleQuote::new(-0.01));
        let b_handle = Handle::new(Shared::clone(&b) as Shared<dyn Quote>);

        let mut helpers3m: Vec<Shared<dyn RateHelper>> = Vec::new();
        for i in 1..=10i32 {
            helpers3m.push(SwapRateHelper::with_details(
                q_handle.clone(),
                Period::new(i, TimeUnit::Years),
                calendar.clone(),
                Frequency::Annual,
                BusinessDayConvention::Following,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor3m,
                Handle::empty(),
                Period::new(0, TimeUnit::Days),
                Some(intcurveois.handle()),
                Pillar::LastRelevantDate,
            ) as Shared<dyn RateHelper>);
        }

        let ptr3m = PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
            today,
            helpers3m,
            Actual360::new(),
            LogLinear,
            GlobalBootstrap::new(Some(1.0e-10), None, Vec::new()),
        )
        .expect("the 3m curve builds");

        let multicurve = MultiCurve::new(1.0e-10);
        let curve3m = multicurve
            .add_bootstrapped_curve(&intcurve3m, ptr3m)
            .expect("adds the 3m contributor");

        let ptrois = shared(ZeroSpreadedTermStructure::new(
            intcurve3m.handle(),
            b_handle,
        ));
        let curveois = multicurve
            .add_non_bootstrapped_curve(
                &intcurveois,
                Shared::clone(&ptrois) as Shared<dyn YieldTermStructure>,
            )
            .expect("adds the ois member");

        let zero_rate = |handle: &Handle<dyn YieldTermStructure>| -> Real {
            handle
                .current_link()
                .expect("the handle resolves")
                .zero_rate(1.0, Compounding::Continuous, Frequency::NoFrequency, false)
                .expect("the zero rate solves")
                .rate()
        };

        assert!(
            (zero_rate(&curveois) - zero_rate(&curve3m) - (-0.01)).abs() < 1.0e-10,
            "the ois-3m spread is not the -0.01 the ZeroSpreadedTermStructure adds"
        );

        for i in 1..=10i32 {
            let npv = swap_npv(i, 0.03, &euribor3m, &settings, &curveois);
            assert!(
                npv.abs() < 1.0e-10,
                "swap {i} does not reprice to zero: {npv}"
            );
        }

        let flag = shared_mut(Flag::default());
        ptrois.observable().register_observer(&flag_observer(&flag));

        let pre = zero_rate(&curve3m);
        q.set_value(0.035);
        let post = zero_rate(&curve3m);
        assert!(
            (post - pre).abs() > 1.0e-10,
            "the q-bump fan-out did not re-solve the 3m curve: {pre} -> {post}"
        );
        assert!(
            flag.borrow().fired,
            "the q-bump did not fan out to the spreaded ois member"
        );
        for i in 1..=10i32 {
            let npv = swap_npv(i, 0.035, &euribor3m, &settings, &curveois);
            assert!(
                npv.abs() < 1.0e-10,
                "rebuilt swap {i} does not reprice to zero at the bumped quote: {npv}"
            );
        }

        b.set_value(-0.005);
        assert!(
            (zero_rate(&curveois) - zero_rate(&curve3m) - (-0.005)).abs() < 1.0e-10,
            "the bumped ois-3m spread is not the new -0.005"
        );
    }

    /// The basis swap of `piecewiseyieldcurve.cpp:1615-1638` / `:1640-1663`:
    /// spot to `maturity`, the 3m leg paying `basis` over the index, the 6m leg
    /// flat, both on notional 1, priced on the exogenous discount curve. Both
    /// indices are the originals, reading the multi-curve's internal handles.
    fn basis_swap_npv(
        maturity: Date,
        basis: Real,
        euribor3m: &Shared<IborIndex>,
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
        discount: &Handle<dyn YieldTermStructure>,
    ) -> Real {
        let start = euribor3m.fixing_calendar().advance(
            settings
                .evaluation_date()
                .expect("the evaluation date is set"),
            euribor3m.fixing_days() as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let leg = |index: &Shared<IborIndex>, spread: Real| {
            let schedule = MakeSchedule::new()
                .from(start)
                .to(maturity)
                .with_tenor(index.tenor())
                .with_calendar(index.fixing_calendar())
                .with_convention(index.business_day_convention())
                .end_of_month(index.end_of_month())
                .forwards()
                .build();
            IborLeg::new(schedule, Shared::clone(index))
                .with_spread(spread)
                .with_notional(1.0)
                .build()
                .expect("the basis-swap leg builds")
        };
        let mut swap = Swap::two_leg(
            leg(euribor3m, basis),
            leg(euribor6m, 0.0),
            Shared::clone(settings),
        );
        let engine = shared_mut(DiscountingSwapEngine::new(
            discount.clone(),
            None,
            None,
            None,
            Shared::clone(settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        swap.npv().expect("the basis swap prices")
    }

    /// Ports `testMultiCurveTwoPiecewiseYieldCurves`
    /// (`piecewiseyieldcurve.cpp:1547-1684`): a 3m curve over 9 FRAs and 9
    /// basis swaps bootstrapping the 3m side, and a 6m curve over 3 basis swaps
    /// bootstrapping the 6m side and 9 vanilla swaps, solved jointly. The
    /// coupling is mutual and non-separable: every basis helper on one curve
    /// forecasts its other leg off the other curve, so no sequential
    /// single-curve pass converges. The oracle is the four self-reprice loops
    /// of `:1608-1682`, at the C++ tolerances: the FRA loop is
    /// `QL_CHECK_CLOSE(.., 1e-10)`, a Boost percentage and so 1e-12 relative,
    /// and the three swap loops are `QL_CHECK_SMALL(.., 1e-10)`, absolute.
    ///
    /// The C++ FRA helpers use the from-scratch constructor (`:1572-1576`),
    /// which builds a `"no-fix"` index of tenor `(i + 3) - i = 3M` with
    /// Euribor3M's fixing days, calendar, convention, end-of-month flag and day
    /// counter and `useIndexedCoupon = true`; [`FraRateHelper::from_months`]
    /// over the Euribor3M index clones exactly those parameters (the name and
    /// currency never enter a forecast), so the two forms are numerically the
    /// same helper.
    #[test]
    fn multicurve_two_piecewise_yield_curves_reprice() {
        let calendar = Target::new();
        let settings = shared(Settings::<Date>::new());
        let today = calendar.adjust(
            Date::new(23, Month::October, 2025),
            BusinessDayConvention::Following,
        );
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let accuracy = 1.0e-10;

        let discount = Handle::new(shared(FlatForward::with_rate(
            settlement,
            0.02,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        let intcurve3m = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let intcurve6m = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let euribor3m = shared(Euribor::three_months(
            intcurve3m.handle(),
            Shared::clone(&settings),
        ));
        let euribor6m = shared(Euribor::six_months(
            intcurve6m.handle(),
            Shared::clone(&settings),
        ));

        let q = Handle::new(shared(SimpleQuote::new(0.03)) as Shared<dyn Quote>);
        let b = Handle::new(shared(SimpleQuote::new(0.0020)) as Shared<dyn Quote>);

        let basis_helper = |tenor: Period, bootstrap_base_curve: bool| {
            IborIborBasisSwapRateHelper::new(
                b.clone(),
                tenor,
                euribor3m.fixing_days(),
                euribor3m.fixing_calendar(),
                euribor3m.business_day_convention(),
                euribor3m.end_of_month(),
                &euribor3m,
                &euribor6m,
                discount.clone(),
                bootstrap_base_curve,
            ) as Shared<dyn RateHelper>
        };

        let mut helpers3m: Vec<Shared<dyn RateHelper>> = Vec::new();
        for i in 1..=9u32 {
            helpers3m.push(FraRateHelper::from_months(
                q.clone(),
                i,
                &euribor3m,
                true,
                Pillar::LastRelevantDate,
            ) as Shared<dyn RateHelper>);
        }
        for i in 2..=10i32 {
            helpers3m.push(basis_helper(Period::new(i, TimeUnit::Years), true));
        }

        let mut helpers6m: Vec<Shared<dyn RateHelper>> = Vec::new();
        for i in 1..=3i32 {
            helpers6m.push(basis_helper(Period::new(i * 6, TimeUnit::Months), false));
        }
        for i in 2..=10i32 {
            helpers6m.push(SwapRateHelper::with_details(
                q.clone(),
                Period::new(i, TimeUnit::Years),
                euribor6m.fixing_calendar(),
                Frequency::Annual,
                BusinessDayConvention::Following,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor6m,
                Handle::empty(),
                Period::new(0, TimeUnit::Days),
                Some(discount.clone()),
                Pillar::LastRelevantDate,
            ) as Shared<dyn RateHelper>);
        }

        let build = |helpers: Vec<Shared<dyn RateHelper>>| {
            PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
                today,
                helpers,
                Actual360::new(),
                LogLinear,
                GlobalBootstrap::new(Some(accuracy), None, Vec::new()),
            )
            .expect("the curve builds")
        };
        let ptr3m = build(helpers3m);
        let ptr6m = build(helpers6m);

        let multicurve = MultiCurve::new(accuracy);
        let curve3m = multicurve
            .add_bootstrapped_curve(&intcurve3m, ptr3m)
            .expect("adds the 3m contributor");
        let _curve6m = multicurve
            .add_bootstrapped_curve(&intcurve6m, ptr6m)
            .expect("adds the 6m contributor");

        let tolerance = 1.0e-10;
        let spot = euribor3m.fixing_calendar().advance(
            today,
            euribor3m.fixing_days() as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        for i in 1..=9i32 {
            let start = euribor3m.fixing_calendar().advance(
                spot,
                i,
                TimeUnit::Months,
                euribor3m.business_day_convention(),
                euribor3m.end_of_month(),
            );
            let mut fra = ForwardRateAgreement::new(
                Shared::clone(&euribor3m),
                start,
                Position::Long,
                0.03,
                1.0,
                curve3m.clone(),
            )
            .expect("the FRA builds");
            let rate = fra.forward_rate().expect("the FRA forwards").rate();
            assert!(
                (rate - 0.03).abs() <= 0.03 * tolerance * 1.0e-2,
                "FRA {i}: forward {rate} is not the 3% quote"
            );
        }

        for i in 2..=10i32 {
            let maturity = euribor3m.fixing_calendar().advance_by_period(
                spot,
                Period::new(i, TimeUnit::Years),
                euribor3m.business_day_convention(),
                false,
            );
            let npv = basis_swap_npv(
                maturity, 0.0020, &euribor3m, &euribor6m, &settings, &discount,
            );
            assert!(npv.abs() < tolerance, "{i}y basis swap NPV {npv}");
        }

        for i in 1..=3i32 {
            let maturity = euribor3m.fixing_calendar().advance_by_period(
                spot,
                Period::new(i * 6, TimeUnit::Months),
                euribor3m.business_day_convention(),
                false,
            );
            let npv = basis_swap_npv(
                maturity, 0.0020, &euribor3m, &euribor6m, &settings, &discount,
            );
            assert!(npv.abs() < tolerance, "{}m basis swap NPV {npv}", i * 6);
        }

        for i in 2..=10i32 {
            let npv = swap_npv(i, 0.03, &euribor6m, &settings, &discount);
            assert!(npv.abs() < tolerance, "{i}y vanilla swap NPV {npv}");
        }
    }
}
