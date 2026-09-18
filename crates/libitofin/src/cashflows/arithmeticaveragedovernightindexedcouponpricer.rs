//! Default, non-telescopic arithmetic averaging of overnight fixings.
//!
//! Matches QuantLib's arithmetic overnight pricer with zero volatility. Known
//! fixings use the elapsed interest span; forecasts retain the whole daily span,
//! including the final span when computing an intermediate accrued amount.

use super::coupon::Coupon;
use super::couponpricer::FloatingRateCouponPricer;
use super::floatingratecoupon::FloatingRateCoupon;
use super::overnightindexedcouponpricer::{OvernightSchedule, determine_number_of_fixings};
use crate::errors::QlResult;
use crate::fail;
use crate::indexes::iborindex::OvernightIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::{Rate, Real, Spread, Time};

pub(super) struct ArithmeticAveragedOvernightIndexedCouponPricer {
    index: Shared<OvernightIndex>,
    schedule: Shared<OvernightSchedule>,
    gearing: Real,
    spread: Spread,
    accrual_period: Time,
    observable: Observable,
}

impl ArithmeticAveragedOvernightIndexedCouponPricer {
    pub(super) fn new(
        index: Shared<OvernightIndex>,
        schedule: Shared<OvernightSchedule>,
        coupon: &FloatingRateCoupon,
    ) -> Self {
        Self {
            index,
            schedule,
            gearing: coupon.gearing(),
            spread: coupon.spread(),
            accrual_period: coupon.accrued_period(coupon.accrual_end_date()),
            observable: Observable::new(),
        }
    }

    pub(super) fn average_rate(&self, date: Date, accrued_period: Time) -> QlResult<Rate> {
        let index = &self.index;
        let Some(today) = index.settings().evaluation_date() else {
            fail!("no evaluation date set: an overnight coupon needs a reference date");
        };
        let schedule = &self.schedule;
        let n = determine_number_of_fixings(&schedule.interest_dates, date);
        let historical_span = |i: usize| {
            if date >= schedule.interest_dates[i + 1] {
                schedule.dt[i]
            } else {
                index
                    .day_counter()
                    .year_fraction(schedule.interest_dates[i], date)
            }
        };
        let mut accumulated_rate = 0.0;
        let mut i = 0;
        while i < n && schedule.fixing_dates[i] < today {
            let Some(fixing) = index.past_fixing(schedule.fixing_dates[i])? else {
                fail!(
                    "Missing {} fixing for {:?}",
                    index.name(),
                    schedule.fixing_dates[i]
                );
            };
            accumulated_rate += fixing * historical_span(i);
            i += 1;
        }
        if i < n
            && schedule.fixing_dates[i] == today
            && let Some(fixing) = index.past_fixing(schedule.fixing_dates[i])?
        {
            accumulated_rate += fixing * historical_span(i);
            i += 1;
        }
        while i < n {
            let fixing = index.fixing(schedule.fixing_dates[i], false)?;
            accumulated_rate += (1.0 + fixing * schedule.dt[i]) - 1.0;
            i += 1;
        }
        Ok(self.gearing * (accumulated_rate / accrued_period) + self.spread)
    }
}

impl AsObservable for ArithmeticAveragedOvernightIndexedCouponPricer {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl FloatingRateCouponPricer for ArithmeticAveragedOvernightIndexedCouponPricer {
    fn initialize(&mut self, _coupon: &FloatingRateCoupon) {}

    fn swaplet_rate(&self) -> QlResult<Rate> {
        self.average_rate(self.schedule.accrual_end(), self.accrual_period)
    }

    fn swaplet_rate_for(&self, _index_fixing: QlResult<Rate>) -> QlResult<Rate> {
        fail!(
            "swaplet_rate_for not applicable: the overnight arithmetic pricer reads the whole daily schedule"
        )
    }

    fn caplet_rate(&self, _effective_cap: Rate, _forward: QlResult<Rate>) -> QlResult<Rate> {
        fail!("caplet rate not ported: overnight cap/floor slice")
    }

    fn floorlet_rate(&self, _effective_floor: Rate, _forward: QlResult<Rate>) -> QlResult<Rate> {
        fail!("floorlet rate not ported: overnight cap/floor slice")
    }
}
