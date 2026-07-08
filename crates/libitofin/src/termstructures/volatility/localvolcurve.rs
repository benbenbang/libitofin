//! Local volatility curve derived from a Black curve.
//!
//! Port of `ql/termstructures/volatility/equityfx/localvolcurve.hpp`: from
//! the relation `integral of sigma_L^2(t) dt over [0, T] = sigma_B^2(T) T`,
//! the local volatility is `sqrt(d(sigma_B^2(t) t)/dt)`, differentiated here
//! with C++'s one-day forward step on the Black variance.
//!
//! ## Divergences from QuantLib
//!
//! - Inspectors on an empty underlying handle return `None`/`Err` (and
//!   [`max_date`](crate::termstructures::TermStructure::max_date) the null
//!   date) where C++ dereferences a null pointer.
//! - C++ captures the underlying's business-day convention at construction;
//!   here it is delegated on each call (falling back to `Following`, the only
//!   value a `BlackVarianceCurve` ever reports, when the handle is empty).
//! - As in C++, a variance decreasing in time (possible only when the curve
//!   was built with `force_monotone_variance` disabled) is not guarded here:
//!   the square root then yields a NaN volatility.
//! - `accept(AcyclicVisitor&)` is not ported, following the crate convention.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::Interpolator;
use crate::math::interpolations::linear::Linear;
use crate::patterns::observable::{AsObservable, Observable};
use crate::termstructures::volatility::{
    BlackVarianceCurve, BlackVolTermStructure, VolatilityTermStructure,
};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

use super::LocalVolTermStructure;

/// Local volatility curve derived from a Black variance curve.
pub struct LocalVolCurve<I: Interpolator + 'static = Linear> {
    base: TermStructureBase,
    curve: Handle<BlackVarianceCurve<I>>,
}

impl<I: Interpolator + 'static> LocalVolCurve<I> {
    /// Wraps the Black variance curve handle, registering with it so relinks
    /// and underlying changes reach this structure's observers.
    pub fn new(curve: Handle<BlackVarianceCurve<I>>) -> LocalVolCurve<I> {
        let base = TermStructureBase::new(None);
        curve.register_observer(&base.updater());
        LocalVolCurve { base, curve }
    }
}

impl<I: Interpolator + 'static> AsObservable for LocalVolCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator + 'static> TermStructure for LocalVolCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.curve.current_link()?.reference_date()
    }

    fn calendar(&self) -> Option<Calendar> {
        self.curve.current_link().ok().and_then(|c| c.calendar())
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.curve.current_link().ok().and_then(|c| c.day_counter())
    }

    fn max_date(&self) -> Date {
        self.curve
            .current_link()
            .map(|c| c.max_date())
            .unwrap_or_else(|_| Date::null())
    }
}

impl<I: Interpolator + 'static> VolatilityTermStructure for LocalVolCurve<I> {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.curve
            .current_link()
            .map(|c| c.business_day_convention())
            .unwrap_or(BusinessDayConvention::Following)
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl<I: Interpolator + 'static> LocalVolTermStructure for LocalVolCurve<I> {
    fn local_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
        let curve = self.curve.current_link()?;
        let dt = 1.0 / 365.0;
        let var1 = curve.black_variance(t, strike, true)?;
        let var2 = curve.black_variance(t + dt, strike, true)?;
        let derivative = (var2 - var1) / dt;
        Ok(derivative.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    fn variance_curve() -> BlackVarianceCurve {
        let reference = Date::new(15, Month::June, 2026);
        BlackVarianceCurve::new(
            reference,
            &[reference + 365, reference + 730],
            &[0.2, 0.25],
            Actual365Fixed::new(),
            true,
        )
        .unwrap()
    }

    fn local_curve() -> LocalVolCurve {
        LocalVolCurve::new(Handle::new(shared(variance_curve())))
    }

    #[test]
    fn local_vol_is_the_square_root_of_the_variance_slope() {
        let local = local_curve();
        // Variance nodes: 0 at t=0, 0.04 at t=1, 0.125 at t=2; linear in
        // between, so the slope is 0.04 on [0,1] and 0.085 on [1,2].
        assert!((local.local_vol(0.5, 100.0, false).unwrap() - 0.2).abs() < 1.0e-12);
        let slope = 0.085_f64;
        assert!((local.local_vol(1.5, 100.0, false).unwrap() - slope.sqrt()).abs() < 1.0e-12);
        // At a node the one-day step reads the next segment forward.
        assert!((local.local_vol(1.0, 100.0, false).unwrap() - slope.sqrt()).abs() < 1.0e-12);
    }

    #[test]
    fn beyond_the_last_node_flat_vol_extrapolation_gives_the_last_vol() {
        let local = local_curve();
        // Flat-volatility extension: var(t) = 0.25^2 t, so the slope is
        // 0.25^2 and the local vol equals the last Black vol.
        assert!((local.local_vol(2.0, 100.0, false).unwrap() - 0.25).abs() < 1.0e-12);
    }

    #[test]
    fn local_vol_matches_the_one_day_forward_vol_of_the_underlying() {
        let local = local_curve();
        let underlying = variance_curve();
        // The forward vol divides by the rounded difference (t + dt) - t, the
        // local vol by dt itself, so agreement is to float precision only.
        for t in [0.0, 0.3, 1.0, 1.7] {
            let expected = underlying
                .black_forward_vol(t, t + 1.0 / 365.0, 100.0, true)
                .unwrap();
            assert!((local.local_vol(t, 100.0, false).unwrap() - expected).abs() < 1.0e-10);
        }
    }

    #[test]
    fn inspectors_delegate_to_the_underlying_curve() {
        let local = local_curve();
        let underlying = variance_curve();
        assert_eq!(
            local.reference_date().unwrap(),
            underlying.reference_date().unwrap()
        );
        assert_eq!(local.max_date(), underlying.max_date());
        assert_eq!(
            local.day_counter().unwrap().name(),
            underlying.day_counter().unwrap().name()
        );
        assert_eq!(
            local.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert_eq!(local.min_strike(), Rate::MIN);
        assert_eq!(local.max_strike(), Rate::MAX);
    }

    #[test]
    fn empty_handle_errors_instead_of_dereferencing_null() {
        let local: LocalVolCurve = LocalVolCurve::new(Handle::empty());
        assert!(local.reference_date().is_err());
        assert!(local.day_counter().is_none());
        assert_eq!(local.max_date(), Date::null());
        assert!(local.local_vol(1.0, 100.0, true).is_err());
    }

    #[test]
    fn relinking_the_underlying_notifies_observers() {
        let relinkable = RelinkableHandle::new(shared(variance_curve()));
        let local = LocalVolCurve::new(relinkable.handle());
        let flag = Flag::new();
        local.observable().register_observer(&as_observer(&flag));

        let reference = Date::new(15, Month::June, 2026);
        let steeper = BlackVarianceCurve::new(
            reference,
            &[reference + 365, reference + 730],
            &[0.3, 0.35],
            Actual365Fixed::new(),
            true,
        )
        .unwrap();
        relinkable.link_to(shared(steeper));

        assert!(Flag::is_up(&flag));
        assert!((local.local_vol(0.5, 100.0, false).unwrap() - 0.3).abs() < 1.0e-12);
    }
}
