//! Discounting swap pricing engine.
//!
//! Port of `ql/pricingengines/swap/discountingswapengine.{hpp,cpp}`:
//! [`DiscountingSwapEngine`] discounts each of a swap's legs over a
//! [`YieldTermStructure`] into the [`SwapResults`] the base
//! [`Swap`](crate::instruments::Swap) reads its leg NPVs, leg BPS and framing
//! discount factors from. It is a `Swap::engine` (`discountingswapengine.hpp:39`),
//! so it prices any swap, and reuses the combined [`CashFlows::npvbps`] pass
//! (#250) per leg. Changes to the discount-curve handle invalidate the attached
//! swap through the usual observable chain.
//!
//! Deviations, documented per D5/D10:
//! - The C++ global `Settings::instance()` becomes an explicit [`Settings`]
//!   handle the engine is built with; it drives the `includeReferenceDateEvents`
//!   fall back for the reference-date flow decision when the optional
//!   `include_settlement_date_flows` ctor argument is unset.
//! - The C++ `Date()` "unset" sentinel for the settlement and NPV dates becomes
//!   [`Option<Date>`]: `None` falls back to the curve reference date.
//! - A leg date before the curve reference date has no discount; the C++
//!   `Null<DiscountFactor>()` sentinel becomes [`Real::null`].
//! - A leg-level pricing failure is prefixed with the leg's index rather than
//!   the C++ ordinal (`io::ordinal`, not ported).

use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::instruments::{SwapArguments, SwapEngine, SwapResults};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::DiscountFactor;
use crate::utilities::null::Null;
use crate::{fail, handle::Handle, require};

/// Discounting engine for swaps.
///
/// Discounts each leg's future cash flows to the discount curve's reference
/// date, filling the per-leg NPV and BPS the base swap prices from.
pub struct DiscountingSwapEngine {
    base: SwapEngine,
    discount_curve: Handle<dyn YieldTermStructure>,
    include_settlement_date_flows: Option<bool>,
    settlement_date: Option<Date>,
    npv_date: Option<Date>,
    settings: Shared<Settings<Date>>,
}

impl DiscountingSwapEngine {
    /// Builds the engine over a discount-curve handle it registers with.
    ///
    /// `include_settlement_date_flows` overrides, when set, the settings'
    /// `include_reference_date_events` flag for the reference-date flow
    /// decision (the C++ `includeSettlementDateFlows` optional). `settlement_date`
    /// and `npv_date` default, when `None`, to the curve reference date (the C++
    /// `Date()` sentinel).
    pub fn new(
        discount_curve: Handle<dyn YieldTermStructure>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
        settings: Shared<Settings<Date>>,
    ) -> DiscountingSwapEngine {
        let base = SwapEngine::new(SwapArguments::default(), SwapResults::default());
        discount_curve.register_observer(&base.observer());
        DiscountingSwapEngine {
            base,
            discount_curve,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
            settings,
        }
    }

    /// The discount-curve handle the engine prices over.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
}

impl AsObservable for DiscountingSwapEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for DiscountingSwapEngine {
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
        let ref_date = curve.reference_date()?;

        let settlement_date = match self.settlement_date {
            None => ref_date,
            Some(date) => {
                require!(
                    date >= ref_date,
                    "settlement date ({date}) before discount curve reference date ({ref_date})"
                );
                date
            }
        };

        let valuation_date = match self.npv_date {
            None => ref_date,
            Some(date) => {
                require!(
                    date >= ref_date,
                    "npv date ({date}) before discount curve reference date ({ref_date})"
                );
                date
            }
        };
        let npv_date_discount = curve.discount_date(valuation_date, false)?;

        let include_ref_date_flows = self
            .include_settlement_date_flows
            .unwrap_or_else(|| self.settings.include_reference_date_events());

        let arguments = self.base.arguments();
        let n = arguments.legs.len();
        let mut leg_npv = Vec::with_capacity(n);
        let mut leg_bps = Vec::with_capacity(n);
        let mut start_discounts = Vec::with_capacity(n);
        let mut end_discounts = Vec::with_capacity(n);
        let mut value = 0.0;

        for (i, leg) in arguments.legs.iter().enumerate() {
            let (npv, bps) = match CashFlows::npvbps(
                leg,
                &*curve,
                &self.settings,
                Some(include_ref_date_flows),
                Some(settlement_date),
                Some(valuation_date),
            ) {
                Ok(pair) => pair,
                Err(e) => fail!("leg #{}: {}", i + 1, e.message()),
            };
            let npv = npv * arguments.payer[i];
            let bps = bps * arguments.payer[i];

            if leg.is_empty() {
                start_discounts.push(DiscountFactor::null());
                end_discounts.push(DiscountFactor::null());
            } else {
                let d1 = CashFlows::start_date(leg)?;
                start_discounts.push(if d1 >= ref_date {
                    curve.discount_date(d1, false)?
                } else {
                    DiscountFactor::null()
                });
                let d2 = CashFlows::maturity_date(leg)?;
                end_discounts.push(if d2 >= ref_date {
                    curve.discount_date(d2, false)?
                } else {
                    DiscountFactor::null()
                });
            }

            leg_npv.push(npv);
            leg_bps.push(bps);
            value += npv;
        }

        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.instrument.error_estimate = None;
        results.instrument.valuation_date = Some(valuation_date);
        results.npv_date_discount = Some(npv_date_discount);
        results.leg_npv = leg_npv;
        results.leg_bps = leg_bps;
        results.start_discounts = start_discounts;
        results.end_discounts = end_discounts;
        Ok(())
    }
}
