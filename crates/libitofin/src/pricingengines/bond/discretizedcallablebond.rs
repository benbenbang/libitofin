//! Discretized callable fixed-rate bond on a lattice.
//!
//! Port of QuantLib's
//! `ql/experimental/callablebonds/discretizedcallablefixedratebond.{hpp,cpp}`.
//! Rolled back over a short-rate lattice, it starts at the redemption, adds
//! coupon amounts at their nodes, and applies each call (`min`) or put (`max`)
//! against the dirty callability price.

use crate::discretizedasset::{CouponAdjustment, DiscretizedAsset, DiscretizedAssetBase};
use crate::errors::QlResult;
use crate::instruments::{CallabilityType, CallableBondArguments};
use crate::math::array::Array;
use crate::pricingengine::Arguments;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

/// A week, used to snap call dates to a nearly coincident coupon date.
const ONE_WEEK: Time = 1.0 / 52.0;

/// The lattice-rolled callable fixed-rate bond.
pub struct DiscretizedCallableFixedRateBond {
    base: DiscretizedAssetBase,
    redemption: Real,
    redemption_time: Time,
    coupon_times: Vec<Time>,
    coupon_amounts: Vec<Real>,
    coupon_adjustments: Vec<CouponAdjustment>,
    callability_times: Vec<Time>,
    callability_types: Vec<CallabilityType>,
    adjusted_callability_prices: Vec<Real>,
}

impl DiscretizedCallableFixedRateBond {
    /// Builds the discretized bond from the engine arguments and the discount
    /// curve (`discretizedcallablefixedratebond.cpp:35-100`).
    ///
    /// # Errors
    ///
    /// Rejects invalid argument shapes/values, curve lookups, nonfinite times
    /// and exercise-price adjustment overflow.
    pub fn new(
        args: &CallableBondArguments,
        curve: &dyn YieldTermStructure,
    ) -> QlResult<DiscretizedCallableFixedRateBond> {
        args.validate()?;
        let reference_date = curve.reference_date()?;
        let day_counter = curve.require_day_counter()?;
        let Some(redemption_date) = args.redemption_date else {
            fail!("null redemption date");
        };
        let redemption_time = day_counter.year_fraction(reference_date, redemption_date);

        let coupon_times: Vec<Time> = args
            .coupon_dates
            .iter()
            .map(|&d| day_counter.year_fraction(reference_date, d))
            .collect();
        let mut coupon_adjustments = vec![CouponAdjustment::Post; args.coupon_dates.len()];

        let mut adjusted = args.callability_prices.clone();
        let mut callability_times = Vec::with_capacity(args.callability_dates.len());
        for (i, &call_date) in args.callability_dates.iter().enumerate() {
            let mut call_time = day_counter.year_fraction(reference_date, call_date);
            for (j, &coupon_time) in coupon_times.iter().enumerate() {
                let coupon_date = args.coupon_dates[j];
                if call_time <= coupon_time
                    && coupon_time <= call_time + ONE_WEEK
                    && call_date < coupon_date
                {
                    call_time = coupon_time;
                    coupon_adjustments[j] = CouponAdjustment::Pre;
                    let df_call = curve.discount_date(call_date, false)?;
                    let df_coupon = curve.discount_date(coupon_date, false)?;
                    require!(
                        df_call.is_finite()
                            && df_call > 0.0
                            && df_coupon.is_finite()
                            && df_coupon > 0.0,
                        "invalid exercise adjustment discount factor"
                    );
                    adjusted[i] *= df_call / df_coupon;
                    break;
                }
            }
            adjusted[i] *= args.face_amount / 100.0;
            require!(adjusted[i].is_finite(), "exercise amount overflow");
            callability_times.push(call_time);
        }

        require!(
            redemption_time.is_finite()
                && coupon_times.iter().all(|t| t.is_finite())
                && callability_times.iter().all(|t| t.is_finite()),
            "nonfinite callable bond time"
        );

        Ok(DiscretizedCallableFixedRateBond {
            base: DiscretizedAssetBase::default(),
            redemption: args.redemption,
            redemption_time,
            coupon_times,
            coupon_amounts: args.coupon_amounts.clone(),
            coupon_adjustments,
            callability_times,
            callability_types: args.callability_types.clone(),
            adjusted_callability_prices: adjusted,
        })
    }

    fn add_coupon(&mut self, i: usize) -> QlResult<()> {
        let amount = self.coupon_amounts[i];
        let values = self.values_mut();
        for j in 0..values.size() {
            values[j] += amount;
            require!(
                values[j].is_finite(),
                "nonfinite callable bond node after coupon"
            );
        }
        Ok(())
    }

    fn apply_callability(&mut self, i: usize) -> QlResult<()> {
        let price = self.adjusted_callability_prices[i];
        let call_type = self.callability_types[i];
        let values = self.values_mut();
        require!(
            values.iter().all(|value| value.is_finite()),
            "nonfinite callable bond continuation value"
        );
        match call_type {
            CallabilityType::Call => {
                for j in 0..values.size() {
                    values[j] = values[j].min(price);
                }
            }
            CallabilityType::Put => {
                for j in 0..values.size() {
                    values[j] = values[j].max(price);
                }
            }
        }
        Ok(())
    }
}

impl DiscretizedAsset for DiscretizedCallableFixedRateBond {
    fn base(&self) -> &DiscretizedAssetBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        &mut self.base
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    fn reset(&mut self, size: Size) -> QlResult<()> {
        *self.values_mut() = Array::filled(size, self.redemption);
        self.adjust_values()
    }

    fn mandatory_times(&self) -> Vec<Time> {
        let mut times = Vec::new();
        if self.redemption_time >= 0.0 {
            times.push(self.redemption_time);
        }
        for &t in &self.coupon_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.callability_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        times
    }

    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.coupon_times.len() {
            if self.coupon_adjustments[i] == CouponAdjustment::Pre
                && self.coupon_times[i] >= 0.0
                && self.is_on_time(self.coupon_times[i])
            {
                self.add_coupon(i)?;
            }
        }
        Ok(())
    }

    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.callability_times.len() {
            if self.callability_times[i] >= 0.0 && self.is_on_time(self.callability_times[i]) {
                self.apply_callability(i)?;
            }
        }
        for i in 0..self.coupon_times.len() {
            if self.coupon_adjustments[i] == CouponAdjustment::Post
                && self.coupon_times[i] >= 0.0
                && self.is_on_time(self.coupon_times[i])
            {
                self.add_coupon(i)?;
            }
        }
        Ok(())
    }
}
