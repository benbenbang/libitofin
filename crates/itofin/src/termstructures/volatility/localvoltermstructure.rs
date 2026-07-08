//! Local-volatility term structure.
//!
//! Port of `ql/termstructures/volatility/equityfx/localvoltermstructure.{hpp,cpp}`:
//! [`LocalVolTermStructure`] extends
//! [`VolatilityTermStructure`](super::VolatilityTermStructure) with the local
//! volatility query, range- and strike-checked before dispatching to the
//! implementation hook exactly as the C++ base does. Volatilities are
//! expressed on an annual basis; the strike argument is the underlying level.
//!
//! `accept(AcyclicVisitor&)` is not ported (dispatch happens through the
//! trait), following the crate convention.

use crate::errors::QlResult;
use crate::time::date::Date;
use crate::types::{Real, Time, Volatility};

use super::VolatilityTermStructure;

/// Local-volatility term structure.
///
/// Mirrors QuantLib's `LocalVolTermStructure`: concrete curves implement
/// [`local_vol_impl`](Self::local_vol_impl), which is called after range
/// checking and must therefore assume extrapolation is required.
pub trait LocalVolTermStructure: VolatilityTermStructure {
    /// Local volatility calculation hook; range checks have already run.
    fn local_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility>;

    /// Local volatility at a date and underlying level.
    fn local_vol_date(
        &self,
        date: Date,
        underlying_level: Real,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_range_date(date, extrapolate)?;
        self.check_strike(underlying_level, extrapolate)?;
        let t = self.time_from_reference(date)?;
        self.local_vol_impl(t, underlying_level)
    }

    /// Local volatility at a time and underlying level.
    fn local_vol(
        &self,
        t: Time,
        underlying_level: Real,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_range_time(t, extrapolate)?;
        self.check_strike(underlying_level, extrapolate)?;
        self.local_vol_impl(t, underlying_level)
    }
}
