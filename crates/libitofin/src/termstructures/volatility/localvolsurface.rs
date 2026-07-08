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
