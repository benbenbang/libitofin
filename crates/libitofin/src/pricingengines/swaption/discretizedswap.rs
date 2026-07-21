//! Swap priced on a lattice.
//!
//! Port of `ql/pricingengines/swap/discretizedswap.{hpp,cpp}`:
//! [`DiscretizedSwap`] is a [`DiscretizedAsset`] built from a
//! [`FixedVsFloatingSwapArguments`] bundle. It rolls the swap value backward
//! through a [`Lattice`], repricing each floating coupon with an embedded
//! [`DiscretizedDiscountBond`] and applying each fixed coupon at its reset node.
//! The Rust placement under `pricingengines/swaption/` (the C++ source lives in
//! `swap/`) follows the consuming `DiscretizedSwaption` (#465).
//!
//! # Reset-time routing (the `CouponAdjustment` split)
//! Each coupon is tagged [`CouponAdjustment::Pre`] or [`CouponAdjustment::Post`].
//! A normal coupon is applied in the pre-adjustment pass at its reset node; a
//! coupon whose reset time is already in the past (relative to the reference
//! date) is instead handled in the post pass, at its pay node, as an
//! already-fixed scalar amount. The past-in-time test is
//! [`is_reset_time_in_past`] (`discretizedswap.cpp:28`).
//!
//! Divergences from QuantLib, all deliberate:
//! - Every driver returns [`QlResult`] (D4/D10) rather than `void`.
//! - C++ copies the whole `VanillaSwap::arguments`; here only the fields the
//!   swap reads are extracted from `&args` at construction (the argument bundle
//!   is not `Clone`, and copying its legs would be wasteful).
//! - C++ reads `Settings::instance().includeTodaysCashFlows()` inside the ctor;
//!   per D5 the [`Settings`] handle is passed in explicitly and the flag read
//!   once at construction.
//! - The C++ `Null<Real>` sentinels become `Option`: a non-constant nominal
//!   (`arguments.nominal == Null`) is `None`, and an unavailable already-fixed
//!   floating coupon is a missing vector entry - both surface as `Err`.

use crate::discretizedasset::{
    CouponAdjustment, DiscretizedAsset, DiscretizedAssetBase, DiscretizedDiscountBond,
};
use crate::errors::QlResult;
use crate::instruments::{FixedVsFloatingSwapArguments, SwapType};
use crate::math::array::Array;
use crate::settings::Settings;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Size, Spread, Time};
use crate::{fail, require};

/// `isResetTimeInPast` (`discretizedswap.cpp:28`): a reset already happened when
/// its time is negative and either the coupon still pays in the future, or it
/// pays today and today's cash flows are included.
#[allow(clippy::float_cmp)]
fn is_reset_time_in_past(
    reset_time: Time,
    pay_time: Time,
    include_todays_cash_flows: bool,
) -> bool {
    reset_time < 0.0 && (pay_time > 0.0 || (include_todays_cash_flows && pay_time == 0.0))
}

/// A swap discretized on a [`Lattice`](crate::methods::lattices::lattice::Lattice)
/// (`discretizedswap.hpp:34`).
///
/// Built from a [`FixedVsFloatingSwapArguments`] with a reference date and day
/// counter; the reset/pay dates become year-fraction times on the lattice grid.
pub struct DiscretizedSwap {
    base: DiscretizedAssetBase,
    swap_type: SwapType,
    nominal: Option<Real>,
    fixed_coupons: Vec<Real>,
    floating_accrual_times: Vec<Time>,
    floating_spreads: Vec<Spread>,
    floating_coupons: Vec<Real>,
    fixed_reset_times: Vec<Time>,
    fixed_pay_times: Vec<Time>,
    fixed_coupon_adjustments: Vec<CouponAdjustment>,
    fixed_reset_is_in_past: Vec<bool>,
    floating_reset_times: Vec<Time>,
    floating_pay_times: Vec<Time>,
    floating_coupon_adjustments: Vec<CouponAdjustment>,
    floating_reset_is_in_past: Vec<bool>,
}

impl DiscretizedSwap {
    /// `DiscretizedSwap(args, referenceDate, dayCounter)` (`discretizedswap.cpp:36`):
    /// every coupon starts tagged [`CouponAdjustment::Pre`], then delegates to
    /// [`with_adjustments`](DiscretizedSwap::with_adjustments).
    ///
    /// # Errors
    /// Propagates [`with_adjustments`](DiscretizedSwap::with_adjustments).
    pub fn new(
        args: &FixedVsFloatingSwapArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        let fixed = vec![CouponAdjustment::Pre; args.fixed_pay_dates.len()];
        let floating = vec![CouponAdjustment::Pre; args.floating_pay_dates.len()];
        Self::with_adjustments(args, reference_date, day_counter, fixed, floating, settings)
    }

    /// `DiscretizedSwap(args, referenceDate, dayCounter, fixedAdjustments,
    /// floatingAdjustments)` (`discretizedswap.cpp:46`): resolve each leg's
    /// reset/pay times, and flip any coupon whose reset is in the past to
    /// [`CouponAdjustment::Post`].
    ///
    /// # Errors
    /// Fails if either adjustment vector's length differs from its leg's pay-date
    /// count, or if the arguments carry no swap type.
    #[allow(clippy::needless_range_loop)]
    pub fn with_adjustments(
        args: &FixedVsFloatingSwapArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        mut fixed_coupon_adjustments: Vec<CouponAdjustment>,
        mut floating_coupon_adjustments: Vec<CouponAdjustment>,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        require!(
            fixed_coupon_adjustments.len() == args.fixed_pay_dates.len(),
            "The fixed coupon adjustments must have the same size as the number of fixed coupons."
        );
        require!(
            floating_coupon_adjustments.len() == args.floating_pay_dates.len(),
            "The floating coupon adjustments must have the same size as the number of floating \
             coupons."
        );

        let Some(swap_type) = args.swap_type else {
            fail!("the swap type is not set on the arguments");
        };

        let include_todays = settings.include_todays_cash_flows() == Some(true);

        let n_fixed = args.fixed_reset_dates.len();
        let mut fixed_reset_times = Vec::with_capacity(n_fixed);
        let mut fixed_pay_times = Vec::with_capacity(n_fixed);
        let mut fixed_reset_is_in_past = Vec::with_capacity(n_fixed);
        for i in 0..n_fixed {
            let reset = day_counter.year_fraction(reference_date, args.fixed_reset_dates[i]);
            let pay = day_counter.year_fraction(reference_date, args.fixed_pay_dates[i]);
            let in_past = is_reset_time_in_past(reset, pay, include_todays);
            fixed_reset_times.push(reset);
            fixed_pay_times.push(pay);
            fixed_reset_is_in_past.push(in_past);
            if in_past {
                fixed_coupon_adjustments[i] = CouponAdjustment::Post;
            }
        }

        let n_float = args.floating_reset_dates.len();
        let mut floating_reset_times = Vec::with_capacity(n_float);
        let mut floating_pay_times = Vec::with_capacity(n_float);
        let mut floating_reset_is_in_past = Vec::with_capacity(n_float);
        for i in 0..n_float {
            let reset = day_counter.year_fraction(reference_date, args.floating_reset_dates[i]);
            let pay = day_counter.year_fraction(reference_date, args.floating_pay_dates[i]);
            let in_past = is_reset_time_in_past(reset, pay, include_todays);
            floating_reset_times.push(reset);
            floating_pay_times.push(pay);
            floating_reset_is_in_past.push(in_past);
            if in_past {
                floating_coupon_adjustments[i] = CouponAdjustment::Post;
            }
        }

        Ok(DiscretizedSwap {
            base: DiscretizedAssetBase::default(),
            swap_type,
            nominal: args.nominal,
            fixed_coupons: args.fixed_coupons.clone(),
            floating_accrual_times: args.floating_accrual_times.clone(),
            floating_spreads: args.floating_spreads.clone(),
            floating_coupons: args.floating_coupons.clone(),
            fixed_reset_times,
            fixed_pay_times,
            fixed_coupon_adjustments,
            fixed_reset_is_in_past,
            floating_reset_times,
            floating_pay_times,
            floating_coupon_adjustments,
            floating_reset_is_in_past,
        })
    }

    /// `addFixedCoupon(i)` (`discretizedswap.cpp:184`): reprice the `i`-th fixed
    /// coupon by rolling a unit discount bond from its pay node back to the
    /// current time, scaling by the coupon, and subtracting it on a payer swap
    /// (adding on a receiver).
    fn add_fixed_coupon(&mut self, i: Size) -> QlResult<()> {
        let method = self.require_method()?;
        let time = self.time();
        let pay_time = self.fixed_pay_times[i];
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(method, pay_time)?;
        bond.rollback(time)?;

        let fixed_coupon = self.fixed_coupons[i];
        let payer = matches!(self.swap_type, SwapType::Payer);
        let values = self.values_mut();
        for j in 0..values.size() {
            let coupon = fixed_coupon * bond.values()[j];
            if payer {
                values[j] -= coupon;
            } else {
                values[j] += coupon;
            }
        }
        Ok(())
    }

    /// `addFloatingCoupon(i)` (`discretizedswap.cpp:199`): reprice the `i`-th
    /// floating coupon as `nominal * (1 - P) + accruedSpread * P`, where `P` is a
    /// unit discount bond rolled from the coupon's pay node to the current time
    /// and `accruedSpread = nominal * accrualTime * spread`. A payer swap adds it
    /// (a receiver subtracts).
    ///
    /// # Errors
    /// Fails if the nominal is non-constant (`arguments.nominal == Null`).
    fn add_floating_coupon(&mut self, i: Size) -> QlResult<()> {
        let method = self.require_method()?;
        let time = self.time();
        let pay_time = self.floating_pay_times[i];
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(method, pay_time)?;
        bond.rollback(time)?;

        let Some(nominal) = self.nominal else {
            fail!("non-constant nominals are not supported yet");
        };
        let accrual = self.floating_accrual_times[i];
        let spread = self.floating_spreads[i];
        let accrued_spread = nominal * accrual * spread;
        let payer = matches!(self.swap_type, SwapType::Payer);
        let values = self.values_mut();
        for j in 0..values.size() {
            let bond_value = bond.values()[j];
            let coupon = nominal * (1.0 - bond_value) + accrued_spread * bond_value;
            if payer {
                values[j] += coupon;
            } else {
                values[j] -= coupon;
            }
        }
        Ok(())
    }
}

impl DiscretizedAsset for DiscretizedSwap {
    fn base(&self) -> &DiscretizedAssetBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        &mut self.base
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    /// `reset(size)` (`discretizedswap.cpp:97`): zero the values, then adjust -
    /// so coupons at the initialization time are applied immediately.
    fn reset(&mut self, size: Size) -> QlResult<()> {
        *self.values_mut() = Array::filled(size, 0.0);
        self.adjust_values()
    }

    /// `mandatoryTimes()` (`discretizedswap.cpp:102`): every non-negative reset
    /// and pay time across both legs.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn mandatory_times(&self) -> Vec<Time> {
        let mut times = Vec::new();
        for &t in &self.fixed_reset_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.fixed_pay_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.floating_reset_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.floating_pay_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        times
    }

    /// `preAdjustValuesImpl()` (`discretizedswap.cpp:123`): apply each
    /// pre-tagged floating coupon, then each pre-tagged fixed coupon, at any node
    /// on its reset time.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.floating_reset_times.len() {
            let t = self.floating_reset_times[i];
            if self.floating_coupon_adjustments[i] == CouponAdjustment::Pre
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_floating_coupon(i)?;
            }
        }
        for i in 0..self.fixed_reset_times.len() {
            let t = self.fixed_reset_times[i];
            if self.fixed_coupon_adjustments[i] == CouponAdjustment::Pre
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_fixed_coupon(i)?;
            }
        }
        Ok(())
    }

    /// `postAdjustValuesImpl()` (`discretizedswap.cpp:140`): apply each
    /// post-tagged coupon at its reset node, then the already-fixed past-reset
    /// coupons as scalar amounts at their pay nodes.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.floating_reset_times.len() {
            let t = self.floating_reset_times[i];
            if self.floating_coupon_adjustments[i] == CouponAdjustment::Post
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_floating_coupon(i)?;
            }
        }
        for i in 0..self.fixed_reset_times.len() {
            let t = self.fixed_reset_times[i];
            if self.fixed_coupon_adjustments[i] == CouponAdjustment::Post
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_fixed_coupon(i)?;
            }
        }

        let payer = matches!(self.swap_type, SwapType::Payer);
        for i in 0..self.fixed_pay_times.len() {
            if self.fixed_reset_is_in_past[i] && self.is_on_time(self.fixed_pay_times[i]) {
                let fixed_coupon = self.fixed_coupons[i];
                for v in self.values_mut().iter_mut() {
                    if payer {
                        *v -= fixed_coupon;
                    } else {
                        *v += fixed_coupon;
                    }
                }
            }
        }

        for i in 0..self.floating_pay_times.len() {
            if self.floating_reset_is_in_past[i] && self.is_on_time(self.floating_pay_times[i]) {
                let Some(&current) = self.floating_coupons.get(i) else {
                    fail!("current floating coupon not given");
                };
                for v in self.values_mut().iter_mut() {
                    if payer {
                        *v += current;
                    } else {
                        *v -= current;
                    }
                }
            }
        }
        Ok(())
    }
}
