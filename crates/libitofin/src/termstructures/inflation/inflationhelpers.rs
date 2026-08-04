//! Bootstrap helpers for zero-inflation term structures.
//!
//! Port of the helper base `ZeroCouponInflationSwapHelper` derives from
//! (`ql/termstructures/inflation/inflationhelpers.hpp:36-37`, a
//! `RelativeDateBootstrapHelper<ZeroInflationTermStructure>`) together with the
//! plain `BootstrapHelper<ZeroInflationTermStructure>` the traits name as their
//! helper type (`inflationtraits.hpp:43`). They are the inflation twins of
//! [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper) and
//! [`DefaultProbabilityHelper`](crate::termstructures::credit::defaultprobabilityhelpers::DefaultProbabilityHelper),
//! and they carry no behaviour of their own: everything is inherited from the
//! shared [`BootstrapHelperBase`], instantiated here over
//! [`ZeroInflationTermStructure`].
//!
//! Where C++ gets every family from one class template, this port needs one
//! trait per family, because a trait generic over its term structure cannot be
//! made into the bare trait object the curves are typed on. The shared driver
//! reaches all three through [`BootstrapHelperShared`], implemented below on
//! `dyn ZeroInflationHelper`.
//!
//! [`ZeroCouponInflationSwapHelper`] (`inflationhelpers.hpp:35`) is the one
//! concrete helper ported, and it plugs into that base.
//!
//! ## Deferred within EPIC Inflation (#705)
//!
//! - The **interpolated observation branch** (`inflationhelpers.cpp:114-153`):
//!   its earliest/latest dates straddle the fixing period and its pillar follows
//!   an interpolation weight. [`ZeroCouponInflationSwapHelper::new`] rejects
//!   [`CpiInterpolationType::Linear`] rather than walking the flat path with it.
//! - The **start/end-date constructor** (`cpp:52-69`), so every helper here
//!   rebuilds its schedule off the evaluation date, and the deprecated
//!   nominal-curve constructor (`cpp:71-86`).
//! - The `Pillar::CustomDate` / `Pillar::MaturityDate` choices, which only the
//!   interpolated branch reads (`cpp:120-152`); the flat branch has one
//!   possible pillar.
//! - `YearOnYearInflationSwapHelper` (`cpp:208-360`), which needs the
//!   year-on-year curve and index.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Weak;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::Index;
use crate::indexes::inflationindex::{
    CpiInterpolationType, InflationIndex, ZeroInflationIndex, inflation_period,
};
use crate::instrument::Instrument;
use crate::instruments::{SwapType, ZeroCouponInflationSwap};
use crate::interestrate::Compounding;
use crate::patterns::observable::AsObservable;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::DiscountingSwapEngine;
use crate::quotes::Quote;
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, BootstrapHelperShared};
use crate::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::Real;

/// The shared state of an inflation bootstrap helper: a
/// [`BootstrapHelperBase`] whose back-pointer is a zero-inflation curve.
pub type ZeroInflationHelperBase = BootstrapHelperBase<dyn ZeroInflationTermStructure>;

/// Bootstrap helper for the zero-inflation-curve bootstrap
/// (`BootstrapHelper<ZeroInflationTermStructure>`).
///
/// Mirrors [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper)
/// exactly, over [`ZeroInflationTermStructure`]: a concrete helper embeds a
/// [`ZeroInflationHelperBase`], returns it from [`base`](Self::base) and
/// supplies [`implied_quote`](Self::implied_quote); the rest of the interface
/// is derived from the base. The same ownership contract holds - the curve is
/// held [`Weak`](std::rc::Weak) and never observed - since it is the one base
/// that enforces it.
pub trait ZeroInflationHelper: AsObservable {
    /// The embedded shared state.
    fn base(&self) -> &ZeroInflationHelperBase;

    /// The quote implied by the current curve, computed by the concrete helper.
    ///
    /// The helper does not observe the curve, so this must force any
    /// recalculation it needs itself rather than trusting a cached value.
    fn implied_quote(&self) -> QlResult<Real>;

    /// The market quote the helper fits the curve to.
    fn quote(&self) -> &Handle<dyn Quote> {
        self.base().quote()
    }

    /// The bootstrap's root: market quote minus implied quote, driven to zero.
    fn quote_error(&self) -> QlResult<Real> {
        Ok(self.base().quote_value()? - self.implied_quote()?)
    }

    /// Sets the curve being bootstrapped (non-owning, unobserved).
    ///
    /// A concrete helper that hands the curve to an instrument overrides this
    /// to relink that handle first, then delegates here.
    fn set_term_structure(&self, term_structure: &Shared<dyn ZeroInflationTermStructure>) {
        self.base().set_term_structure(term_structure);
    }

    /// The earliest date data are needed at.
    fn earliest_date(&self) -> Date {
        self.base().earliest_date()
    }

    /// The instrument's maturity date.
    fn maturity_date(&self) -> Date {
        self.base().maturity_date()
    }

    /// The latest date data are needed at.
    fn latest_relevant_date(&self) -> Date {
        self.base().latest_relevant_date()
    }

    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> Date {
        self.base().pillar_date()
    }

    /// The latest date, equal to the pillar date.
    fn latest_date(&self) -> Date {
        self.base().latest_date()
    }
}

/// Inflation bootstrap helper whose date schedule is relative to the
/// evaluation date (`RelativeDateBootstrapHelper<ZeroInflationTermStructure>`).
///
/// `ZeroCouponInflationSwapHelper` derives from this
/// (`inflationhelpers.hpp:36-37`): its swap schedule is rebuilt whenever the
/// evaluation date moves. The concrete helper builds its base with
/// [`BootstrapHelperBase::new_relative`], passing a closure that calls
/// [`initialize_dates`](Self::initialize_dates).
pub trait RelativeDateZeroInflationHelper: ZeroInflationHelper {
    /// Rebuilds the helper's date schedule off the current evaluation date.
    fn initialize_dates(&self);
}

/// The inflation half of the driver bound. Like the yield and credit impls,
/// every method routes through the [`ZeroInflationHelper`] trait rather than
/// straight to the base, so a concrete helper's override still runs -
/// `set_term_structure` in particular, which a helper overrides to relink its
/// own pricing handle before recording the curve.
impl BootstrapHelperShared for dyn ZeroInflationHelper {
    type TS = dyn ZeroInflationTermStructure;

    fn set_term_structure(&self, term_structure: &Shared<dyn ZeroInflationTermStructure>) {
        ZeroInflationHelper::set_term_structure(self, term_structure);
    }

    fn quote_value(&self) -> QlResult<Real> {
        self.base().quote_value()
    }

    fn quote_error(&self) -> QlResult<Real> {
        ZeroInflationHelper::quote_error(self)
    }

    fn pillar_date(&self) -> Date {
        ZeroInflationHelper::pillar_date(self)
    }

    fn latest_relevant_date(&self) -> Date {
        ZeroInflationHelper::latest_relevant_date(self)
    }

    fn maturity_date(&self) -> Date {
        ZeroInflationHelper::maturity_date(self)
    }
}

/// Bootstrap helper quoting a zero-coupon inflation swap
/// (`ZeroCouponInflationSwapHelper`, `inflationhelpers.hpp:35`).
///
/// The helper prices a unit-notional zero-strike swap of its own against the
/// curve being bootstrapped and reports that contract's
/// [`fair_rate`](ZeroCouponInflationSwap::fair_rate) as
/// [`implied_quote`](ZeroInflationHelper::implied_quote); the bootstrap drives
/// `quoted rate - fair rate` to zero. **Not** the contract's NPV: the fair rate
/// is the only quantity here that is discount-invariant, both legs paying on the
/// same adjusted maturity so their discount factors cancel (`cpp:66-68`).
///
/// That invariance is why the helper needs no nominal curve from its caller and
/// builds a flat 0 % one itself (`cpp:48`). The curve reaches the contract's
/// [`DiscountingSwapEngine`] and hence its NPV, which is consequently **not**
/// zero at the bootstrapped solution; [`fair_rate`](ZeroCouponInflationSwap::fair_rate)
/// reads the indexed flow directly and never consults it.
///
/// The helper prices through a copy of the caller's index
/// ([`clone_linked_to`](ZeroInflationIndex::clone_linked_to), `cpp:106`) linked to
/// its own relinkable handle, so [`set_term_structure`](ZeroInflationHelper::set_term_structure)
/// can point the copy at the curve under construction while the caller's index
/// keeps whatever curve it had. The copy is then unregistered from that handle
/// (`cpp:107-110`): a relink per solver step would otherwise notify the copy,
/// the copy the helper, and the helper the curve that is relinking it.
pub struct ZeroCouponInflationSwapHelper {
    base: ZeroInflationHelperBase,
    swap_obs_lag: Period,
    maturity: Date,
    calendar: Calendar,
    payment_convention: BusinessDayConvention,
    day_counter: DayCounter,
    index: Shared<ZeroInflationIndex>,
    observation_interpolation: CpiInterpolationType,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
    term_structure_handle: RelinkableHandle<dyn ZeroInflationTermStructure>,
    settings: Shared<Settings<Date>>,
    swap: RefCell<QlResult<ZeroCouponInflationSwap>>,
}

impl ZeroCouponInflationSwapHelper {
    /// A helper on a swap maturing at `maturity` (`cpp:34-49`).
    ///
    /// The swap's start date is the evaluation date and follows it: this is the
    /// constructor C++ marks relative by passing a null start date (`cpp:100`),
    /// so the contract is rebuilt whenever the evaluation date moves.
    ///
    /// The helper observes its quote, the index copy it prices through, and the
    /// nominal curve (`cpp:173-174`); it does **not** observe the curve it is
    /// bootstrapped against.
    ///
    /// # Errors
    ///
    /// [`CpiInterpolationType::Linear`] is rejected: its date and pillar logic is
    /// a documented deferral (see the module docs). The swap the helper prices is
    /// built here too, so a `swap_obs_lag` the index cannot observe through fails
    /// at construction, as the C++ constructor's `initializeDates` throws.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        quote: Handle<dyn Quote>,
        swap_obs_lag: Period,
        maturity: Date,
        calendar: Calendar,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        zii: &Shared<ZeroInflationIndex>,
        observation_interpolation: CpiInterpolationType,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<ZeroCouponInflationSwapHelper>> {
        require!(
            observation_interpolation == CpiInterpolationType::Flat,
            "the interpolated observation branch (inflationhelpers.cpp:114-153) is not ported"
        );
        let fixing_period = inflation_period(maturity - swap_obs_lag, zii.frequency())?;
        let nominal_term_structure = Handle::new(shared(FlatForward::moving_with_rate(
            0,
            NullCalendar::new(),
            0.0,
            day_counter.clone(),
            Compounding::Continuous,
            Frequency::Annual,
            Shared::clone(&settings),
        )) as Shared<dyn YieldTermStructure>);

        let helper = Shared::new_cyclic(|weak: &Weak<ZeroCouponInflationSwapHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let base = ZeroInflationHelperBase::new_relative(
                quote,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            let term_structure_handle = RelinkableHandle::empty();
            let index = shared(zii.clone_linked_to(term_structure_handle.handle()));
            term_structure_handle
                .handle()
                .unregister_observer(&index.inflation_base().observer());
            index.observable().register_observer(&base.observer());
            nominal_term_structure.register_observer(&base.observer());

            base.set_earliest_date(fixing_period.0);
            base.set_latest_date(fixing_period.0);
            base.set_pillar_date(fixing_period.0);

            let helper = ZeroCouponInflationSwapHelper {
                base,
                swap_obs_lag,
                maturity,
                calendar,
                payment_convention,
                day_counter,
                index,
                observation_interpolation,
                nominal_term_structure,
                term_structure_handle,
                settings,
                swap: RefCell::new(Err(crate::errors::QlError::new(
                    "the helper's swap is built by initialize_dates",
                    file!(),
                    line!(),
                ))),
            };
            helper.initialize_dates();
            helper
        });
        if let Err(error) = helper.swap.borrow().as_ref() {
            return Err(error.clone());
        }
        Ok(helper)
    }

    /// The cached swap, or the error that stopped it being built (`swap()`,
    /// `hpp:84`).
    pub fn swap(&self) -> Ref<'_, QlResult<ZeroCouponInflationSwap>> {
        self.swap.borrow()
    }

    /// The cached swap, mutably, for its on-demand pricing accessors.
    pub fn swap_mut(&self) -> RefMut<'_, QlResult<ZeroCouponInflationSwap>> {
        self.swap.borrow_mut()
    }

    /// The index copy the helper prices through, linked to its own handle.
    pub fn inflation_index(&self) -> &Shared<ZeroInflationIndex> {
        &self.index
    }

    /// The self-built flat 0 % curve the contract's engine discounts on
    /// (`cpp:48`).
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }

    /// The unit-notional, zero-strike swap the helper quotes, under a discounting
    /// engine over the flat 0 % curve (`initializeDates`, `cpp:186-195`).
    ///
    /// The start date is the evaluation date, which a relative-date helper always
    /// tracks; C++ reads its own `evaluationDate_` there. An evaluation date that
    /// was never set is an error rather than a panic (D10), surfaced when the
    /// cached result is next read.
    fn build_swap(&self) -> QlResult<ZeroCouponInflationSwap> {
        let start_date = match self.base.evaluation_date() {
            Some(date) => date,
            None => crate::fail!("no evaluation date set: the helper's swap starts at it"),
        };
        let mut swap = ZeroCouponInflationSwap::new(
            SwapType::Payer,
            1.0,
            start_date,
            self.maturity,
            self.calendar.clone(),
            self.payment_convention,
            self.day_counter.clone(),
            0.0,
            Shared::clone(&self.index),
            self.swap_obs_lag,
            self.observation_interpolation,
            None,
            None,
            Shared::clone(&self.settings),
        )?;
        let engine = DiscountingSwapEngine::new(
            self.nominal_term_structure.clone(),
            None,
            None,
            None,
            Shared::clone(&self.settings),
        );
        swap.base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
        Ok(swap)
    }
}

impl AsObservable for ZeroCouponInflationSwapHelper {
    fn observable(&self) -> &crate::patterns::observable::Observable {
        self.base.observable()
    }
}

impl ZeroInflationHelper for ZeroCouponInflationSwapHelper {
    fn base(&self) -> &ZeroInflationHelperBase {
        &self.base
    }

    /// The swap's fair rate (`impliedQuote`, `cpp:181-184`).
    ///
    /// The `deepUpdate` is kept for fidelity but carries no value here: the fair
    /// rate is read off the indexed flow, which forecasts through the index copy
    /// on every call and caches nothing, so it already reflects a node the
    /// bootstrap has just moved.
    fn implied_quote(&self) -> QlResult<Real> {
        let mut swap = self.swap.borrow_mut();
        let swap = swap.as_mut().map_err(|error| error.clone())?;
        swap.swap_mut().deep_update();
        swap.fair_rate()
    }

    /// Points the index copy's handle at the curve, then records it
    /// (`setTermStructure`, `cpp:197-206`).
    ///
    /// The link is weak and unobserved, the port of the C++ `null_deleter` plus
    /// `observer = false`: the curve owns this helper, which owns the swap, which
    /// owns the index copy, and an owning link would close that ring.
    fn set_term_structure(&self, term_structure: &Shared<dyn ZeroInflationTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateZeroInflationHelper for ZeroCouponInflationSwapHelper {
    /// Rebuilds the swap off the current evaluation date (`initializeDates`,
    /// `cpp:186-195`).
    ///
    /// The helper's own dates are not rebuilt: they come from the fixing period
    /// of `maturity - swap_obs_lag`, which no evaluation date moves.
    fn initialize_dates(&self) {
        *self.swap.borrow_mut() = self.build_swap();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::patterns::observable::Observable;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    /// A helper that overrides both hooks a concrete helper overrides, and
    /// records that each ran. Its quote is 0.03 and its implied quote 0.01, so
    /// the default `quote_error` would be 0.02 - a value the overridden one
    /// deliberately does not return.
    struct StubHelper {
        base: ZeroInflationHelperBase,
        curve_set: Cell<bool>,
        error_called: Cell<bool>,
    }

    impl StubHelper {
        fn new() -> Shared<StubHelper> {
            let quote = shared(SimpleQuote::new(Some(0.03)));
            let base = ZeroInflationHelperBase::new(Handle::new(quote));
            base.set_pillar_date(Date::new(1, Month::June, 2030));
            base.set_latest_relevant_date(Date::new(1, Month::July, 2030));
            base.set_maturity_date(Date::new(1, Month::June, 2030));
            shared(StubHelper {
                base,
                curve_set: Cell::new(false),
                error_called: Cell::new(false),
            })
        }
    }

    impl AsObservable for StubHelper {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl ZeroInflationHelper for StubHelper {
        fn base(&self) -> &ZeroInflationHelperBase {
            &self.base
        }

        fn implied_quote(&self) -> QlResult<Real> {
            Ok(0.01)
        }

        fn quote_error(&self) -> QlResult<Real> {
            self.error_called.set(true);
            Ok(-1.0)
        }

        fn set_term_structure(&self, term_structure: &Shared<dyn ZeroInflationTermStructure>) {
            self.curve_set.set(true);
            self.base.set_term_structure(term_structure);
        }
    }

    fn curve() -> Shared<dyn ZeroInflationTermStructure> {
        let reference = Date::new(27, Month::January, 2026);
        let dates = vec![
            Date::new(1, Month::December, 2025),
            Date::new(1, Month::December, 2030),
        ];
        let curve = ZeroInflationCurve::new(
            reference,
            dates,
            vec![0.02, 0.02],
            Frequency::Monthly,
            Actual360::new(),
            crate::math::interpolations::linear::Linear,
        )
        .unwrap();
        shared(curve) as Shared<dyn ZeroInflationTermStructure>
    }

    /// The inflation family satisfies the bound the bootstrap driver puts on
    /// `PiecewiseCurve::Helper`, so a piecewise zero-inflation curve can name
    /// `dyn ZeroInflationHelper` there against a
    /// `dyn ZeroInflationTermStructure` curve. The two associated types must
    /// agree, which is the whole point of the generalization, and only a paired
    /// instantiation checks it.
    #[test]
    fn inflation_helpers_satisfy_the_driver_bound() {
        fn accepts_driver_helper<H>()
        where
            H: BootstrapHelperShared<TS = dyn ZeroInflationTermStructure> + ?Sized,
        {
        }
        accepts_driver_helper::<dyn ZeroInflationHelper>();
    }

    /// What a driver sees of a helper.
    struct DriverView {
        quote_value: Real,
        quote_error: Real,
        pillar_date: Date,
        latest_relevant_date: Date,
        maturity_date: Date,
    }

    /// Exercises the helper the way
    /// [`IterativeBootstrap::calculate`](crate::termstructures::iterativebootstrap::IterativeBootstrap::calculate)
    /// does: through a type parameter bounded by [`BootstrapHelperShared`], so
    /// only that impl's methods are in scope.
    ///
    /// Reaching the bound through a type parameter is what makes the assertions
    /// below bite. Calling the same method names straight on a
    /// `Shared<dyn ZeroInflationHelper>` resolves to [`ZeroInflationHelper`]
    /// instead and never enters the impl under test, which would leave the
    /// routing claim untested.
    fn drive<H>(helper: &Shared<H>, curve: &Shared<dyn ZeroInflationTermStructure>) -> DriverView
    where
        H: BootstrapHelperShared<TS = dyn ZeroInflationTermStructure> + ?Sized,
    {
        helper.set_term_structure(curve);
        DriverView {
            quote_value: helper.quote_value().unwrap(),
            quote_error: helper.quote_error().unwrap(),
            pillar_date: helper.pillar_date(),
            latest_relevant_date: helper.latest_relevant_date(),
            maturity_date: helper.maturity_date(),
        }
    }

    /// Every driver-facing method routes through the [`ZeroInflationHelper`]
    /// trait, so a concrete helper's overrides run. Short-circuiting the impl
    /// to the base would leave a helper pricing off an unlinked handle and
    /// would silently swap its quote error for the default one - here, the
    /// stub's `-1.0` would become the default `0.03 - 0.01`. The curve is bound
    /// to a local because the base holds it weakly and never owns it.
    #[test]
    fn the_driver_bound_routes_through_the_trait_so_overrides_run() {
        let helper = StubHelper::new();
        let driver: Shared<dyn ZeroInflationHelper> = Shared::clone(&helper) as _;
        let curve = curve();

        let view = drive(&driver, &curve);

        assert!(
            helper.curve_set.get(),
            "set_term_structure override skipped"
        );
        assert!(helper.base.term_structure().is_ok());
        assert!(helper.error_called.get(), "quote_error override skipped");
        assert_eq!(view.quote_error, -1.0);
        assert_eq!(view.quote_value, 0.03);
    }

    #[test]
    fn the_driver_bound_reports_the_bases_dates() {
        let helper = StubHelper::new();
        let driver: Shared<dyn ZeroInflationHelper> = Shared::clone(&helper) as _;
        let curve = curve();

        let view = drive(&driver, &curve);

        assert_eq!(view.pillar_date, Date::new(1, Month::June, 2030));
        assert_eq!(view.latest_relevant_date, Date::new(1, Month::July, 2030));
        assert_eq!(view.maturity_date, Date::new(1, Month::June, 2030));
    }

    mod zero_coupon_swap_helper {
        //! The helper's place inside a bootstrap is exercised by the piecewise
        //! zero-inflation curve; what is checkable here is everything the helper
        //! decides on its own - its dates, the swap it caches, and the two
        //! wirings a bootstrap depends on but no numeric assertion would catch:
        //! that its index copy reads the curve handed to
        //! [`set_term_structure`](ZeroInflationHelper::set_term_structure), and
        //! that the relink does not travel back to the helper.

        use super::*;
        use crate::indexes::Index;
        use crate::indexes::inflation::UkRpi;
        use crate::instrument::Instrument;
        use crate::math::interpolations::linear::Linear;
        use crate::test_support::{Flag, as_observer};
        use crate::time::businessdayconvention::BusinessDayConvention;
        use crate::time::calendars::unitedkingdom::{Market, UnitedKingdom};
        use crate::time::date::Month::{August, June, May};
        use crate::time::daycounters::actual365fixed::Actual365Fixed;
        use crate::time::period::Period;
        use crate::time::timeunit::TimeUnit;
        use crate::types::Rate;

        /// The May 2007 figure, which the swap's base observation reads.
        const BASE_FIXING: Real = 195.0;
        /// The June 2007 figure, which is also the curve's base date.
        const CURVE_BASE_FIXING: Real = 200.0;

        fn today() -> Date {
            Date::new(13, Month::August, 2007)
        }

        /// One year out, so the fixing period observed under a three-month lag is
        /// May 2008 - a date the curve carries a node at.
        fn maturity() -> Date {
            Date::new(13, August, 2008)
        }

        fn curve_base_date() -> Date {
            Date::new(1, June, 2007)
        }

        fn lag() -> Period {
            Period::new(3, TimeUnit::Months)
        }

        fn settings_today() -> Shared<Settings<Date>> {
            let settings = shared(Settings::<Date>::new());
            settings.set_evaluation_date(today());
            settings
        }

        fn a_curve(rates: Vec<Rate>) -> Shared<dyn ZeroInflationTermStructure> {
            shared(
                ZeroInflationCurve::new(
                    today(),
                    vec![
                        curve_base_date(),
                        Date::new(1, May, 2008),
                        Date::new(1, June, 2012),
                    ],
                    rates,
                    Frequency::Monthly,
                    Actual360::new(),
                    Linear,
                )
                .expect("a well-formed zero inflation curve"),
            ) as Shared<dyn ZeroInflationTermStructure>
        }

        /// UK RPI with the two figures the swap needs on record: its base
        /// observation and the curve's base date. The index is left on an empty
        /// curve handle, so any forecast it produced itself would fail - only the
        /// helper's own copy, relinked to the bootstrapped curve, can forecast.
        fn an_index(settings: &Shared<Settings<Date>>) -> Shared<ZeroInflationIndex> {
            let index = shared(UkRpi::new(Shared::clone(settings)));
            index
                .add_fixing(Date::new(1, May, 2007), BASE_FIXING)
                .expect("a published figure");
            index
                .add_fixing(curve_base_date(), CURVE_BASE_FIXING)
                .expect("a published figure");
            index
        }

        fn a_helper(
            settings: &Shared<Settings<Date>>,
            interpolation: CpiInterpolationType,
        ) -> QlResult<Shared<ZeroCouponInflationSwapHelper>> {
            ZeroCouponInflationSwapHelper::new(
                Handle::new(shared(SimpleQuote::new(Some(0.03))) as Shared<dyn Quote>),
                lag(),
                maturity(),
                UnitedKingdom::new(Market::Settlement),
                BusinessDayConvention::ModifiedFollowing,
                Actual365Fixed::new(),
                &an_index(settings),
                interpolation,
                Shared::clone(settings),
            )
        }

        /// All four dates collapse onto the first day of the observed fixing
        /// period (`cpp:158-159`): 13 August 2008 less three months is 13 May
        /// 2008, whose monthly period starts on 1 May 2008. C++ leaves the
        /// relevant, maturity and pillar fields unset there and lets the base's
        /// fallbacks answer; this port sets the pillar explicitly - the same value
        /// - and leaves the other two to the fallbacks, as C++ does.
        ///
        /// The swap observes the *unrounded* 13 May 2008, the fixing period being
        /// the helper's rounding and not the contract's.
        #[test]
        fn the_dates_collapse_onto_the_observed_fixing_period() {
            let helper = a_helper(&settings_today(), CpiInterpolationType::Flat)
                .expect("a three-month lag covers UK RPI's availability");
            let period_start = Date::new(1, May, 2008);

            assert_eq!(helper.earliest_date(), period_start);
            assert_eq!(helper.latest_date(), period_start);
            assert_eq!(helper.pillar_date(), period_start);
            assert_eq!(helper.latest_relevant_date(), period_start);
            assert_eq!(helper.maturity_date(), period_start);

            let swap = helper.swap();
            let swap = swap.as_ref().expect("the swap builds");
            assert_eq!(swap.obs_date(), Date::new(13, May, 2008));
            assert_eq!(swap.maturity_date(), maturity());
            assert_eq!(swap.fixed_rate(), 0.0);
            assert_eq!(swap.nominal(), 1.0);
        }

        /// The contract starts at the evaluation date and follows it, which is
        /// what makes this a relative-date helper (`cpp:100`, `cpp:188`). The
        /// helper's own dates come from the maturity and so do not move.
        #[test]
        fn moving_the_evaluation_date_rebuilds_the_swap() {
            let settings = settings_today();
            let helper = a_helper(&settings, CpiInterpolationType::Flat).expect("a valid lag");
            assert_eq!(
                helper
                    .swap()
                    .as_ref()
                    .expect("the swap builds")
                    .start_date(),
                today()
            );

            let moved = Date::new(14, August, 2007);
            settings.set_evaluation_date(moved);

            assert_eq!(
                helper
                    .swap()
                    .as_ref()
                    .expect("the swap rebuilds")
                    .start_date(),
                moved
            );
            assert_eq!(helper.pillar_date(), Date::new(1, May, 2008));
        }

        /// The oracle, without a bootstrap: the quote the helper implies off a
        /// directly built curve is the fair rate of the same swap built by hand
        /// on that curve.
        ///
        /// It pins the whole chain the bootstrap relies on. The helper's index is
        /// a *copy* reading a handle that is empty until `set_term_structure`
        /// relinks it, so a copy that kept the caller's curve, or a relink that
        /// never reached the copy, would fail to forecast at all rather than
        /// answer a different number.
        #[test]
        fn the_implied_quote_is_the_swaps_fair_rate_on_the_curve_it_is_given() {
            let settings = settings_today();
            let helper = a_helper(&settings, CpiInterpolationType::Flat).expect("a valid lag");
            let curve = a_curve(vec![0.02, 0.03, 0.04]);

            assert!(
                helper.implied_quote().is_err(),
                "no curve has been handed over yet"
            );
            ZeroInflationHelper::set_term_structure(helper.as_ref(), &curve);

            let by_hand = ZeroCouponInflationSwap::new(
                SwapType::Payer,
                1.0,
                today(),
                maturity(),
                UnitedKingdom::new(Market::Settlement),
                BusinessDayConvention::ModifiedFollowing,
                Actual365Fixed::new(),
                0.0,
                shared(an_index(&settings).clone_linked_to(Handle::new(Shared::clone(&curve)))),
                lag(),
                CpiInterpolationType::Flat,
                None,
                None,
                Shared::clone(&settings),
            )
            .expect("a valid lag");

            let implied = helper.implied_quote().expect("the curve forecasts");
            assert!(implied > 0.0);
            assert!(
                (implied - by_hand.fair_rate().expect("the curve forecasts")).abs() < 1e-14,
                "implied {implied}"
            );
        }

        /// The H7 hazard, pinned: the helper's own swap is struck at zero on a
        /// flat 0 % nominal curve, so its NPV is nowhere near zero at the same
        /// point where the implied quote is a perfectly good rate. A bootstrap
        /// written against `helper.swap().npv() == 0` would never converge.
        ///
        /// The curve is bound to a local: the helper links it weakly, so a
        /// temporary would be dropped before the quote is read.
        #[test]
        fn the_cached_swap_does_not_price_to_zero_at_the_implied_quote() {
            let settings = settings_today();
            let helper = a_helper(&settings, CpiInterpolationType::Flat).expect("a valid lag");
            let curve = a_curve(vec![0.02, 0.03, 0.04]);
            ZeroInflationHelper::set_term_structure(helper.as_ref(), &curve);

            let implied = helper.implied_quote().expect("the curve forecasts");
            let mut swap = helper.swap_mut();
            let npv = swap
                .as_mut()
                .expect("the swap builds")
                .npv()
                .expect("the flat nominal curve discounts");

            assert!(implied.is_finite() && implied > 0.0);
            assert!(npv.abs() > 0.01, "npv was {npv}");
        }

        /// `cpp:107-110` and `cpp:199`, asserted structurally: the helper must not
        /// hear about the curve it is being bootstrapped against. Two paths could
        /// carry the news back - the copy still observing the handle the helper
        /// relinks, and the relink itself subscribing to the curve - and both are
        /// closed here. Either one open turns a solver step into a notification
        /// the helper rebroadcasts to the curve that is moving it.
        #[test]
        fn the_relink_reaches_neither_the_helper_nor_its_index_copy() {
            let helper =
                a_helper(&settings_today(), CpiInterpolationType::Flat).expect("a valid lag");
            let curve = a_curve(vec![0.02, 0.03, 0.04]);

            let on_helper = Flag::new();
            helper
                .observable()
                .register_observer(&as_observer(&on_helper));
            let on_index = Flag::new();
            helper
                .inflation_index()
                .observable()
                .register_observer(&as_observer(&on_index));

            ZeroInflationHelper::set_term_structure(helper.as_ref(), &curve);
            assert!(
                !Flag::is_up(&on_index),
                "the copy still observes the handle"
            );
            assert!(!Flag::is_up(&on_helper), "the relink reached the helper");

            curve.observable().notify_observers();
            assert!(
                !Flag::is_up(&on_helper),
                "the helper must not observe the curve it is bootstrapped against"
            );
        }

        /// The interpolated branch is omitted visibly: a caller asking for it is
        /// told, not quietly given the flat dates.
        #[test]
        fn the_interpolated_observation_branch_is_rejected() {
            let error = a_helper(&settings_today(), CpiInterpolationType::Linear)
                .err()
                .expect("the interpolated branch is deferred");
            assert!(error.message().contains("not ported"), "err was: {error}");
        }
    }
}
