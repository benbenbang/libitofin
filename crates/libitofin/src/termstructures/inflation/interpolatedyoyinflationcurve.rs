//! Inflation term structure based on the interpolation of year-on-year rates.
//!
//! Port of `ql/termstructures/inflation/interpolatedyoyinflationcurve.hpp`
//! (header-only): [`InterpolatedYoYInflationCurve`] builds a
//! [`YoYInflationTermStructure`] from (date, year-on-year rate) nodes
//! interpolated in rate space, composing the shared
//! [`InflationTermStructureBase`] holder with an [`InterpolatedCurve`];
//! [`YoYInflationCurve`] is the C++ `typedef` for the linearly interpolated
//! case (`interpolatedyoyinflationcurve.hpp:87`).
//!
//! The quoted rates are *not* year-on-year inflation-swap quotes
//! (`interpolatedyoyinflationcurve.hpp:37`); a curve fitted to those is
//! [`PiecewiseYoYInflationCurve`](super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve).
//!
//! It mirrors
//! [`InterpolatedZeroInflationCurve`](super::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve)
//! throughout, including the node times being measured from the *reference
//! date* rather than from the first node, which leaves the first time negative.
//! The one divergence between the two is the base rate: C++ forwards
//! `rates[0]` into the base constructor's `baseRate` slot
//! (`interpolatedyoyinflationcurve.hpp:99-100`) where the zero curve passes
//! nothing, so [`base_rate`](InflationTermStructure::base_rate) succeeds here
//! and errors there.
//!
//! ## Divergences from QuantLib
//!
//! - The protected constructor for descendants that cannot supply their nodes
//!   at construction (`interpolatedyoyinflationcurve.hpp:79-85,124-133`) is
//!   omitted rather than deferred: it exists in C++ only because
//!   `PiecewiseYoYInflationCurve` *derives* from this class, and this port's
//!   piecewise curve is standalone (as its zero sibling is), so nothing could
//!   call it.
//! - The seasonality constructor argument (`:50`) is ported, but the
//!   consistency gate C++ runs from the base constructor runs here from this
//!   one, once the curve exists; see the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergences.
//! - C++ reads `dates.at(0)` in the initializer list *before* checking the
//!   size, throwing `out_of_range` on an empty vector; here the size check
//!   runs first and every input problem is a `QlError` per D4.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::Shared;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, YoYInflationTermStructure,
};
use crate::termstructures::inflation::seasonality::Seasonality;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

/// Inflation term structure based on the interpolation of year-on-year rates.
///
/// The first date is the base date, the last date for which the fixing is
/// known; the reference date is passed separately and normally follows it. The
/// first rate is the base rate the curve publishes.
pub struct InterpolatedYoYInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
}

/// Inflation term structure based on linear interpolation of year-on-year
/// rates (C++'s `YoYInflationCurve` typedef).
pub type YoYInflationCurve = InterpolatedYoYInflationCurve<Linear>;

impl<I: Interpolator> InterpolatedYoYInflationCurve<I> {
    /// Curve through the year-on-year inflation rates quoted at the given
    /// dates, the first of which is the base date
    /// (`interpolatedyoyinflationcurve.hpp:93-121`).
    ///
    /// The validations run in C++'s order, so their precedence is preserved.
    /// The bound on the rates starts at the *second* node (`:112`), leaving the
    /// base rate unconstrained, and admits negative rates down to but not
    /// including `-1.0`: year-on-year inflation may be negative, but a fall of
    /// a hundred per cent or more is not a rate. It is spelled as the negation
    /// C++'s `QL_REQUIRE` reduces to, so `NaN` fails it exactly as it does
    /// there.
    pub fn new(
        reference_date: Date,
        dates: Vec<Date>,
        rates: Vec<Rate>,
        frequency: Frequency,
        day_counter: DayCounter,
        interpolator: I,
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> QlResult<InterpolatedYoYInflationCurve<I>> {
        require!(dates.len() > 1, "too few dates: {}", dates.len());
        require!(
            rates.len() == dates.len(),
            "indices/dates count mismatch: {} vs {}",
            rates.len(),
            dates.len()
        );
        for &rate in &rates[1..] {
            if rate <= -1.0 || rate.is_nan() {
                fail!("year-on-year inflation data < -100 %");
            }
        }

        let base_date = dates[0];
        let base_rate = rates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, rates, interpolator);
        curve.setup_interpolation()?;
        let inflation = InflationTermStructureBase::with_reference_date(
            reference_date,
            base_date,
            frequency,
            Some(day_counter),
            Some(base_rate),
            seasonality,
        );
        let curve = InterpolatedYoYInflationCurve {
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

    /// The node values (year-on-year inflation rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node values (year-on-year inflation rates).
    pub fn rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The curve nodes as (date, year-on-year rate) pairs.
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

impl<I: Interpolator> AsObservable for InterpolatedYoYInflationCurve<I> {
    fn observable(&self) -> &Observable {
        self.inflation.term_structure_base().observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedYoYInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| self.last_date())
    }
}

impl<I: Interpolator> InflationTermStructure for InterpolatedYoYInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }

    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
        self
    }
}

impl<I: Interpolator> YoYInflationTermStructure for InterpolatedYoYInflationCurve<I> {
    /// C++ evaluates with extrapolation allowed at the interpolation level,
    /// `interpolation_(t, true)` (`interpolatedyoyinflationcurve.hpp:149`);
    /// range policy lives in the callers above, and this impl must assume
    /// extrapolation is required. Past the last node the last segment
    /// continues on its own slope, which for [`Linear`] is exactly C++'s
    /// extension - the same extension #907 applied to
    /// [`PiecewiseYoYInflationCurve`](super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve).
    fn yoy_rate_impl(&self, t: Time) -> QlResult<Rate> {
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
    //! QuantLib constructs this curve nowhere in its test suite - the only
    //! year-on-year curve `inflation.cpp` builds is the piecewise one
    //! (`:1109`), through the swap helper - so there is no C++ number to pin
    //! against. What is pinned instead is the interpolation itself: the rates
    //! and dates go in explicitly, so every published rate is a hand-computable
    //! function of them, and the fixture's rates are chosen so that no two
    //! agree and the base rate matches none of the interior ones.

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

    /// Node dates on the first of a month, so that a query date quantized by
    /// [`inflation_period`](crate::indexes::inflationindex::inflation_period)
    /// lands on a node rather than between two.
    fn sample_dates() -> Vec<Date> {
        vec![
            base_date(),
            Date::new(1, Month::December, 2022),
            Date::new(1, Month::December, 2023),
            Date::new(1, Month::December, 2024),
        ]
    }

    /// Every rate distinct, and the base rate distinct from all of them: a
    /// readback of the base rate would be degenerate otherwise.
    fn sample_rates() -> Vec<Rate> {
        vec![0.029, 0.021, 0.024, 0.026]
    }

    fn build(dates: Vec<Date>, rates: Vec<Rate>) -> QlResult<YoYInflationCurve> {
        YoYInflationCurve::new(
            today(),
            dates,
            rates,
            Frequency::Monthly,
            Actual360::new(),
            Linear,
            None,
        )
    }

    fn sample() -> YoYInflationCurve {
        build(sample_dates(), sample_rates()).unwrap()
    }

    /// The base rate is the first quoted rate, where a zero curve carries none
    /// at all. The fixture's `0.029` is not any interior rate, so this cannot
    /// pass on a curve that read the wrong node.
    #[test]
    fn the_base_rate_is_the_first_rate_where_a_zero_curve_has_none() {
        let curve = sample();
        assert_eq!(curve.base_rate().unwrap(), 0.029);
        assert_eq!(curve.base_date(), base_date());
        assert!(
            !curve.rates()[1..].contains(&curve.base_rate().unwrap()),
            "the base rate must differ from every interior node"
        );
    }

    /// At a node the published rate is that node's rate, and between two it is
    /// the hand-computed linear interpolant. Both entry points are checked: the
    /// date one quantizes to the period start first, so a mid-month date must
    /// answer its month's rate exactly.
    #[test]
    fn the_curve_interpolates_linearly_between_the_quoted_nodes() {
        let curve = sample();
        let (times, rates) = (curve.times(), curve.rates());

        for (i, date) in sample_dates().iter().enumerate() {
            assert!((curve.yoy_rate(times[i], false).unwrap() - rates[i]).abs() < 1.0e-12);
            assert!((curve.yoy_rate_date(*date, false).unwrap() - rates[i]).abs() < 1.0e-12);
        }

        let midpoint = 0.5 * (times[1] + times[2]);
        let expected = 0.5 * (rates[1] + rates[2]);
        assert!((curve.yoy_rate(midpoint, false).unwrap() - expected).abs() < 1.0e-12);

        let mid_month = Date::new(17, Month::December, 2023);
        assert!((curve.yoy_rate_date(mid_month, false).unwrap() - rates[2]).abs() < 1.0e-12);
    }

    #[test]
    fn the_base_node_sits_before_the_reference_date_at_a_negative_time() {
        let curve = sample();
        assert_eq!(curve.reference_date().unwrap(), today());
        assert_eq!(curve.times()[0], -57.0 / 360.0);
        assert_eq!(
            curve.times()[0],
            curve.time_from_reference(base_date()).unwrap()
        );
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
        assert!(build_err(vec![base_date()], vec![0.029]).contains("too few dates"));
        assert!(
            build_err(sample_dates(), vec![0.029, 0.021]).contains("indices/dates count mismatch")
        );
    }

    /// The `> -1.0` bound skips the base node, so a base rate below it is
    /// accepted while the same figure at an interior node is not; `NaN` fails
    /// the same check, as it fails C++'s.
    #[test]
    fn the_minus_one_bound_skips_the_base_node() {
        let mut rates = sample_rates();
        rates[1] = -1.0;
        assert!(build_err(sample_dates(), rates).contains("year-on-year inflation data < -100 %"));

        let mut rates = sample_rates();
        rates[1] = Rate::NAN;
        assert!(build_err(sample_dates(), rates).contains("year-on-year inflation data < -100 %"));

        let mut rates = sample_rates();
        rates[1] = -0.5;
        assert_eq!(build(sample_dates(), rates).unwrap().rates()[1], -0.5);

        let mut rates = sample_rates();
        rates[0] = -1.5;
        assert_eq!(
            build(sample_dates(), rates).unwrap().base_rate().unwrap(),
            -1.5
        );
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = sample();
        assert_eq!(curve.dates(), &sample_dates()[..]);
        assert_eq!(curve.rates(), &sample_rates()[..]);
        assert_eq!(curve.data(), curve.rates());
        assert_eq!(curve.nodes().len(), sample_dates().len());
        assert_eq!(curve.nodes()[0], (base_date(), 0.029));
        assert_eq!(curve.frequency(), Frequency::Monthly);
        assert_eq!(curve.max_date(), Date::new(1, Month::December, 2024));
        assert_eq!(curve.max_time().unwrap(), *curve.times().last().unwrap());
    }

    /// The seasonality gate runs from this constructor, as it does on the zero
    /// curve: twelve monthly factors are consistent with any curve, a
    /// twenty-four-factor set is the multi-year case this port defers.
    #[test]
    fn the_constructor_installs_and_gates_a_seasonality() {
        fn built(count: usize) -> QlResult<YoYInflationCurve> {
            let seasonality = shared(
                MultiplicativePriceSeasonality::new(
                    Date::new(31, Month::January, 2022),
                    Frequency::Monthly,
                    (0..count).map(|i| 1.0 + i as Rate / 1000.0).collect(),
                )
                .expect("a whole multiple of twelve factors"),
            ) as Shared<dyn Seasonality>;
            YoYInflationCurve::new(
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

    /// Past the last node the last segment continues on its own slope, as
    /// C++'s `interpolation_(t, true)` extends it; the range check still gates
    /// until extrapolation is requested, and the base date still bounds below.
    #[test]
    fn extrapolating_past_the_last_node_extends_the_end_segment_as_cpp_does() {
        let curve = sample();
        let beyond = curve.max_date() + Period::new(1, TimeUnit::Years);

        let gated = curve.yoy_rate_date(beyond, false).unwrap_err();
        assert!(gated.message().contains("is past max curve date"));

        let (t_lo, t_hi) = (curve.times()[2], curve.times()[3]);
        assert_eq!(t_lo, 673.0 / 360.0);
        assert_eq!(t_hi, 1039.0 / 360.0);
        let t = curve.time_from_reference(beyond).unwrap();
        assert_eq!(t, 1404.0 / 360.0);
        let expected = 0.026 + (0.026 - 0.024) / (t_hi - t_lo) * (t - t_hi);

        let extended = curve.yoy_rate_date(beyond, true).unwrap();
        assert!((extended - expected).abs() < 1.0e-12);

        let before = curve
            .yoy_rate_date(Date::new(15, Month::November, 2021), false)
            .unwrap_err();
        assert!(before.message().contains("is before base date"));
    }
}
