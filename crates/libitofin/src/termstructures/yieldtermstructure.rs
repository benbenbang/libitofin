//! Interest-rate term structure.
//!
//! Port of `ql/termstructures/yieldtermstructure.{hpp,cpp}`: the
//! [`YieldTermStructure`] trait extends [`TermStructure`] with the
//! discount-factor, zero-yield and forward-rate algebra, all derived from the
//! single required [`discount_impl`](YieldTermStructure::discount_impl).
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (the turn-of-year effect) are held by [`YieldJumps`]; curves
//!   opt in through [`YieldTermStructure::jumps`]. Jump times are calculated
//!   on demand rather than cached by `setJumps()` (`yieldtermstructure.cpp:59-76`),
//!   so the C++ `update()` override that re-runs `setJumps` on a new reference
//!   date (`yieldtermstructure.cpp:174-180`) is not needed and the base
//!   updater ([`TermStructureBase::updater`](super::TermStructureBase::updater))
//!   stays complete. This also fixes a C++ staleness: the moving
//!   `PiecewiseYieldCurve` does not forward to that override
//!   (`piecewiseyieldcurve.hpp:220-229`), so its `jumpTimes_` keep the
//!   construction reference date. As in QuantLib, automatic year-end dates
//!   are generated once; a moving reference date changes jump times, not the
//!   generated dates.
//! - Jumps on `InterpolatedZeroCurve` / `InterpolatedForwardCurve` are deferred (#1319). Curves that override
//!   [`discount`](YieldTermStructure::discount) (`PiecewiseYieldCurve`)
//!   multiply in [`jump_factor`](YieldTermStructure::jump_factor) themselves.
//!   Bindings are deferred (#1320).
//! - A null jump date is an `Err` at construction; C++ accepts it unchecked.
//! - C++ overloads on `Date`/`Time`/`Period` become distinct method names
//!   (`discount_date`, `forward_rate_between`, ...), following the
//!   [`InterestRate`] port convention.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::{Compounding, InterestRate};
use crate::patterns::observable::Observer;
use crate::quotes::Quote;
use crate::require;
use crate::shared::SharedMut;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::{Date, Month};
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::{DiscountFactor, Time};

const DT: Time = 0.0001;

/// Observable multiplicative discount jumps carried by yield curves.
///
/// This implements `yieldtermstructure.cpp:59-95`. Unlike the credit
/// [`DefaultProbabilityJumps`](super::credit::defaulttermstructure::DefaultProbabilityJumps),
/// every jump with `0 < t_i < t` is applied regardless of input order (no
/// early break, `yieldtermstructure.cpp:84-85`), and a jump value need only be
/// positive (`yieldtermstructure.cpp:89`). Quote validation is lazy: a quote
/// is read only once its jump time has been crossed.
pub struct YieldJumps {
    quotes: Vec<Handle<dyn Quote>>,
    dates: Vec<Date>,
}

impl YieldJumps {
    /// Registers jump quotes with the curve's updater and resolves dates.
    ///
    /// Empty dates select December 31 of the initial reference year and each
    /// subsequent year. Explicit dates must have one entry per quote.
    ///
    /// # Errors
    /// Returns an error for mismatched lengths, null dates, an unavailable
    /// reference date, or automatic dates beyond the supported date range.
    pub fn new(
        base: &TermStructureBase,
        quotes: Vec<Handle<dyn Quote>>,
        dates: Vec<Date>,
    ) -> QlResult<Self> {
        Self::with_observer(base, &base.updater(), quotes, dates)
    }

    /// As [`new`](Self::new), registering the jump quotes with `observer`
    /// instead of the base updater: a bootstrapped curve passes the private
    /// updater that invalidates its cache.
    ///
    /// # Errors
    /// Returns the [`new`](Self::new) errors.
    pub fn with_observer(
        base: &TermStructureBase,
        observer: &SharedMut<dyn Observer>,
        quotes: Vec<Handle<dyn Quote>>,
        mut dates: Vec<Date>,
    ) -> QlResult<Self> {
        if dates.is_empty() && !quotes.is_empty() {
            let year = base.reference_date()?.year();
            require!(
                quotes.len() <= (Date::max_date().year() - year + 1) as usize,
                "automatic jump dates exceed the maximum supported date"
            );
            dates = (0..quotes.len())
                .map(|i| Date::new(31, Month::December, year + i as i32))
                .collect();
        }
        require!(
            quotes.len() == dates.len(),
            "mismatch between number of jumps ({}) and jump dates ({})",
            quotes.len(),
            dates.len()
        );
        require!(
            dates.iter().all(|date| *date != Date::null()),
            "null jump date"
        );
        let jumps = Self { quotes, dates };
        jumps.register_observer(observer);
        Ok(jumps)
    }

    /// The immutable jump dates, including any generated year-end dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// Registers `observer` with every jump quote.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) {
        for quote in &self.quotes {
            quote.register_observer(observer);
        }
    }

    fn factor<T: TermStructure + ?Sized>(&self, curve: &T, t: Time) -> QlResult<DiscountFactor> {
        let mut factor = 1.0;
        for (i, (date, handle)) in self.dates.iter().zip(&self.quotes).enumerate() {
            let jump_time = curve.time_from_reference(*date)?;
            if jump_time > 0.0 && jump_time < t {
                let quote = handle.current_link()?;
                require!(quote.is_valid(), "invalid jump quote #{}", i + 1);
                let value = quote.value()?;
                if value.is_nan() || value <= 0.0 {
                    crate::fail!("invalid jump value #{}: {value}", i + 1);
                }
                factor *= value;
            }
        }
        Ok(factor)
    }
}

/// Interest-rate term structure.
///
/// Mirrors QuantLib's `YieldTermStructure`: concrete curves implement
/// [`discount_impl`](Self::discount_impl) (called after range checking, so it
/// must assume extrapolation is required) and inherit the rest.
pub trait YieldTermStructure: TermStructure {
    /// Discount factor calculation, implemented by concrete curves.
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor>;

    /// The curve as [`Any`](std::any::Any), for the callers that must recover
    /// its concrete type from a `dyn YieldTermStructure`.
    ///
    /// The port of C++'s `dynamic_pointer_cast` on a `Handle<YieldTermStructure>`:
    /// `Rc` carries no downcast of its own, so a curve opts in by overriding
    /// this. The default `None` reads as "this curve declines to be
    /// introspected", which
    /// [`isda_node_grid`](crate::pricingengines::credit::isda_node_grid)
    /// - the only caller - treats as the C++ `QL_FAIL` arm.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// Optional observable jump state; existing curves default to no jumps.
    fn jumps(&self) -> Option<&YieldJumps> {
        None
    }

    /// Jump dates in constructor order.
    fn jump_dates(&self) -> &[Date] {
        self.jumps().map_or(&[], YieldJumps::dates)
    }

    /// Jump times recalculated against the current reference date.
    fn jump_times(&self) -> QlResult<Vec<Time>> {
        self.jump_dates()
            .iter()
            .map(|date| self.time_from_reference(*date))
            .collect()
    }

    /// The product of every jump strictly between the reference date and time
    /// `t` (1.0 without jumps).
    ///
    /// Applied by the default [`discount`](Self::discount); curves that
    /// override `discount` multiply it in themselves.
    fn jump_factor(&self, t: Time) -> QlResult<DiscountFactor> {
        match self.jumps() {
            Some(jumps) => jumps.factor(self, t),
            None => Ok(1.0),
        }
    }

    /// The discount factor from `date` to the reference date.
    fn discount_date(&self, date: Date, extrapolate: bool) -> QlResult<DiscountFactor> {
        self.discount(self.time_from_reference(date)?, extrapolate)
    }

    /// The discount factor from time `t` to the reference date.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn discount(&self, t: Time, extrapolate: bool) -> QlResult<DiscountFactor> {
        self.check_range_time(t, extrapolate)?;
        Ok(self.jump_factor(t)? * self.discount_impl(t)?)
    }

    /// The implied zero-yield rate for `date`, carrying the required
    /// day-counting rule.
    fn zero_rate_date(
        &self,
        date: Date,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let t = self.time_from_reference(date)?;
        if t == 0.0 {
            let compound = 1.0 / self.discount(DT, extrapolate)?;
            return InterestRate::implied_rate(compound, result_day_counter, comp, freq, DT);
        }
        let compound = 1.0 / self.discount(t, extrapolate)?;
        InterestRate::implied_rate_between(
            compound,
            result_day_counter,
            comp,
            freq,
            self.reference_date()?,
            date,
        )
    }

    /// The implied zero-yield rate for time `t`, carrying the same
    /// day-counting rule used by the term structure.
    ///
    /// The time must be calculated with that same rule.
    fn zero_rate(
        &self,
        t: Time,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let day_counter = self.require_day_counter()?;
        let t = if t == 0.0 { DT } else { t };
        let compound = 1.0 / self.discount(t, extrapolate)?;
        InterestRate::implied_rate(compound, day_counter, comp, freq, t)
    }

    /// The forward interest rate between two dates, carrying the required
    /// day-counting rule; equal dates yield the instantaneous forward.
    fn forward_rate_between(
        &self,
        d1: Date,
        d2: Date,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        if d1 == d2 {
            self.check_range_date(d1, extrapolate)?;
            let t1 = Time::max(self.time_from_reference(d1)? - DT / 2.0, 0.0);
            let t2 = t1 + DT;
            let compound = self.discount(t1, true)? / self.discount(t2, true)?;
            return InterestRate::implied_rate(compound, result_day_counter, comp, freq, DT);
        }
        require!(d1 < d2, "{d1} later than {d2}");
        let compound =
            self.discount_date(d1, extrapolate)? / self.discount_date(d2, extrapolate)?;
        InterestRate::implied_rate_between(compound, result_day_counter, comp, freq, d1, d2)
    }

    /// The forward interest rate over the period `p` starting at `date`;
    /// dates are not adjusted for holidays.
    fn forward_rate_period(
        &self,
        date: Date,
        p: Period,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        self.forward_rate_between(date, date + p, result_day_counter, comp, freq, extrapolate)
    }

    /// The forward interest rate between two times, carrying the same
    /// day-counting rule used by the term structure; equal times yield the
    /// instantaneous forward.
    ///
    /// The times must be calculated with that same rule.
    fn forward_rate(
        &self,
        t1: Time,
        t2: Time,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let day_counter = self.require_day_counter()?;
        let (t1, t2, compound) = if t2 == t1 {
            self.check_range_time(t1, extrapolate)?;
            let t1 = Time::max(t1 - DT / 2.0, 0.0);
            let t2 = t1 + DT;
            let compound = self.discount(t1, true)? / self.discount(t2, true)?;
            (t1, t2, compound)
        } else {
            if t1.is_nan() || t2.is_nan() || t2 <= t1 {
                crate::fail!("t1 ({t1}) >= t2 ({t2})");
            }
            let compound = self.discount(t1, extrapolate)? / self.discount(t2, extrapolate)?;
            (t1, t2, compound)
        };
        InterestRate::implied_rate(compound, day_counter, comp, freq, t2 - t1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::{Shared, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Real;

    struct ExponentialCurve {
        base: TermStructureBase,
        rate: Real,
        jumps: Option<YieldJumps>,
    }

    impl ExponentialCurve {
        fn new(reference: Date, rate: Real) -> ExponentialCurve {
            ExponentialCurve {
                base: TermStructureBase::with_reference_date(
                    reference,
                    None,
                    Some(Actual360::new()),
                ),
                rate,
                jumps: None,
            }
        }
    }

    impl AsObservable for ExponentialCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for ExponentialCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl YieldTermStructure for ExponentialCurve {
        fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
            Ok((-self.rate * t).exp())
        }

        fn jumps(&self) -> Option<&YieldJumps> {
            self.jumps.as_ref()
        }
    }

    fn curve() -> ExponentialCurve {
        ExponentialCurve::new(Date::new(15, Month::June, 2026), 0.05)
    }

    #[test]
    fn discount_checks_range_then_delegates() {
        let curve = curve();
        let df = curve.discount(2.0, false).unwrap();
        assert!((df - (-0.1_f64).exp()).abs() < 1.0e-15);
        assert!(curve.discount(-0.5, false).is_err());
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn discount_date_converts_through_the_day_counter() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let df = curve.discount_date(reference + 180, false).unwrap();
        assert!((df - (-0.05_f64 * 0.5).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn zero_rate_recovers_the_continuous_rate() {
        let curve = curve();
        let zero = curve
            .zero_rate(1.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-12);

        let zero = curve
            .zero_rate(0.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-12);
    }

    #[test]
    fn zero_rate_date_round_trips_the_discount() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let date = reference + 360;
        let zero = curve
            .zero_rate_date(
                date,
                Actual360::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                false,
            )
            .unwrap();
        let df = curve.discount_date(date, false).unwrap();
        let round_trip = zero.discount_factor_between(reference, date).unwrap();
        assert!((round_trip - df).abs() < 1.0e-15);

        let at_reference = curve
            .zero_rate_date(
                reference,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert!((at_reference.rate() - 0.05).abs() < 1.0e-10);
    }

    #[test]
    fn forward_rate_matches_the_flat_rate() {
        let curve = curve();
        let forward = curve
            .forward_rate(0.5, 1.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - 0.05).abs() < 1.0e-12);

        let instantaneous = curve
            .forward_rate(1.0, 1.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((instantaneous.rate() - 0.05).abs() < 1.0e-9);

        assert!(
            curve
                .forward_rate(1.5, 0.5, Compounding::Continuous, Frequency::Annual, false)
                .is_err()
        );
    }

    #[test]
    fn forward_rate_between_dates_and_periods_agree() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let d1 = reference + 90;
        let d2 = reference + 270;
        let between = curve
            .forward_rate_between(
                d1,
                d2,
                Actual360::new(),
                Compounding::Simple,
                Frequency::Annual,
                false,
            )
            .unwrap();
        let by_period = curve
            .forward_rate_period(
                d1,
                Period::new(180, TimeUnit::Days),
                Actual360::new(),
                Compounding::Simple,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert_eq!(between.rate(), by_period.rate());

        let df1 = curve.discount_date(d1, false).unwrap();
        let df2 = curve.discount_date(d2, false).unwrap();
        let implied = between.compound_factor_between(d1, d2).unwrap();
        assert!((implied - df1 / df2).abs() < 1.0e-15);

        assert!(
            curve
                .forward_rate_between(
                    d2,
                    d1,
                    Actual360::new(),
                    Compounding::Simple,
                    Frequency::Annual,
                    false,
                )
                .is_err()
        );

        let instantaneous = curve
            .forward_rate_between(
                d1,
                d1,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert!((instantaneous.rate() - 0.05).abs() < 1.0e-9);
    }

    fn quote(value: Real) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    fn jumped(jumps: Vec<Handle<dyn Quote>>, dates: Vec<Date>) -> QlResult<ExponentialCurve> {
        let mut curve = curve();
        curve.jumps = Some(YieldJumps::new(&curve.base, jumps, dates)?);
        Ok(curve)
    }

    #[test]
    fn discount_multiplies_every_jump_strictly_inside_the_interval() {
        let reference = Date::new(15, Month::June, 2026);
        let dates = vec![reference + 90, reference + 180];
        let curve = jumped(vec![quote(0.99), quote(0.98)], dates.clone()).unwrap();
        assert_eq!(curve.jump_dates(), dates);
        assert_eq!(curve.jump_times().unwrap(), vec![0.25, 0.5]);
        for (days, factor) in [
            (0, 1.0),
            (89, 1.0),
            (90, 1.0),
            (91, 0.99),
            (180, 0.99),
            (181, 0.99 * 0.98),
            (720, 0.99 * 0.98),
        ] {
            let t = days as Real / 360.0;
            let expected = factor * (-0.05 * t).exp();
            assert_eq!(curve.jump_factor(t).unwrap(), factor);
            assert!((curve.discount(t, false).unwrap() - expected).abs() < 1.0e-15);
            let df = curve.discount_date(reference + days, false).unwrap();
            assert!((df - expected).abs() < 1.0e-15);
        }
        let zero = curve
            .zero_rate(1.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        let expected = 0.05 - (0.99_f64 * 0.98).ln();
        assert!((zero.rate() - expected).abs() < 1.0e-14);
    }

    #[test]
    fn out_of_order_jumps_are_all_counted_without_a_prefix_break() {
        let reference = Date::new(15, Month::June, 2026);
        let curve = jumped(
            vec![quote(0.97), quote(0.99)],
            vec![reference + 180, reference + 90],
        )
        .unwrap();
        assert_eq!(curve.jump_factor(0.4).unwrap(), 0.99);
        assert_eq!(curve.jump_factor(0.6).unwrap(), 0.99 * 0.97);
    }

    #[test]
    fn jumps_on_or_before_the_reference_date_are_skipped() {
        let reference = Date::new(15, Month::June, 2026);
        let curve = jumped(
            vec![quote(0.5), quote(-1.0), quote(0.99)],
            vec![reference, reference - 30, reference + 90],
        )
        .unwrap();
        assert_eq!(curve.jump_factor(1.0).unwrap(), 0.99);
        assert!(curve.discount(-0.5, false).is_err());
    }

    #[test]
    fn invalid_jumps_are_checked_only_after_their_boundary() {
        let date = Date::new(15, Month::June, 2026) + 360;
        for value in [0.0, -0.1, f64::NAN] {
            let curve = jumped(vec![quote(value)], vec![date]).unwrap();
            assert!(curve.discount(1.0, false).is_ok());
            assert!(curve.discount(1.01, false).is_err());
        }
        for handle in [
            Handle::empty(),
            Handle::new(shared(SimpleQuote::default()) as Shared<dyn Quote>),
        ] {
            let curve = jumped(vec![handle], vec![date]).unwrap();
            assert!(curve.discount(1.0, false).is_ok());
            assert!(curve.discount(1.01, false).is_err());
        }
        let above_one = jumped(vec![quote(1.02)], vec![date]).unwrap();
        let expected = 1.02 * (-0.05_f64 * 1.5).exp();
        assert!((above_one.discount(1.5, false).unwrap() - expected).abs() < 1.0e-15);
        assert!(jumped(vec![quote(0.9)], vec![date, date + 1]).is_err());
        assert!(jumped(vec![], vec![date]).is_err());
        assert!(jumped(vec![quote(0.9)], vec![Date::null()]).is_err());
        assert!(curve().jumps().is_none());
        assert_eq!(curve().jump_factor(5.0).unwrap(), 1.0);
    }

    #[test]
    fn empty_dates_generate_consecutive_year_ends() {
        let curve = jumped(vec![quote(0.99), quote(0.98), quote(0.97)], vec![]).unwrap();
        assert_eq!(
            curve.jump_dates(),
            [
                Date::new(31, Month::December, 2026),
                Date::new(31, Month::December, 2027),
                Date::new(31, Month::December, 2028),
            ]
        );
    }

    #[test]
    fn jump_quote_change_notifies_the_curve_observers() {
        let jump = shared(SimpleQuote::new(0.99));
        let curve = jumped(
            vec![Handle::new(jump.clone() as Shared<dyn Quote>)],
            vec![Date::new(15, Month::June, 2026) + 90],
        )
        .unwrap();
        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        jump.set_value(0.95);
        assert!(Flag::is_up(&flag));
        let expected = 0.95 * (-0.05_f64).exp();
        assert!((curve.discount(1.0, false).unwrap() - expected).abs() < 1.0e-15);
    }

    #[test]
    fn moving_jump_times_follow_the_reference_date_while_generated_dates_stay() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::June, 2026));
        let base = TermStructureBase::moving(
            0,
            NullCalendar::new(),
            Some(Actual360::new()),
            settings.clone(),
        );
        let jumps = YieldJumps::new(&base, vec![quote(0.99), quote(0.98)], vec![]).unwrap();
        let curve = ExponentialCurve {
            base,
            rate: 0.05,
            jumps: Some(jumps),
        };
        let dates = [
            Date::new(31, Month::December, 2026),
            Date::new(31, Month::December, 2027),
        ];
        assert_eq!(curve.jump_dates(), dates);
        assert_eq!(
            curve.jump_times().unwrap(),
            vec![199.0 / 360.0, 564.0 / 360.0]
        );
        assert_eq!(curve.jump_factor(1.0).unwrap(), 0.99);

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        settings.set_evaluation_date(Date::new(2, Month::January, 2027));
        assert!(Flag::is_up(&flag));
        assert_eq!(curve.jump_dates(), dates);
        assert_eq!(
            curve.jump_times().unwrap(),
            vec![-2.0 / 360.0, 363.0 / 360.0]
        );
        assert_eq!(curve.jump_factor(1.1).unwrap(), 0.98);
    }
}
