//! Cash flow paying a ratio of two index fixings.
//!
//! Port of `ql/cashflows/indexedcashflow.{hpp,cpp}`. An [`IndexedCashFlow`] is
//! not a coupon: nothing accrues, and the amount is the notional scaled by
//! `i(T)/i(0)`, or by `i(T)/i(0) - 1` when `growth_only` is set (the bond-type
//! and swap-type settings of `indexedcashflow.hpp:38-43`). The dates are taken
//! as given; the instrument around the flow does the adjusting.
//!
//! ## Reaching the index
//!
//! C++ stores `shared_ptr<Index>`. [`Index`] is object-*un*safe here - its
//! `add_fixings` is generic over the iterator it takes - so the flow is generic
//! over the index instead of holding a trait object. The index type is
//! therefore fixed at construction, which is what `ZeroInflationCashFlow`
//! wants anyway: it needs the concrete `ZeroInflationIndex` face that C++
//! recovers by keeping a second, downcast pointer beside the base's.
//!
//! ## Divergences from QuantLib
//!
//! The C++ flow is a `LazyObject` caching `amount_`. As with the rest of the
//! cash-flow layer the cache is omitted: [`amount`](CashFlow::amount) rereads
//! the two fixings each call, which is a pure function of the same inputs. The
//! behavioural half is kept - the flow forwards its index's notifications to
//! its own observers, the port of `registerWith(index_)`.
//!
//! `QL_REQUIRE(index_, "no index provided")` has no port: the index is a
//! non-null [`Shared`], so its presence is structural. `accept(AcyclicVisitor&)`
//! is unported, as elsewhere in the cash-flow layer.
//!
//! The `growth_only = false` ratio form is ported but has no consumer yet: the
//! zero-coupon inflation swap that #705 builds towards is growth-only.

use crate::cashflow::{CashFlow, cash_flow_has_occurred};
use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::indexes::index::Index;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::types::Real;

/// Cash flow dependent on an index ratio (`IndexedCashFlow`).
///
/// Built with [`new`](Self::new) over any [`Index`]; the fixings are read at
/// [`base_date`](Self::base_date) and [`fixing_date`](Self::fixing_date), and
/// the amount is paid at [`date`](Event::date).
pub struct IndexedCashFlow<I> {
    notional: Real,
    index: Shared<I>,
    base_date: Date,
    fixing_date: Date,
    payment_date: Date,
    growth_only: bool,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl<I: Index> IndexedCashFlow<I> {
    /// Builds a flow paying `notional` scaled by the `index` ratio between
    /// `base_date` and `fixing_date`.
    ///
    /// The flow registers a forwarding observer with the index, so a new
    /// fixing - or, through the index, a change of evaluation date - reaches
    /// the flow's own observers.
    pub fn new(
        notional: Real,
        index: Shared<I>,
        base_date: Date,
        fixing_date: Date,
        payment_date: Date,
        growth_only: bool,
    ) -> Self {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        let flow = IndexedCashFlow {
            notional,
            index,
            base_date,
            fixing_date,
            payment_date,
            growth_only,
            observable,
            forwarder,
        };
        flow.register_with(flow.index.observable());
        flow
    }

    /// Registers this flow's forwarding observer with `observable`, the port of
    /// `registerWith` (`indexedcashflow.cpp:34`).
    fn register_with(&self, observable: &Observable) {
        observable.register_observer(&(self.forwarder.clone() as SharedMut<dyn Observer>));
    }

    /// The notional the ratio scales.
    pub fn notional(&self) -> Real {
        self.notional
    }

    /// The index the ratio is taken on.
    pub fn index(&self) -> &Shared<I> {
        &self.index
    }

    /// The date the base fixing is read at.
    pub fn base_date(&self) -> Date {
        self.base_date
    }

    /// The date the index fixing is read at.
    pub fn fixing_date(&self) -> Date {
        self.fixing_date
    }

    /// Whether the flow pays the growth `i(T)/i(0) - 1` rather than the ratio
    /// `i(T)/i(0)`.
    pub fn growth_only(&self) -> bool {
        self.growth_only
    }

    /// The fixing at [`base_date`](Self::base_date).
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Index::fixing`] raises.
    pub fn base_fixing(&self) -> QlResult<Real> {
        self.index.fixing(self.base_date, false)
    }

    /// The fixing at [`fixing_date`](Self::fixing_date).
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Index::fixing`] raises.
    pub fn index_fixing(&self) -> QlResult<Real> {
        self.index.fixing(self.fixing_date, false)
    }

    /// The `performCalculations` ratio (`indexedcashflow.cpp:43-51`) applied to
    /// a base fixing `i0` and an index fixing `i1`.
    ///
    /// Exposed so that a flow observing the index differently - as
    /// `ZeroInflationCashFlow` does, through lagged fixings - pays the same
    /// ratio computed off *its* two fixings. Rust has no virtual dispatch to
    /// reach such an override from [`amount`](CashFlow::amount) here.
    pub(super) fn amount_from(&self, i0: Real, i1: Real) -> Real {
        if self.growth_only {
            self.notional * (i1 / i0 - 1.0)
        } else {
            self.notional * (i1 / i0)
        }
    }
}

impl<I> AsObservable for IndexedCashFlow<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Index> Event for IndexedCashFlow<I> {
    fn date(&self) -> Date {
        self.payment_date
    }

    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool> {
        cash_flow_has_occurred(self.payment_date, settings, ref_date, include_ref_date)
    }
}

impl<I: Index> CashFlow for IndexedCashFlow<I> {
    fn amount(&self) -> QlResult<Real> {
        Ok(self.amount_from(self.base_fixing()?, self.index_fixing()?))
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }

    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::Observer;
    use crate::shared::{shared, shared_mut};
    use crate::time::calendar::Calendar;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::Month::{December, January};
    use crate::types::Rate;

    /// A bare [`Index`] over the D11 store: every date is a valid fixing date,
    /// and a fixing is whatever was recorded on that exact day. It stands in
    /// for the `shared_ptr<Index>` C++ takes, so the flow is exercised against
    /// the abstract base it ports rather than against one refinement of it.
    struct TestIndex {
        settings: Shared<Settings<Date>>,
        observable: Observable,
    }

    impl Index for TestIndex {
        fn name(&self) -> String {
            "TestIndex".into()
        }
        fn fixing_calendar(&self) -> Calendar {
            NullCalendar::new()
        }
        fn is_valid_fixing_date(&self, _fixing_date: Date) -> bool {
            true
        }
        fn fixing(&self, fixing_date: Date, _forecast_todays_fixing: bool) -> QlResult<Rate> {
            match self.past_fixing(fixing_date)? {
                Some(rate) => Ok(rate),
                None => crate::fail!("no fixing for {fixing_date}"),
            }
        }
        fn settings(&self) -> &Settings<Date> {
            &self.settings
        }
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    fn base_date() -> Date {
        Date::new(1, January, 2021)
    }

    fn fixing_date() -> Date {
        Date::new(1, January, 2022)
    }

    fn payment_date() -> Date {
        Date::new(15, January, 2022)
    }

    /// An index holding 100 at the base date and 110 at the fixing date, so the
    /// ratio is 1.1 and the two dates are told apart by the value each returns.
    fn an_index() -> Shared<TestIndex> {
        let index = shared(TestIndex {
            settings: shared(Settings::<Date>::new()),
            observable: Observable::new(),
        });
        index.add_fixing(base_date(), 100.0).unwrap();
        index.add_fixing(fixing_date(), 110.0).unwrap();
        index
    }

    fn a_flow(growth_only: bool) -> IndexedCashFlow<TestIndex> {
        IndexedCashFlow::new(
            1000.0,
            an_index(),
            base_date(),
            fixing_date(),
            payment_date(),
            growth_only,
        )
    }

    /// The bond-type setting of `indexedcashflow.hpp:42`: the amount is the
    /// full ratio, `1000 * 110/100 = 1100`.
    #[test]
    fn an_indexed_cash_flow_pays_the_index_ratio() {
        let flow = a_flow(false);

        assert_eq!(flow.notional(), 1000.0);
        assert!(!flow.growth_only());
        assert_eq!(flow.base_date(), base_date());
        assert_eq!(flow.fixing_date(), fixing_date());
        assert_eq!(flow.date(), payment_date());
        assert_eq!(flow.ex_coupon_date(), None);
        assert!(flow.as_coupon().is_none());

        assert!((flow.base_fixing().unwrap() - 100.0).abs() < 1e-12);
        assert!((flow.index_fixing().unwrap() - 110.0).abs() < 1e-12);
        assert!((flow.amount().unwrap() - 1100.0).abs() < 1e-10);
    }

    /// The swap-type setting of `indexedcashflow.hpp:43`: the amount is the
    /// growth alone, `1000 * (110/100 - 1) = 100`.
    #[test]
    fn a_growth_only_indexed_cash_flow_pays_the_ratio_less_one() {
        let flow = a_flow(true);

        assert!(flow.growth_only());
        assert!((flow.amount().unwrap() - 100.0).abs() < 1e-10);
    }

    /// `registerWith(index_)` (`indexedcashflow.cpp:34`): what the index
    /// broadcasts, the flow rebroadcasts.
    #[test]
    fn an_indexed_cash_flow_forwards_its_index_notifications() {
        #[derive(Default)]
        struct Flag {
            up: bool,
        }
        impl Observer for Flag {
            fn update(&mut self) {
                self.up = true;
            }
        }

        let flow = a_flow(true);
        let flag = shared_mut(Flag::default());
        flow.observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        Index::observable(&**flow.index()).notify_observers();
        assert!(flag.borrow().up);
    }

    /// A fixing the store does not hold is an error rather than a silent
    /// `Null<Real>` ratio (D4).
    #[test]
    fn a_missing_fixing_surfaces_as_an_error() {
        let flow = IndexedCashFlow::new(
            1000.0,
            an_index(),
            base_date(),
            Date::new(1, December, 2021),
            payment_date(),
            true,
        );

        assert!(flow.amount().is_err());
    }
}
