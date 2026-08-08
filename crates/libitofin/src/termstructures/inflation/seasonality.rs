//! Seasonality corrections applied to inflation term structures.
//!
//! Port of `ql/termstructures/inflation/seasonality.{hpp,cpp}`:
//! [`Seasonality`] is the transformation an inflation curve folds into the
//! rates it publishes, and [`MultiplicativePriceSeasonality`] is the one
//! implementation QuantLib ships for price indexes (CPI/RPI/HICP), where the
//! factors multiply the index level itself.
//!
//! Seasonality fills in an inflation curve between the integer-year maturities
//! the market quotes. Stationary (one year of factors) multiplicative price
//! seasonality moves zero-coupon rates but leaves year-on-year rates alone;
//! multi-year factor sets move both. The correction is piecewise constant, so
//! it works with un-interpolated inflation indexes
//! (`seasonality.hpp:34-53,81-118`).
//!
//! ## Divergences from QuantLib
//!
//! - `KerkhofSeasonality` (`seasonality.hpp:170-187`,
//!   `seasonality.cpp:220-277`) is **not ported**. It is a distinct
//!   cumulative-product subclass, not a parametrization of
//!   [`MultiplicativePriceSeasonality`], and is out of scope here (#729).
//! - [`MultiplicativePriceSeasonality::is_consistent`] ports the two
//!   short-circuits (`seasonality.cpp:69-70`) but **not** the multi-year
//!   whole-year comparison loop (`seasonality.cpp:72-85`), which returns a
//!   typed error naming the deferral (#807) rather than a silent `true`. A
//!   stationary factor set - the same count as the frequency - is unaffected.
//! - C++'s `set` assigns the fields and *then* validates, leaving an invalid
//!   object behind on failure; [`MultiplicativePriceSeasonality::set`] builds
//!   the replacement first, so a rejected input leaves the receiver untouched.
//! - `iTS.dayCounter()` may be an empty day counter in C++; here it is
//!   [`TermStructure::require_day_counter`], which errors on a curve carrying
//!   none (D10).
//! - The factor lookup, the validation and the corrections all return
//!   [`QlResult`] where C++ throws, `inflation_period` being fallible.

use crate::errors::QlResult;
use crate::indexes::inflationindex::inflation_period;
use crate::termstructures::inflation::inflationtermstructure::InflationTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Rate};
use crate::{fail, require};

/// A transformation of an existing inflation rate.
///
/// Mirrors QuantLib's abstract `Seasonality` (`seasonality.hpp:55-79`): the
/// two `correctXXXRate` methods return the rate with the correction folded in,
/// and [`is_consistent`](Self::is_consistent) reports whether the factor set
/// can coexist with the curve it is given to.
pub trait Seasonality {
    /// The zero-coupon inflation rate for `date`, corrected.
    fn correct_zero_rate(
        &self,
        date: Date,
        rate: Rate,
        its: &dyn InflationTermStructure,
    ) -> QlResult<Rate>;

    /// The year-on-year inflation rate for `date`, corrected.
    fn correct_yoy_rate(
        &self,
        date: Date,
        rate: Rate,
        its: &dyn InflationTermStructure,
    ) -> QlResult<Rate>;

    /// Whether the correction is consistent with `its`
    /// (`seasonality.cpp:29-31`).
    ///
    /// Multi-year seasonalities can contradict the quoted instruments a curve
    /// is built from: for price seasonality the corrections at whole years
    /// after the curve's base date must agree. Implementing the test is
    /// optional, and the default reports consistency.
    fn is_consistent(&self, _its: &dyn InflationTermStructure) -> QlResult<bool> {
        Ok(true)
    }
}

/// Multiplicative seasonality in the price index (CPI/RPI/HICP).
///
/// Port of `seasonality.hpp:119-167`. Factors come in whole multiples of the
/// count the frequency dictates - twelve for [`Monthly`](Frequency::Monthly) -
/// and are reused as long as needed, so twenty-four monthly factors repeat
/// every two years and twelve of them are stationary.
///
/// The factors are normalized against a reference date rather than used raw:
/// for a zero-coupon rate that is the curve's true base date, whose fixing is
/// known and whose factor must therefore divide out to one; for a year-on-year
/// rate it is always one year earlier.
///
/// Multi-year (non-stationary) factor sets are fragile: the corrections at
/// whole years either side of the curve's base date must match or the curve
/// contradicts its own quotes. See
/// [`is_consistent`](MultiplicativePriceSeasonality::is_consistent) for how
/// much of that check is ported.
#[derive(Debug)]
pub struct MultiplicativePriceSeasonality {
    seasonality_base_date: Date,
    frequency: Frequency,
    seasonality_factors: Vec<Rate>,
}

impl MultiplicativePriceSeasonality {
    /// The seasonality whose factors start at `seasonality_base_date` and step
    /// at `frequency` (`seasonality.cpp:91-95`).
    ///
    /// # Errors
    ///
    /// Rejects a frequency outside semiannual-through-daily, an empty factor
    /// set, and a factor count that is not a whole multiple of the frequency.
    pub fn new(
        seasonality_base_date: Date,
        frequency: Frequency,
        seasonality_factors: Vec<Rate>,
    ) -> QlResult<MultiplicativePriceSeasonality> {
        let seasonality = MultiplicativePriceSeasonality {
            seasonality_base_date,
            frequency,
            seasonality_factors,
        };
        seasonality.validate()?;
        Ok(seasonality)
    }

    /// Replaces the whole factor specification (`seasonality.cpp:97-108`),
    /// leaving the receiver untouched if the new one is rejected.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn set(
        &mut self,
        seasonality_base_date: Date,
        frequency: Frequency,
        seasonality_factors: Vec<Rate>,
    ) -> QlResult<()> {
        *self = MultiplicativePriceSeasonality::new(
            seasonality_base_date,
            frequency,
            seasonality_factors,
        )?;
        Ok(())
    }

    /// The date the factor set is anchored on (`seasonality.cpp:110-112`).
    pub fn seasonality_base_date(&self) -> Date {
        self.seasonality_base_date
    }

    /// The frequency the factors step at (`seasonality.cpp:114-116`).
    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// The factors, in order from the seasonality base date
    /// (`seasonality.cpp:118-120`).
    pub fn seasonality_factors(&self) -> &[Rate] {
        &self.seasonality_factors
    }

    /// The factor covering `to`, normalized against nothing at all
    /// (`seasonality.cpp:145-188`).
    ///
    /// The offset from the seasonality base date is counted in whole factor
    /// periods and wrapped modulo the factor count, so a set shorter than the
    /// span repeats. For a month-based frequency the offset is *stepped*: the
    /// first guess is `31 * length` days per period and the loop then advances
    /// one period at a time until it lands inside `to`'s inflation period,
    /// which is not the same as advancing by the multiple in one go (month-end
    /// clamping is not associative).
    ///
    /// # Errors
    ///
    /// Rejects a year-based factor period, which cannot express seasonality.
    pub fn seasonality_factor(&self, to: Date) -> QlResult<Rate> {
        let from = self.seasonality_base_date;
        let factor_frequency = self.frequency;
        let n_factors = self.seasonality_factors.len();
        let factor_period = Period::try_from(factor_frequency)?;

        let which = if from == to {
            0
        } else {
            let diff_days = (to - from).abs();
            let dir: Integer = if from > to { -1 } else { 1 };
            let diff = match factor_period.units() {
                TimeUnit::Days => dir * diff_days,
                TimeUnit::Weeks => dir * (diff_days / 7),
                TimeUnit::Months => {
                    let lim = inflation_period(to, factor_frequency)?;
                    let mut steps = diff_days / (31 * factor_period.length());
                    let mut go = from + factor_period * (dir * steps);
                    while !(lim.0 <= go && go <= lim.1) {
                        go = go + factor_period * dir;
                        steps += 1;
                    }
                    dir * steps
                }
                TimeUnit::Years => fail!(
                    "seasonality period time unit is not allowed to be : {}",
                    factor_period.units()
                ),
                unit => fail!("Unknown time unit: {unit}"),
            };
            if dir == 1 {
                (diff as usize) % n_factors
            } else {
                (n_factors - ((-diff) as usize) % n_factors) % n_factors
            }
        };

        Ok(self.seasonality_factors[which])
    }

    /// The factor set as it applies to `frequency`, rejecting the frequencies
    /// QuantLib refuses (`seasonality.cpp:36-61`).
    fn validate(&self) -> QlResult<()> {
        match self.frequency {
            Frequency::Semiannual
            | Frequency::EveryFourthMonth
            | Frequency::Quarterly
            | Frequency::Bimonthly
            | Frequency::Monthly
            | Frequency::Biweekly
            | Frequency::Weekly
            | Frequency::Daily => {
                require!(
                    !self.seasonality_factors.is_empty(),
                    "no seasonality factors given"
                );
                let per_year = self.frequency as i16;
                require!(
                    self.seasonality_factors
                        .len()
                        .is_multiple_of(per_year as usize),
                    "For frequency {} require multiple of {per_year} factors {} were given.",
                    self.frequency,
                    self.seasonality_factors.len()
                );
                Ok(())
            }
            frequency => fail!(
                "bad frequency specified: {frequency}, \
                 only semi-annual through daily permitted."
            ),
        }
    }

    /// `rate` with the correction folded in (`seasonality.cpp:191-217`).
    ///
    /// Two factors are needed, not one: the raw factor at `at_date` divided by
    /// the one at the reference date, which is `curve_base_date` for a zero
    /// rate (whose fixing is known there, so the correction must be the
    /// identity) and one year earlier for a year-on-year rate. The zero path
    /// then spreads the ratio over the time from the curve base to the start
    /// of `at_date`'s inflation period.
    ///
    /// At the curve's own base date that time is zero and the ratio is exactly
    /// one, so `1.powf(inf)` returns one and the correction is the identity -
    /// the path every bootstrap takes on its first node. C++ has no guard
    /// there and neither does this.
    fn seasonality_correction(
        &self,
        rate: Rate,
        at_date: Date,
        day_counter: &DayCounter,
        curve_base_date: Date,
        is_zero_rate: bool,
    ) -> QlResult<Rate> {
        let factor_at = self.seasonality_factor(at_date)?;

        let f = if is_zero_rate {
            let factor_base = self.seasonality_factor(curve_base_date)?;
            let seasonality_at = factor_at / factor_base;
            let (period_start, _) = inflation_period(at_date, self.frequency)?;
            let time_from_curve_base = day_counter.year_fraction(curve_base_date, period_start);
            seasonality_at.powf(1.0 / time_from_curve_base)
        } else {
            let a_year_before = at_date - Period::new(1, TimeUnit::Years);
            factor_at / self.seasonality_factor(a_year_before)?
        };

        Ok((rate + 1.0) * f - 1.0)
    }
}

impl Seasonality for MultiplicativePriceSeasonality {
    /// The zero rate corrected against the curve's *true* base date
    /// (`seasonality.cpp:123-133`).
    ///
    /// The reference is `its.base_date()` itself, not the end of its inflation
    /// period, and `date` is quantized to the start of its own period, so this
    /// picks the same base date and effective fixing date
    /// [`ZeroInflationIndex::forecast_fixing`] does and the input seasonality
    /// adjustment is recovered by their ratio.
    ///
    /// [`ZeroInflationIndex::forecast_fixing`]: crate::indexes::inflationindex::ZeroInflationIndex
    fn correct_zero_rate(
        &self,
        date: Date,
        rate: Rate,
        its: &dyn InflationTermStructure,
    ) -> QlResult<Rate> {
        let curve_base_date = its.base_date();
        let (effective_fixing_date, _) = inflation_period(date, its.frequency())?;
        self.seasonality_correction(
            rate,
            effective_fixing_date,
            &its.require_day_counter()?,
            curve_base_date,
            true,
        )
    }

    /// The year-on-year rate corrected against one year earlier
    /// (`seasonality.cpp:136-142`).
    ///
    /// The reference here is the *end* of the base date's inflation period,
    /// and `date` reaches the correction unquantized. A stationary factor set
    /// leaves the rate alone: the factor a year earlier is the same one.
    fn correct_yoy_rate(
        &self,
        date: Date,
        rate: Rate,
        its: &dyn InflationTermStructure,
    ) -> QlResult<Rate> {
        let (_, curve_base_date) = inflation_period(its.base_date(), its.frequency())?;
        self.seasonality_correction(
            rate,
            date,
            &its.require_day_counter()?,
            curve_base_date,
            false,
        )
    }

    /// Consistency with the curve (`seasonality.cpp:64-88`), as far as it is
    /// ported.
    ///
    /// Daily seasonality is consistent by fiat: weekends, holidays and leap
    /// years make it otherwise never so (`seasonality.cpp:67-69`). A
    /// stationary set - one factor per period of the year - is consistent
    /// because it repeats exactly on whole years (`seasonality.cpp:70`).
    ///
    /// # Errors
    ///
    /// Any other (multi-year) factor count errors: the whole-year comparison
    /// loop that decides those (`seasonality.cpp:72-85`) is deferred to #807,
    /// and reporting consistency without running it would let an inconsistent
    /// curve through unnoticed.
    fn is_consistent(&self, _its: &dyn InflationTermStructure) -> QlResult<bool> {
        if self.frequency == Frequency::Daily {
            return Ok(true);
        }
        if (self.frequency as i16 as usize) == self.seasonality_factors.len() {
            return Ok(true);
        }
        fail!(
            "multi-year seasonality consistency check is not ported (#807): \
             {} factors at {} frequency",
            self.seasonality_factors.len(),
            self.frequency
        )
    }
}
