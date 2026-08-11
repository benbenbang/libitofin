//! Year-on-year inflation optionlet volatility.
//!
//! Port of the part of
//! `ql/termstructures/volatility/inflation/yoyinflationoptionletvolatilitystructure.{hpp,cpp}`
//! a year-on-year cap/floor coupon reads: [`YoYOptionletVolatilitySurface`] is
//! the surface a `YoYInflationOptionletCouponPricer` prices against, and
//! [`ConstantYoYOptionletVolatility`] is the flat one.
//!
//! Inflation volatility is quoted against *dates*. The observation lag and the
//! publication period make a date the only unambiguous key, so C++ gives its
//! lagged queries - `volatility(Date)`, `volatility(Period)` and
//! `totalVariance` - no time-based form at all, and says so (`hpp:63`, `:91`).
//! It does carry one raw `volatility(Time, Rate)` (`hpp:75`) that applies no lag
//! or period adjustment; nothing reads it, so the port omits it too.
//!
//! ## Shape
//!
//! The trait carries the three members the pricer calls
//! (`inflationcouponpricer.cpp:99`, `:114-117`) and nothing else. C++ reaches
//! them through `VolatilityTermStructure`, whose `timeFromBase`, `baseLevel`,
//! `checkRange` and tenor-keyed overloads serve the stripping hierarchy rather
//! than the coupon; [`ConstantYoYOptionletVolatility`] implements
//! [`TermStructure`](crate::termstructures::TermStructure) and
//! [`VolatilityTermStructure`](super::VolatilityTermStructure) itself, so a
//! caller holding the concrete surface keeps the whole face and a caller
//! holding `dyn YoYOptionletVolatilitySurface` carries only what it prices with.
//!
//! ## Divergences from QuantLib
//!
//! Every query takes its observation lag explicitly. C++ defaults the argument
//! to the sentinel `Period(-1, Days)` and substitutes the surface's own
//! `observationLag()` for it (`.cpp:98-102`, `:136-139`); the port has no
//! sentinel because it has no default argument to carry one - the pricer passes
//! `Period(0, Days)` verbatim, as C++ does (`inflationcouponpricer.cpp:114-117`),
//! and a caller wanting the surface's lag passes
//! [`observation_lag`](ConstantYoYOptionletVolatility::observation_lag).
//!
//! [`base_date`](YoYOptionletVolatilitySurface::base_date) returns a
//! [`QlResult`] where C++ returns a bare `Date`: it reads the reference date,
//! which under D10 refuses rather than inventing one when no evaluation date is
//! set, and the period snapping refuses a frequency finer than monthly.
//!
//! ## Deferred (visible)
//!
//! Only the flat surface lands here. The stripped and interpolated hierarchy -
//! `InterpolatedYoYOptionletVolatilityCurve`,
//! `KInterpolatedYoYOptionletVolatilitySurface`, the optionlet strippers and
//! `PiecewiseYoYOptionletVolatilityCurve`, all of `ql/experimental/inflation/` -
//! has no port, and neither do the `YoYInflationCapFloor` engines that consume
//! it (`#851`). `baseLevel`, which exists to seed those bootstraps, goes with
//! them.

mod constantyoyoptionletvol;

pub use constantyoyoptionletvol::ConstantYoYOptionletVolatility;

use crate::errors::QlResult;
use crate::patterns::observable::AsObservable;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Volatility};

/// Volatility surface for year-on-year inflation optionlets.
///
/// Mirrors QuantLib's `YoYOptionletVolatilitySurface` over the three members a
/// coupon pricer reads. Held behind a [`Handle`](crate::handle::Handle), so a
/// relinked or notifying surface reprices the coupons written against it.
pub trait YoYOptionletVolatilitySurface: AsObservable {
    /// The date the surface measures its variance from (`baseDate`).
    ///
    /// The reference date pulled back by the surface's own observation lag, and
    /// snapped to the start of its publication period unless the index is
    /// interpolated (`.cpp:51-64`). A coupon fixing on or before it is
    /// determined, and prices as its intrinsic value with no volatility at all.
    ///
    /// # Errors
    ///
    /// When the reference date cannot be resolved, or the surface's frequency
    /// admits no publication period.
    fn base_date(&self) -> QlResult<Date>;

    /// The volatility for an exercise on `date` struck at `strike`, observing
    /// inflation `obs_lag` back (`volatility`).
    ///
    /// # Errors
    ///
    /// When the observed date falls before [`base_date`](Self::base_date), or
    /// `strike` lies outside the surface's strike domain.
    fn volatility(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Volatility>;

    /// The total integrated variance for an exercise on `date` struck at
    /// `strike`, observing inflation `obs_lag` back (`totalVariance`).
    ///
    /// "Total" because it scales the time out of the optionlet formulae without
    /// committing to the distribution reading it: the same figure feeds the
    /// Black, displaced and Bachelier pricers.
    ///
    /// # Errors
    ///
    /// As [`volatility`](Self::volatility), plus a surface carrying no day
    /// counter to measure the elapsed time with.
    fn total_variance(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Real>;
}
