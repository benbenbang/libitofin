//! Rate helpers over basis swaps.
//!
//! Port of `ql/experimental/termstructures/basisswapratehelpers.{hpp,cpp}`:
//! [`IborIborBasisSwapRateHelper`], the coupling instrument of a joint
//! multi-curve bootstrap (a 3m curve and a 6m curve each reading the other
//! through the same basis quotes). `OvernightIborBasisSwapRateHelper`
//! (`basisswapratehelpers.hpp:85`, `.cpp:121`) is not ported here; it is
//! tracked separately in [#1060](https://github.com/benbenbang/libitofin/issues/1060).

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
/// Observation follows `.cpp:53-55`: the helper observes the kept index, the
/// discount handle, and the cloned index's fixing history. Subscribing directly
/// to that history preserves fixing updates without observing the clone's
/// forecasting handle, matching C++'s `unregisterWith(termStructureHandle_)`.
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
            let (cloned, kept) = if bootstrap_base_curve {
                (&base_index, &other_index)
            } else {
                (&other_index, &base_index)
            };
            cloned
                .settings()
                .register_fixing_observer(&cloned.name(), &base.observer());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::quotes::SimpleQuote;
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::yields::{FlatForward, PiecewiseYieldCurve};
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    fn flat(reference: Date, rate: Real) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    struct Market {
        settings: Shared<Settings<Date>>,
        euribor3m: Shared<IborIndex>,
        euribor6m: Shared<IborIndex>,
        curve3m: Shared<dyn YieldTermStructure>,
        curve6m: Shared<dyn YieldTermStructure>,
        discount: Handle<dyn YieldTermStructure>,
    }

    /// Flat 3m, 6m and discount curves, the two Euribor indices forecasting
    /// off their own curve, all on one settings.
    fn market() -> Market {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(23, Month::October, 2025);
        settings.set_evaluation_date(today);
        let curve3m = flat(today, 0.03);
        let curve6m = flat(today, 0.035);
        let discount = Handle::new(flat(today, 0.02));
        let euribor3m = shared(Euribor::three_months(
            Handle::new(Shared::clone(&curve3m)),
            Shared::clone(&settings),
        ));
        let euribor6m = shared(Euribor::six_months(
            Handle::new(Shared::clone(&curve6m)),
            Shared::clone(&settings),
        ));
        Market {
            settings,
            euribor3m,
            euribor6m,
            curve3m,
            curve6m,
            discount,
        }
    }

    fn helper(m: &Market, bootstrap_base_curve: bool) -> Shared<IborIborBasisSwapRateHelper> {
        IborIborBasisSwapRateHelper::new(
            Handle::new(shared(SimpleQuote::new(0.002)) as Shared<dyn Quote>),
            Period::new(5, TimeUnit::Years),
            m.euribor3m.fixing_days(),
            m.euribor3m.fixing_calendar(),
            m.euribor3m.business_day_convention(),
            m.euribor3m.end_of_month(),
            &m.euribor3m,
            &m.euribor6m,
            m.discount.clone(),
            bootstrap_base_curve,
        )
    }

    /// An independent copy of the helper's swap on the original indices, with
    /// `basis` on the base leg and notional 1, priced off the discount curve.
    fn independent_swap_npv(m: &Market, h: &IborIborBasisSwapRateHelper, basis: Real) -> Real {
        let leg = |index: &Shared<IborIndex>, spread: Real| {
            let schedule = MakeSchedule::new()
                .from(h.earliest_date())
                .to(h.maturity_date())
                .with_tenor(index.tenor())
                .with_calendar(m.euribor3m.fixing_calendar())
                .with_convention(m.euribor3m.business_day_convention())
                .end_of_month(m.euribor3m.end_of_month())
                .forwards()
                .build();
            IborLeg::new(schedule, Shared::clone(index))
                .with_spread(spread)
                .with_notional(1.0)
                .build()
                .expect("the leg builds")
        };
        let mut swap = Swap::two_leg(
            leg(&m.euribor3m, basis),
            leg(&m.euribor6m, 0.0),
            Shared::clone(&m.settings),
        );
        let engine = shared_mut(DiscountingSwapEngine::new(
            m.discount.clone(),
            None,
            None,
            None,
            Shared::clone(&m.settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        swap.npv().expect("the swap prices")
    }

    /// Either way round, the implied basis is the spread that zeroes the same
    /// swap built independently on the un-cloned indices, once the helper's own
    /// handle points at the curve the kept index already reads.
    #[test]
    fn implied_quote_zeroes_the_independent_swap() {
        let m = market();
        for (bootstrap_base_curve, curve) in [(true, &m.curve3m), (false, &m.curve6m)] {
            let h = helper(&m, bootstrap_base_curve);
            assert!(h.implied_quote().is_err(), "no curve set yet");
            h.set_term_structure(curve);
            let basis = h.implied_quote().expect("the basis solves");
            assert!(basis.abs() > 1.0e-4, "flat 3% vs 3.5% is a real basis");
            let npv = independent_swap_npv(&m, &h, basis);
            assert!(npv.abs() < 1.0e-12, "swap at the implied basis: {npv}");
            let off = independent_swap_npv(&m, &h, basis + 1.0e-4);
            assert!(off.abs() > 1.0e-6, "a 1bp bump moves the swap: {off}");
        }
    }

    /// Spot is two TARGET days after today, maturity is the tenor past spot,
    /// and the pillar sits at the latest relevant date, past the maturity by
    /// the last coupon's estimation tail (`.cpp:64-66`, `:87-91`).
    #[test]
    fn initialize_dates_places_the_pillar_at_the_latest_relevant_date() {
        let m = market();
        let h = helper(&m, true);
        let calendar = Target::new();
        let spot = calendar.advance(
            Date::new(23, Month::October, 2025),
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        assert_eq!(h.earliest_date(), spot);
        assert_eq!(
            h.maturity_date(),
            calendar.advance_by_period(
                spot,
                Period::new(5, TimeUnit::Years),
                BusinessDayConvention::ModifiedFollowing,
                false
            )
        );
        assert!(h.latest_relevant_date() >= h.maturity_date());
        assert_eq!(h.pillar_date(), h.latest_relevant_date());
        assert_eq!(h.latest_date(), h.latest_relevant_date());
    }

    #[test]
    fn either_index_fixing_invalidates_and_recalibrates_the_live_curve() {
        for bootstrap_base in [true, false] {
            for change_base in [true, false] {
                let m = market();
                let h = helper(&m, bootstrap_base);
                let today = m.settings.evaluation_date().unwrap();
                let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
                    today,
                    vec![Shared::clone(&h) as Shared<dyn RateHelper>],
                    Actual360::new(),
                    LogLinear,
                )
                .unwrap();
                let before = curve.discount_date(h.pillar_date(), false).unwrap();
                let helper_flag = Flag::new();
                let curve_flag = Flag::new();
                h.observable().register_observer(&as_observer(&helper_flag));
                curve.register_observer(&as_observer(&curve_flag));
                let index = if change_base {
                    &m.euribor3m
                } else {
                    &m.euribor6m
                };
                index.add_fixing(today, 0.08).unwrap();
                assert!(
                    Flag::is_up(&helper_flag),
                    "bootstrap_base={bootstrap_base}, change_base={change_base}"
                );
                assert!(Flag::is_up(&curve_flag));
                let after = curve.discount_date(h.pillar_date(), false).unwrap();
                assert!((after - before).abs() > 1e-4);
                assert!(h.quote_error().unwrap().abs() < 1e-12);
                let weak_curve = Shared::downgrade(&curve);
                drop(curve);
                assert!(weak_curve.upgrade().is_none());
                assert!(h.implied_quote().is_err());
            }
        }
    }

    #[test]
    fn fitted_curve_changes_and_relinks_do_not_notify_the_helper() {
        for bootstrap_base in [true, false] {
            let m = market();
            let h = helper(&m, bootstrap_base);
            let quote = shared(SimpleQuote::new(0.04));
            let curve = shared(FlatForward::new(
                m.settings.evaluation_date().unwrap(),
                Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>;
            let flag = Flag::new();
            h.observable().register_observer(&as_observer(&flag));
            h.set_term_structure(&curve);
            assert!(!Flag::is_up(&flag));
            let before = h.implied_quote().unwrap();
            quote.set_value(0.05);
            assert!(!Flag::is_up(&flag));
            let after = h.implied_quote().unwrap();
            assert!((after - before).abs() > 1e-4);
            h.set_term_structure(&m.curve3m);
            assert!(!Flag::is_up(&flag));
        }
    }
}
