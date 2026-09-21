//! Hull-White lattice valuation of caps, floors and collars.

use super::DiscretizedCapFloor;
use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::instrument::InstrumentResults;
use crate::instruments::CapFloorArguments;
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::lattice::Lattice;
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::shared::{Shared, SharedMut, shared};
use crate::types::Size;
use crate::{fail, require};

/// Prices caplets and floorlets as bond options on a Hull-White tree.
///
/// The model supplies the reference curve; non-Hull-White models are not exposed.
/// Trees are rebuilt on calculation so model and curve updates take effect.
/// Coupon starts before the model reference date are rejected, matching the
/// upstream engine's non-negative grid requirement. Known historical coupons
/// can instead be represented directly with [`DiscretizedCapFloor`].
pub struct TreeCapFloorEngine {
    base: GenericEngine<CapFloorArguments, InstrumentResults>,
    model: SharedMut<HullWhite>,
    time_steps: Size,
    time_grid: Option<TimeGrid>,
}

impl TreeCapFloorEngine {
    /// Constructs a tree using coupon dates and a positive target step count.
    ///
    /// # Errors
    /// Rejects a zero step count.
    pub fn new(model: SharedMut<HullWhite>, time_steps: Size) -> QlResult<Self> {
        require!(time_steps > 0, "timeSteps must be positive");
        Ok(Self::build(model, time_steps, None))
    }

    /// Constructs an engine on a fixed grid containing every live coupon date.
    ///
    /// # Errors
    /// Rejects an empty, non-finite, non-increasing or zero-horizon grid.
    pub fn with_time_grid(model: SharedMut<HullWhite>, grid: TimeGrid) -> QlResult<Self> {
        require!(
            grid.size() >= 2 && grid.times().iter().all(|t| t.is_finite()),
            "invalid cap/floor time grid"
        );
        require!(
            grid.front().is_some_and(|time| time >= 0.0)
                && grid.back().is_some_and(|time| time > 0.0),
            "invalid cap/floor time grid bounds"
        );
        require!(
            grid.times().windows(2).all(|p| p[1] > p[0]),
            "invalid cap/floor time grid"
        );
        Ok(Self::build(model, 0, Some(grid)))
    }

    fn build(model: SharedMut<HullWhite>, time_steps: Size, time_grid: Option<TimeGrid>) -> Self {
        let base = GenericEngine::new(CapFloorArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Self {
            base,
            model,
            time_steps,
            time_grid,
        }
    }
}

impl AsObservable for TreeCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for TreeCapFloorEngine {
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
        let model = self.model.borrow();
        let curve = model.term_structure().current_link()?;
        let reference = curve.reference_date()?;
        let dc = curve.require_day_counter()?;
        let args = self.base.arguments();
        let mut capfloor = DiscretizedCapFloor::new(args, reference, &dc)?;
        let (Some(&start), Some(&end)) = (args.start_dates.first(), args.end_dates.last()) else {
            fail!("cap/floor has no coupons");
        };
        let first = dc.year_fraction(reference, start);
        let last = dc.year_fraction(reference, end);
        require!(
            first >= 0.0 && last > 0.0,
            "cap/floor tree requires non-negative start and positive end times"
        );
        let times = capfloor.mandatory_times();
        let grid = if let Some(grid) = &self.time_grid {
            for &time in &times {
                grid.index(time)?;
            }
            grid.clone()
        } else {
            TimeGrid::with_mandatory_times(&times, self.time_steps)?
        };
        let lattice: Shared<dyn Lattice> = shared(model.tree(grid)?);
        drop(model);
        capfloor.initialize(lattice, last)?;
        capfloor.rollback(first)?;
        let value = capfloor.present_value()?;
        require!(value.is_finite(), "non-finite cap/floor lattice result");
        self.base.results_mut().value = Some(value);
        Ok(())
    }
}

#[cfg(test)]
#[path = "treecapfloorengine_tests.rs"]
mod tests;
