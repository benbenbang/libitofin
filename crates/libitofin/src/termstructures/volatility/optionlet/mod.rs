//! Optionlet (caplet/floorlet) volatility structures.
//!
//! Port of `ql/termstructures/volatility/optionlet/`.
//! [`OptionletVolatilityStructure`] adds the caplet volatility and Black
//! variance queries on top of [`VolatilityTermStructure`], in tenor, date and
//! time form, range- and strike-checked exactly as the C++ base performs them
//! before dispatching to the volatility hook. The volatility type and
//! displacement select the pricing model the surface feeds the coupon pricer.
//!
//! ## Divergences from QuantLib
//!
//! - The `smileSection` family and the `smileSectionImpl` hook are not ported:
//!   they need the smile-section layer, which is not in the crate yet. The
//!   required hook is therefore [`volatility_impl`](OptionletVolatilityStructure::volatility_impl)
//!   alone, mirroring C++'s pure-virtual `volatilityImpl(Time, Rate)`; the
//!   `Date`-based volatility path converts to time and dispatches to it, as the
//!   C++ default `volatilityImpl(Date, Rate)` does.
//! - Only the constant surface is ported here ([`ConstantOptionletVolatility`]).
//!   The stripped and interpolated optionlet surfaces are deferred.

mod constantoptionletvol;

pub use constantoptionletvol::ConstantOptionletVolatility;

use crate::errors::QlResult;
use crate::termstructures::volatility::{VolatilityTermStructure, VolatilityType};
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time, Volatility};

/// Optionlet (caplet/floorlet) volatility structure.
///
/// Mirrors QuantLib's `OptionletVolatilityStructure`: concrete surfaces
/// implement [`volatility_impl`](Self::volatility_impl); the provided queries
/// run the range and strike checks and dispatch to it, deriving the Black
/// variance as `volatility^2 * time`. Volatilities are expressed on an annual
/// basis.
pub trait OptionletVolatilityStructure: VolatilityTermStructure {
    /// Volatility calculation hook; range and strike checks have already run.
    fn volatility_impl(&self, option_time: Time, strike: Rate) -> QlResult<Volatility>;

    /// The pricing model the quoted volatilities are expressed in.
    fn volatility_type(&self) -> VolatilityType {
        VolatilityType::ShiftedLognormal
    }

    /// The lognormal shift applied to forward and strike; `0.0` for the
    /// unshifted lognormal and the normal model.
    fn displacement(&self) -> Real {
        0.0
    }

    /// Volatility for a given option date and strike rate.
    fn volatility_date(
        &self,
        option_date: Date,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_range_date(option_date, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        let t = self.time_from_reference(option_date)?;
        self.volatility_impl(t, strike)
    }

    /// Volatility for a given option time and strike rate.
    fn volatility(
        &self,
        option_time: Time,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_range_time(option_time, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        self.volatility_impl(option_time, strike)
    }

    /// Volatility for a given option tenor and strike rate.
    fn volatility_tenor(
        &self,
        option_tenor: Period,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        let option_date = self.option_date_from_tenor(option_tenor)?;
        self.volatility_date(option_date, strike, extrapolate)
    }

    /// Black variance for a given option date and strike rate.
    fn black_variance_date(
        &self,
        option_date: Date,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Real> {
        let v = self.volatility_date(option_date, strike, extrapolate)?;
        let t = self.time_from_reference(option_date)?;
        Ok(v * v * t)
    }

    /// Black variance for a given option time and strike rate.
    fn black_variance(&self, option_time: Time, strike: Rate, extrapolate: bool) -> QlResult<Real> {
        let v = self.volatility(option_time, strike, extrapolate)?;
        Ok(v * v * option_time)
    }

    /// Black variance for a given option tenor and strike rate.
    fn black_variance_tenor(
        &self,
        option_tenor: Period,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Real> {
        let option_date = self.option_date_from_tenor(option_tenor)?;
        self.black_variance_date(option_date, strike, extrapolate)
    }
}
