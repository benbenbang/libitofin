//! Interpolated survival probabilities with flat terminal hazard extrapolation.
//!
//! Port of `ql/termstructures/credit/interpolatedsurvivalprobabilitycurve.hpp`.
//! Jump quotes are not supported, consistently with the credit base trait.

use crate::errors::QlResult;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::survivalprobabilitystructure::SurvivalProbabilityStructure;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Real, Time};

/// Credit curve interpolating survival probabilities at fixed dates.
pub struct InterpolatedSurvivalProbabilityCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

impl<I: Interpolator> InterpolatedSurvivalProbabilityCurve<I> {
    /// Builds a curve whose first date is the reference date.
    ///
    /// # Errors
    /// Rejects invalid dates, mismatched lengths, a first probability other than
    /// one, or subsequent probabilities that are not positive and nonincreasing.
    pub fn new(
        dates: Vec<Date>,
        probabilities: Vec<Probability>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Self> {
        Self::with_calendar(dates, probabilities, day_counter, None, interpolator)
    }

    /// Builds a survival curve carrying an optional calendar.
    pub fn with_calendar(
        dates: Vec<Date>,
        probabilities: Vec<Probability>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<Self> {
        require!(
            dates.len() >= interpolator.required_points().max(1),
            "not enough input dates given"
        );
        require!(
            dates.len() == probabilities.len(),
            "dates/data count mismatch"
        );
        require!(
            probabilities[0] == 1.0,
            "the first probability must be == 1.0"
        );
        require!(
            probabilities
                .windows(2)
                .all(|pair| pair[1] > 0.0 && pair[1] <= pair[0]),
            "survival probabilities must be positive and nonincreasing"
        );
        let reference_date = dates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, probabilities, interpolator);
        curve.setup_interpolation()?;
        Ok(Self {
            base: TermStructureBase::with_reference_date(
                reference_date,
                calendar,
                Some(day_counter),
            ),
            dates,
            curve,
        })
    }

    /// The node dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node survival probabilities.
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node survival probabilities.
    pub fn survival_probabilities(&self) -> &[Probability] {
        self.data()
    }

    /// The date and survival-probability nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.data().iter().copied())
            .collect()
    }
}

pub(crate) fn survival_from_nodes<I: Interpolation>(
    interpolation: &I,
    t: Time,
) -> QlResult<Probability> {
    let end = interpolation.x_max();
    if t <= end {
        return interpolation.value(t);
    }
    let survival = interpolation.value(end)?;
    let hazard = -interpolation.derivative(end)? / survival;
    Ok(survival * (-hazard * (t - end)).exp())
}

pub(crate) fn density_from_nodes<I: Interpolation>(interpolation: &I, t: Time) -> QlResult<Real> {
    let end = interpolation.x_max();
    if t <= end {
        return Ok(-interpolation.derivative(t)?);
    }
    let survival = interpolation.value(end)?;
    let hazard = -interpolation.derivative(end)? / survival;
    Ok(survival * hazard * (-hazard * (t - end)).exp())
}

impl<I: Interpolator> AsObservable for InterpolatedSurvivalProbabilityCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedSurvivalProbabilityCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("the constructor requires node dates")
    }
}

impl<I: Interpolator + 'static> SurvivalProbabilityStructure
    for InterpolatedSurvivalProbabilityCurve<I>
{
}

impl<I: Interpolator + 'static> DefaultProbabilityTermStructure
    for InterpolatedSurvivalProbabilityCurve<I>
{
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        survival_from_nodes(self.curve.interpolation()?, t)
    }
    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        density_from_nodes(self.curve.interpolation()?, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::linear::Linear;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn dates() -> Vec<Date> {
        let reference = Date::new(9, Month::June, 2006);
        vec![reference, reference + 360, reference + 720]
    }

    #[test]
    fn loglinear_survival_has_exact_piecewise_hazards_and_flat_tail() {
        let curve = InterpolatedSurvivalProbabilityCurve::new(
            dates(),
            vec![1.0, (-0.02_f64).exp(), (-0.05_f64).exp()],
            Actual360::new(),
            LogLinear,
        )
        .unwrap();
        for (t, integrated, hazard) in [
            (0.0, 0.0_f64, 0.02),
            (0.5, 0.01, 0.02),
            (1.5, 0.035, 0.03),
            (2.0, 0.05, 0.03),
            (4.0, 0.11, 0.03),
        ] {
            let survival = (-integrated).exp();
            assert!((curve.survival_probability(t, true).unwrap() - survival).abs() < 1e-14);
            assert!((curve.default_density(t, true).unwrap() - hazard * survival).abs() < 1e-14);
            assert!((curve.hazard_rate(t, true).unwrap() - hazard).abs() < 1e-14);
        }
        assert!(curve.survival_probability(4.0, false).is_err());
        assert!(curve.survival_probability(-0.1, true).is_err());
        assert_eq!(curve.dates(), dates());
        assert_eq!(curve.times(), [0.0, 1.0, 2.0]);
        assert_eq!(curve.survival_probabilities(), curve.data());
        assert_eq!(curve.nodes()[2], (dates()[2], (-0.05_f64).exp()));
    }

    #[test]
    fn linear_survival_extrapolates_hazard_instead_of_probability() {
        let curve = InterpolatedSurvivalProbabilityCurve::new(
            dates(),
            vec![1.0, 0.9, 0.8],
            Actual360::new(),
            Linear,
        )
        .unwrap();
        let expected = 0.8 * (-0.125_f64).exp();
        assert!((curve.survival_probability(3.0, true).unwrap() - expected).abs() < 1e-14);
        assert!((curve.default_density(3.0, true).unwrap() - expected * 0.125).abs() < 1e-14);
        for t in [0.0, 0.5, 1.5] {
            assert!(
                (curve.default_density_from_survival_probability(t).unwrap() - 0.1).abs() < 1e-11
            );
        }
    }

    #[test]
    fn invalid_probability_nodes_are_rejected() {
        for probabilities in [
            vec![],
            vec![1.0],
            vec![0.9, 0.8, 0.7],
            vec![1.0, 0.9, 0.95],
            vec![1.0, 0.9, 0.0],
            vec![1.0, f64::NAN, 0.8],
            vec![1.0, f64::INFINITY, 0.8],
        ] {
            assert!(
                InterpolatedSurvivalProbabilityCurve::new(
                    dates(),
                    probabilities,
                    Actual360::new(),
                    LogLinear
                )
                .is_err()
            );
        }
        let mut reversed = dates();
        reversed.swap(1, 2);
        assert!(
            InterpolatedSurvivalProbabilityCurve::new(
                reversed,
                vec![1.0, 0.9, 0.8],
                Actual360::new(),
                LogLinear
            )
            .is_err()
        );
        assert!(
            InterpolatedSurvivalProbabilityCurve::new(vec![], vec![], Actual360::new(), LogLinear)
                .is_err()
        );
    }
}
