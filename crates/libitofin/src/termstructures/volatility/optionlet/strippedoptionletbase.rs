//! Stripped-optionlet interface (`StrippedOptionletBase`).
//!
//! Port of `ql/termstructures/volatility/optionlet/strippedoptionletbase.hpp`:
//! the abstract interface for a time-indexed vector of strike-indexed optionlet
//! (caplet/floorlet) volatilities. A concrete stripper (the
//! [`OptionletStripper`](super::OptionletStripper) base plus the stripping
//! algorithm in #575) implements it; the interpolated
//! optionlet surface (#576) reads through it.
//!
//! ## Divergences from QuantLib
//!
//! - C++ `StrippedOptionletBase` derives `LazyObject`; the vector accessors call
//!   `calculate()` and can therefore fail, so each returns a [`QlResult`] and an
//!   owned vector rather than the C++ `const&`. The laziness itself lives on the
//!   concrete stripper (#575), not on this interface.
//! - Observability (the `LazyObject` base) is deferred to the concrete stripper,
//!   which is where the notification graph is wired.

use crate::errors::QlResult;
use crate::termstructures::volatility::VolatilityType;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Rate, Real, Time, Volatility};

/// Abstract interface for a (time-indexed) vector of (strike-indexed) optionlet
/// volatilities (`StrippedOptionletBase`).
pub trait StrippedOptionletBase {
    /// The optionlet strikes for the `i`-th maturity.
    fn optionlet_strikes(&self, i: usize) -> QlResult<Vec<Rate>>;

    /// The optionlet volatilities for the `i`-th maturity.
    fn optionlet_volatilities(&self, i: usize) -> QlResult<Vec<Volatility>>;

    /// The optionlet fixing dates, one per maturity.
    fn optionlet_fixing_dates(&self) -> QlResult<Vec<Date>>;

    /// The optionlet fixing times, one per maturity.
    fn optionlet_fixing_times(&self) -> QlResult<Vec<Time>>;

    /// The number of optionlet maturities.
    fn optionlet_maturities(&self) -> usize;

    /// The at-the-money optionlet forward rates, one per maturity.
    fn atm_optionlet_rates(&self) -> QlResult<Vec<Rate>>;

    /// The day counter used for date/time conversion.
    fn day_counter(&self) -> Option<DayCounter>;

    /// The calendar used for date arithmetic.
    fn calendar(&self) -> Option<Calendar>;

    /// The settlement days.
    fn settlement_days(&self) -> QlResult<Natural>;

    /// The business-day convention.
    fn business_day_convention(&self) -> BusinessDayConvention;

    /// The model the stripped volatilities are expressed in.
    fn volatility_type(&self) -> VolatilityType;

    /// The lognormal shift applied to forward and strike.
    fn displacement(&self) -> Real;
}
