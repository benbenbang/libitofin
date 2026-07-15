//! Bond rate helpers for the yield-curve bootstrap.
//!
//! Port of `ql/termstructures/yield/bondhelpers.{hpp,cpp}`: [`BondHelper`], a
//! bootstrap helper that fits a bond's quoted clean or dirty price, and
//! [`FixedRateBondHelper`], the fixed-coupon constructor over it.
//!
//! Unlike the schedule-derived helpers of
//! [`ratehelpers`](super::ratehelpers), a bond helper is a *fixed*-date helper
//! (the C++ `BondHelper : public RateHelper`, not a `RelativeDateRateHelper`):
//! its bond and its dates are built once, and the hpp warns that "the reference
//! date does not change between calls of `setTermStructure()`". So it uses
//! [`BootstrapHelperBase::new`] and does not observe the evaluation date.
//!
//! The C++ ctor clones the passed bond (`ext::make_shared<Bond>(*bond)`) so the
//! helper has sole ownership of it, guarding against external pricing-engine
//! installs corrupting the bootstrap. This port takes the [`Bond`] by value:
//! the move gives the same sole ownership structurally, with no clone.
//!
//! [`CPIBondHelper`](https://www.quantlib.org) (`bondhelpers.cpp:116`) is not
//! ported: the inflation sibling needs the CPI-bond machinery, which is not on
//! main. The C++ `accept`/`AcyclicVisitor` hooks are dropped, as on every other
//! helper.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::instrument::Instrument;
use crate::instruments::{Bond, BondPriceType, FixedRateBond};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::DiscountingBondEngine;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, RateHelper};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real};

/// Bootstrap helper over a bond's quoted price (`BondHelper`).
///
/// The helper fits the fixed clean (or dirty) price of a bond on the
/// bootstrapping curve. It owns its bond and prices it through a
/// [`DiscountingBondEngine`] installed on the helper's own relinkable handle
/// (`bondhelpers.cpp:40`), so the bond discounts against the curve it is being
/// bootstrapped against.
///
/// [`implied_quote`](RateHelper::implied_quote) forces the bond to recalculate
/// before reading its price (`bondhelpers.cpp:61`): the helper deliberately does
/// not observe the curve (its handle is weak-linked, unobserved), so the bond's
/// cached settlement value would otherwise go stale when the bootstrap moves the
/// curve.
pub struct BondHelper {
    base: BootstrapHelperBase,
    bond: RefCell<Bond>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    price_type: BondPriceType,
}

impl BondHelper {
    /// A bond helper fitting `price` (a clean- or dirty-price handle per
    /// `price_type`) with the schedule of `bond`, which the helper takes sole
    /// ownership of.
    ///
    /// The latest date is the bond's last cash-flow date (later than maturity
    /// when the last coupon is date-adjusted), the earliest date its next
    /// cash-flow date (`bondhelpers.cpp:36-38`).
    ///
    /// # Errors
    ///
    /// The bond's next-cash-flow date resolves the settlement date off the
    /// evaluation date, which must be set (D5/D10).
    pub fn new(
        price: Handle<dyn Quote>,
        bond: Bond,
        price_type: BondPriceType,
    ) -> QlResult<Shared<BondHelper>> {
        let latest = bond
            .cashflows()
            .last()
            .map_or_else(Date::null, |flow| flow.date());
        let earliest = bond.next_cash_flow_date(None)?.unwrap_or_else(Date::null);

        let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let engine: SharedMut<dyn PricingEngine> = shared_mut(DiscountingBondEngine::new(
            term_structure_handle.handle(),
            None,
            bond.settings_handle(),
        ));
        let mut bond = bond;
        bond.base_mut().set_pricing_engine(engine);

        let base = BootstrapHelperBase::new(price);
        base.set_earliest_date(earliest);
        base.set_latest_date(latest);

        Ok(shared(BondHelper {
            base,
            bond: RefCell::new(bond),
            term_structure_handle,
            price_type,
        }))
    }

    /// The price convention the helper fits.
    pub fn price_type(&self) -> BondPriceType {
        self.price_type
    }
}

impl AsObservable for BondHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for BondHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The bond price implied by the current curve (`bondhelpers.cpp:53-71`).
    ///
    /// The bond is force-recalculated first (the C++ `bond_->recalculate()`,
    /// forced because the helper does not observe the curve); then the clean or
    /// dirty price is returned per the helper's price type.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        let mut bond = self.bond.borrow_mut();
        bond.recalculate()?;
        match self.price_type {
            BondPriceType::Clean => bond.clean_price(),
            BondPriceType::Dirty => bond.dirty_price(),
        }
    }

    /// Weak-links the helper's own pricing handle to the bootstrapping curve,
    /// then records the curve on the base - both non-owning and unobserved (the
    /// `null_deleter`/`observer = false` of `bondhelpers.cpp:44-51`).
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

/// Fixed-coupon bond helper for curve bootstrap (`FixedRateBondHelper`).
///
/// The C++ `FixedRateBondHelper` subclasses [`BondHelper`], adding only a
/// constructor that builds a [`FixedRateBond`] internally and an
/// `accept`/visitor override the port drops. With no behaviour to carry, this is
/// a bare constructor returning a [`BondHelper`] over the built bond.
pub struct FixedRateBondHelper;

impl FixedRateBondHelper {
    /// Builds a [`FixedRateBond`] from the coupon schedule and wraps it in a
    /// [`BondHelper`] (`bondhelpers.cpp:81-99`).
    ///
    /// # Errors
    ///
    /// Propagates [`FixedRateBond::new`] and [`BondHelper::new`].
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        price: Handle<dyn Quote>,
        settlement_days: Natural,
        face_amount: Real,
        schedule: Schedule,
        coupons: Vec<Rate>,
        day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        redemption: Real,
        issue_date: Option<Date>,
        payment_calendar: Option<Calendar>,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        price_type: BondPriceType,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<BondHelper>> {
        let bond = FixedRateBond::new(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            day_counter,
            payment_convention,
            redemption,
            issue_date,
            payment_calendar,
            ex_coupon_period,
            ex_coupon_calendar,
            ex_coupon_convention,
            ex_coupon_end_of_month,
            None,
            settings,
        )?;
        BondHelper::new(price, bond.into_bond(), price_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::frequency::Frequency;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn flat_curve(reference: Date, rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    /// A five-year semiannual 5% bond issued a year before the evaluation date.
    fn bond_schedule() -> Schedule {
        Schedule::new(
            Date::new(15, Month::June, 2025),
            Date::new(15, Month::June, 2030),
            Period::try_from(Frequency::Semiannual).unwrap(),
            Target::new(),
            BusinessDayConvention::Following,
            BusinessDayConvention::Following,
            DateGeneration::Backward,
            false,
            Date::null(),
            Date::null(),
        )
    }

    fn a_helper(
        price: Handle<dyn Quote>,
        price_type: BondPriceType,
        settings: Shared<Settings<Date>>,
    ) -> Shared<BondHelper> {
        FixedRateBondHelper::new(
            price,
            3,
            100.0,
            bond_schedule(),
            vec![0.05],
            ActualActual::with_convention(Convention::ISDA),
            BusinessDayConvention::Following,
            100.0,
            Some(Date::new(15, Month::June, 2025)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            price_type,
            settings,
        )
        .unwrap()
    }

    /// An independently built copy of the same bond, priced over `curve` by its
    /// own engine, for a cross-implementation identity against the helper.
    fn independent_bond(
        curve: &Shared<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> FixedRateBond {
        let mut bond = FixedRateBond::new(
            3,
            100.0,
            bond_schedule(),
            vec![0.05],
            ActualActual::with_convention(Convention::ISDA),
            BusinessDayConvention::Following,
            100.0,
            Some(Date::new(15, Month::June, 2025)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            None,
            Shared::clone(&settings),
        )
        .unwrap();
        let handle = Handle::new(Shared::clone(curve));
        let engine = shared_mut(DiscountingBondEngine::new(handle, None, settings))
            as SharedMut<dyn PricingEngine>;
        bond.bond_mut().base_mut().set_pricing_engine(engine);
        bond
    }

    /// The helper's implied quote is the clean price the same bond fetches from
    /// an independent engine over the same curve - the helper prices its own
    /// bond off the curve it is bootstrapped against.
    #[test]
    fn implied_quote_matches_the_independent_clean_price() {
        let settings = settings_on(today());
        let helper = a_helper(
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            BondPriceType::Clean,
            Shared::clone(&settings),
        );
        let curve = flat_curve(today(), 0.04);
        helper.set_term_structure(&curve);

        let implied = helper.implied_quote().unwrap();
        let mut independent = independent_bond(&curve, settings);
        let clean = independent.bond_mut().clean_price().unwrap();
        assert!(
            (implied - clean).abs() < 1e-10,
            "implied {implied} vs clean {clean}"
        );
    }

    /// The dirty price type folds in the accrued interest, so it exceeds the
    /// clean quote by exactly the bond's accrued amount at settlement.
    #[test]
    fn dirty_price_type_adds_the_accrued_amount() {
        let settings = settings_on(today());
        let curve = flat_curve(today(), 0.04);

        let clean_helper = a_helper(
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            BondPriceType::Clean,
            Shared::clone(&settings),
        );
        clean_helper.set_term_structure(&curve);
        let clean = clean_helper.implied_quote().unwrap();

        let dirty_helper = a_helper(
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            BondPriceType::Dirty,
            Shared::clone(&settings),
        );
        dirty_helper.set_term_structure(&curve);
        let dirty = dirty_helper.implied_quote().unwrap();

        let independent = independent_bond(&curve, settings);
        let settlement = independent.bond().settlement_date(None).unwrap();
        let accrued = independent.bond().accrued_amount(Some(settlement)).unwrap();
        assert!(accrued > 0.0, "the bond is mid-coupon and accruing");
        assert!(
            (dirty - (clean + accrued)).abs() < 1e-10,
            "dirty {dirty} vs clean+accrued {}",
            clean + accrued
        );
    }

    /// Moving the curve (mutating its quote, not relinking) changes the implied
    /// quote though the helper never observes the curve and gets no
    /// notification - proving `implied_quote` forces a fresh calculation rather
    /// than reading the bond's stale settlement value.
    #[test]
    fn moving_the_curve_updates_the_quote_without_notifying_the_helper() {
        let settings = settings_on(today());
        let helper = a_helper(
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            BondPriceType::Clean,
            settings,
        );

        let quote = shared(SimpleQuote::new(0.03));
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
            today(),
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        helper.set_term_structure(&curve);
        let before = helper.implied_quote().unwrap();

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        quote.set_value(0.06);
        assert!(
            !Flag::is_up(&flag),
            "the helper must not observe the bootstrapping curve"
        );

        let after = helper.implied_quote().unwrap();
        assert!(
            (after - before).abs() > 1e-6,
            "the forced recalculation must surface the curve move without a notification"
        );
    }

    /// The helper's dates come from the bond: the earliest is the next
    /// cash-flow date, the latest the last cash-flow date (later than the
    /// 2030-06-15 maturity because that Saturday redemption rolls to Monday
    /// under `Following`, the `bondhelpers.cpp:36` adjustment), and (with no
    /// pillar set) the pillar falls back to the latest.
    #[test]
    fn dates_follow_the_bond_cashflows() {
        let settings = settings_on(today());
        let helper = a_helper(
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            BondPriceType::Clean,
            settings,
        );

        assert_eq!(
            helper.latest_date(),
            Date::new(17, Month::June, 2030),
            "the last cash flow is the redemption rolled off the 2030-06-15 maturity"
        );
        assert!(
            helper.latest_date() > Date::new(15, Month::June, 2030),
            "the adjusted last-payment date is later than the maturity date"
        );
        assert!(
            helper.earliest_date() > today(),
            "the next cash flow after settlement is in the future"
        );
        assert_eq!(
            helper.pillar_date(),
            helper.latest_date(),
            "with no pillar set, the pillar falls back to the latest date"
        );
    }
}
