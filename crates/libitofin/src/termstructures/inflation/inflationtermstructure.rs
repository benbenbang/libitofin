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
//! - C++ gates [`Seasonality::is_consistent`] inside the three
//!   `InflationTermStructure` constructors, against a partially constructed
//!   `*this` (`inflationtermstructure.cpp:34-38,57-61,79-83`). A Rust holder
//!   is not the curve and cannot produce the `&dyn InflationTermStructure`
//!   that check needs, so the gate moves *out* to the concrete curve
//!   constructors, which run
//!   [`check_seasonality`](InflationTermStructure::check_seasonality) once
//!   they have a whole curve. The holder still stores whatever it is given.
//! - C++'s `setSeasonality` closes with a call to the virtual `update()`
//!   (`inflationtermstructure.cpp:87`); the Rust equivalent is
//!   [`update_after_seasonality_change`](InflationTermStructure::update_after_seasonality_change),
//!   a separate overridable hook, because the notification a bootstrapped
//!   curve owes its observers also has to invalidate its own cache.
//! - The seasonality is held in a [`RefCell`] rather than as a plain field:
//!   C++ `setSeasonality` is non-const and called on the concrete curve, which
//!   here lives behind a [`Shared`] that hands out only shared references.
//! - C++'s `checkRange` overloads *hide* `TermStructure::checkRange`; Rust
//!   traits have no name hiding, so they are ported under distinct names
//!   ([`check_inflation_range_date`](InflationTermStructure::check_inflation_range_date)
//!   and
//!   [`check_inflation_range_time`](InflationTermStructure::check_inflation_range_time)).
//!   An inflation curve must call those, never the
//!   [`TermStructure`] ones, whose lower bounds are strictly tighter.
//! - C++ overloads on `Date`/`Time` become distinct method names, the `_date`
//!   suffix taking dates.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::indexes::inflationindex::inflation_period;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::inflation::seasonality::Seasonality;
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
    seasonality: RefCell<Option<Shared<dyn Seasonality>>>,
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
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::new(day_counter),
            frequency,
            base_rate,
            base_date,
            seasonality: RefCell::new(seasonality),
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
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::with_reference_date(reference_date, None, day_counter),
            frequency,
            base_rate,
            base_date,
            seasonality: RefCell::new(seasonality),
        }
    }

    /// Holder whose reference date moves off the evaluation date
    /// (`inflationtermstructure.cpp:57-70`); the [`Settings`] handle is
    /// explicit per D5.
    #[allow(clippy::too_many_arguments)]
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        base_date: Date,
        frequency: Frequency,
        day_counter: Option<DayCounter>,
        base_rate: Option<Rate>,
        seasonality: Option<Shared<dyn Seasonality>>,
        settings: Shared<Settings<Date>>,
    ) -> InflationTermStructureBase {
        InflationTermStructureBase {
            base: TermStructureBase::moving(settlement_days, calendar, day_counter, settings),
            frequency,
            base_rate,
            base_date,
            seasonality: RefCell::new(seasonality),
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

    /// The curve as the trait object a [`Seasonality`] reads it through.
    ///
    /// C++ passes `*this` straight into `correctZeroRate`
    /// (`inflationtermstructure.cpp:171`). A trait's default body cannot
    /// unsize `&Self` - `Self` is not known to be sized there - so every
    /// concrete curve supplies the coercion, whose only implementation is
    /// `self`.
    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure;

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

    /// The seasonality correction folded into the rates the curve publishes,
    /// if any (`inflationtermstructure.hpp:95`).
    fn seasonality(&self) -> Option<Shared<dyn Seasonality>> {
        self.inflation_base().seasonality.borrow().clone()
    }

    /// Whether the curve carries a seasonality correction
    /// (`inflationtermstructure.hpp:96`).
    fn has_seasonality(&self) -> bool {
        self.inflation_base().seasonality.borrow().is_some()
    }

    /// Installs, replaces or (with `None`) clears the seasonality correction
    /// (`inflationtermstructure.cpp:76-88`).
    ///
    /// The store happens first and unconditionally, as C++'s does; a
    /// correction that then fails the consistency gate is left in place and
    /// the notification does not fire.
    ///
    /// # Errors
    ///
    /// Propagates the consistency gate.
    fn set_seasonality(&self, seasonality: Option<Shared<dyn Seasonality>>) -> QlResult<()> {
        *self.inflation_base().seasonality.borrow_mut() = seasonality;
        self.check_seasonality()?;
        self.update_after_seasonality_change();
        Ok(())
    }

    /// The consistency gate C++ runs from its constructors and its setter
    /// (`inflationtermstructure.cpp:34-38,84-86`).
    ///
    /// A curve without a seasonality passes trivially.
    ///
    /// # Errors
    ///
    /// Reports the seasonality's own verdict, and any error it raised
    /// reaching one.
    fn check_seasonality(&self) -> QlResult<()> {
        if let Some(seasonality) = self.seasonality() {
            require!(
                seasonality.is_consistent(self.as_inflation_term_structure())?,
                "Seasonality inconsistent with inflation term structure"
            );
        }
        Ok(())
    }

    /// The notification closing a successful
    /// [`set_seasonality`](Self::set_seasonality), the port of C++'s
    /// `update()` call (`inflationtermstructure.cpp:87`).
    ///
    /// The default behaves as `TermStructure::update()`: it refreshes a moving
    /// reference date and broadcasts. A curve that caches anything derived
    /// from the correction - every bootstrapped one does - overrides this to
    /// invalidate that cache first.
    fn update_after_seasonality_change(&self) {
        self.base().updater().borrow_mut().update();
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
    ///
    /// Any seasonality correction is folded in last
    /// (`inflationtermstructure.cpp:170-172`). It receives the *original*
    /// date, as C++ does on this path - the lag it subtracts there is zero -
    /// and quantizes for itself. This is the only entry point that corrects:
    /// [`zero_rate`](Self::zero_rate), taking a time, cannot recover the date
    /// the correction is a function of, and C++ leaves it uncorrected too
    /// (`inflationtermstructure.cpp:176-180`).
    fn zero_rate_date(&self, date: Date, extrapolate: bool) -> QlResult<Rate> {
        let (period_start, _) = inflation_period(date, self.frequency())?;
        self.check_inflation_range_date(period_start, extrapolate)?;
        let t = self.time_from_reference(period_start)?;
        let zero_rate = self.zero_rate_impl(t)?;
        match self.seasonality() {
            Some(seasonality) => {
                seasonality.correct_zero_rate(date, zero_rate, self.as_inflation_term_structure())
            }
            None => Ok(zero_rate),
        }
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
    use crate::shared::shared;
    use crate::termstructures::inflation::seasonality::MultiplicativePriceSeasonality;
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
                None,
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

        fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
            self
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

    /// Twelve monthly factors anchored on a month other than the curve's base
    /// month, so the correction is not the identity; `count` above twelve makes
    /// it the multi-year case the consistency gate defers.
    fn a_seasonality(count: usize) -> Shared<dyn Seasonality> {
        shared(
            MultiplicativePriceSeasonality::new(
                Date::new(31, Month::January, 2026),
                Frequency::Monthly,
                (0..count).map(|i| 1.0 + i as Rate / 1000.0).collect(),
            )
            .expect("a whole multiple of twelve factors"),
        ) as Shared<dyn Seasonality>
    }

    /// The correction is folded into the date-taking query and into nothing
    /// else, and clearing it puts the raw rate back.
    ///
    /// [`zero_rate`](ZeroInflationTermStructure::zero_rate) cannot correct: a
    /// time does not name the date the factors are a function of, and C++
    /// leaves it alone too.
    #[test]
    fn only_the_date_query_folds_the_seasonality_and_clearing_it_undoes_that() {
        let curve = curve(None);
        let date = Date::new(15, Month::March, 2026);
        let raw = curve.zero_rate_date(date, false).unwrap();
        let seasonality = a_seasonality(12);

        curve
            .set_seasonality(Some(Shared::clone(&seasonality)))
            .unwrap();
        assert!(curve.has_seasonality());
        let corrected = curve.zero_rate_date(date, false).unwrap();
        assert_eq!(
            corrected,
            seasonality.correct_zero_rate(date, raw, &curve).unwrap()
        );
        assert_ne!(corrected, raw, "the fold must move the rate");
        assert_eq!(
            curve.zero_rate(raw, false).unwrap(),
            raw,
            "the time query stays raw"
        );

        curve.set_seasonality(None).unwrap();
        assert!(!curve.has_seasonality());
        assert_eq!(curve.zero_rate_date(date, false).unwrap(), raw);
    }

    /// The gate reports the seasonality's verdict, and - as C++ does - stores
    /// the correction before consulting it.
    #[test]
    fn an_inconsistent_seasonality_is_reported_by_the_gate() {
        let curve = curve(None);

        let err = curve.set_seasonality(Some(a_seasonality(24))).unwrap_err();
        assert!(err.message().contains("#807"), "{}", err.message());
        assert!(curve.has_seasonality(), "C++ stores before it checks");
        assert!(curve.check_seasonality().is_err());
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
