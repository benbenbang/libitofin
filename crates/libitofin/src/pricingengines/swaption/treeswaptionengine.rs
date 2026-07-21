//! Numerical lattice engine for swaptions.
//!
//! Port of `ql/pricingengines/swaption/treeswaptionengine.{hpp,cpp}` together
//! with the folded-in `ql/pricingengines/latticeshortratemodelengine.hpp`.
//! [`TreeSwaptionEngine`] prices a [`Swaption`](crate::instruments::Swaption)
//! under [`HullWhite`] on a short-rate tree: it builds a [`DiscretizedSwaption`],
//! fits a [`TimeGrid`] over its mandatory times, grows the model tree on that
//! grid, then rolls the swaption back to the first exercise and reads its present
//! value (`treeswaptionengine.cpp:51`).
//!
//! # Model binding (documented deferral)
//! C++'s engine is `LatticeShortRateModelEngine<Swaption::arguments,
//! Swaption::results>`, generic over any `ShortRateModel`. As the
//! [`JamshidianSwaptionEngine`](super::JamshidianSwaptionEngine) precedent does,
//! the port binds concretely to [`HullWhite`] - the only ported model that grows
//! a tree - so `model.tree(grid)` is a direct call, not a virtual dispatch.
//!
//! # `LatticeShortRateModelEngine` folded in, not built
//! The generic C++ base stores `time_steps` / `time_grid` / `lattice` and has an
//! `update()` that rebuilds the lattice from a fixed grid. With a single concrete
//! consumer, no generic base is built: `time_steps` is a field here and the tree
//! is grown per `calculate()`. The fixed-`TimeGrid`-at-construction variant (and
//! its `update()` rebuild), the `Handle<ShortRateModel>` ctor and the engine-level
//! `termStructure_` fallback for a non-term-structure-consistent model
//! (`treeswaptionengine.cpp:63-70`, the `else` branch) are DEFERRED: Hull-White is
//! always term-structure consistent, so only the `tsmodel` branch is live.
//!
//! # Divergences from QuantLib
//! - **Settings on the ctor (D5).** [`DiscretizedSwap`](super::DiscretizedSwap)
//!   reads `includeTodaysCashFlows` from what C++ takes off the `Settings`
//!   singleton. With no singleton the engine holds a [`Settings`] handle and
//!   threads it into the [`DiscretizedSwaption`], exactly as #464 threads it.
//! - **`Result`, not UB, on a degenerate exercise.** C++ dereferences
//!   `stoppingTimes.back()` and `find_if(>= 0)` unchecked; the port returns
//!   [`QlResult`] errors for an empty or all-negative exercise schedule.

use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::instrument::InstrumentResults;
use crate::instruments::{SettlementMethod, SwaptionArguments, SwaptionEngine};
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::lattice::Lattice;
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared};
use crate::time::date::Date;
use crate::types::{Size, Time};
use crate::{fail, require};

use super::DiscretizedSwaption;

/// Numerical lattice engine for swaptions under [`HullWhite`]
/// (`treeswaptionengine.hpp:44`).
pub struct TreeSwaptionEngine {
    base: SwaptionEngine,
    model: SharedMut<HullWhite>,
    time_steps: Size,
    settings: Shared<Settings<Date>>,
}

impl TreeSwaptionEngine {
    /// Builds the engine over a Hull-White `model` with a fixed step count
    /// (`treeswaptionengine.cpp:26` + `latticeshortratemodelengine.hpp:56`). The
    /// engine observes the model, so a model change invalidates a swaption priced
    /// by it.
    ///
    /// # Errors
    /// `time_steps` must be positive (`latticeshortratemodelengine.hpp:60`).
    pub fn new(
        model: SharedMut<HullWhite>,
        time_steps: Size,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<TreeSwaptionEngine> {
        require!(
            time_steps > 0,
            "timeSteps must be positive, {time_steps} not allowed"
        );
        let base = SwaptionEngine::new(SwaptionArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(TreeSwaptionEngine {
            base,
            model,
            time_steps,
            settings,
        })
    }
}

impl AsObservable for TreeSwaptionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for TreeSwaptionEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `calculate()` (`treeswaptionengine.cpp:51`): guard the settlement method,
    /// take the reference date / day counter from the model's term structure,
    /// build the [`DiscretizedSwaption`], grow the tree over its mandatory times,
    /// then roll back to the first exercise and read the present value.
    fn calculate(&mut self) -> QlResult<()> {
        require!(
            self.base.arguments().settlement_method != SettlementMethod::ParYieldCurve,
            "cash settled (ParYieldCurve) swaptions not priced with TreeSwaptionEngine"
        );

        let model = self.model.borrow();
        let (reference_date, day_counter) = {
            let curve = model.term_structure().current_link()?;
            (curve.reference_date()?, curve.require_day_counter()?)
        };

        let (mut swaption, stopping_times) = {
            let args = self.base.arguments();
            let Some(exercise) = args.exercise.as_ref() else {
                fail!("exercise not set");
            };
            let stopping_times: Vec<Time> = exercise
                .dates()
                .iter()
                .map(|&date| day_counter.year_fraction(reference_date, date))
                .collect();
            let swaption =
                DiscretizedSwaption::new(args, reference_date, &day_counter, &self.settings)?;
            (swaption, stopping_times)
        };

        let times = swaption.mandatory_times();
        let grid = TimeGrid::with_mandatory_times(&times, self.time_steps)?;
        let lattice: Shared<dyn Lattice> = shared(model.tree(grid)?);
        drop(model);

        let Some(&last) = stopping_times.last() else {
            fail!("swaption has no exercise dates");
        };
        swaption.initialize(Shared::clone(&lattice), last)?;

        let Some(next_exercise) = stopping_times.iter().copied().find(|&t| t >= 0.0) else {
            fail!("swaption has no non-negative exercise time");
        };
        swaption.rollback(next_exercise)?;

        self.base.results_mut().value = Some(swaption.present_value()?);
        Ok(())
    }
}
