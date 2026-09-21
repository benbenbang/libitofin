//! Cap, floor and collar cash flows on a short-rate lattice.

use crate::discretizedasset::{DiscretizedAsset, DiscretizedAssetBase, DiscretizedDiscountBond};
use crate::errors::QlResult;
use crate::instruments::{CapFloorArguments, CapFloorType};
use crate::math::array::Array;
use crate::pricingengine::Arguments;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

/// Bond-option representation of caplets and floorlets, following QuantLib.
pub struct DiscretizedCapFloor {
    base: DiscretizedAssetBase,
    kind: CapFloorType,
    start_times: Vec<Time>,
    end_times: Vec<Time>,
    accruals: Vec<Time>,
    nominals: Vec<Real>,
    gearings: Vec<Real>,
    cap_rates: Vec<Option<Real>>,
    floor_rates: Vec<Option<Real>>,
    forwards: Vec<Option<Real>>,
}

impl DiscretizedCapFloor {
    /// Copies the coupon inputs and converts dates to lattice times.
    ///
    /// # Errors
    /// Rejects inconsistent arguments and missing or non-finite required values.
    pub fn new(args: &CapFloorArguments, reference: Date, dc: &DayCounter) -> QlResult<Self> {
        args.validate()?;
        require!(
            reference != Date::null(),
            "cap/floor reference date is null"
        );
        let Some(kind) = args.cap_floor_type else {
            fail!("cap/floor type not set");
        };
        for i in 0..args.end_dates.len() {
            require!(
                args.start_dates[i] != Date::null()
                    && args.end_dates[i] != Date::null()
                    && args.end_dates[i] >= args.start_dates[i],
                "payment precedes accrual start"
            );
            require!(
                args.accrual_times[i].is_finite() && args.accrual_times[i] > 0.0,
                "invalid accrual time"
            );
            require!(
                (args.nominals[i] * args.gearings[i] * args.accrual_times[i]).is_finite(),
                "invalid nominal or gearing"
            );
            if matches!(kind, CapFloorType::Cap | CapFloorType::Collar) {
                require!(
                    args.cap_rates[i].is_some_and(|r| r.is_finite()
                        && (r * args.accrual_times[i]).is_finite()
                        && 1.0 + r * args.accrual_times[i] > 0.0),
                    "invalid cap rate"
                );
            }
            if matches!(kind, CapFloorType::Floor | CapFloorType::Collar) {
                require!(
                    args.floor_rates[i].is_some_and(|r| r.is_finite()
                        && (r * args.accrual_times[i]).is_finite()
                        && 1.0 + r * args.accrual_times[i] > 0.0),
                    "invalid floor rate"
                );
            }
            if args.start_dates[i] < reference && args.end_dates[i] >= reference {
                require!(
                    args.forwards[i].is_some_and(Real::is_finite),
                    "a still-live cap/floor coupon has no finite forward set"
                );
            }
        }
        Ok(Self {
            base: DiscretizedAssetBase::default(),
            kind,
            start_times: args
                .start_dates
                .iter()
                .map(|&d| dc.year_fraction(reference, d))
                .collect(),
            end_times: args
                .end_dates
                .iter()
                .map(|&d| dc.year_fraction(reference, d))
                .collect(),
            accruals: args.accrual_times.clone(),
            nominals: args.nominals.clone(),
            gearings: args.gearings.clone(),
            cap_rates: args.cap_rates.clone(),
            floor_rates: args.floor_rates.clone(),
            forwards: args.forwards.clone(),
        })
    }
}

impl DiscretizedAsset for DiscretizedCapFloor {
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
        let method = self.require_method()?;
        for time in self
            .mandatory_times()
            .into_iter()
            .filter(|time| *time >= 0.0)
        {
            method.time_grid().index(time)?;
        }
        *self.values_mut() = Array::filled(size, 0.0);
        self.adjust_values()
    }
    fn mandatory_times(&self) -> Vec<Time> {
        self.start_times
            .iter()
            .chain(&self.end_times)
            .copied()
            .collect()
    }
    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.start_times.len() {
            if self.start_times[i] < 0.0 || !self.is_on_time(self.start_times[i]) {
                continue;
            }
            let mut bond = DiscretizedDiscountBond::new();
            bond.initialize(self.require_method()?, self.end_times[i])?;
            bond.rollback(self.time())?;
            let scale = self.nominals[i] * self.gearings[i];
            for j in 0..self.values().size() {
                if matches!(self.kind, CapFloorType::Cap | CapFloorType::Collar)
                    && let Some(rate) = self.cap_rates[i]
                {
                    let accrual = 1.0 + rate * self.accruals[i];
                    self.values_mut()[j] +=
                        scale * accrual * (1.0 / accrual - bond.values()[j]).max(0.0);
                }
                if matches!(self.kind, CapFloorType::Floor | CapFloorType::Collar)
                    && let Some(rate) = self.floor_rates[i]
                {
                    let accrual = 1.0 + rate * self.accruals[i];
                    let sign = if self.kind == CapFloorType::Floor {
                        1.0
                    } else {
                        -1.0
                    };
                    self.values_mut()[j] +=
                        scale * accrual * sign * (bond.values()[j] - 1.0 / accrual).max(0.0);
                }
            }
        }
        Ok(())
    }
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.end_times.len() {
            if self.start_times[i] >= 0.0
                || self.end_times[i] < 0.0
                || !self.is_on_time(self.end_times[i])
            {
                continue;
            }
            let Some(fixing) = self.forwards[i] else {
                fail!("past-start coupon has no forward");
            };
            let scale = self.nominals[i] * self.accruals[i] * self.gearings[i];
            let mut amount = 0.0;
            if matches!(self.kind, CapFloorType::Cap | CapFloorType::Collar)
                && let Some(rate) = self.cap_rates[i]
            {
                amount += scale * (fixing - rate).max(0.0);
            }
            if matches!(self.kind, CapFloorType::Floor | CapFloorType::Collar)
                && let Some(rate) = self.floor_rates[i]
            {
                let sign = if self.kind == CapFloorType::Floor {
                    1.0
                } else {
                    -1.0
                };
                amount += sign * scale * (rate - fixing).max(0.0);
            }
            for value in self.values_mut().iter_mut() {
                *value += amount;
            }
        }
        Ok(())
    }
}
