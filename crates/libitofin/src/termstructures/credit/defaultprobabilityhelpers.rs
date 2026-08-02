//! Bootstrap helpers for default-probability term structures.
//!
//! Port of the two typedefs at the head of
//! `ql/termstructures/credit/defaultprobabilityhelpers.hpp:41-44`:
//! `DefaultProbabilityHelper` is `BootstrapHelper<DefaultProbabilityTermStructure>`
//! and `RelativeDateDefaultProbabilityHelper` is
//! `RelativeDateBootstrapHelper<DefaultProbabilityTermStructure>`. They are the
//! credit twins of
//! [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper) and
//! [`RelativeDateRateHelper`](crate::termstructures::bootstraphelper::RelativeDateRateHelper),
//! and they carry no behaviour of their own: everything is inherited from the
//! shared [`BootstrapHelperBase`], instantiated here over
//! [`DefaultProbabilityTermStructure`] instead of the yield curve.
//!
//! Where C++ gets both families from one class template, this port needs two
//! traits, because the yield layer is typed on the bare `dyn RateHelper` object
//! and a trait generic over its term structure cannot be made into one. The
//! shared driver reaches both through [`BootstrapHelperShared`], implemented
//! below on `dyn DefaultProbabilityHelper`.
//!
//! `CdsHelper` and its `SpreadCdsHelper` / `UpfrontCdsHelper` subclasses
//! (`defaultprobabilityhelpers.hpp:47`) are not here yet; they follow within
//! EPIC Credit (#676).

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::AsObservable;
use crate::quotes::Quote;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, BootstrapHelperShared};
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::time::date::Date;
use crate::types::Real;

/// The shared state of a credit bootstrap helper: a
/// [`BootstrapHelperBase`] whose back-pointer is a default-probability curve.
pub type DefaultProbabilityHelperBase = BootstrapHelperBase<dyn DefaultProbabilityTermStructure>;

/// Bootstrap helper for the credit-curve bootstrap
/// (`DefaultProbabilityHelper`).
///
/// Mirrors [`RateHelper`](crate::termstructures::bootstraphelper::RateHelper)
/// exactly, over [`DefaultProbabilityTermStructure`]: a concrete helper embeds
/// a [`DefaultProbabilityHelperBase`], returns it from [`base`](Self::base) and
/// supplies [`implied_quote`](Self::implied_quote); the rest of the interface is
/// derived from the base. The same ownership contract holds - the curve is held
/// [`Weak`](std::rc::Weak) and never observed - since it is the one base that
/// enforces it.
pub trait DefaultProbabilityHelper: AsObservable {
    /// The embedded shared state.
    fn base(&self) -> &DefaultProbabilityHelperBase;

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
    /// A concrete helper that hands the curve to a pricing engine overrides
    /// this to relink that handle first, then delegates here.
    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
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

/// Credit bootstrap helper whose date schedule is relative to the evaluation
/// date (`RelativeDateDefaultProbabilityHelper`).
///
/// `CdsHelper` derives from this: a CDS schedule is rebuilt whenever the
/// evaluation date moves. The concrete helper builds its base with
/// [`BootstrapHelperBase::new_relative`], passing a closure that calls
/// [`initialize_dates`](Self::initialize_dates).
pub trait RelativeDateDefaultProbabilityHelper: DefaultProbabilityHelper {
    /// Rebuilds the helper's date schedule off the current evaluation date.
    fn initialize_dates(&self);
}

/// The credit half of the driver bound. Like the yield impl, every method
/// routes through the [`DefaultProbabilityHelper`] trait rather than straight
/// to the base, so a concrete helper's override still runs.
impl BootstrapHelperShared for dyn DefaultProbabilityHelper {
    type TS = dyn DefaultProbabilityTermStructure;

    fn set_term_structure(&self, term_structure: &Shared<dyn DefaultProbabilityTermStructure>) {
        DefaultProbabilityHelper::set_term_structure(self, term_structure);
    }

    fn quote_value(&self) -> QlResult<Real> {
        self.base().quote_value()
    }

    fn quote_error(&self) -> QlResult<Real> {
        DefaultProbabilityHelper::quote_error(self)
    }

    fn pillar_date(&self) -> Date {
        DefaultProbabilityHelper::pillar_date(self)
    }

    fn latest_relevant_date(&self) -> Date {
        DefaultProbabilityHelper::latest_relevant_date(self)
    }

    fn maturity_date(&self) -> Date {
        DefaultProbabilityHelper::maturity_date(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credit family satisfies the bound the bootstrap driver puts on
    /// `PiecewiseCurve::Helper`, so a credit piecewise curve can name
    /// `dyn DefaultProbabilityHelper` there against a
    /// `dyn DefaultProbabilityTermStructure` curve. The two associated types
    /// must agree, which is the whole point of the generalization, and only a
    /// paired instantiation checks it. This is a compile-time assertion: the
    /// credit bootstrap itself ports no behaviour yet.
    #[test]
    fn credit_helpers_satisfy_the_driver_bound() {
        fn accepts_driver_helper<H>()
        where
            H: BootstrapHelperShared<TS = dyn DefaultProbabilityTermStructure> + ?Sized,
        {
        }
        accepts_driver_helper::<dyn DefaultProbabilityHelper>();
    }
}
