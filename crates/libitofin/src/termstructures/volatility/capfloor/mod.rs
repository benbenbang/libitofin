//! Cap/floor term-volatility structures.
//!
//! Port of `ql/termstructures/volatility/capfloor/`.
//! [`CapFloorTermVolatilityStructure`] adds the cap/floor term-volatility query
//! on top of [`VolatilityTermStructure`], mirroring how
//! [`SwaptionVolatilityStructure`](super::SwaptionVolatilityStructure) layers the
//! swaption volatility on the same base. Unlike the swaption surface, which is
//! indexed by option date and swap length, this structure is indexed by
//! cap/floor length (option time) and strike alone; the length- and strike-range
//! checks run exactly as the C++ base performs them before dispatching to the
//! volatility hook.
//!
//! ## Divergences from QuantLib
//!
//! - QuantLib overloads `volatility` across three argument shapes (option tenor,
//!   end date, end time). Rust has no overloading, so the three forms are ported
//!   as [`volatility_tenor`](CapFloorTermVolatilityStructure::volatility_tenor),
//!   [`volatility_date`](CapFloorTermVolatilityStructure::volatility_date) and
//!   [`volatility_time`](CapFloorTermVolatilityStructure::volatility_time). The
//!   required hook is [`volatility_impl`](CapFloorTermVolatilityStructure::volatility_impl)
//!   alone, mirroring C++'s pure-virtual `volatilityImpl(Time, Rate)`.

use crate::errors::QlResult;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Time, Volatility};

/// Cap/floor term-volatility structure.
///
/// Mirrors QuantLib's `CapFloorTermVolatilityStructure`: concrete structures
/// implement [`volatility_impl`](Self::volatility_impl); the provided queries run
/// the length-range and strike checks and dispatch to it. Volatilities are
/// expressed on an annual basis.
pub trait CapFloorTermVolatilityStructure: VolatilityTermStructure {
    /// Volatility calculation hook for a given cap/floor length (in time) and
    /// strike; the range and strike checks have already run.
    fn volatility_impl(&self, length: Time, strike: Rate) -> QlResult<Volatility>;

    /// Volatility for a given cap/floor tenor and strike rate.
    fn volatility_tenor(
        &self,
        length: Period,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        let end = self.option_date_from_tenor(length)?;
        self.volatility_date(end, strike, extrapolate)
    }

    /// Volatility for a given cap/floor end date and strike rate.
    fn volatility_date(&self, end: Date, strike: Rate, extrapolate: bool) -> QlResult<Volatility> {
        self.check_range_date(end, extrapolate)?;
        let t = self.time_from_reference(end)?;
        self.volatility_time(t, strike, extrapolate)
    }

    /// Volatility for a given cap/floor end time and strike rate.
    fn volatility_time(&self, t: Time, strike: Rate, extrapolate: bool) -> QlResult<Volatility> {
        self.check_range_time(t, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        self.volatility_impl(t, strike)
    }
}
