//! Inflation term structures.
//!
//! Port of `ql/termstructures/inflationtermstructure.{hpp,cpp}`:
//! [`InflationTermStructureBase`] is the shared holder concrete inflation
//! curves embed (C++'s data members), [`InflationTermStructure`] is the
//! inflation contract over [`TermStructure`], and
//! [`ZeroInflationTermStructure`] adds the zero-coupon inflation rate derived
//! from the single required
//! [`zero_rate_impl`](ZeroInflationTermStructure::zero_rate_impl).
//!
//! ## Divergences from QuantLib
//!
//! - `observationLag_` / `observationLag()` are not ported: they are
//!   deprecated at the curve level since QuantLib 1.39
//!   (`inflationtermstructure.hpp:68-73,82-89,107-112`), the lag being an
//!   observation property of the helpers and instruments rather than of the
//!   curve. `hasExplicitBaseDate()` (`inflationtermstructure.hpp:83-89`),
//!   deprecated alongside them and a constant `true`, is dropped with them.
//! - The deprecated four-argument `zeroRate(d, instObsLag,
//!   forceLinearInterpolation, extrapolate)`
//!   (`inflationtermstructure.hpp:150-155`) is not ported, and with it the
//!   `forceLinearInterpolation` branch that interpolates between the two
//!   period ends (`inflationtermstructure.cpp:147-159`). The ported
//!   [`zero_rate_date`](ZeroInflationTermStructure::zero_rate_date) is the
//!   live two-argument overload, which delegates with a zero lag and no
//!   forced interpolation (`inflationtermstructure.cpp:134-138`), so it takes
//!   the `else` branch only.
//! - Seasonality (`seasonality_`, `setSeasonality`, `seasonality()`,
//!   `hasSeasonality()` and the `correctZeroRate` fold at
//!   `inflationtermstructure.cpp:170-172`) is not ported. Every use of it in
//!   this file is guarded by `hasSeasonality()`, which is false for a curve
//!   built without one, so the omission is behaviour-free for the curves
//!   landing now; it follows with the seasonality classes in EPIC Inflation
//!   (#705). The accessors are omitted rather than stubbed to a constant
//!   `false`, so a curve needing seasonality fails to compile instead of
//!   silently skipping the correction.
//! - C++'s `checkRange` overloads *hide* `TermStructure::checkRange`; Rust
//!   traits have no name hiding, so they are ported under distinct names
//!   ([`check_inflation_range_date`](InflationTermStructure::check_inflation_range_date)
//!   and
//!   [`check_inflation_range_time`](InflationTermStructure::check_inflation_range_time)).
//!   An inflation curve must call those, never the
//!   [`TermStructure`] ones, whose lower bounds are strictly tighter.
//! - C++ overloads on `Date`/`Time` become distinct method names, the `_date`
//!   suffix taking dates.

use crate::errors::QlResult;
use crate::indexes::inflationindex::inflation_period;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Natural, Rate, Time};
use crate::{fail, require};

/// Shared base holder for inflation term structures.
///
/// Concrete curves embed one alongside the [`TermStructureBase`] it wraps and
/// delegate [`InflationTermStructure::inflation_base`] to it
/// (`inflationtermstructure.hpp:98-118`).
pub struct InflationTermStructureBase {
    base: TermStructureBase,
    frequency: Frequency,
    base_rate: Option<Rate>,
    base_date: Date,
}

impl InflationTermStructureBase {
    /// Holder for a curve that manages its own reference date by overriding
    /// [`TermStructure::reference_date`] (`inflationtermstructure.cpp:28-40`).
    ///
    /// A curve built this way *must* override that method: every
    /// `time_from_reference` call, and so both range checks, errors until it
    /// does.
    pub fn new(
        base_date: Date,
        frequency: Frequency,
        day_counter: Option<DayCounter>,
        base_rate: Option<Rate>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::new(day_counter),
            frequency,
            base_rate,
            base_date,
        }
    }

    /// Holder with a fixed reference date
    /// (`inflationtermstructure.cpp:42-55`; C++ passes an empty calendar).
    pub fn with_reference_date(
        reference_date: Date,
        base_date: Date,
        frequency: Frequency,
        day_counter: Option<DayCounter>,
        base_rate: Option<Rate>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::with_reference_date(reference_date, None, day_counter),
            frequency,
            base_rate,
            base_date,
        }
    }

    /// Holder whose reference date moves off the evaluation date
    /// (`inflationtermstructure.cpp:57-70`); the [`Settings`] handle is
    /// explicit per D5.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        base_date: Date,
        frequency: Frequency,
        day_counter: Option<DayCounter>,
        base_rate: Option<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::moving(settlement_days, calendar, day_counter, settings),
            frequency,
            base_rate,
            base_date,
        }
    }

    /// The wrapped term-structure holder.
    pub fn term_structure_base(&self) -> &TermStructureBase {
        &self.base
    }
}

/// Inflation term structure.
///
/// Mirrors QuantLib's `InflationTermStructure`
/// (`inflationtermstructure.hpp:36-118`): concrete curves supply
/// [`inflation_base`](Self::inflation_base) and inherit the inspectors and
/// the two range checks.
pub trait InflationTermStructure: TermStructure {
    /// The embedded shared holder.
    fn inflation_base(&self) -> &InflationTermStructureBase;

    /// The frequency of the inflation fixings the curve is built on
    /// (`inflationtermstructure.hpp:261-263`).
    fn frequency(&self) -> Frequency {
        self.inflation_base().frequency
    }

    /// The minimum (base) date: the last date for which the fixing is known
    /// (`inflationtermstructure.cpp:72-74`). Curves that cannot supply it at
    /// construction override this (C++ declares it `virtual`,
    /// `inflationtermstructure.hpp:79`).
    fn base_date(&self) -> Date {
        self.inflation_base().base_date
    }

    /// The base rate, an error where C++ has `Null<Rate>`
    /// (`inflationtermstructure.hpp:265-268`); zero curves carry none.
    fn base_rate(&self) -> QlResult<Rate> {
        match self.inflation_base().base_rate {
            Some(rate) => Ok(rate),
            None => fail!("base rate not available"),
        }
    }

    /// Date-range check (`inflationtermstructure.cpp:91-98`): `date` must not
    /// precede the *base* date nor, unless extrapolation applies, exceed the
    /// maximum date.
    ///
    /// The lower bound is strictly looser than
    /// [`TermStructure::check_range_date`]'s: the base date precedes the
    /// reference date, so dates in `[base_date, reference_date)` are valid
    /// here and rejected there.
    fn check_inflation_range_date(&self, date: Date, extrapolate: bool) -> QlResult<()> {
        let base_date = self.base_date();
        require!(
            date >= base_date,
            "date ({date}) is before base date ({base_date})"
        );
        require!(
            extrapolate || self.allows_extrapolation() || date <= self.max_date(),
            "date ({date}) is past max curve date ({max})",
            max = self.max_date()
        );
        Ok(())
    }

    /// Time-range check (`inflationtermstructure.cpp:100-107`): `t` must not
    /// precede the base date's time nor, unless extrapolation applies, exceed
    /// the maximum time.
    ///
    /// That lower bound is *negative* (the base date precedes the reference
    /// date), so this accepts times [`TermStructure::check_range_time`]
    /// rejects outright. The bounds are spelled as the negations C++'s
    /// `QL_REQUIRE`s reduce to, so `NaN` fails the lower one exactly as it
    /// does there. Unlike the sibling check this compares against the maximum
    /// time exactly, as the C++ does.
    fn check_inflation_range_time(&self, t: Time, extrapolate: bool) -> QlResult<()> {
        let base_time = self.time_from_reference(self.base_date())?;
        if t < base_time || t.is_nan() {
            fail!("time ({t}) is before base date");
        }
        if extrapolate || self.allows_extrapolation() {
            return Ok(());
        }
        let max_time = self.max_time()?;
        if t > max_time {
            fail!("time ({t}) is past max curve time ({max_time})");
        }
        Ok(())
    }
}

/// Zero-coupon inflation term structure.
///
/// Mirrors QuantLib's `ZeroInflationTermStructure`
/// (`inflationtermstructure.hpp:121-177`): concrete curves implement
/// [`zero_rate_impl`](Self::zero_rate_impl) (called after range checking, so
/// it must assume extrapolation is required) and inherit the rest. A zero
/// curve carries no base rate, so its holder takes `None` and
/// [`base_rate`](InflationTermStructure::base_rate) errors
/// (`inflationtermstructure.cpp:110-131`).
pub trait ZeroInflationTermStructure: InflationTermStructure {
    /// Zero-rate calculation, implemented by concrete curves.
    fn zero_rate_impl(&self, t: Time) -> QlResult<Rate>;

    /// The zero-coupon inflation rate for `date`, on yearly compounding as
    /// ZCIIS quotes assume (`inflationtermstructure.cpp:134-138` delegating
    /// to the `else` branch at `:164-168`).
    ///
    /// The query date is quantized to the start of its inflation period
    /// before it reaches the curve: an inflation fixing applies to a whole
    /// period, so every date inside one must yield that period's rate. The
    /// range check and the time conversion both use the quantized date.
    fn zero_rate_date(&self, date: Date, extrapolate: bool) -> QlResult<Rate> {
        let (period_start, _) = inflation_period(date, self.frequency())?;
        self.check_inflation_range_date(period_start, extrapolate)?;
        let t = self.time_from_reference(period_start)?;
        self.zero_rate_impl(t)
    }

    /// The zero-coupon inflation rate for time `t`
    /// (`inflationtermstructure.cpp:176-180`).
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure. Inflation being tightly bound to dates (lags,
    /// interpolation, seasonality), this accounts for none of those effects:
    /// the caller manages them.
    fn zero_rate(&self, t: Time, extrapolate: bool) -> QlResult<Rate> {
        self.check_inflation_range_time(t, extrapolate)?;
        self.zero_rate_impl(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    struct TestCurve {
        inflation: InflationTermStructureBase,
        max: Date,
    }

    fn reference() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn base_date() -> Date {
        Date::new(1, Month::December, 2025)
    }

    fn curve_with(frequency: Frequency, base_rate: Option<Rate>) -> TestCurve {
        TestCurve {
            inflation: InflationTermStructureBase::with_reference_date(
                reference(),
                base_date(),
                frequency,
                Some(Actual360::new()),
                base_rate,
            ),
            max: Date::new(15, Month::January, 2036),
        }
    }

    fn curve(base_rate: Option<Rate>) -> TestCurve {
        curve_with(Frequency::Monthly, base_rate)
    }

    impl AsObservable for TestCurve {
        fn observable(&self) -> &Observable {
            self.inflation.term_structure_base().observable()
        }
    }

    impl TermStructure for TestCurve {
        fn base(&self) -> &TermStructureBase {
            self.inflation.term_structure_base()
        }

        fn max_date(&self) -> Date {
            self.max
        }
    }

    impl InflationTermStructure for TestCurve {
        fn inflation_base(&self) -> &InflationTermStructureBase {
            &self.inflation
        }
    }

    impl ZeroInflationTermStructure for TestCurve {
        fn zero_rate_impl(&self, t: Time) -> QlResult<Rate> {
            Ok(t)
        }
    }

    #[test]
    fn date_range_check_bounds_on_the_base_date_not_the_reference_date() {
        let curve = curve(None);
        let between = Date::new(15, Month::December, 2025);

        assert!(curve.check_inflation_range_date(base_date(), false).is_ok());
        assert!(curve.check_inflation_range_date(between, false).is_ok());
        assert!(TermStructure::check_range_date(&curve, between, false).is_err());

        let before = curve
            .check_inflation_range_date(base_date() - 1, false)
            .unwrap_err();
        assert!(before.message().contains("is before base date"));
    }

    #[test]
    fn date_range_check_enforces_the_max_date_unless_extrapolating() {
        let curve = curve(None);

        assert!(curve.check_inflation_range_date(curve.max, false).is_ok());
        let past = curve
            .check_inflation_range_date(curve.max + 1, false)
            .unwrap_err();
        assert!(past.message().contains("past max curve date"));

        assert!(
            curve
                .check_inflation_range_date(curve.max + 1, true)
                .is_ok()
        );
        curve.enable_extrapolation();
        assert!(
            curve
                .check_inflation_range_date(curve.max + 1, false)
                .is_ok()
        );
    }

    #[test]
    fn time_range_check_bounds_on_the_negative_base_date_time() {
        let curve = curve(None);
        let base_time = curve.time_from_reference(base_date()).unwrap();
        assert_eq!(base_time, -0.125);

        assert!(curve.check_inflation_range_time(base_time, false).is_ok());
        assert!(curve.check_inflation_range_time(-0.1, false).is_ok());
        assert!(TermStructure::check_range_time(&curve, base_time, false).is_err());

        let before = curve
            .check_inflation_range_time(base_time - 0.001, false)
            .unwrap_err();
        assert!(before.message().contains("is before base date"));
        assert!(curve.check_inflation_range_time(Time::NAN, true).is_err());

        let past = curve
            .check_inflation_range_time(curve.max_time().unwrap() + 1.0, false)
            .unwrap_err();
        assert!(past.message().contains("past max curve time"));
    }

    #[test]
    fn zero_rate_date_quantizes_to_the_start_of_the_inflation_period() {
        let curve = curve(None);
        let mid_month = Date::new(15, Month::March, 2026);
        let period_start = Date::new(1, Month::March, 2026);

        let quantized = curve.time_from_reference(period_start).unwrap();
        let unquantized = curve.time_from_reference(mid_month).unwrap();
        assert_ne!(quantized, unquantized);

        assert_eq!(curve.zero_rate_date(mid_month, false).unwrap(), quantized);
        assert_eq!(quantized, 0.125);
    }

    #[test]
    fn zero_rate_date_quantizes_to_the_curve_frequency_not_to_the_month() {
        let curve = curve_with(Frequency::Quarterly, None);
        let quarter_start = Date::new(1, Month::January, 2026);

        let rate = curve
            .zero_rate_date(Date::new(15, Month::March, 2026), false)
            .unwrap();
        assert_eq!(rate, curve.time_from_reference(quarter_start).unwrap());
        assert!(rate < 0.0);
    }

    #[test]
    fn zero_rate_date_reaches_dates_between_the_base_and_reference_dates() {
        let curve = curve(None);
        let rate = curve
            .zero_rate_date(Date::new(15, Month::December, 2025), false)
            .unwrap();
        assert_eq!(rate, curve.time_from_reference(base_date()).unwrap());
    }

    #[test]
    fn zero_rate_passes_the_time_through_unquantized() {
        let curve = curve(None);
        assert_eq!(curve.zero_rate(0.375, false).unwrap(), 0.375);
        assert_eq!(curve.zero_rate(-0.1, false).unwrap(), -0.1);
    }

    #[test]
    fn base_rate_is_available_only_when_the_curve_carries_one() {
        let zero = curve(None);
        let err = zero.base_rate().unwrap_err();
        assert!(err.message().contains("base rate not available"));

        assert_eq!(curve(Some(0.02)).base_rate().unwrap(), 0.02);
    }

    #[test]
    fn inspectors_report_the_constructor_arguments() {
        let curve = curve(None);
        assert_eq!(curve.frequency(), Frequency::Monthly);
        assert_eq!(curve.base_date(), base_date());
        assert_eq!(curve.reference_date().unwrap(), reference());
    }
}
