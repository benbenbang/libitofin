//! Rate helpers over basis swaps.
//!
//! Port of `ql/experimental/termstructures/basisswapratehelpers.{hpp,cpp}`:
//! [`IborIborBasisSwapRateHelper`], the coupling instrument of a joint
//! multi-curve bootstrap (a 3m curve and a 6m curve each reading the other
//! through the same basis quotes). `OvernightIborBasisSwapRateHelper`
//! (`basisswapratehelpers.hpp:85`, `.cpp:121`) is not ported here; it is
//! omitted visibly and queued as its own follow-up.

use std::cell::RefCell;
use std::rc::Weak;

use crate::cashflow::CashFlow;
use crate::cashflows::IborLeg;
use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::iborindex::IborIndex;
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

/// Bootstrap helper over an ibor-ibor basis swap
/// (`IborIborBasisSwapRateHelper`, `basisswapratehelpers.hpp:41`).
///
/// The swap pays `base_index + basis` and receives `other_index`
/// (`.hpp:31-39`). Exactly one of the two forecast curves is the one being
/// bootstrapped: with `bootstrap_base_curve` the base index is re-curved onto
/// the helper's own [`RelinkableHandle`] through [`IborIndex::clone_with`] and
/// the other index is kept as supplied, and the other way round without it
/// (`.cpp:43-52`). The kept index must carry its own forecast curve. Discounting
/// is always exogenous, off `discount_handle` (`.hpp:39`).
///
/// [`implied_quote`](RateHelper::implied_quote) is the basis that zeroes the
/// swap, `-(NPV / legBPS(0)) * 1e-4` (`.cpp:106-109`): the swap is
/// force-recalculated first (the C++ `swap_->deepUpdate()`), because the
/// helper's pricing handle is weak-linked and unobserved, so cached results go
/// stale as the bootstrap moves the curve.
///
/// Observation follows `.cpp:53-55` minus the cloned index: the helper observes
/// the kept index and the discount handle, and never its own clone (see the
/// module doc of [`ratehelpers`](super::ratehelpers) for why the C++
/// `unregisterWith(termStructureHandle_)` has no Rust counterpart).
pub struct IborIborBasisSwapRateHelper {
    base: BootstrapHelperBase,
    swap: RefCell<Option<Swap>>,
    tenor: Period,
    settlement_days: Natural,
    calendar: Calendar,
    convention: BusinessDayConvention,
    end_of_month: bool,
    base_index: Shared<IborIndex>,
    other_index: Shared<IborIndex>,
    discount_handle: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
}

impl IborIborBasisSwapRateHelper {
    /// A basis-swap helper fitting the `basis` quote over a spot-starting swap
    /// of `tenor` (`basisswapratehelpers.cpp:29-61`). `bootstrap_base_curve`
    /// selects which index's forecast curve the helper bootstraps; the other
    /// index is used as supplied, so it must already forecast off a curve.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        basis: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: Natural,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        base_index: &Shared<IborIndex>,
        other_index: &Shared<IborIndex>,
        discount_handle: Handle<dyn YieldTermStructure>,
        bootstrap_base_curve: bool,
    ) -> Shared<IborIborBasisSwapRateHelper> {
        let settings = base_index.base().settings().clone();
        Shared::new_cyclic(|weak: &Weak<IborIborBasisSwapRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let (base_index, other_index) = if bootstrap_base_curve {
                (
                    shared(base_index.clone_with(term_structure_handle.handle())),
                    Shared::clone(other_index),
                )
            } else {
                (
                    Shared::clone(base_index),
                    shared(other_index.clone_with(term_structure_handle.handle())),
                )
            };
            let base = BootstrapHelperBase::new_relative(
                basis,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            let kept = if bootstrap_base_curve {
                &other_index
            } else {
                &base_index
            };
            kept.observable().register_observer(&base.observer());
            discount_handle.register_observer(&base.observer());
            let helper = IborIborBasisSwapRateHelper {
                base,
                swap: RefCell::new(None),
                tenor,
                settlement_days,
                calendar,
                convention,
                end_of_month,
                base_index,
                other_index,
                discount_handle,
                settings,
                term_structure_handle,
            };
            helper.initialize_dates();
            helper
        })
    }

    /// One leg of the helper's swap on `index`, notional 100 (`.cpp:68-83`),
    /// with its last coupon's fixing end date.
    fn leg(&self, index: &Shared<IborIndex>) -> (Vec<Shared<dyn CashFlow>>, Date) {
        let schedule = MakeSchedule::new()
            .from(self.base.earliest_date())
            .to(self.base.maturity_date())
            .with_tenor(index.tenor())
            .with_calendar(self.calendar.clone())
            .with_convention(self.convention)
            .end_of_month(self.end_of_month)
            .forwards()
            .build();
        let coupons = IborLeg::new(schedule, Shared::clone(index))
            .with_notional(100.0)
            .coupons()
            .expect("a spot-to-maturity ibor leg with a notional builds");
        let last_fixing_end = coupons
            .last()
            .expect("a leg over a non-empty schedule has a last coupon")
            .fixing_end_date()
            .expect("the last coupon's estimation period is well defined");
        let leg = coupons
            .into_iter()
            .map(|coupon| coupon as Shared<dyn CashFlow>)
            .collect();
        (leg, last_fixing_end)
    }
}

impl AsObservable for IborIborBasisSwapRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for IborIborBasisSwapRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The basis that zeroes the swap on the current curve
    /// (`basisswapratehelpers.cpp:106-109`), after a forced recalculation.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        let mut guard = self.swap.borrow_mut();
        let swap = guard
            .as_mut()
            .expect("initialize_dates populates the swap at construction");
        swap.recalculate()?;
        const BASIS_POINT: Real = 1.0e-4;
        Ok(-(swap.npv()? / swap.leg_bps(0)?) * BASIS_POINT)
    }

    /// Weak-links the helper's own forecasting handle to the bootstrapping
    /// curve, non-owning and unobserved (`.cpp:97-104`, `observer = false`),
    /// then records the curve on the base.
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for IborIborBasisSwapRateHelper {
    /// Rebuilds the swap off the current evaluation date (`initializeDates`,
    /// `.cpp:63-94`): spot is `settlement_days` business days after today
    /// (`Following`), maturity is `tenor` past spot on the helper's convention,
    /// and each leg runs spot to maturity on its index's tenor. The latest
    /// relevant date, which is also the pillar, is the latest of the maturity
    /// and both legs' last fixing end dates (`.cpp:87-90`).
    fn initialize_dates(&self) {
        let today = self
            .base
            .evaluation_date()
            .expect("a relative-date helper always tracks an evaluation date");
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
        self.base.set_earliest_date(earliest);
        self.base.set_maturity_date(maturity);

        let (base_leg, base_fixing_end) = self.leg(&self.base_index);
        let (other_leg, other_fixing_end) = self.leg(&self.other_index);

        let latest_relevant = maturity.max(base_fixing_end.max(other_fixing_end));
        self.base.set_latest_relevant_date(latest_relevant);
        self.base.set_pillar_date(latest_relevant);
        self.base.set_latest_date(latest_relevant);

        let mut swap = Swap::two_leg(base_leg, other_leg, Shared::clone(&self.settings));
        let engine = shared_mut(DiscountingSwapEngine::new(
            self.discount_handle.clone(),
            None,
            None,
            None,
            Shared::clone(&self.settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        *self.swap.borrow_mut() = Some(swap);
    }
}
