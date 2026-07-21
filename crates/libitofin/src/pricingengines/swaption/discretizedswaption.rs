//! Swaption priced on a lattice.
//!
//! Port of `ql/pricingengines/swaption/discretizedswaption.{hpp,cpp}`:
//! [`DiscretizedSwaption`] is a [`DiscretizedOption`] whose underlying is a
//! [`DiscretizedSwap`](super::DiscretizedSwap). On each exercise node the option
//! condition takes `max(continuation, exercise)`, where the exercise value is the
//! underlying swap rolled to that node.
//!
//! # Composition, not inheritance (the single load-bearing decision)
//! C++'s `DiscretizedSwaption` derives from `DiscretizedOption`, overriding only
//! [`reset`](DiscretizedSwaption::reset). Rust has no subclassing, so the type
//! EMBEDS a [`DiscretizedOption`] and forwards [`base`](DiscretizedAsset::base) /
//! [`base_mut`](DiscretizedAsset::base_mut) to it - the swaption owns NO
//! [`DiscretizedAssetBase`] of its own. This is essential: the lattice mutates
//! state (time, values, method) through the swaption trait object, and the
//! delegated [`post_adjust_values_impl`](DiscretizedAsset::post_adjust_values_impl)
//! reads that same state off `self.option`. Two separate base storages would make
//! the exercise pass read zeros and silently misprice (the
//! `rust-composition-loses-virtual-dispatch` trap). Every other adjustment method
//! forwards to the embedded option unchanged, mirroring C++'s single virtual
//! subobject.
//!
//! # Deferred: date snapping (straight-through ctor)
//! C++'s ctor always calls `prepareSwaptionWithSnappedDates`
//! (`discretizedswaption.cpp:39`), which collapses coupon dates that fall within a
//! week of an exercise date onto that date (flipping the collapsed coupons to the
//! post-adjustment pass) before building the underlying swap. That is DEFERRED:
//! this ctor builds `exercise_times`, `last_payment` and the [`DiscretizedSwap`]
//! straight from the raw (unsnapped) [`SwaptionArguments`], i.e. the all-`Pre`
//! coupon path of [`DiscretizedSwap::new`](super::DiscretizedSwap::new). The
//! snapping loop never touches the last schedule date
//! (`discretizedswaption.cpp:88`, `j < size - 1`), so `last_payment` is identical
//! either way; only a coupon reset landing within a week of an exercise is priced
//! at its own node rather than snapped onto the exercise node. The European
//! convergence oracle does not need snapping; the cached Bermudan does (see the
//! oracle module).

use crate::discretizedasset::{DiscretizedAsset, DiscretizedAssetBase, DiscretizedOption};
use crate::errors::QlResult;
use crate::fail;
use crate::instruments::SwaptionArguments;
use crate::settings::Settings;
use crate::shared::{SharedMut, shared_mut};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Size, Time};

use super::DiscretizedSwap;

/// A swaption discretized on a [`Lattice`](crate::methods::lattices::lattice::Lattice)
/// (`discretizedswaption.hpp:34`).
///
/// Built from a [`SwaptionArguments`] with a reference date and day counter; the
/// exercise dates become year-fraction times and the underlying swap is a
/// [`DiscretizedSwap`](super::DiscretizedSwap).
pub struct DiscretizedSwaption {
    option: DiscretizedOption,
    last_payment: Time,
}

impl DiscretizedSwaption {
    /// `DiscretizedSwaption(args, referenceDate, dayCounter)`
    /// (`discretizedswaption.cpp:36`), built straight from the raw arguments (see
    /// the module docs on the deferred snapping).
    ///
    /// # Errors
    /// Fails if the arguments carry no exercise, no fixed coupons or no floating
    /// coupons, or if [`DiscretizedSwap::new`](super::DiscretizedSwap::new) fails.
    pub fn new(
        args: &SwaptionArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        let Some(exercise) = args.exercise.as_ref() else {
            fail!("exercise not set");
        };
        let swap_args = &args.swap_arguments;

        let exercise_times: Vec<Time> = exercise
            .dates()
            .iter()
            .map(|&date| day_counter.year_fraction(reference_date, date))
            .collect();
        let exercise_type = exercise.exercise_type();

        let Some(&last_fixed_date) = swap_args.fixed_pay_dates.last() else {
            fail!("swap has no fixed coupons");
        };
        let Some(&last_floating_date) = swap_args.floating_pay_dates.last() else {
            fail!("swap has no floating coupons");
        };
        let last_fixed = day_counter.year_fraction(reference_date, last_fixed_date);
        let last_floating = day_counter.year_fraction(reference_date, last_floating_date);
        let last_payment = last_fixed.max(last_floating);

        let swap = DiscretizedSwap::new(swap_args, reference_date, day_counter, settings)?;
        let underlying: SharedMut<dyn DiscretizedAsset> = shared_mut(swap);
        let option = DiscretizedOption::new(underlying, exercise_type, exercise_times);

        Ok(DiscretizedSwaption {
            option,
            last_payment,
        })
    }
}

impl DiscretizedAsset for DiscretizedSwaption {
    fn base(&self) -> &DiscretizedAssetBase {
        self.option.base()
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        self.option.base_mut()
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    /// `reset(size)` (`discretizedswaption.cpp:73`): initialize the underlying swap
    /// at `last_payment` FIRST, then run the [`DiscretizedOption`] reset (which
    /// checks option and underlying share a method, zeros the values and adjusts).
    fn reset(&mut self, size: Size) -> QlResult<()> {
        let method = self.require_method()?;
        let underlying = SharedMut::clone(self.option.underlying());
        underlying
            .borrow_mut()
            .initialize(method, self.last_payment)?;
        self.option.reset(size)
    }

    /// The embedded option's times (its underlying's plus the exercise times).
    fn mandatory_times(&self) -> Vec<Time> {
        self.option.mandatory_times()
    }

    /// Forwarded to the embedded option (the C++ non-overridden virtual).
    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        self.option.pre_adjust_values_impl()
    }

    /// The exercise machinery, forwarded to the embedded option. It reads the base
    /// state the lattice mutated through this swaption (single-storage base).
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        self.option.post_adjust_values_impl()
    }
}
