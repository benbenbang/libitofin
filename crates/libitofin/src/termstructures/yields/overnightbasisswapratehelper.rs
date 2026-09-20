//! Overnight-Ibor basis helper from `basisswapratehelpers.cpp:121-198`.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Weak;

use crate::cashflow::CashFlow;
use crate::cashflows::{IborLeg, OvernightLeg};
use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::iborindex::{IborIndex, OvernightIndex};
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::Swap;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::swap::DiscountingSwapEngine;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::bootstraphelper::{
    BootstrapHelperBase, RateHelper, RelativeDateRateHelper,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::time::schedule::MakeSchedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Real};

/// Fits the Ibor forecast curve to a spread paid on a compounded overnight leg.
///
/// The overnight index keeps its supplied forecast curve. An empty discount
/// handle uses the fitted Ibor curve, matching the upstream implementation
/// (the upstream header describes a different fallback).
pub struct OvernightIborBasisSwapRateHelper {
    base: BootstrapHelperBase,
    swap: RefCell<Option<Swap>>,
    date_error: RefCell<Option<QlError>>,
    tenor: Period,
    settlement_days: Natural,
    calendar: Calendar,
    convention: BusinessDayConvention,
    end_of_month: bool,
    base_index: Shared<OvernightIndex>,
    other_index: Shared<IborIndex>,
    discount_handle: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
}

impl OvernightIborBasisSwapRateHelper {
    /// Builds a spot-starting helper with both legs on the Ibor payment schedule.
    ///
    /// # Errors
    /// Returns an error for missing quotes, mismatched settings, unsupported
    /// periods, or an invalid evaluation date or schedule.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        basis: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: Natural,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        base_index: &Shared<OvernightIndex>,
        other_index: &Shared<IborIndex>,
        discount_handle: Handle<dyn YieldTermStructure>,
    ) -> QlResult<Shared<Self>> {
        crate::require!(!basis.is_empty(), "basis quote handle is empty");
        crate::require!(
            Shared::ptr_eq(base_index.base().settings(), other_index.base().settings()),
            "basis indices must share settings"
        );
        let span = i64::from(Date::max_date() - Date::min_date());
        crate::require!(
            i64::from(settlement_days) <= span,
            "settlement days exceed supported range"
        );
        crate::require!(
            i64::from(base_index.fixing_days()) <= span
                && i64::from(other_index.fixing_days()) <= span,
            "index fixing days exceed supported range"
        );
        for period in [tenor, other_index.tenor()] {
            let days = match period.units() {
                TimeUnit::Days => 1,
                TimeUnit::Weeks => 7,
                TimeUnit::Months => 28,
                TimeUnit::Years => 365,
                _ => crate::fail!("basis periods must use days, weeks, months or years"),
            };
            crate::require!(
                period.length() > 0 && i64::from(period.length()) * days <= span,
                "basis periods must be positive and within supported range"
            );
        }
        let settings = base_index.base().settings().clone();
        let helper = Shared::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let other_index = shared(other_index.clone_with(term_structure_handle.handle()));
            let base = BootstrapHelperBase::new_relative(
                basis,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            other_index
                .settings()
                .register_fixing_observer(&other_index.name(), &base.observer());
            base_index.observable().register_observer(&base.observer());
            discount_handle.register_observer(&base.observer());
            Self {
                base,
                swap: RefCell::new(None),
                date_error: RefCell::new(None),
                tenor,
                settlement_days,
                calendar,
                convention,
                end_of_month,
                base_index: Shared::clone(base_index),
                other_index,
                discount_handle,
                settings,
                term_structure_handle,
            }
        });
        helper.try_initialize_dates()?;
        Ok(helper)
    }
}

impl AsObservable for OvernightIborBasisSwapRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for OvernightIborBasisSwapRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    fn implied_quote(&self) -> QlResult<Real> {
        if let Some(error) = self.date_error.borrow().as_ref() {
            return Err(error.clone());
        }
        self.base.term_structure()?;
        let mut guard = self.swap.borrow_mut();
        let swap = guard.as_mut().expect("initialize_dates populates the swap");
        swap.recalculate()?;
        Ok(-(swap.npv()? / swap.leg_bps(0)?) * 1.0e-4)
    }

    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl OvernightIborBasisSwapRateHelper {
    /// Rebuilds atomically, retaining date failures until a successful update.
    ///
    /// Legacy date/schedule assertions are contained before cached state changes.
    pub fn try_initialize_dates(&self) -> QlResult<()> {
        let result = catch_unwind(AssertUnwindSafe(|| self.rebuild_dates())).unwrap_or_else(|_| {
            Err(QlError::new(
                "invalid overnight basis schedule or date range",
                file!(),
                line!(),
            ))
        });
        match result {
            Ok((swap, earliest, maturity, latest_relevant)) => {
                *self.swap.borrow_mut() = Some(swap);
                self.base.set_earliest_date(earliest);
                self.base.set_maturity_date(maturity);
                self.base.set_latest_relevant_date(latest_relevant);
                self.base.set_pillar_date(latest_relevant);
                self.base.set_latest_date(latest_relevant);
                *self.date_error.borrow_mut() = None;
                Ok(())
            }
            Err(error) => {
                *self.date_error.borrow_mut() = Some(error.clone());
                Err(error)
            }
        }
    }

    fn rebuild_dates(&self) -> QlResult<(Swap, Date, Date, Date)> {
        let today = self
            .base
            .evaluation_date()
            .ok_or_else(|| QlError::new("evaluation date is not set", file!(), line!()))?;
        crate::require!(
            today >= Date::min_date() && today <= Date::max_date(),
            "evaluation date outside supported range"
        );
        let earliest = self.calendar.advance(
            today,
            self.settlement_days as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let maturity =
            self.calendar
                .advance_by_period(earliest, self.tenor, self.convention, false);
        let schedule = MakeSchedule::new()
            .from(earliest)
            .to(maturity)
            .with_tenor(self.other_index.tenor())
            .with_calendar(self.calendar.clone())
            .with_convention(self.convention)
            .end_of_month(self.end_of_month)
            .forwards()
            .build();
        let base_leg = OvernightLeg::new(schedule.clone(), Shared::clone(&self.base_index))
            .with_notional(100.0)
            .build()?;
        let coupons = IborLeg::new(schedule, Shared::clone(&self.other_index))
            .with_notional(100.0)
            .coupons()?;
        let last_fixing_end = coupons
            .last()
            .ok_or_else(|| QlError::new("Ibor leg is empty", file!(), line!()))?
            .fixing_end_date()?;
        let latest_relevant = maturity.max(last_fixing_end);
        let other_leg = coupons
            .into_iter()
            .map(|coupon| coupon as Shared<dyn CashFlow>)
            .collect();
        let mut swap = Swap::two_leg(base_leg, other_leg, Shared::clone(&self.settings));
        let discount = if self.discount_handle.is_empty() {
            self.term_structure_handle.handle()
        } else {
            self.discount_handle.clone()
        };
        let engine = shared_mut(DiscountingSwapEngine::new(
            discount,
            None,
            None,
            None,
            Shared::clone(&self.settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        Ok((swap, earliest, maturity, latest_relevant))
    }
}

impl RelativeDateRateHelper for OvernightIborBasisSwapRateHelper {
    fn initialize_dates(&self) {
        let _ = self.try_initialize_dates();
    }
}

#[cfg(test)]
#[path = "overnightbasisswapratehelper_tests.rs"]
mod tests;
