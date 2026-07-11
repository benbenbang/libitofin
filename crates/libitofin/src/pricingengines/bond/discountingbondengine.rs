//! Discounting bond pricing engine.
//!
//! Port of `ql/pricingengines/bond/discountingbondengine.{hpp,cpp}`:
//! [`DiscountingBondEngine`] discounts a bond's cash flows over a
//! [`YieldTermStructure`] to fill the [`BondResults`] the base
//! [`Bond`](crate::instruments::Bond) reads its settlement value and prices
//! from. Changes to the discount-curve handle invalidate the attached bond
//! through the usual observable chain.
//!
//! Deviations, documented per D5/D10:
//! - The C++ global `Settings::instance()` becomes an explicit
//!   [`Settings`] handle the engine is built with, mirroring how the base
//!   bond carries its settings; it drives the `includeReferenceDateEvents`
//!   fall back for the reference-date flow decision.

use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::instruments::{BondArguments, BondEngine, BondResults};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::{fail, handle::Handle, require};

/// Discounting engine for bonds.
///
/// Discounts the bond's future cash flows to the discount curve's reference
/// date for the value, and to the settlement date for the settlement value.
pub struct DiscountingBondEngine {
    base: BondEngine,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    settings: Shared<Settings<Date>>,
}

impl DiscountingBondEngine {
    /// Builds the engine over a discount-curve handle it registers with.
    ///
    /// `include_settlement_date_flows` overrides, when set, the settings'
    /// `include_reference_date_events` flag for the reference-date flow
    /// decision (the C++ `includeSettlementDateFlows` optional).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settings: Shared<Settings<Date>>,
    ) -> DiscountingBondEngine {
        let base = BondEngine::new(
            BondArguments {
                settlement_date: None,
                cashflows: Vec::new(),
                calendar: NullCalendar::new(),
            },
            BondResults::default(),
        );
        discount_curve.register_observer(&base.observer());
        DiscountingBondEngine {
            base,
            discount_curve,
            include_settlement_date_flows,
            settings,
        }
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for DiscountingBondEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for DiscountingBondEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        require!(
            !self.discount_curve.is_empty(),
            "discounting term structure handle is empty"
        );
        let curve = self.discount_curve.current_link()?;
        let valuation_date = curve.reference_date()?;

        let include_ref_date_flows = self
            .include_settlement_date_flows
            .unwrap_or_else(|| self.settings.include_reference_date_events());

        let Some(settlement_date) = self.base.arguments().settlement_date else {
            fail!("no settlement date provided");
        };

        let value = CashFlows::npv(
            &self.base.arguments().cashflows,
            &*curve,
            &self.settings,
            Some(include_ref_date_flows),
            Some(valuation_date),
            Some(valuation_date),
        )?;

        let settlement_value = if !include_ref_date_flows && valuation_date == settlement_date {
            value
        } else {
            CashFlows::npv(
                &self.base.arguments().cashflows,
                &*curve,
                &self.settings,
                Some(false),
                Some(settlement_date),
                Some(settlement_date),
            )?
        };

        let results = self.base.results_mut();
        results.instrument.valuation_date = Some(valuation_date);
        results.instrument.value = Some(value);
        results.settlement_value = Some(settlement_value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::Instrument;
    use crate::instruments::FixedRateBond;
    use crate::interestrate::Compounding;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    fn today() -> Date {
        Date::new(22, Month::November, 2004)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// `bonds.cpp` testCachedFixed, bond1 (`:832`): a fixed-coupon government
    /// bond priced end to end over a flat 3% curve reproduces the cached clean
    /// price 99.298100 to 1e-6. This is the first bond priced against a real
    /// C++ number, closing the vertical slice.
    #[test]
    fn cached_fixed_bond1_reproduces_the_c_clean_price() {
        let settings = settings_today();

        let discount_curve: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::with_rate(
                today(),
                0.03,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);

        let schedule = MakeSchedule::new()
            .from(Date::new(30, Month::November, 2004))
            .to(Date::new(30, Month::November, 2008))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();

        let mut bond = FixedRateBond::new(
            1,
            1_000_000.0,
            schedule,
            vec![0.02875],
            ActualActual::with_convention(Convention::ISMA),
            BusinessDayConvention::ModifiedFollowing,
            100.0,
            Some(Date::new(30, Month::November, 2004)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            None,
            Shared::clone(&settings),
        )
        .unwrap();

        let engine = shared_mut(DiscountingBondEngine::new(
            discount_curve,
            None,
            Shared::clone(&settings),
        ));
        bond.bond_mut()
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

        let price = bond.bond_mut().clean_price().unwrap();
        assert!(
            (price - 99.298100).abs() <= 1.0e-6,
            "clean price {price} vs cached 99.298100 (error {})",
            (price - 99.298100).abs()
        );
    }

    /// An empty discount-curve handle is rejected before any discounting, as
    /// the C++ `QL_REQUIRE(!discountCurve_.empty(), ...)`.
    #[test]
    fn an_empty_discount_curve_is_rejected() {
        let mut engine = DiscountingBondEngine::new(Handle::empty(), None, settings_today());
        assert_eq!(
            engine.calculate().unwrap_err().message(),
            "discounting term structure handle is empty"
        );
    }
}
