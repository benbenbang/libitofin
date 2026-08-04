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
//! The concrete `ZeroCouponInflationSwapHelper` (`inflationhelpers.hpp:35`)
//! follows within EPIC Inflation (#705); this module is the base it plugs into.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::AsObservable;
use crate::quotes::Quote;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, BootstrapHelperShared};
use crate::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
use crate::time::date::Date;
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
}
