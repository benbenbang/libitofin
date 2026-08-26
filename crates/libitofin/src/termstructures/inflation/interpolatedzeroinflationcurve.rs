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
//! - The seasonality constructor argument
//!   (`interpolatedzeroinflationcurve.hpp:47`) is ported, but the consistency
//!   gate C++ runs from the base constructor runs here from this one, once the
//!   curve exists; see the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergences.
//! - The protected constructor for descendants that cannot supply their nodes
//!   at construction (`interpolatedzeroinflationcurve.hpp:75-80,116-126`)
//!   follows with the bootstrapped curves that use it.
//! - C++ reads `dates.at(0)` in the initializer list *before* checking the
//!   size, throwing `out_of_range` on an empty vector; here the size check
//!   runs first and every input problem is a `QlError` per D4.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::Shared;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, ZeroInflationTermStructure,
};
use crate::termstructures::inflation::seasonality::Seasonality;
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
        seasonality: Option<Shared<dyn Seasonality>>,
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
            seasonality,
        );
        let curve = InterpolatedZeroInflationCurve {
            inflation,
            curve,
            dates,
        };
        curve.check_seasonality()?;
        Ok(curve)
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

    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
        self
    }
}

impl<I: Interpolator> ZeroInflationTermStructure for InterpolatedZeroInflationCurve<I> {
    /// C++ evaluates with extrapolation allowed at the interpolation level,
    /// `interpolation_(t, true)` (`interpolatedzeroinflationcurve.hpp:137`);
    /// range policy lives in the callers above, and this impl must assume
    /// extrapolation is required. Past the last node the last segment
    /// continues on its own slope, which for [`Linear`] is exactly C++'s
    /// extension - the same extension #806 applied to
    /// [`PiecewiseZeroInflationCurve`](super::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve).
    fn zero_rate_impl(&self, t: Time) -> QlResult<Rate> {
        let interpolation = self.curve.interpolation()?;
        let t_max = interpolation.x_max();
        if t <= t_max {
            return interpolation.value(t);
        }
        let value_max = interpolation.value(t_max)?;
        let slope_max = interpolation.derivative(t_max)?;
        Ok(value_max + slope_max * (t - t_max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::termstructures::inflation::seasonality::MultiplicativePriceSeasonality;
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
            None,
        )
    }

    fn sample() -> ZeroInflationCurve {
        build(sample_dates(), sample_rates()).unwrap()
    }

    /// A seasonality passed at construction is installed and gated there, where
    /// C++ gates it in the base constructor: twelve monthly factors are
    /// consistent with any curve, a twenty-four-factor set is the multi-year
    /// case this port defers.
    #[test]
    fn the_constructor_installs_and_gates_a_seasonality() {
        fn built(count: usize) -> QlResult<ZeroInflationCurve> {
            let seasonality = shared(
                MultiplicativePriceSeasonality::new(
                    Date::new(31, Month::January, 2022),
                    Frequency::Monthly,
                    (0..count).map(|i| 1.0 + i as Rate / 1000.0).collect(),
                )
                .expect("a whole multiple of twelve factors"),
            ) as Shared<dyn Seasonality>;
            ZeroInflationCurve::new(
                today(),
                sample_dates(),
                sample_rates(),
                Frequency::Monthly,
                Actual360::new(),
                Linear,
                Some(seasonality),
            )
        }

        assert!(built(12).unwrap().has_seasonality());
        let deferred = match built(24) {
            Ok(_) => panic!("expected the multi-year seasonality to be rejected"),
            Err(err) => err,
        };
        assert!(
            deferred.message().contains("#807"),
            "{}",
            deferred.message()
        );
        assert!(!sample().has_seasonality());
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

    /// Past the last node the last segment continues on its own slope, as
    /// C++'s `interpolation_(t, true)` extends it; the range check still gates
    /// until extrapolation is requested. The fixture's last segment is flat
    /// (`0.03` to `0.03`), which a clamping bug would reproduce, so the last
    /// rate is bumped to make the continuation discriminating.
    #[test]
    fn extrapolating_past_the_last_node_extends_the_end_segment_as_cpp_does() {
        let mut rates = sample_rates();
        let last = rates.len() - 1;
        rates[last] = 0.032;
        let curve = build(sample_dates(), rates).unwrap();
        let beyond = curve.max_date() + Period::new(1, TimeUnit::Years);

        let gated = curve.zero_rate_date(beyond, false).unwrap_err();
        assert!(gated.message().contains("is past max curve date"));

        let (t_lo, t_hi) = (curve.times()[last - 1], curve.times()[last]);
        assert_eq!(t_lo, 1826.0 / 360.0);
        assert_eq!(t_hi, 3652.0 / 360.0);
        let t = curve
            .time_from_reference(Date::new(1, Month::January, 2033))
            .unwrap();
        assert_eq!(t, 3992.0 / 360.0);
        let expected = 0.032 + (0.032 - 0.03) / (t_hi - t_lo) * (t - t_hi);

        let extended = curve.zero_rate_date(beyond, true).unwrap();
        assert!((extended - expected).abs() < 1.0e-12);

        curve.enable_extrapolation();
        assert_eq!(curve.zero_rate_date(beyond, false).unwrap(), extended);
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
