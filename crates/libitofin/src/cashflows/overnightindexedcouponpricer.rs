//! The compounding pricer for an overnight-indexed coupon.
//!
//! Port of `ql/cashflows/overnightindexedcouponpricer.{hpp,cpp}`, the
//! [`CompoundingOvernightIndexedCouponPricer`]. It compounds the daily overnight
//! fixings of an [`OvernightIndexedCoupon`] over the coupon's value-date
//! schedule (the [`OvernightSchedule`] this module also builds).
//!
//! ## The schedule
//!
//! [`OvernightSchedule`] is the coupon's rate-computation data
//! (`overnightindexedcoupon.cpp:92-179`): the value dates (business days
//! spanning the accrual, front-adjusted `Preceding` and back-adjusted
//! `Following`), the interest dates (the value dates with the true accrual start
//! and end pinned at the ends), the fixing dates (each period's value date, the
//! last excluded), and the daily accrual fractions `dt`. Both the coupon and its
//! pricer hold the one shared schedule.
//!
//! ## The bulk fixing read and the today border case (D11)
//!
//! `overnightindexedcouponpricer.cpp:96` bulk-reads `index->timeSeries()` and
//! indexes it directly, never calling `index->fixing(d)` for the already-fixed
//! part - so `enforcesTodaysHistoricFixings` (consulted only inside
//! `InterestRateIndex::fixing`) never applies inside the compounding loop. The
//! port reads each past fixing through [`Index::past_fixing`], the raw store read
//! that returns `None` on a miss (never enforcing, never forecasting). A past
//! fixing that is missing is an error; today's, when missing, deliberately falls
//! through to the forecast (`overnightindexedcouponpricer.cpp:141-156`) - which
//! is what pins `testCurrentCouponRate`. Only the forward part forecasts, through
//! [`Index::fixing`].
//!
//! ## Telescoping
//!
//! `canApplyTelescopicFormula()` is true on the default path, so C++ replaces the
//! interior forward product with a discount ratio
//! (`overnightindexedcouponpricer.cpp:176-197`). Telescoping is an algebraic
//! identity - the interior daily forward growth factors multiply to exactly that
//! ratio - so the port compounds the forward part as a plain product and
//! reproduces the same rate without reading discount factors directly.
//!
//! ## Divergences from QuantLib
//!
//! `swapletPrice`/`capletPrice`/`floorletPrice` and the caplet/floorlet rates
//! all `QL_FAIL` in C++ for this pricer; the port keeps only the ported surface
//! ([`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate) and the concrete
//! [`average_rate`](Self::average_rate) / [`effective_spread`](Self::effective_spread)
//! / [`effective_index_fixing`](Self::effective_index_fixing)) and refuses the
//! rest. The `OvernightIndexedCouponPricer` base (its optionlet-volatility
//! members and the arithmetic-averaging pricer) is not ported; see the module
//! doc of [`overnightindexedcoupon`](super::overnightindexedcoupon).

use super::couponpricer::FloatingRateCouponPricer;
use super::floatingratecoupon::FloatingRateCoupon;
use crate::errors::QlResult;
use crate::indexes::iborindex::OvernightIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::types::{Rate, Real, Spread, Time};
use crate::{fail, require};

/// The value-date schedule an [`OvernightIndexedCoupon`] compounds over.
///
/// Built by [`new`](Self::new) from the accrual `[start, end]` and the index's
/// fixing calendar and day counter. Held behind a [`Shared`] by both the coupon
/// (for its inspectors) and the pricer (for the compounding loop).
///
/// [`OvernightIndexedCoupon`]: super::overnightindexedcoupon::OvernightIndexedCoupon
pub struct OvernightSchedule {
    /// Value dates for the rates to be compounded (`valueDates_`).
    pub(crate) value_dates: Vec<Date>,
    /// Interest dates: the value dates with the accrual start/end pinned at the
    /// ends (`interestDates_`).
    pub(crate) interest_dates: Vec<Date>,
    /// Fixing dates for the rates to be compounded (`fixingDates_`).
    pub(crate) fixing_dates: Vec<Date>,
    /// Daily accrual (compounding) periods (`dt_`).
    pub(crate) dt: Vec<Time>,
}

impl OvernightSchedule {
    /// Builds the schedule over `[start_date, end_date]` on `index`'s fixing
    /// calendar (`overnightindexedcoupon.cpp:92-179`, the default path).
    ///
    /// The value dates are the business days from the `Preceding`-adjusted start
    /// to the `Following`-adjusted end; the interest dates copy them with the
    /// true accrual start and end pinned at the ends; the fixing dates are each
    /// period's value date (the last value date has no fixing, as the index's
    /// fixing days are zero); and each `dt` is the index day counter's fraction
    /// between successive interest dates. Lookback, lockout, observation shift,
    /// and telescopic value-date optimisation are not ported (see the coupon
    /// module doc).
    pub(crate) fn new(
        index: &OvernightIndex,
        start_date: Date,
        end_date: Date,
    ) -> QlResult<OvernightSchedule> {
        let fixing_calendar = index.fixing_calendar();
        let value_dates = fixing_calendar.business_day_list(
            fixing_calendar.adjust(start_date, BusinessDayConvention::Preceding),
            fixing_calendar.adjust(end_date, BusinessDayConvention::Following),
        );
        require!(value_dates.len() >= 2, "degenerate schedule");
        let n = value_dates.len() - 1;

        let mut interest_dates = value_dates.clone();
        interest_dates[0] = start_date;
        interest_dates[n] = end_date;

        let fixing_dates = value_dates[..n].to_vec();

        let day_counter = index.day_counter();
        let dt = (0..n)
            .map(|i| day_counter.year_fraction(interest_dates[i], interest_dates[i + 1]))
            .collect();

        Ok(OvernightSchedule {
            value_dates,
            interest_dates,
            fixing_dates,
            dt,
        })
    }

    /// The accrual end (last interest date), the date the swaplet rate is
    /// compounded to.
    fn accrual_end(&self) -> Date {
        *self
            .interest_dates
            .last()
            .expect("an overnight schedule always has at least two interest dates")
    }
}

/// The number of fixings whose interest date falls before `date`
/// (`determineNumberOfFixings`): the lower bound of `date` in the interest dates
/// excluding the last.
fn determine_number_of_fixings(interest_dates: &[Date], date: Date) -> usize {
    let searchable = &interest_dates[..interest_dates.len() - 1];
    searchable.partition_point(|&d| d < date)
}

/// Prices an [`OvernightIndexedCoupon`] by daily compounding
/// (`CompoundingOvernightIndexedCouponPricer`).
///
/// It captures at construction everything the compounding needs - the overnight
/// index, the shared [`OvernightSchedule`], and the coupon's gearing, spread and
/// spread-compounding flag - so [`swaplet_rate`](FloatingRateCouponPricer::swaplet_rate)
/// is a pure function of those and the current evaluation date, fixing store and
/// forwarding curve.
///
/// [`OvernightIndexedCoupon`]: super::overnightindexedcoupon::OvernightIndexedCoupon
pub struct CompoundingOvernightIndexedCouponPricer {
    index: Shared<OvernightIndex>,
    schedule: Shared<OvernightSchedule>,
    gearing: Real,
    spread: Spread,
    compound_spread_daily: bool,
    observable: Observable,
}

impl CompoundingOvernightIndexedCouponPricer {
    /// Builds a pricer capturing the coupon's compounding inputs.
    pub(crate) fn new(
        index: Shared<OvernightIndex>,
        schedule: Shared<OvernightSchedule>,
        gearing: Real,
        spread: Spread,
        compound_spread_daily: bool,
    ) -> CompoundingOvernightIndexedCouponPricer {
        CompoundingOvernightIndexedCouponPricer {
            index,
            schedule,
            gearing,
            spread,
            compound_spread_daily,
            observable: Observable::new(),
        }
    }

    /// The compounded rate accrued up to `date` (`averageRate`): gearing and
    /// spread folded in, the daily fixings compounded over the schedule up to
    /// `date`.
    pub fn average_rate(&self, date: Date) -> QlResult<Rate> {
        Ok(self.compute(date)?.0)
    }

    /// The spread that reproduces the coupon amount as
    /// `gearing * effectiveIndexFixing + effectiveSpread` (`effectiveSpread`):
    /// the coupon's own spread unless it compounds daily.
    pub fn effective_spread(&self) -> QlResult<Spread> {
        if !self.compound_spread_daily {
            return Ok(self.spread);
        }
        Ok(self.compute(self.schedule.accrual_end())?.1)
    }

    /// The index fixing that reproduces the coupon amount alongside
    /// [`effective_spread`](Self::effective_spread) (`effectiveIndexFixing`).
    pub fn effective_index_fixing(&self) -> QlResult<Rate> {
        Ok(self.compute(self.schedule.accrual_end())?.2)
    }

    /// The compounded rate, effective spread and effective index fixing at
    /// `date` (`CompoundingOvernightIndexedCouponPricer::compute`).
    fn compute(&self, date: Date) -> QlResult<(Rate, Spread, Rate)> {
        let index = &self.index;
        let today = match index.settings().evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: an overnight coupon needs a reference date"),
        };
        let schedule = &self.schedule;
        let day_counter = index.day_counter();
        let coupon_spread = self.spread;
        let compound_spread_daily = self.compound_spread_daily;

        let n = determine_number_of_fixings(&schedule.interest_dates, date);

        let growth_factor = |fixing: Rate, idx: usize| -> (Real, Real) {
            let span = if date >= schedule.interest_dates[idx + 1] {
                schedule.dt[idx]
            } else {
                day_counter.year_fraction(schedule.interest_dates[idx], date)
            };
            let gf = 1.0 + fixing * span;
            let gf_spread = if compound_spread_daily {
                gf + coupon_spread * span
            } else {
                gf
            };
            (gf, gf_spread)
        };

        let mut compound_factor = 1.0;
        let mut compound_factor_without_spread = 1.0;
        let mut i = 0;

        while i < n && schedule.fixing_dates[i] < today {
            let fixing = match index.past_fixing(schedule.fixing_dates[i])? {
                Some(fixing) => fixing,
                None => fail!(
                    "Missing {} fixing for {:?}",
                    index.name(),
                    schedule.fixing_dates[i]
                ),
            };
            let (gf, gf_spread) = growth_factor(fixing, i);
            compound_factor_without_spread *= gf;
            compound_factor *= gf_spread;
            i += 1;
        }

        if i < n
            && schedule.fixing_dates[i] == today
            && let Some(fixing) = index.past_fixing(schedule.fixing_dates[i])?
        {
            let (gf, gf_spread) = growth_factor(fixing, i);
            compound_factor_without_spread *= gf;
            compound_factor *= gf_spread;
            i += 1;
        }

        while i < n {
            let fixing = index.fixing(schedule.fixing_dates[i], false)?;
            let (gf, gf_spread) = growth_factor(fixing, i);
            compound_factor_without_spread *= gf;
            compound_factor *= gf_spread;
            i += 1;
        }

        let rate_accrual_end = date.min(schedule.interest_dates[n]);
        let tau = day_counter.year_fraction(schedule.interest_dates[0], rate_accrual_end);
        let rate = (compound_factor - 1.0) / tau;

        let mut swaplet_rate = self.gearing * rate;
        let effective_spread;
        let effective_index_fixing;
        if !compound_spread_daily {
            swaplet_rate += coupon_spread;
            effective_spread = coupon_spread;
            effective_index_fixing = rate;
        } else {
            effective_spread = rate - (compound_factor_without_spread - 1.0) / tau;
            effective_index_fixing = rate - effective_spread;
        }

        Ok((swaplet_rate, effective_spread, effective_index_fixing))
    }
}

impl AsObservable for CompoundingOvernightIndexedCouponPricer {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl FloatingRateCouponPricer for CompoundingOvernightIndexedCouponPricer {
    fn initialize(&mut self, _coupon: &FloatingRateCoupon) {}

    fn swaplet_rate(&self) -> QlResult<Rate> {
        Ok(self.compute(self.schedule.accrual_end())?.0)
    }

    fn swaplet_rate_for(&self, _index_fixing: QlResult<Rate>) -> QlResult<Rate> {
        fail!(
            "swaplet_rate_for not applicable: the overnight compounding pricer reads the whole daily schedule"
        )
    }

    fn caplet_rate(&self, _effective_cap: Rate, _forward: QlResult<Rate>) -> QlResult<Rate> {
        fail!("caplet rate not ported: overnight cap/floor slice")
    }

    fn floorlet_rate(&self, _effective_floor: Rate, _forward: QlResult<Rate>) -> QlResult<Rate> {
        fail!("floorlet rate not ported: overnight cap/floor slice")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::Sofr;
    use crate::settings::Settings;
    use crate::shared::{shared, shared_mut};
    use crate::time::date::Month;

    fn sofr() -> Shared<OvernightIndex> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(23, Month::November, 2021));
        shared(Sofr::new(Handle::empty(), settings))
    }

    fn schedule(index: &OvernightIndex) -> Shared<OvernightSchedule> {
        shared(
            OvernightSchedule::new(
                index,
                Date::new(18, Month::October, 2021),
                Date::new(18, Month::November, 2021),
            )
            .unwrap(),
        )
    }

    /// `OvernightSchedule::new` (overnightindexedcoupon.cpp:92-179, default path):
    /// one more value date than fixing date, the true accrual start and end pinned
    /// at the interest-date ends, and one accrual fraction per fixing.
    #[test]
    fn the_schedule_has_consistent_lengths_and_endpoints() {
        let index = sofr();
        let start = Date::new(18, Month::October, 2021);
        let end = Date::new(18, Month::November, 2021);
        let schedule = OvernightSchedule::new(&index, start, end).unwrap();

        assert!(schedule.value_dates.len() >= 2);
        assert_eq!(schedule.fixing_dates.len(), schedule.value_dates.len() - 1);
        assert_eq!(schedule.dt.len(), schedule.fixing_dates.len());
        assert_eq!(schedule.interest_dates.len(), schedule.value_dates.len());
        assert_eq!(*schedule.interest_dates.first().unwrap(), start);
        assert_eq!(*schedule.interest_dates.last().unwrap(), end);
    }

    /// A start date after the end date has no business days between: a degenerate
    /// schedule is refused.
    #[test]
    fn a_degenerate_schedule_is_refused() {
        let index = sofr();
        let day = Date::new(18, Month::October, 2021);
        assert!(OvernightSchedule::new(&index, day, day).is_err());
    }

    /// The compounding pricer prices only the swaplet path; the caplet, floorlet
    /// and per-fixing entry points refuse (the cap/floor slice is not ported).
    #[test]
    fn the_pricer_refuses_the_unported_entry_points() {
        let index = sofr();
        let schedule = schedule(&index);
        let pricer = shared_mut(CompoundingOvernightIndexedCouponPricer::new(
            index, schedule, 1.0, 0.0, false,
        ));
        let pricer = pricer.borrow();
        assert!(pricer.caplet_rate(0.03, Ok(0.01)).is_err());
        assert!(pricer.floorlet_rate(0.01, Ok(0.01)).is_err());
        assert!(pricer.swaplet_rate_for(Ok(0.01)).is_err());
    }
}
