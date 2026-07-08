//! Volatility term structures.
//!
//! Port of `ql/termstructures/voltermstructure.{hpp,cpp}` and
//! `ql/termstructures/volatility/equityfx/blackvoltermstructure.{hpp,cpp}`.
//! [`VolatilityTermStructure`] adds the strike domain and the business-day
//! convention to the [`TermStructure`] contract; [`BlackVolTermStructure`]
//! layers the Black volatility and variance queries on top, in both date and
//! time form, range- and strike-checked exactly as the C++ base performs them
//! before dispatching to the implementation hooks.
//!
//! ## Divergences from QuantLib
//!
//! - C++ splits the implementation adapters into two abstract classes:
//!   `BlackVolatilityTermStructure` (derives variance from volatility) and
//!   `BlackVarianceTermStructure` (derives volatility from variance). The
//!   first is folded in here as the default
//!   [`black_variance_impl`](BlackVolTermStructure::black_variance_impl);
//!   volatility-quoted curves implement
//!   [`black_vol_impl`](BlackVolTermStructure::black_vol_impl) alone. The
//!   variance-quoted adapter follows with `BlackVarianceCurve`/`Surface`
//!   (EPIC-4); such curves override both hooks, deriving the volatility as
//!   `sqrt(var(max(t, 1e-5)) / max(t, 1e-5))`.
//! - `smileSection`/`atmLevel` need the smile-section layer and follow with
//!   it; `accept(AcyclicVisitor&)` is not ported (dispatch happens through
//!   the traits).
//! - `QL_ENSURE` on non-decreasing variances becomes an `Err`, per D4.

mod blackconstantvol;

pub use blackconstantvol::BlackConstantVol;

use crate::errors::QlResult;
use crate::termstructures::TermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

/// Volatility term structure.
///
/// Mirrors QuantLib's `VolatilityTermStructure`: the strike domain, the
/// business-day convention used in tenor-to-date conversion, and the
/// strike-range check shared by every volatility query.
pub trait VolatilityTermStructure: TermStructure {
    /// The business day convention used in tenor to date conversion.
    fn business_day_convention(&self) -> BusinessDayConvention;

    /// The minimum strike for which the term structure can return vols.
    fn min_strike(&self) -> Rate;

    /// The maximum strike for which the term structure can return vols.
    fn max_strike(&self) -> Rate;

    /// Period/date conversion, swaption style: the reference date advanced by
    /// `period` on the structure's calendar per its business-day convention.
    fn option_date_from_tenor(&self, period: Period) -> QlResult<Date> {
        let Some(calendar) = self.calendar() else {
            fail!("no calendar provided for this volatility term structure");
        };
        Ok(calendar.advance_by_period(
            self.reference_date()?,
            period,
            self.business_day_convention(),
            false,
        ))
    }

    /// Strike-range check: `strike` must sit inside the curve domain unless
    /// extrapolation applies.
    fn check_strike(&self, strike: Rate, extrapolate: bool) -> QlResult<()> {
        require!(
            extrapolate
                || self.allows_extrapolation()
                || (strike >= self.min_strike() && strike <= self.max_strike()),
            "strike ({strike}) is outside the curve domain [{min},{max}]",
            min = self.min_strike(),
            max = self.max_strike()
        );
        Ok(())
    }
}

/// Black-volatility term structure.
///
/// Mirrors QuantLib's `BlackVolTermStructure`: concrete curves implement
/// [`black_vol_impl`](Self::black_vol_impl) (and, when variance-quoted,
/// [`black_variance_impl`](Self::black_variance_impl)); the provided queries
/// perform the range and strike checks and dispatch to the hooks, which may
/// therefore assume extrapolation is required. Volatilities are expressed on
/// an annual basis.
pub trait BlackVolTermStructure: VolatilityTermStructure {
    /// Black volatility calculation hook; range checks have already run.
    fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility>;

    /// Black variance calculation hook; range checks have already run.
    ///
    /// The default derives the variance from the volatility as `vol^2 * t`
    /// (C++'s `BlackVolatilityTermStructure` adapter).
    fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
        let vol = self.black_vol_impl(t, strike)?;
        Ok(vol * vol * t)
    }

    /// Spot volatility at a date.
    fn black_vol_date(
        &self,
        maturity: Date,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_range_date(maturity, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        let t = self.time_from_reference(maturity)?;
        self.black_vol_impl(t, strike)
    }

    /// Spot volatility at a time.
    fn black_vol(&self, maturity: Time, strike: Real, extrapolate: bool) -> QlResult<Volatility> {
        self.check_range_time(maturity, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        self.black_vol_impl(maturity, strike)
    }

    /// Spot variance at a date.
    fn black_variance_date(
        &self,
        maturity: Date,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Real> {
        self.check_range_date(maturity, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        let t = self.time_from_reference(maturity)?;
        self.black_variance_impl(t, strike)
    }

    /// Spot variance at a time.
    fn black_variance(&self, maturity: Time, strike: Real, extrapolate: bool) -> QlResult<Real> {
        self.check_range_time(maturity, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        self.black_variance_impl(maturity, strike)
    }

    /// Forward (at-the-money) volatility between two dates.
    fn black_forward_vol_dates(
        &self,
        date1: Date,
        date2: Date,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        require!(date1 <= date2, "{date1} later than {date2}");
        self.check_range_date(date2, extrapolate)?;
        let time1 = self.time_from_reference(date1)?;
        let time2 = self.time_from_reference(date2)?;
        self.black_forward_vol(time1, time2, strike, extrapolate)
    }

    /// Forward (at-the-money) volatility between two times.
    fn black_forward_vol(
        &self,
        time1: Time,
        time2: Time,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        if time1 > time2 || time1.is_nan() || time2.is_nan() {
            fail!("{time1} later than {time2}");
        }
        self.check_range_time(time2, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        if time2 == time1 {
            if time1 == 0.0 {
                let epsilon = 1.0e-5;
                let var = self.black_variance_impl(epsilon, strike)?;
                Ok((var / epsilon).sqrt())
            } else {
                let epsilon = Time::min(1.0e-5, time1);
                let var1 = self.black_variance_impl(time1 - epsilon, strike)?;
                let var2 = self.black_variance_impl(time1 + epsilon, strike)?;
                if var2 < var1 || var1.is_nan() || var2.is_nan() {
                    fail!("variances must be non-decreasing");
                }
                Ok(((var2 - var1) / (2.0 * epsilon)).sqrt())
            }
        } else {
            let var1 = self.black_variance_impl(time1, strike)?;
            let var2 = self.black_variance_impl(time2, strike)?;
            if var2 < var1 || var1.is_nan() || var2.is_nan() {
                fail!("variances must be non-decreasing");
            }
            Ok(((var2 - var1) / (time2 - time1)).sqrt())
        }
    }

    /// Forward (at-the-money) variance between two dates.
    fn black_forward_variance_dates(
        &self,
        date1: Date,
        date2: Date,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Real> {
        require!(date1 <= date2, "{date1} later than {date2}");
        self.check_range_date(date2, extrapolate)?;
        let time1 = self.time_from_reference(date1)?;
        let time2 = self.time_from_reference(date2)?;
        self.black_forward_variance(time1, time2, strike, extrapolate)
    }

    /// Forward (at-the-money) variance between two times.
    fn black_forward_variance(
        &self,
        time1: Time,
        time2: Time,
        strike: Real,
        extrapolate: bool,
    ) -> QlResult<Real> {
        if time1 > time2 || time1.is_nan() || time2.is_nan() {
            fail!("{time1} later than {time2}");
        }
        self.check_range_time(time2, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        let v1 = self.black_variance_impl(time1, strike)?;
        let v2 = self.black_variance_impl(time2, strike)?;
        if v2 < v1 || v1.is_nan() || v2.is_nan() {
            fail!("variances must be non-decreasing");
        }
        Ok(v2 - v1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::TermStructureBase;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::timeunit::TimeUnit;

    struct MockVolCurve {
        base: TermStructureBase,
        vol: Volatility,
        variance_override: Option<fn(Time) -> Real>,
        strike_domain: (Rate, Rate),
    }

    impl MockVolCurve {
        fn flat(vol: Volatility) -> MockVolCurve {
            MockVolCurve {
                base: TermStructureBase::with_reference_date(
                    Date::new(15, Month::June, 2026),
                    Some(Target::new()),
                    Some(Actual360::new()),
                ),
                vol,
                variance_override: None,
                strike_domain: (Rate::MIN, Rate::MAX),
            }
        }
    }

    impl AsObservable for MockVolCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for MockVolCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::new(15, Month::June, 2036)
        }
    }

    impl VolatilityTermStructure for MockVolCurve {
        fn business_day_convention(&self) -> BusinessDayConvention {
            BusinessDayConvention::Following
        }

        fn min_strike(&self) -> Rate {
            self.strike_domain.0
        }

        fn max_strike(&self) -> Rate {
            self.strike_domain.1
        }
    }

    impl BlackVolTermStructure for MockVolCurve {
        fn black_vol_impl(&self, _t: Time, _strike: Real) -> QlResult<Volatility> {
            Ok(self.vol)
        }

        fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
            match self.variance_override {
                Some(f) => Ok(f(t)),
                None => {
                    let vol = self.black_vol_impl(t, strike)?;
                    Ok(vol * vol * t)
                }
            }
        }
    }

    #[test]
    fn variance_defaults_to_vol_squared_times_time() {
        let curve = MockVolCurve::flat(0.25);
        let var = curve.black_variance(2.0, 100.0, false).unwrap();
        assert!((var - 0.25 * 0.25 * 2.0).abs() < 1e-15);
    }

    #[test]
    fn forward_vol_of_a_flat_curve_is_the_flat_vol() {
        let curve = MockVolCurve::flat(0.2);
        for (t1, t2) in [(0.5, 1.5), (0.0, 0.0), (1.0, 1.0), (0.0, 2.0)] {
            let fwd = curve.black_forward_vol(t1, t2, 100.0, false).unwrap();
            assert!((fwd - 0.2).abs() < 1e-12, "t1={t1} t2={t2} fwd={fwd}");
        }
    }

    #[test]
    fn decreasing_variances_are_errors() {
        let mut curve = MockVolCurve::flat(0.2);
        curve.variance_override = Some(|t| 0.1 * (10.0 - t));
        let err = curve.black_forward_vol(1.0, 2.0, 100.0, false).unwrap_err();
        assert!(err.message().contains("non-decreasing"));
        let err = curve
            .black_forward_variance(1.0, 2.0, 100.0, false)
            .unwrap_err();
        assert!(err.message().contains("non-decreasing"));
    }

    #[test]
    fn strike_checks_gate_the_curve_domain() {
        let mut curve = MockVolCurve::flat(0.2);
        curve.strike_domain = (90.0, 110.0);

        assert!(curve.black_vol(1.0, 100.0, false).is_ok());
        let err = curve.black_vol(1.0, 80.0, false).unwrap_err();
        assert!(err.message().contains("outside the curve domain"));
        assert!(curve.black_vol(1.0, 80.0, true).is_ok());
        curve.enable_extrapolation();
        assert!(curve.black_vol(1.0, 80.0, false).is_ok());
        curve.disable_extrapolation();
        assert!(curve.black_vol(1.0, Rate::NAN, false).is_err());
    }
    #[test]
    fn forward_variance_of_a_flat_curve_is_additive() {
        let curve = MockVolCurve::flat(0.2);
        let fwd = curve
            .black_forward_variance(1.0, 3.0, 100.0, false)
            .unwrap();
        assert!((fwd - 0.04 * 2.0).abs() < 1e-15);
    }

    #[test]
    fn reversed_times_and_dates_are_errors() {
        let curve = MockVolCurve::flat(0.2);
        let err = curve.black_forward_vol(2.0, 1.0, 100.0, false).unwrap_err();
        assert!(err.message().contains("later than"));

        let reference = Date::new(15, Month::June, 2026);
        let err = curve
            .black_forward_variance_dates(reference + 30, reference + 10, 100.0, false)
            .unwrap_err();
        assert!(err.message().contains("later than"));
    }

    #[test]
    fn range_checks_gate_time_and_date() {
        let curve = MockVolCurve::flat(0.2);
        assert!(curve.black_vol(-0.5, 100.0, false).is_err());
        let before = Date::new(14, Month::June, 2026);
        assert!(curve.black_vol_date(before, 100.0, false).is_err());
        assert!(curve.black_variance_date(before, 100.0, false).is_err());
    }

    #[test]
    fn option_date_from_tenor_advances_swaption_style() {
        let curve = MockVolCurve::flat(0.2);
        let expected = Target::new().advance(
            Date::new(15, Month::June, 2026),
            3,
            TimeUnit::Months,
            BusinessDayConvention::Following,
            false,
        );
        assert_eq!(
            curve
                .option_date_from_tenor(Period::new(3, TimeUnit::Months))
                .unwrap(),
            expected
        );
    }
}
