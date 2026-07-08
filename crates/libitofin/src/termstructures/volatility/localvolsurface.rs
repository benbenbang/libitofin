//! Local volatility surface derived from a Black vol surface.
//!
//! Port of `ql/termstructures/volatility/equityfx/localvolsurface.{hpp,cpp}`:
//! the Dupire local volatility in the log-moneyness form of Gatheral's
//! "Stochastic Volatility and Local Volatility" lecture notes, with strike
//! derivatives by symmetric finite differences in `y = ln(strike/forward)`
//! and the time derivative along the forward-moneyness ray. The C++ header
//! carries a `\bug this class is untested, probably unreliable` note; the
//! formula is ported verbatim and locked by tests here.
//!
//! ## Divergences from QuantLib
//!
//! - `QL_ENSURE`s (decreasing variance in time, negative local vol^2) become
//!   `Err`s per D4, NaN-aware the way the C++ comparisons throw on NaN.
//! - Inspectors on an empty Black handle return `None`/`Err`, `max_date` the
//!   null date and the strike bounds NaN (failing every strike check) where
//!   C++ dereferences a null pointer; the C++ constructors dereference it for
//!   the day counter, so construction with empty handles succeeds here.
//! - C++ captures the Black surface's business-day convention at
//!   construction; here it is delegated on each call (falling back to
//!   `Following` when the handle is empty).
//! - `accept(AcyclicVisitor&)` is not ported, following the crate convention.

use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, make_quote_handle};
use crate::termstructures::volatility::{BlackVolTermStructure, VolatilityTermStructure};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

use super::LocalVolTermStructure;

/// Local volatility surface derived from a Black vol surface via the Dupire
/// formula, using the risk-free and dividend curves and the underlying spot.
pub struct LocalVolSurface {
    base: TermStructureBase,
    black_ts: Handle<dyn BlackVolTermStructure>,
    risk_free_ts: Handle<dyn YieldTermStructure>,
    dividend_ts: Handle<dyn YieldTermStructure>,
    underlying: Handle<dyn Quote>,
}

impl LocalVolSurface {
    fn assemble(
        black_ts: Handle<dyn BlackVolTermStructure>,
        risk_free_ts: Handle<dyn YieldTermStructure>,
        dividend_ts: Handle<dyn YieldTermStructure>,
        underlying: Handle<dyn Quote>,
        observe_underlying: bool,
    ) -> LocalVolSurface {
        let base = TermStructureBase::new(None);
        black_ts.register_observer(&base.updater());
        risk_free_ts.register_observer(&base.updater());
        dividend_ts.register_observer(&base.updater());
        if observe_underlying {
            underlying.register_observer(&base.updater());
        }
        LocalVolSurface {
            base,
            black_ts,
            risk_free_ts,
            dividend_ts,
            underlying,
        }
    }

    /// Surface over a Black vol handle, yield handles and a spot quote
    /// handle; changes in any of them notify the structure's observers.
    pub fn new(
        black_ts: Handle<dyn BlackVolTermStructure>,
        risk_free_ts: Handle<dyn YieldTermStructure>,
        dividend_ts: Handle<dyn YieldTermStructure>,
        underlying: Handle<dyn Quote>,
    ) -> LocalVolSurface {
        Self::assemble(black_ts, risk_free_ts, dividend_ts, underlying, true)
    }

    /// Surface over a fixed spot value, wrapped in an unobserved quote as the
    /// C++ `Real` constructor does.
    pub fn with_underlying_value(
        black_ts: Handle<dyn BlackVolTermStructure>,
        risk_free_ts: Handle<dyn YieldTermStructure>,
        dividend_ts: Handle<dyn YieldTermStructure>,
        underlying: Real,
    ) -> LocalVolSurface {
        Self::assemble(
            black_ts,
            risk_free_ts,
            dividend_ts,
            make_quote_handle(underlying).handle(),
            false,
        )
    }

    fn ensure_non_decreasing(
        w_earlier: Real,
        w_later: Real,
        strike: Real,
        t_earlier: Time,
        t_later: Time,
    ) -> QlResult<()> {
        if w_later < w_earlier || w_earlier.is_nan() || w_later.is_nan() {
            fail!(
                "decreasing variance at strike {strike} between time {t_earlier} and time {t_later}"
            );
        }
        Ok(())
    }
}

impl AsObservable for LocalVolSurface {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for LocalVolSurface {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.black_ts.current_link()?.reference_date()
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.black_ts
            .current_link()
            .ok()
            .and_then(|black| black.day_counter())
    }

    fn max_date(&self) -> Date {
        self.black_ts
            .current_link()
            .map(|black| black.max_date())
            .unwrap_or_else(|_| Date::null())
    }
}

impl VolatilityTermStructure for LocalVolSurface {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.black_ts
            .current_link()
            .map(|black| black.business_day_convention())
            .unwrap_or(BusinessDayConvention::Following)
    }

    fn min_strike(&self) -> Rate {
        self.black_ts
            .current_link()
            .map(|black| black.min_strike())
            .unwrap_or(Rate::NAN)
    }

    fn max_strike(&self) -> Rate {
        self.black_ts
            .current_link()
            .map(|black| black.max_strike())
            .unwrap_or(Rate::NAN)
    }
}

impl LocalVolTermStructure for LocalVolSurface {
    fn local_vol_impl(&self, t: Time, underlying_level: Real) -> QlResult<Volatility> {
        let black = self.black_ts.current_link()?;
        let risk_free = self.risk_free_ts.current_link()?;
        let dividend = self.dividend_ts.current_link()?;

        let dr = risk_free.discount(t, true)?;
        let dq = dividend.discount(t, true)?;
        let forward_value = self.underlying.current_link()?.value()? * dq / dr;

        let strike = underlying_level;
        let y = (strike / forward_value).ln();
        let dy = if y.abs() > 0.001 {
            y * 0.0001
        } else {
            0.000001
        };
        let strikep = strike * dy.exp();
        let strikem = strike / dy.exp();
        let w = black.black_variance(t, strike, true)?;
        let wp = black.black_variance(t, strikep, true)?;
        let wm = black.black_variance(t, strikem, true)?;
        let dwdy = (wp - wm) / (2.0 * dy);
        let d2wdy2 = (wp - 2.0 * w + wm) / (dy * dy);

        let dwdt = if t == 0.0 {
            let dt = 0.0001;
            let drpt = risk_free.discount(t + dt, true)?;
            let dqpt = dividend.discount(t + dt, true)?;
            let strikept = strike * dr * dqpt / (drpt * dq);

            let wpt = black.black_variance(t + dt, strikept, true)?;
            Self::ensure_non_decreasing(w, wpt, strike, t, t + dt)?;
            (wpt - w) / dt
        } else {
            let dt = Time::min(0.0001, t / 2.0);
            let drpt = risk_free.discount(t + dt, true)?;
            let drmt = risk_free.discount(t - dt, true)?;
            let dqpt = dividend.discount(t + dt, true)?;
            let dqmt = dividend.discount(t - dt, true)?;

            let strikept = strike * dr * dqpt / (drpt * dq);
            let strikemt = strike * dr * dqmt / (drmt * dq);

            let wpt = black.black_variance(t + dt, strikept, true)?;
            let wmt = black.black_variance(t - dt, strikemt, true)?;

            Self::ensure_non_decreasing(w, wpt, strike, t, t + dt)?;
            Self::ensure_non_decreasing(wmt, w, strike, t - dt, t)?;

            (wpt - wmt) / (2.0 * dt)
        };

        if dwdy == 0.0 && d2wdy2 == 0.0 {
            Ok(dwdt.sqrt())
        } else {
            let den1 = 1.0 - y / w * dwdy;
            let den2 = 0.25 * (-0.25 - 1.0 / w + y * y / w / w) * dwdy * dwdy;
            let den3 = 0.5 * d2wdy2;
            let den = den1 + den2 + den3;
            let result = dwdt / den;

            if result < 0.0 || result.is_nan() {
                fail!(
                    "negative local vol^2 at strike {strike} and time {t}; the black vol surface is not smooth enough"
                );
            }
            Ok(result.sqrt())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use crate::termstructures::volatility::BlackConstantVol;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Real) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::new(
            reference(),
            make_quote_handle(rate).handle(),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn flat_black(vol: Volatility) -> Handle<dyn BlackVolTermStructure> {
        Handle::new(shared(BlackConstantVol::new(
            reference(),
            None,
            vol,
            Actual365Fixed::new(),
        )) as Shared<dyn BlackVolTermStructure>)
    }

    /// Analytic smile surface w(t, y) = t (a + b y^2) with y measured
    /// against a fixed anchor forward, so the Dupire pieces have closed
    /// forms to compare the finite differences against.
    struct SmileSurface {
        base: TermStructureBase,
        a: Real,
        b: Real,
        anchor: Real,
    }

    impl SmileSurface {
        fn new(a: Real, b: Real, anchor: Real) -> SmileSurface {
            SmileSurface {
                base: TermStructureBase::with_reference_date(
                    reference(),
                    None,
                    Some(Actual365Fixed::new()),
                ),
                a,
                b,
                anchor,
            }
        }

        fn handle(a: Real, b: Real, anchor: Real) -> Handle<dyn BlackVolTermStructure> {
            Handle::new(shared(SmileSurface::new(a, b, anchor)) as Shared<dyn BlackVolTermStructure>)
        }
    }

    impl AsObservable for SmileSurface {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for SmileSurface {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl VolatilityTermStructure for SmileSurface {
        fn business_day_convention(&self) -> BusinessDayConvention {
            BusinessDayConvention::Following
        }

        fn min_strike(&self) -> Rate {
            Rate::MIN
        }

        fn max_strike(&self) -> Rate {
            Rate::MAX
        }
    }

    impl BlackVolTermStructure for SmileSurface {
        fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
            let non_zero = if t == 0.0 { 0.00001 } else { t };
            Ok((self.black_variance_impl(non_zero, strike)? / non_zero).sqrt())
        }

        fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
            let y = (strike / self.anchor).ln();
            Ok(t * (self.a + self.b * y * y))
        }
    }

    fn dupire_surface(black: Handle<dyn BlackVolTermStructure>, spot: Real) -> LocalVolSurface {
        LocalVolSurface::new(
            black,
            flat_yield(0.0),
            flat_yield(0.0),
            Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>),
        )
    }

    #[test]
    fn flat_black_vol_gives_a_flat_local_vol() {
        let surface = LocalVolSurface::new(
            flat_black(0.2),
            flat_yield(0.05),
            flat_yield(0.02),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
        );
        for t in [0.0, 0.25, 1.0, 5.0] {
            for strike in [50.0, 100.0, 150.0] {
                let vol = surface.local_vol(t, strike, false).unwrap();
                assert!((vol - 0.2).abs() < 1.0e-10, "t={t} k={strike} vol={vol}");
            }
        }
    }

    #[test]
    fn dupire_matches_the_analytic_derivatives_on_a_smile() {
        // With r = q = 0 the forward is the spot and the moneyness shift of
        // the time differencing vanishes, so for w(t, y) = t (a + b y^2):
        // dw/dt = a + b y^2, dw/dy = 2 b t y, d2w/dy2 = 2 b t, evaluated
        // exactly; the port's finite differences must agree to their own
        // truncation error.
        let (a, b, spot) = (0.04, 0.2, 100.0);
        let surface = dupire_surface(SmileSurface::handle(a, b, spot), spot);
        for (t, strike) in [(0.5, 100.0), (1.0, 110.0), (2.0, 80.0), (0.5, 130.0)] {
            let y = (strike / spot as Real).ln();
            let w = t * (a + b * y * y);
            let dwdt = a + b * y * y;
            let dwdy = 2.0 * b * t * y;
            let d2wdy2 = 2.0 * b * t;
            let expected = if dwdy == 0.0 && d2wdy2 == 0.0 {
                dwdt.sqrt()
            } else {
                let den1 = 1.0 - y / w * dwdy;
                let den2 = 0.25 * (-0.25 - 1.0 / w + y * y / w / w) * dwdy * dwdy;
                let den3 = 0.5 * d2wdy2;
                (dwdt / (den1 + den2 + den3)).sqrt()
            };
            let vol = surface.local_vol(t, strike, false).unwrap();
            assert!(
                (vol - expected).abs() < 1.0e-7,
                "t={t} k={strike} vol={vol} expected={expected}"
            );
        }
    }

    #[test]
    fn fixed_underlying_constructor_matches_the_quoted_one() {
        let (a, b, spot) = (0.04, 0.2, 100.0);
        let quoted = dupire_surface(SmileSurface::handle(a, b, spot), spot);
        let fixed = LocalVolSurface::with_underlying_value(
            SmileSurface::handle(a, b, spot),
            flat_yield(0.0),
            flat_yield(0.0),
            spot,
        );
        let q = quoted.local_vol(1.0, 110.0, false).unwrap();
        let f = fixed.local_vol(1.0, 110.0, false).unwrap();
        assert_eq!(q, f);
    }

    #[test]
    fn decreasing_variance_in_time_is_an_error() {
        struct DecreasingSurface {
            inner: SmileSurface,
        }
        impl AsObservable for DecreasingSurface {
            fn observable(&self) -> &Observable {
                self.inner.observable()
            }
        }
        impl TermStructure for DecreasingSurface {
            fn base(&self) -> &TermStructureBase {
                self.inner.base()
            }
            fn max_date(&self) -> Date {
                Date::max_date()
            }
        }
        impl VolatilityTermStructure for DecreasingSurface {
            fn business_day_convention(&self) -> BusinessDayConvention {
                BusinessDayConvention::Following
            }
            fn min_strike(&self) -> Rate {
                Rate::MIN
            }
            fn max_strike(&self) -> Rate {
                Rate::MAX
            }
        }
        impl BlackVolTermStructure for DecreasingSurface {
            fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
                let non_zero = if t == 0.0 { 0.00001 } else { t };
                Ok((self.black_variance_impl(non_zero, strike)? / non_zero).sqrt())
            }
            fn black_variance_impl(&self, t: Time, _strike: Real) -> QlResult<Real> {
                Ok(0.04 * t * (2.0 - t))
            }
        }

        let black = Handle::new(shared(DecreasingSurface {
            inner: SmileSurface::new(0.0, 0.0, 100.0),
        }) as Shared<dyn BlackVolTermStructure>);
        let surface = dupire_surface(black, 100.0);
        let err = surface.local_vol(1.5, 100.0, false).unwrap_err();
        assert!(err.message().contains("decreasing variance"));
    }

    #[test]
    fn negative_local_variance_is_an_error() {
        // b < 0 makes d2w/dy2 negative enough to flip the denominator sign
        // while dw/dt stays positive, so local vol^2 goes negative.
        let surface = dupire_surface(SmileSurface::handle(0.04, -1.2, 100.0), 100.0);
        let err = surface.local_vol(1.0, 105.0, false).unwrap_err();
        assert!(err.message().contains("negative local vol^2"));
    }

    #[test]
    fn inspectors_delegate_and_empty_handles_error() {
        let surface = LocalVolSurface::new(
            flat_black(0.2),
            flat_yield(0.05),
            flat_yield(0.02),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
        );
        assert_eq!(surface.reference_date().unwrap(), reference());
        assert_eq!(surface.max_date(), Date::max_date());
        assert_eq!(
            surface.day_counter().unwrap().name(),
            Actual365Fixed::new().name()
        );
        assert_eq!(surface.min_strike(), Rate::MIN);
        assert_eq!(surface.max_strike(), Rate::MAX);

        let empty = LocalVolSurface::new(
            Handle::empty(),
            flat_yield(0.0),
            flat_yield(0.0),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
        );
        assert!(empty.reference_date().is_err());
        assert!(empty.day_counter().is_none());
        assert_eq!(empty.max_date(), Date::null());
        assert!(empty.min_strike().is_nan());
        assert!(empty.local_vol(1.0, 100.0, true).is_err());
    }

    #[test]
    fn spot_and_black_changes_notify_observers() {
        let spot = shared(SimpleQuote::new(100.0));
        let surface = LocalVolSurface::new(
            flat_black(0.2),
            flat_yield(0.05),
            flat_yield(0.02),
            Handle::new(spot.clone() as Shared<dyn Quote>),
        );
        let flag = Flag::new();
        surface.observable().register_observer(&as_observer(&flag));

        spot.set_value(105.0);
        assert!(Flag::is_up(&flag));
    }
}
