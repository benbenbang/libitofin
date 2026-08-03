//! Inflation term structure based on the interpolation of zero rates.
//!
//! Port of `ql/termstructures/inflation/interpolatedzeroinflationcurve.hpp`
//! (header-only): [`InterpolatedZeroInflationCurve`] builds a
//! [`ZeroInflationTermStructure`] from (date, zero-rate) nodes interpolated in
//! zero-rate space, composing the shared
//! [`InflationTermStructureBase`] holder with an [`InterpolatedCurve`];
//! [`ZeroInflationCurve`] is the C++ `typedef` for the linearly interpolated
//! case (`interpolatedzeroinflationcurve.hpp:83`).
//!
//! The node times are measured from the *reference date*, not from the first
//! node (`interpolatedzeroinflationcurve.hpp:112`). The base date is the first
//! node and precedes the reference date, so the first node time is negative -
//! the divergence from the yield-side [`InterpolatedZeroCurve`], which passes
//! its own first date and starts at zero.
//!
//! [`InterpolatedZeroCurve`]: crate::termstructures::yields::InterpolatedZeroCurve
//!
//! ## Divergences from QuantLib
//!
//! - Seasonality is not ported, following the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergence: the
//!   constructor argument (`interpolatedzeroinflationcurve.hpp:47`) is omitted
//!   rather than accepted and ignored, so a curve needing seasonality fails to
//!   compile.
//! - The protected constructor for descendants that cannot supply their nodes
//!   at construction (`interpolatedzeroinflationcurve.hpp:75-80,116-126`)
//!   follows with the bootstrapped curves that use it.
//! - C++ reads `dates.at(0)` in the initializer list *before* checking the
//!   size, throwing `out_of_range` on an empty vector; here the size check
//!   runs first and every input problem is a `QlError` per D4.
//! - [`zero_rate_impl`](ZeroInflationTermStructure::zero_rate_impl) evaluates
//!   the interpolation without extrapolation where C++ always extrapolates
//!   (`interpolatedzeroinflationcurve.hpp:137` passes `true`): the
//!   extrapolation flag is carried per interpolation object and set at
//!   construction, so it cannot be flipped through a generic
//!   [`Interpolator::Output`] - the limitation already documented on
//!   [`BlackVarianceCurve`](crate::termstructures::volatility::BlackVarianceCurve).
//!   The gap is exactly bounded: the inflation range checks admit
//!   `[base_date, max_date]`, whose ends are the interpolation's own `x_min`
//!   (the base date *is* the first node) and `x_max` (the maximum date *is*
//!   the last node), so every non-extrapolating query matches C++ and only an
//!   explicit `extrapolate = true` or an
//!   [`enable_extrapolation`](TermStructure::enable_extrapolation) past the
//!   last node errors where C++ extends the end segment.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, ZeroInflationTermStructure,
};
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

/// Inflation term structure based on the interpolation of zero rates.
///
/// The first date is the base date, the last date for which the fixing is
/// known; the reference date is passed separately and normally follows it.
pub struct InterpolatedZeroInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
}

/// Inflation term structure based on linear interpolation of zero rates
/// (C++'s `ZeroInflationCurve` typedef).
pub type ZeroInflationCurve = InterpolatedZeroInflationCurve<Linear>;

impl<I: Interpolator> InterpolatedZeroInflationCurve<I> {
    /// Curve through the zero-coupon inflation rates quoted at the given
    /// dates, the first of which is the base date
    /// (`interpolatedzeroinflationcurve.hpp:89-114`).
    ///
    /// The validations run in C++'s order, so their precedence is preserved:
    /// a curve with both unsorted dates and an out-of-range rate reports the
    /// rate. The lower bound on the rates starts at the *second* node
    /// (`interpolatedzeroinflationcurve.hpp:107`), leaving the base rate
    /// unconstrained, and is spelled as the negation C++'s `QL_REQUIRE`
    /// reduces to, so `NaN` fails it exactly as it does there.
    pub fn new(
        reference_date: Date,
        dates: Vec<Date>,
        rates: Vec<Rate>,
        frequency: Frequency,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<InterpolatedZeroInflationCurve<I>> {
        require!(dates.len() > 1, "too few dates: {}", dates.len());
        require!(
            rates.len() == dates.len(),
            "indices/dates count mismatch: {} vs {}",
            rates.len(),
            dates.len()
        );
        for &rate in &rates[1..] {
            if rate <= -1.0 || rate.is_nan() {
                fail!("zero inflation data < -100 %");
            }
        }

        let base_date = dates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, rates, interpolator);
        curve.setup_interpolation()?;
        let inflation = InflationTermStructureBase::with_reference_date(
            reference_date,
            base_date,
            frequency,
            Some(day_counter),
            None,
        );
        Ok(InterpolatedZeroInflationCurve {
            inflation,
            curve,
            dates,
        })
    }

    /// The node times, measured from the reference date; the first is
    /// negative whenever the base date precedes it.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates, the first of which is the base date.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values (zero-coupon inflation rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node values (zero-coupon inflation rates).
    pub fn rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The curve nodes as (date, zero-rate) pairs.
    pub fn nodes(&self) -> Vec<(Date, Rate)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }

    fn last_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("construction rejected empty dates")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedZeroInflationCurve<I> {
    fn observable(&self) -> &Observable {
        self.inflation.term_structure_base().observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedZeroInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| self.last_date())
    }
}

impl<I: Interpolator> InflationTermStructure for InterpolatedZeroInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }
}

impl<I: Interpolator> ZeroInflationTermStructure for InterpolatedZeroInflationCurve<I> {
    fn zero_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.curve.interpolation()?.value(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn today() -> Date {
        Date::new(27, Month::January, 2022)
    }

    fn base_date() -> Date {
        Date::new(1, Month::December, 2021)
    }

    fn sample_dates() -> Vec<Date> {
        let today = today();
        vec![
            base_date(),
            today + 7,
            today + 14,
            today + Period::new(1, TimeUnit::Months),
            today + Period::new(2, TimeUnit::Months),
            today + Period::new(3, TimeUnit::Months),
            today + Period::new(6, TimeUnit::Months),
            today + Period::new(1, TimeUnit::Years),
            today + Period::new(2, TimeUnit::Years),
            today + Period::new(5, TimeUnit::Years),
            today + Period::new(10, TimeUnit::Years),
        ]
    }

    fn sample_rates() -> Vec<Rate> {
        vec![
            0.01, 0.01, 0.011, 0.012, 0.013, 0.015, 0.018, 0.02, 0.025, 0.03, 0.03,
        ]
    }

    fn build(dates: Vec<Date>, rates: Vec<Rate>) -> QlResult<ZeroInflationCurve> {
        ZeroInflationCurve::new(
            today(),
            dates,
            rates,
            Frequency::Monthly,
            Actual360::new(),
            Linear,
        )
    }

    fn sample() -> ZeroInflationCurve {
        build(sample_dates(), sample_rates()).unwrap()
    }

    #[test]
    fn nodes_round_trip_the_input_dates() {
        let curve = sample();
        let dates = sample_dates();
        let nodes = curve.nodes();

        assert_eq!(nodes.len(), dates.len());
        for (i, date) in dates.iter().enumerate() {
            assert_eq!(*date, nodes[i].0);
        }
    }

    #[test]
    fn the_base_node_sits_before_the_reference_date_at_a_negative_time() {
        let curve = sample();
        assert_eq!(curve.base_date(), base_date());
        assert_eq!(curve.reference_date().unwrap(), today());
        assert_eq!(curve.times()[0], -57.0 / 360.0);
        assert_eq!(curve.times()[1], 7.0 / 360.0);
        assert_eq!(
            curve.times()[0],
            curve.time_from_reference(base_date()).unwrap()
        );
    }

    #[test]
    fn zero_rate_date_quantizes_to_the_period_start_and_interpolates() {
        let curve = sample();
        let mid_march = Date::new(15, Month::March, 2022);

        let t = 33.0 / 360.0;
        assert_eq!(
            t,
            curve
                .time_from_reference(Date::new(1, Month::March, 2022))
                .unwrap()
        );
        let (t_lo, t_hi) = (curve.times()[3], curve.times()[4]);
        let (r_lo, r_hi) = (curve.rates()[3], curve.rates()[4]);
        let expected = r_lo + (t - t_lo) / (t_hi - t_lo) * (r_hi - r_lo);

        let rate = curve.zero_rate_date(mid_march, false).unwrap();
        assert!((rate - expected).abs() < 1.0e-15);

        let unquantized = curve
            .zero_rate(curve.time_from_reference(mid_march).unwrap(), false)
            .unwrap();
        assert_ne!(rate, unquantized);
    }

    #[test]
    fn zero_rate_date_reaches_the_base_node_and_stops_below_it() {
        let curve = sample();
        let rate = curve.zero_rate_date(Date::new(15, Month::December, 2021), false);
        assert!(rate.unwrap().is_finite());

        let before = curve
            .zero_rate_date(Date::new(15, Month::November, 2021), false)
            .unwrap_err();
        assert!(before.message().contains("is before base date"));
    }

    #[test]
    fn extrapolating_past_the_last_node_errors_where_cpp_extends_the_end_segment() {
        let curve = sample();
        let beyond = curve.max_date() + Period::new(1, TimeUnit::Years);

        let err = curve.zero_rate_date(beyond, true).unwrap_err();
        assert!(err.message().contains("extrapolation"));

        curve.enable_extrapolation();
        assert!(curve.zero_rate_date(beyond, false).is_err());
    }

    fn build_err(dates: Vec<Date>, rates: Vec<Rate>) -> String {
        match build(dates, rates) {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err.message().to_string(),
        }
    }

    #[test]
    fn constructor_rejects_too_few_dates_before_reading_the_base_date() {
        assert!(build_err(vec![], vec![]).contains("too few dates"));
        assert!(build_err(vec![base_date()], vec![0.01]).contains("too few dates"));
    }

    #[test]
    fn constructor_rejects_a_count_mismatch_and_unsorted_dates() {
        assert!(
            build_err(sample_dates(), vec![0.01, 0.01]).contains("indices/dates count mismatch")
        );

        let mut dates = sample_dates();
        dates.swap(1, 2);
        assert!(build_err(dates, sample_rates()).contains("dates not sorted"));
    }

    #[test]
    fn the_minus_one_bound_skips_the_base_node() {
        let mut rates = sample_rates();
        rates[1] = -1.0;
        assert!(build_err(sample_dates(), rates).contains("zero inflation data < -100 %"));

        let mut rates = sample_rates();
        rates[1] = Rate::NAN;
        assert!(build_err(sample_dates(), rates).contains("zero inflation data < -100 %"));

        let mut rates = sample_rates();
        rates[0] = -1.5;
        let curve = build(sample_dates(), rates).unwrap();
        assert_eq!(curve.rates()[0], -1.5);
    }

    #[test]
    fn the_rate_bound_is_checked_before_the_dates_are_converted_to_times() {
        let mut dates = sample_dates();
        dates.swap(1, 2);
        let mut rates = sample_rates();
        rates[1] = -2.0;

        assert!(build_err(dates, rates).contains("zero inflation data < -100 %"));
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = sample();
        assert_eq!(curve.dates(), &sample_dates()[..]);
        assert_eq!(curve.rates(), &sample_rates()[..]);
        assert_eq!(curve.data(), curve.rates());
        assert_eq!(curve.times().len(), sample_dates().len());
        assert_eq!(curve.max_time().unwrap(), *curve.times().last().unwrap());
        assert_eq!(curve.frequency(), Frequency::Monthly);
        assert_eq!(curve.max_date(), today() + Period::new(10, TimeUnit::Years));
        assert!(curve.base_rate().is_err());
    }
}
