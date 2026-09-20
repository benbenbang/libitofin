//! Atomic assembly of two mutually coupled global discount curves.

use super::{IborIborBasisSwapRateHelper, PiecewiseYieldCurve};
use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::math::interpolations::loglinear::LogLinear;
use crate::quotes::Quote;
use crate::require;
use crate::shared::{Shared, shared};
use crate::termstructures::RateHelper;
use crate::termstructures::bootstraptraits::Discount;
use crate::termstructures::globalbootstrap::GlobalBootstrap;
use crate::termstructures::multicurve::MultiCurve;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date,
    daycounter::DayCounter, period::Period,
};

/// Owned construction inputs for a basis helper, reusable as a joint-curve template.
#[derive(Clone)]
pub struct BasisSwapHelperConfig {
    /// Live basis spread on the base index leg.
    pub quote: Handle<dyn Quote>,
    /// Swap maturity tenor.
    pub tenor: Period,
    /// Spot settlement lag.
    pub settlement_days: u32,
    /// Schedule calendar.
    pub calendar: Calendar,
    /// Schedule adjustment.
    pub convention: BusinessDayConvention,
    /// Preserve end-of-month dates.
    pub end_of_month: bool,
    /// Index on the spread-paying leg.
    pub base_index: Shared<IborIndex>,
    /// Index on the opposite leg.
    pub other_index: Shared<IborIndex>,
    /// Exogenous discount curve.
    pub discount: Handle<dyn YieldTermStructure>,
    /// Fit the base-index curve when true, the other-index curve otherwise.
    pub bootstrap_base_curve: bool,
}

impl BasisSwapHelperConfig {
    /// Construct a standalone helper retaining these market inputs.
    pub fn build(&self) -> QlResult<Shared<IborIborBasisSwapRateHelper>> {
        self.build_with(&self.base_index, &self.other_index)
    }

    fn build_with(
        &self,
        base: &Shared<IborIndex>,
        other: &Shared<IborIndex>,
    ) -> QlResult<Shared<IborIborBasisSwapRateHelper>> {
        IborIborBasisSwapRateHelper::try_new(
            self.quote.clone(),
            self.tenor,
            self.settlement_days,
            self.calendar.clone(),
            self.convention,
            self.end_of_month,
            base,
            other,
            self.discount.clone(),
            self.bootstrap_base_curve,
        )
    }
}

type Curve = PiecewiseYieldCurve<Discount, LogLinear, GlobalBootstrap>;

/// A fixed pair of Discount/LogLinear/GlobalBootstrap curves.
///
/// Internal forecast links are private and weak. Exported handles retain the
/// joint owner, so consumers retaining a handle keep both contributors alive.
/// Extracting only `Handle::current_link()` discards that additional ownership.
/// Helpers supplied in the plain strips are reserved for this assembly and must
/// not subsequently be reused by another curve. Basis configurations are copied;
/// their standalone helpers are never rebound.
pub struct JointYieldCurves {
    curves: [Handle<dyn YieldTermStructure>; 2],
}

impl JointYieldCurves {
    /// Assemble both curves atomically, with member 0 fitting the base index.
    ///
    /// Each member requires a basis template. All templates must share the same
    /// index objects and settings. Plain helpers must be distinct and unassigned.
    /// Bootstrap failures remain fallible on the first query and later updates.
    pub fn new(
        reference: Date,
        mut helpers: [Vec<Shared<dyn RateHelper>>; 2],
        basis: &[BasisSwapHelperConfig],
        dc: DayCounter,
        accuracy: f64,
    ) -> QlResult<Self> {
        require!(
            reference != Date::null(),
            "joint reference date must not be null"
        );
        require!(
            accuracy.is_finite() && accuracy > 0.0,
            "accuracy must be finite and positive"
        );
        require!(
            !basis.is_empty(),
            "joint curves require basis helpers on both members"
        );
        let first = &basis[0];
        let settings = first.base_index.base().settings();
        require!(
            !Shared::ptr_eq(&first.base_index, &first.other_index),
            "joint indices must be distinct"
        );
        require!(
            Shared::ptr_eq(settings, first.other_index.base().settings()),
            "joint indices must share settings"
        );
        let mut sides = [false; 2];
        for item in basis {
            require!(
                Shared::ptr_eq(&item.base_index, &first.base_index)
                    && Shared::ptr_eq(&item.other_index, &first.other_index),
                "basis templates must use the same index objects"
            );
            sides[usize::from(!item.bootstrap_base_curve)] = true;
        }
        require!(
            sides.into_iter().all(|side| side),
            "joint curves require basis helpers on both members"
        );
        let all: Vec<_> = helpers.iter().flatten().collect();
        for (i, helper) in all.iter().enumerate() {
            require!(
                !helper.base().has_curve_owner() && helper.base().term_structure().is_err(),
                "helper already belongs to a curve"
            );
            require!(
                all[..i].iter().all(|other| !Shared::ptr_eq(helper, other)),
                "duplicate joint helper"
            );
            require!(
                helper
                    .base()
                    .settings()
                    .is_none_or(|value| Shared::ptr_eq(value, settings)),
                "helper settings differ from joint indices"
            );
        }
        let slots = [RelinkableHandle::empty(), RelinkableHandle::empty()];
        let base = shared(first.base_index.clone_with(slots[0].handle()));
        let other = shared(first.other_index.clone_with(slots[1].handle()));
        for item in basis {
            helpers[usize::from(!item.bootstrap_base_curve)].push(item.build_with(&base, &other)?);
        }
        let build = |strip| {
            Curve::with_bootstrap(
                reference,
                strip,
                dc.clone(),
                LogLinear,
                GlobalBootstrap::new(Some(accuracy), None, Vec::new()),
            )
        };
        let [a, b] = helpers;
        let first = build(a)?;
        let second = build(b)?;
        let owner = MultiCurve::new(accuracy);
        let a = owner.add_bootstrapped_curve(&slots[0], first.clone())?;
        let b = owner.add_bootstrapped_curve(&slots[1], second.clone())?;
        Ok(Self {
            curves: [a.retaining(owner.clone()), b.retaining(owner)],
        })
    }

    /// Return an externally usable handle retaining both contributors.
    pub fn curve(&self, member: usize) -> QlResult<Handle<dyn YieldTermStructure>> {
        require!(member < 2, "joint curve member must be 0 or 1");
        Ok(self.curves[member].clone())
    }
}

#[cfg(test)]
#[path = "jointyieldcurves_tests.rs"]
mod tests;
