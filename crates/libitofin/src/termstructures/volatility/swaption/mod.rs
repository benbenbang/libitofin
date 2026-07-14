//! Swaption volatility structures.
//!
//! Port of `ql/termstructures/volatility/swaption/`.
//! [`SwaptionVolatilityStructure`] adds the swaption volatility, Black variance
//! and shift queries on top of [`VolatilityTermStructure`]. Unlike the optionlet
//! surface, which is indexed by option date and strike, the swaption surface is
//! two-dimensional: it is indexed by option (exercise) date and swap length as
//! well as strike, range- and strike-checked exactly as the C++ base performs
//! them before dispatching to the volatility hook. The volatility type and shift
//! select the pricing model the surface feeds the swaption engine.
//!
//! ## Divergences from QuantLib
//!
//! - The `smileSection` family and the `smileSectionImpl` hook are not ported:
//!   they need the smile-section layer, which is not in the crate yet. The
//!   required hook is therefore [`volatility_impl`](SwaptionVolatilityStructure::volatility_impl)
//!   alone, mirroring C++'s pure-virtual `volatilityImpl(Time, Time, Rate)`; the
//!   `Date`-based volatility paths convert to time and dispatch to it, as the
//!   C++ inline `volatility(Date, ...)` overloads do.
//! - QuantLib overloads `volatility`, `blackVariance` and `shift` across six
//!   argument shapes each (option tenor/date/time times swap tenor/length).
//!   Rust has no overloading, so the canonical option-date and option-time forms
//!   are ported, plus the option-tenor/swap-tenor convenience form; the remaining
//!   mixed combinations compose these with the already-ported
//!   [`option_date_from_tenor`](VolatilityTermStructure::option_date_from_tenor)
//!   and [`swap_length_tenor`](SwaptionVolatilityStructure::swap_length_tenor)
//!   conversions and are omitted.
//! - QuantLib exposes the lognormal shift through `shift()`, not the optionlet's
//!   `displacement()`; this port follows the swaption source and names it
//!   [`shift`](SwaptionVolatilityStructure::shift).
//! - Only the constant surface is ported in this module. The stripped,
//!   interpolated and cube swaption vol surfaces are deferred.

use crate::errors::QlResult;
use crate::termstructures::volatility::{VolatilityTermStructure, VolatilityType};
use crate::time::date::Date;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

/// Swaption volatility structure.
///
/// Mirrors QuantLib's `SwaptionVolatilityStructure`: concrete surfaces implement
/// [`volatility_impl`](Self::volatility_impl); the provided queries run the swap,
/// range and strike checks and dispatch to it, deriving the Black variance as
/// `volatility^2 * time`. Volatilities are expressed on an annual basis.
pub trait SwaptionVolatilityStructure: VolatilityTermStructure {
    /// Volatility calculation hook; swap, range and strike checks have already
    /// run.
    fn volatility_impl(
        &self,
        option_time: Time,
        swap_length: Time,
        strike: Rate,
    ) -> QlResult<Volatility>;

    /// The largest swap tenor for which the surface can return vols.
    fn max_swap_tenor(&self) -> Period;

    /// The pricing model the quoted volatilities are expressed in.
    fn volatility_type(&self) -> VolatilityType {
        VolatilityType::ShiftedLognormal
    }

    /// Shift calculation hook. The default enforces that a shift only makes
    /// sense for lognormal volatilities and returns `0.0`.
    fn shift_impl(&self, _option_time: Time, _swap_length: Time) -> QlResult<Real> {
        require_lognormal_for_shift(self.volatility_type())?;
        Ok(0.0)
    }

    /// The largest swap length (in time) for which the surface can return vols.
    fn max_swap_length(&self) -> QlResult<Time> {
        self.swap_length_tenor(self.max_swap_tenor())
    }

    /// Conversion between a swap tenor and its swap length in years. Only
    /// month- and year-denominated tenors are meaningful.
    fn swap_length_tenor(&self, swap_tenor: Period) -> QlResult<Time> {
        require!(
            swap_tenor.length() > 0,
            "non-positive swap tenor ({swap_tenor}) given"
        );
        match swap_tenor.units() {
            TimeUnit::Months => Ok(swap_tenor.length() as Time / 12.0),
            TimeUnit::Years => Ok(swap_tenor.length() as Time),
            other => fail!("invalid time unit ({other}) for swap length"),
        }
    }

    /// Conversion between swap start and end dates and swap length in years,
    /// rounded to whole months as QuantLib does with `ClosestRounding(0)`.
    fn swap_length(&self, start: Date, end: Date) -> QlResult<Time> {
        require!(
            end > start,
            "swap end date ({end}) must be greater than start ({start})"
        );
        let months = ((end - start) as Time / 365.25 * 12.0).round();
        Ok(months / 12.0)
    }

    /// Swap-tenor range check: `swap_tenor` must be positive and, unless
    /// extrapolation applies, no longer than [`max_swap_tenor`](Self::max_swap_tenor).
    fn check_swap_tenor(&self, swap_tenor: Period, extrapolate: bool) -> QlResult<()> {
        require!(
            swap_tenor.length() > 0,
            "non-positive swap tenor ({swap_tenor}) given"
        );
        require!(
            extrapolate || self.allows_extrapolation() || swap_tenor <= self.max_swap_tenor(),
            "swap tenor ({swap_tenor}) is past max tenor ({max})",
            max = self.max_swap_tenor()
        );
        Ok(())
    }

    /// Swap-length range check: `swap_length` must be positive and, unless
    /// extrapolation applies, no longer than [`max_swap_length`](Self::max_swap_length).
    fn check_swap_length(&self, swap_length: Time, extrapolate: bool) -> QlResult<()> {
        if swap_length <= 0.0 {
            fail!("non-positive swap length ({swap_length}) given");
        }
        require!(
            extrapolate || self.allows_extrapolation() || swap_length <= self.max_swap_length()?,
            "swap length ({swap_length}) is past max length ({max})",
            max = self.max_swap_length()?
        );
        Ok(())
    }

    /// Volatility for a given option date, swap length and strike rate.
    fn volatility(
        &self,
        option_date: Date,
        swap_length: Time,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_swap_length(swap_length, extrapolate)?;
        self.check_range_date(option_date, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        let option_time = self.time_from_reference(option_date)?;
        self.volatility_impl(option_time, swap_length, strike)
    }

    /// Volatility for a given option time, swap length and strike rate.
    fn volatility_time(
        &self,
        option_time: Time,
        swap_length: Time,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        self.check_swap_length(swap_length, extrapolate)?;
        self.check_range_time(option_time, extrapolate)?;
        self.check_strike(strike, extrapolate)?;
        self.volatility_impl(option_time, swap_length, strike)
    }

    /// Volatility for a given option tenor, swap tenor and strike rate.
    fn volatility_tenors(
        &self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Volatility> {
        let option_date = self.option_date_from_tenor(option_tenor)?;
        let swap_length = self.swap_length_tenor(swap_tenor)?;
        self.volatility(option_date, swap_length, strike, extrapolate)
    }

    /// Black variance for a given option date, swap length and strike rate.
    fn black_variance(
        &self,
        option_date: Date,
        swap_length: Time,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Real> {
        let v = self.volatility(option_date, swap_length, strike, extrapolate)?;
        let option_time = self.time_from_reference(option_date)?;
        Ok(v * v * option_time)
    }

    /// Black variance for a given option time, swap length and strike rate.
    fn black_variance_time(
        &self,
        option_time: Time,
        swap_length: Time,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Real> {
        let v = self.volatility_time(option_time, swap_length, strike, extrapolate)?;
        Ok(v * v * option_time)
    }

    /// Black variance for a given option tenor, swap tenor and strike rate.
    fn black_variance_tenors(
        &self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: Rate,
        extrapolate: bool,
    ) -> QlResult<Real> {
        let option_date = self.option_date_from_tenor(option_tenor)?;
        let swap_length = self.swap_length_tenor(swap_tenor)?;
        self.black_variance(option_date, swap_length, strike, extrapolate)
    }

    /// Lognormal shift for a given option date and swap length.
    fn shift(&self, option_date: Date, swap_length: Time, extrapolate: bool) -> QlResult<Real> {
        self.check_swap_length(swap_length, extrapolate)?;
        self.check_range_date(option_date, extrapolate)?;
        let option_time = self.time_from_reference(option_date)?;
        self.shift_impl(option_time, swap_length)
    }

    /// Lognormal shift for a given option time and swap length.
    fn shift_time(
        &self,
        option_time: Time,
        swap_length: Time,
        extrapolate: bool,
    ) -> QlResult<Real> {
        self.check_swap_length(swap_length, extrapolate)?;
        self.check_range_time(option_time, extrapolate)?;
        self.shift_impl(option_time, swap_length)
    }
}

fn require_lognormal_for_shift(volatility_type: VolatilityType) -> QlResult<()> {
    require!(
        volatility_type == VolatilityType::ShiftedLognormal,
        "shift parameter only makes sense for lognormal volatilities"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::{TermStructure, TermStructureBase};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    struct MockSwaptionVol {
        base: TermStructureBase,
        vol: Volatility,
        volatility_type: VolatilityType,
        shift: Real,
    }

    impl MockSwaptionVol {
        fn flat(vol: Volatility) -> MockSwaptionVol {
            MockSwaptionVol {
                base: TermStructureBase::with_reference_date(
                    Date::new(15, Month::June, 2026),
                    Some(Target::new()),
                    Some(Actual360::new()),
                ),
                vol,
                volatility_type: VolatilityType::ShiftedLognormal,
                shift: 0.0,
            }
        }
    }

    impl AsObservable for MockSwaptionVol {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for MockSwaptionVol {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl VolatilityTermStructure for MockSwaptionVol {
        fn business_day_convention(
            &self,
        ) -> crate::time::businessdayconvention::BusinessDayConvention {
            crate::time::businessdayconvention::BusinessDayConvention::Following
        }

        fn min_strike(&self) -> Rate {
            Rate::MIN
        }

        fn max_strike(&self) -> Rate {
            Rate::MAX
        }
    }

    impl SwaptionVolatilityStructure for MockSwaptionVol {
        fn volatility_impl(&self, _t: Time, _l: Time, _strike: Rate) -> QlResult<Volatility> {
            Ok(self.vol)
        }

        fn max_swap_tenor(&self) -> Period {
            Period::new(100, TimeUnit::Years)
        }

        fn volatility_type(&self) -> VolatilityType {
            self.volatility_type
        }

        fn shift_impl(&self, option_time: Time, swap_length: Time) -> QlResult<Real> {
            require_lognormal_for_shift(self.volatility_type())?;
            let _ = (option_time, swap_length);
            Ok(self.shift)
        }
    }

    #[test]
    fn swap_length_from_tenor_uses_months_and_years() {
        let s = MockSwaptionVol::flat(0.2);
        assert_eq!(
            s.swap_length_tenor(Period::new(6, TimeUnit::Months))
                .unwrap(),
            0.5
        );
        assert_eq!(
            s.swap_length_tenor(Period::new(5, TimeUnit::Years))
                .unwrap(),
            5.0
        );
        assert!(
            s.swap_length_tenor(Period::new(0, TimeUnit::Years))
                .is_err()
        );
        assert!(s.swap_length_tenor(Period::new(7, TimeUnit::Days)).is_err());
    }

    #[test]
    fn swap_length_from_dates_rounds_to_whole_months() {
        let s = MockSwaptionVol::flat(0.2);
        let start = Date::new(15, Month::June, 2026);
        let five_years = s.swap_length(start, start + 5 * 365).unwrap();
        assert!((five_years - 5.0).abs() < 1e-12);
        let one_month = s.swap_length(start, start + 30).unwrap();
        assert!((one_month - 1.0 / 12.0).abs() < 1e-12);
        assert!(s.swap_length(start, start).is_err());
    }

    #[test]
    fn black_variance_is_vol_squared_times_option_time() {
        let s = MockSwaptionVol::flat(0.25);
        let var = s.black_variance_time(2.0, 5.0, 0.03, false).unwrap();
        assert!((var - 0.25 * 0.25 * 2.0).abs() < 1e-15);

        let date = s.reference_date().unwrap() + 180;
        let t = s.time_from_reference(date).unwrap();
        let by_date = s.black_variance(date, 5.0, 0.03, false).unwrap();
        let by_time = s.black_variance_time(t, 5.0, 0.03, false).unwrap();
        assert!((by_date - by_time).abs() < 1e-15);
        assert!((by_date - 0.25 * 0.25 * t).abs() < 1e-15);
    }

    #[test]
    fn tenor_forms_convert_both_axes() {
        let s = MockSwaptionVol::flat(0.2);
        let vol = s
            .volatility_tenors(
                Period::new(1, TimeUnit::Years),
                Period::new(5, TimeUnit::Years),
                0.03,
                false,
            )
            .unwrap();
        assert_eq!(vol, 0.2);
        let var = s
            .black_variance_tenors(
                Period::new(1, TimeUnit::Years),
                Period::new(5, TimeUnit::Years),
                0.03,
                false,
            )
            .unwrap();
        assert!(var > 0.0);
    }

    #[test]
    fn shift_is_gated_by_volatility_type() {
        let mut s = MockSwaptionVol::flat(0.2);
        s.shift = 0.01;
        assert_eq!(
            s.shift(s.reference_date().unwrap() + 90, 5.0, false)
                .unwrap(),
            0.01
        );

        s.volatility_type = VolatilityType::Normal;
        assert!(s.shift_time(1.0, 5.0, false).is_err());
    }

    #[test]
    fn non_positive_swap_length_is_rejected() {
        let s = MockSwaptionVol::flat(0.2);
        assert!(s.volatility_time(1.0, 0.0, 0.03, false).is_err());
        assert!(s.volatility_time(1.0, -1.0, 0.03, false).is_err());
    }

    #[test]
    fn max_swap_length_follows_max_swap_tenor() {
        let s = MockSwaptionVol::flat(0.2);
        assert_eq!(s.max_swap_length().unwrap(), 100.0);
    }
}
