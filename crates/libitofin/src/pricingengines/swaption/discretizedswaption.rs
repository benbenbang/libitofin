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
//! # Date snapping
//! Coupon schedule dates within seven days of an exercise date are collapsed
//! onto it before rebuilding the vanilla swap. Dates moved forward use the
//! post-adjustment pass. The final schedule dates remain unchanged.
//! Flattened arguments without their source swap and non-vanilla underlyings
//! are rejected: they cannot reproduce the source's coupon reconstruction.

use crate::discretizedasset::{
    CouponAdjustment, DiscretizedAsset, DiscretizedAssetBase, DiscretizedOption,
};
use crate::errors::QlResult;
use crate::indexes::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{FixedVsFloatingSwapArguments, SwaptionArguments, VanillaSwap};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::schedule::Schedule;
use crate::types::{Size, Time};
use crate::{fail, require};

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
    /// (`discretizedswaption.cpp:36`), rebuilding the underlying vanilla swap
    /// after collapsing nearby schedule dates onto exercise dates.
    ///
    /// # Errors
    /// Fails for missing exercise/source swap, non-vanilla underlyings, invalid
    /// snapped schedules, or an error rebuilding/discretizing the coupons.
    pub fn new(
        args: &SwaptionArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        let Some(exercise) = args.exercise.as_ref() else {
            fail!("exercise not set");
        };
        let prepared = prepare_swaption_with_snapped_dates(args)?;
        let swap_args = &prepared.arguments;

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

        let swap = DiscretizedSwap::with_adjustments(
            swap_args,
            reference_date,
            day_counter,
            prepared.fixed_adjustments,
            prepared.floating_adjustments,
            settings,
        )?;
        let underlying: SharedMut<dyn DiscretizedAsset> = shared_mut(swap);
        let option = DiscretizedOption::new(underlying, exercise_type, exercise_times);

        Ok(DiscretizedSwaption {
            option,
            last_payment,
        })
    }
}

struct PreparedSwaption {
    arguments: FixedVsFloatingSwapArguments,
    fixed_adjustments: Vec<CouponAdjustment>,
    floating_adjustments: Vec<CouponAdjustment>,
}

fn snap_dates(dates: &mut [Date], exercise_dates: &[Date]) -> QlResult<Vec<CouponAdjustment>> {
    require!(
        dates.len() >= 2,
        "swap schedule requires at least two dates"
    );
    let mut adjustments = vec![CouponAdjustment::Pre; dates.len() - 1];
    for &exercise_date in exercise_dates {
        for (date, adjustment) in dates.iter_mut().zip(&mut adjustments) {
            let distance = exercise_date - *date;
            if distance != 0 && distance.abs() <= 7 {
                *date = exercise_date;
                if distance > 0 {
                    *adjustment = CouponAdjustment::Post;
                }
            }
        }
    }
    require!(
        dates.windows(2).all(|pair| pair[0] < pair[1]),
        "snapped swap schedule must remain strictly increasing"
    );
    Ok(adjustments)
}

fn prepare_swaption_with_snapped_dates(args: &SwaptionArguments) -> QlResult<PreparedSwaption> {
    let Some(source) = args.swap.as_ref() else {
        fail!("source swap not set");
    };
    let source = source.borrow();
    require!(
        source.is_vanilla,
        "date snapping requires a vanilla Ibor swap"
    );
    let Some(exercise) = args.exercise.as_ref() else {
        fail!("exercise not set");
    };
    require!(!exercise.dates().is_empty(), "no exercise date given");
    require!(
        exercise.dates().iter().all(|d| *d != Date::null()),
        "null exercise date"
    );
    let mut fixed_dates = source.fixed_schedule().dates().to_vec();
    let mut floating_dates = source.floating_schedule().dates().to_vec();
    let fixed_adjustments = snap_dates(&mut fixed_dates, exercise.dates())?;
    let floating_adjustments = snap_dates(&mut floating_dates, exercise.dates())?;
    let index = source.ibor_index();
    let rebuilt = VanillaSwap::new(
        source.swap_type(),
        source.nominal()?,
        Schedule::from_dates(fixed_dates),
        source.fixed_rate(),
        source.fixed_day_count().clone(),
        Schedule::from_dates(floating_dates),
        Shared::clone(index),
        source.spread(),
        source.floating_day_count().clone(),
        Some(source.payment_convention()),
        Shared::clone(index.base().settings()),
    )?;
    let mut arguments = FixedVsFloatingSwapArguments::default();
    rebuilt.setup_arguments(&mut arguments)?;
    Ok(PreparedSwaption {
        arguments,
        fixed_adjustments,
        floating_adjustments,
    })
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

#[cfg(test)]
#[path = "discretizedswaption_tests.rs"]
mod tests;
