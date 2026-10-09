//! Black variance rebased to a fixed future reference date.
//!
//! Port of `ql/termstructures/volatility/equityfx/impliedvoltermstructure.hpp`.
//! Variance is the original curve's forward variance over the shifted interval.
//! The underlying handle remains live, including its day counter and domains.
//!
//! As in QuantLib, the implied structure has its own extrapolation flag,
//! no calendar, and the `Following` business-day convention. Internal forward
//! variance queries allow extrapolation, after the public query's own checks.
//! Empty-handle inspectors use `None`, the null maximum date and unbounded
//! strike sentinels; numerical queries return errors rather than panic.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::shared::SharedMut;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

use super::{BlackVolTermStructure, VolatilityTermStructure};

/// Implied Black variance at a fixed future reference date.
///
/// The structure keeps the original curve alive through its handle and
/// forwards change notifications using the base's weak observer registration.
/// No shifted times or variances are cached. Financial use should be restricted
/// to time-dependent curves: rebasing an asset-dependent smile is generally
/// not meaningful, although the numerical interface preserves the strike.
pub struct ImpliedVolTermStructure {
    base: TermStructureBase,
    original: Handle<dyn BlackVolTermStructure>,
}

impl ImpliedVolTermStructure {
    /// Rebase `original` to `reference_date` and observe its live handle.
    ///
    /// The implied reference date does not follow evaluation-date changes.
    /// Construction permits an empty handle, which can be linked later.
    /// A null reference date, missing day counter or empty handle is rejected
    /// when a numerical query needs it, following the term-structure contract.
    pub fn new(original: Handle<dyn BlackVolTermStructure>, reference_date: Date) -> Self {
        let base = TermStructureBase::with_reference_date(reference_date, None, None);
        original.register_observer(&base.updater());
        Self { base, original }
    }
}

impl AsObservable for ImpliedVolTermStructure {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ImpliedVolTermStructure {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn register_upstream(&self, observer: &SharedMut<dyn Observer>) {
        self.original.register_observer(observer);
    }

    fn max_date(&self) -> Date {
        self.original
            .current_link()
            .map(|curve| curve.max_date())
            .unwrap_or_else(|_| Date::null())
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.original
            .current_link()
            .ok()
            .and_then(|curve| curve.day_counter())
    }
}

impl VolatilityTermStructure for ImpliedVolTermStructure {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }

    fn min_strike(&self) -> Rate {
        self.original
            .current_link()
            .map(|curve| curve.min_strike())
            .unwrap_or(Rate::MIN)
    }

    fn max_strike(&self) -> Rate {
        self.original
            .current_link()
            .map(|curve| curve.max_strike())
            .unwrap_or(Rate::MAX)
    }
}

impl BlackVolTermStructure for ImpliedVolTermStructure {
    fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
        let maturity = if t == 0.0 { 1.0e-5 } else { t };
        let variance = self.black_variance_impl(maturity, strike)?;
        Ok((variance / maturity).sqrt())
    }

    fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
        let original = self.original.current_link()?;
        let day_counter = original.require_day_counter()?;
        let shift = day_counter.year_fraction(original.reference_date()?, self.reference_date()?);
        original.black_forward_variance(shift, shift + t, strike, true)
    }
}
