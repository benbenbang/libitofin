//! Hull-White lattice valuation for callable fixed-rate bonds.
//!
//! Port of `ql/experimental/callablebonds/treecallablebondengine.{hpp,cpp}`,
//! concretely bound to the parent's existing Hull-White tree, as its swaption
//! tree engine is. General short-rate model dispatch and OAS are outside this
//! engine's contract.

use super::discretizedcallablebond::DiscretizedCallableFixedRateBond;
use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::instruments::{BondResults, CallableBondArguments};
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::lattice::Lattice;
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::HullWhite;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::time::date::Date;
use crate::types::Size;
use crate::{fail, require};

type CallableBondEngineBase = GenericEngine<CallableBondArguments, BondResults>;

/// A callable/puttable fixed-rate bond engine on a fitted Hull-White tree.
///
/// Rebuilds the event-aligned lattice on each calculation. Observes both model
/// and evaluation-date changes so input updates invalidate attached instruments.
pub struct TreeCallableFixedRateBondEngine {
    base: CallableBondEngineBase,
    model: SharedMut<HullWhite>,
    time_steps: Size,
    _settings: Shared<Settings<Date>>,
}

impl TreeCallableFixedRateBondEngine {
    /// Constructs an engine over a Hull-White model and positive step count.
    ///
    /// # Errors
    ///
    /// Rejects a zero step count.
    pub fn new(
        model: SharedMut<HullWhite>,
        time_steps: Size,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(time_steps > 0, "timeSteps must be positive");
        let base =
            CallableBondEngineBase::new(CallableBondArguments::default(), BondResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        settings.register_eval_date_observer(&base.observer());
        Ok(Self {
            base,
            model,
            time_steps,
            _settings: settings,
        })
    }
}

impl AsObservable for TreeCallableFixedRateBondEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for TreeCallableFixedRateBondEngine {
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
        self.base.reset();
        self.base.arguments().validate()?;
        let params = self.model.borrow().calibrated_model().params();
        require!(
            params.iter().all(|value| value.is_finite()),
            "nonfinite Hull-White parameters"
        );
        let curve = self.model.borrow().term_structure().current_link()?;
        let reference_date = curve.reference_date()?;
        let day_counter = curve.require_day_counter()?;
        let Some(settlement_date) = self.base.arguments().settlement_date else {
            fail!("null settlement date");
        };
        let Some(redemption_date) = self.base.arguments().redemption_date else {
            fail!("null redemption date");
        };
        require!(
            settlement_date >= reference_date,
            "settlement before curve reference date"
        );
        require!(
            redemption_date > reference_date,
            "redemption must follow curve reference date"
        );
        let mut bond = DiscretizedCallableFixedRateBond::new(self.base.arguments(), &*curve)?;
        let grid = TimeGrid::with_mandatory_times(&bond.mandatory_times(), self.time_steps)?;
        for time in grid.times() {
            let discount = curve.discount(*time, false)?;
            require!(
                discount.is_finite() && discount > 0.0,
                "invalid callable bond curve discount"
            );
        }
        let lattice: Shared<dyn Lattice> = shared(self.model.borrow().tree(grid)?);
        let redemption_time = day_counter.year_fraction(reference_date, redemption_date);
        bond.initialize(Shared::clone(&lattice), redemption_time)?;
        bond.rollback(0.0)?;
        let value = bond.present_value()?;
        let discount = curve.discount_date(settlement_date, false)?;
        require!(
            value.is_finite() && discount.is_finite() && discount > 0.0,
            "invalid callable bond value or settlement discount"
        );
        let settlement_value = value / discount;
        require!(settlement_value.is_finite(), "settlement value overflow");
        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.instrument.valuation_date = Some(reference_date);
        results.settlement_value = Some(settlement_value);
        Ok(())
    }
}
