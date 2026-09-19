//! Analytic gates for QuantLib's default-density adapter and interpolation contract.

use libitofin::errors::QlResult;
use libitofin::math::interpolations::{Interpolator, flat::BackwardFlat, linear::Linear};
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::termstructures::credit::defaultdensitystructure::DefaultDensityStructure;
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::interpolateddefaultdensitycurve::InterpolatedDefaultDensityCurve;
use libitofin::termstructures::{TermStructure, TermStructureBase};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;

fn dates() -> Vec<Date> {
    let start = Date::new(9, Month::June, 2006);
    vec![start, start + 360, start + 720]
}

fn curve<I: Interpolator>(interpolator: I) -> InterpolatedDefaultDensityCurve<I> {
    InterpolatedDefaultDensityCurve::new(
        dates(),
        vec![0.02, 0.04, 0.08],
        Actual360::new(),
        interpolator,
    )
    .unwrap()
}

#[test]
fn default_density_linear_integrates_trapezoids_and_extrapolates_density() {
    let curve = curve(Linear);
    for (t, density, integrated) in [
        (0.0, 0.02, 0.0),
        (0.5, 0.03, 0.0125),
        (1.0, 0.04, 0.03),
        (1.5, 0.06, 0.055),
        (2.0, 0.08, 0.09),
        (4.0, 0.08, 0.25),
    ] {
        let survival = 1.0 - integrated;
        assert!((curve.default_density(t, true).unwrap() - density).abs() < 1e-14);
        assert!((curve.survival_probability(t, true).unwrap() - survival).abs() < 1e-14);
        assert!((curve.hazard_rate(t, true).unwrap() - density / survival).abs() < 1e-14);
    }
    assert_eq!(curve.survival_probability(20.0, true).unwrap(), 0.0);
    assert_eq!(curve.hazard_rate(20.0, true).unwrap(), 0.0);
    assert_eq!(curve.default_density(20.0, true).unwrap(), 0.08);
    assert!(curve.survival_probability(4.0, false).is_err());
    assert!(curve.default_density(-0.1, true).is_err());
    assert_eq!(curve.dates(), dates());
    assert_eq!(curve.times(), [0.0, 1.0, 2.0]);
    assert_eq!(curve.data(), curve.default_densities());
    assert_eq!(curve.nodes()[2], (dates()[2], 0.08));
}

#[test]
fn default_density_backward_flat_integrates_rectangles() {
    let curve = curve(BackwardFlat);
    for (t, density, survival) in [
        (0.0, 0.02, 1.0),
        (0.5, 0.04, 0.98),
        (1.0, 0.04, 0.96),
        (1.5, 0.08, 0.92),
        (3.0, 0.08, 0.80),
    ] {
        assert!((curve.default_density(t, true).unwrap() - density).abs() < 1e-14);
        assert!((curve.survival_probability(t, true).unwrap() - survival).abs() < 1e-14);
    }
}

#[test]
fn default_density_validates_nodes() {
    for densities in [
        vec![0.01, -0.01, 0.02],
        vec![0.01, f64::NAN, 0.02],
        vec![0.01, f64::INFINITY, 0.02],
        vec![0.01],
    ] {
        assert!(
            InterpolatedDefaultDensityCurve::new(dates(), densities, Actual360::new(), Linear)
                .is_err()
        );
    }
    for invalid in [
        vec![],
        vec![Date::null(), dates()[1]],
        vec![dates()[1], dates()[0]],
        vec![dates()[0], dates()[0]],
    ] {
        assert!(
            InterpolatedDefaultDensityCurve::new(
                invalid.clone(),
                vec![0.01; invalid.len()],
                Actual360::new(),
                Linear
            )
            .is_err()
        );
    }
    let zero =
        InterpolatedDefaultDensityCurve::new(dates(), vec![0.0; 3], Actual360::new(), Linear)
            .unwrap();
    assert_eq!(zero.survival_probability(100.0, true).unwrap(), 1.0);
    let single = InterpolatedDefaultDensityCurve::new(
        vec![dates()[0]],
        vec![0.1],
        Actual360::new(),
        BackwardFlat,
    )
    .unwrap();
    assert!((single.survival_probability(3.0, true).unwrap() - 0.7).abs() < 1e-14);
}

struct DensityAdapter {
    base: TermStructureBase,
    failing: bool,
    level: f64,
}
impl AsObservable for DensityAdapter {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl TermStructure for DensityAdapter {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        Date::max_date()
    }
}
impl DefaultDensityStructure for DensityAdapter {}
impl DefaultProbabilityTermStructure for DensityAdapter {
    fn survival_probability_impl(&self, t: f64) -> QlResult<f64> {
        self.survival_probability_from_default_density(t)
    }
    fn default_density_impl(&self, t: f64) -> QlResult<f64> {
        if self.failing {
            libitofin::fail!("density unavailable");
        }
        Ok(self.level + 0.01 * t)
    }
}

#[test]
fn default_density_adapter_matches_independent_chebyshev_weights_and_propagates_errors() {
    let mut curve = DensityAdapter {
        base: TermStructureBase::with_reference_date(dates()[0], None, Some(Actual360::new())),
        failing: false,
        level: 0.02,
    };
    for t in [0.0, 0.5, 2.0, 20.0] {
        let integral: f64 = (1..=48)
            .map(|k| {
                let theta = (2 * k - 1) as f64 * std::f64::consts::PI / 96.0;
                let time = (theta.cos() + 1.0) * t / 2.0;
                std::f64::consts::PI / 48.0 * theta.sin() * (0.02 + 0.01 * time)
            })
            .sum();
        let expected = (1.0 - integral * t / 2.0).max(0.0);
        assert!((curve.survival_probability(t, true).unwrap() - expected).abs() < 1e-13);
    }
    curve.failing = true;
    for t in [0.0, 1.0] {
        assert!(
            curve
                .survival_probability(t, true)
                .unwrap_err()
                .message()
                .contains("density unavailable")
        );
    }
}

#[test]
fn default_density_bootstrap_traits_preserve_positive_brackets_and_first_node() {
    use libitofin::termstructures::bootstraptraits::BootstrapTraits;
    use libitofin::termstructures::credit::probabilitytraits::DefaultDensity;

    let times = [0.0, 1.0, 2.0];
    let mut data = [0.01, 0.02, 0.04];
    assert_eq!(DefaultDensity::initial_value(), 0.01);
    assert_eq!(DefaultDensity::max_iterations(), 30);
    assert_eq!(DefaultDensity::guess(1, &times, &data, false), 0.01);
    assert_eq!(DefaultDensity::guess(2, &times, &data, false), 0.02);
    assert_eq!(DefaultDensity::guess(2, &times, &data, true), 0.04);
    assert_eq!(
        DefaultDensity::min_value_after(2, &times, &data, false),
        f64::EPSILON
    );
    assert_eq!(
        DefaultDensity::max_value_after(2, &times, &data, false),
        1.0
    );
    assert_eq!(
        DefaultDensity::min_value_after(2, &times, &data, true),
        0.005
    );
    assert_eq!(
        DefaultDensity::max_value_after(2, &times, &data, true),
        0.08
    );
    DefaultDensity::update_guess(&mut data, 0.03, 1);
    assert_eq!(data, [0.03, 0.03, 0.04]);
    DefaultDensity::update_guess(&mut data, 0.05, 2);
    assert_eq!(data, [0.03, 0.03, 0.05]);
}

#[test]
fn default_density_adapter_does_not_mask_nan_as_zero_survival() {
    let curve = DensityAdapter {
        base: TermStructureBase::with_reference_date(dates()[0], None, Some(Actual360::new())),
        failing: false,
        level: f64::NAN,
    };
    assert!(curve.survival_probability(1.0, false).unwrap().is_nan());
}

#[test]
fn default_density_linear_does_not_mask_overflow_nan_as_zero_survival() {
    let start = dates()[0];
    let curve = InterpolatedDefaultDensityCurve::new(
        vec![start, start + 1, start + 2],
        vec![1e308, 0.0, 1e308],
        Actual360::new(),
        Linear,
    )
    .unwrap();
    assert!(
        curve
            .survival_probability_date(start + 1, false)
            .unwrap()
            .is_nan()
    );
}
