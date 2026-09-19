//! Hazard-rate term structure.
//!
//! Port of `ql/termstructures/credit/hazardratestructure.{hpp,cpp}`: the
//! [`HazardRateStructure`] adapter lets a credit curve quote only the hazard
//! rate and derive the default density from it,
//! `h(t) S(t)` (`hazardratestructure.hpp:106-108`).
//!
//! Hazard rates are defined with annual frequency and continuous compounding.
//!
//! ## Divergences from QuantLib
//!
//! - C++ derives the abstract class from `DefaultProbabilityTermStructure` and
//!   overrides `defaultDensityImpl`; a Rust blanket impl of
//!   [`DefaultProbabilityTermStructure`] for every [`HazardRateStructure`]
//!   would conflict (E0119) with curves implementing it directly, and with the
//!   sibling survival-probability and default-density adapters that follow in
//!   EPIC Credit (#676). The derivation is therefore the provided
//!   [`default_density_from_hazard_rate`](HazardRateStructure::default_density_from_hazard_rate),
//!   which each concrete curve wires in, as
//!   [`ZeroYieldStructure`](crate::termstructures::yields::ZeroYieldStructure)
//!   already does for discounts:
//!
//! ```ignore
//! impl DefaultProbabilityTermStructure for MyCurve {
//!     fn default_density_impl(&self, t: Time) -> QlResult<Real> {
//!         self.default_density_from_hazard_rate(t)
//!     }
//! }
//! ```
//!
//! - C++ re-abstracts `hazardRateImpl` to a `QL_FAIL`
//!   (`hazardratestructure.cpp:73-75`) to break a cycle: its base
//!   `hazardRateImpl` (density / survival) and this adapter's
//!   `defaultDensityImpl` (hazard * survival) are both inherited defaults, so
//!   a derived class overriding neither would recurse forever. This port needs
//!   no counterpart. The rate the derivation multiplies is
//!   [`hazard_rate_curve_impl`](HazardRateStructure::hazard_rate_curve_impl),
//!   a required method of this trait and not the base's derived
//!   [`hazard_rate_impl`](DefaultProbabilityTermStructure::hazard_rate_impl),
//!   so the density never routes back through that default; a curve leaving it
//!   in place evaluates it as `h(t) S(t) / S(t)`, which terminates on the
//!   quoted rate. The guard that remains is a compile-time one from the other
//!   side: [`HazardRateStructure`] cannot be implemented without supplying the
//!   hook, so the case C++ reports at run time cannot be written here.
//!
//! - The numeric `survivalProbabilityImpl` fallback is provided by
//!   [`survival_probability_from_hazard_rate`](HazardRateStructure::survival_probability_from_hazard_rate).
//!   It uses the same 48-point Gauss-Chebyshev rule, remapping and Jacobian
//!   as `hazardratestructure.cpp:77-82`. Concrete curves opt into it through
//!   their required `survival_probability_impl`; existing closed-form curves
//!   retain their exact formulas. Even constant hazards have a small quadrature
//!   error because this rule approximates an unweighted integral. Tests pin
//!   QuantLib 1.43 `GaussChebyshevIntegration(48)` results for `h(t) = .04`
//!   and `h(t) = .04 + .01*t + .002*t*t` at `1e-13`. Separate analytic
//!   checks bound the rule's approximation error over the tested five years.
//!   Regenerate the pins with `tests/fixtures/credit_hazard_quadrature.py`.
//!
//! - The three C++ constructors (`hazardratestructure.cpp:52-71`) only forward
//!   the day counter, jumps and jump dates to the base, so this adapter is a
//!   stateless trait. Concrete curves own optional jump state through the
//!   [`defaulttermstructure`](crate::termstructures::credit::defaulttermstructure)
//!   module documentation.

use std::sync::LazyLock;

use crate::errors::QlResult;
use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::types::{Probability, Rate, Real, Time};

/// Hazard-rate term structure: implement
/// [`hazard_rate_curve_impl`](Self::hazard_rate_curve_impl) and wire
/// [`default_density_from_hazard_rate`](Self::default_density_from_hazard_rate)
/// into
/// [`default_density_impl`](DefaultProbabilityTermStructure::default_density_impl).
///
/// Curves quoting the hazard rate should also override the base's derived
/// [`hazard_rate_impl`](DefaultProbabilityTermStructure::hazard_rate_impl)
/// with the same hook, which answers the quoted rate without the round trip
/// through the density.
pub trait HazardRateStructure: DefaultProbabilityTermStructure {
    /// Hazard-rate calculation, called after range checking; it must assume
    /// extrapolation is required.
    ///
    /// This is C++'s `hazardRateImpl` (`hazardratestructure.hpp:82`) under a
    /// name of its own: the base trait's
    /// [`hazard_rate_impl`](DefaultProbabilityTermStructure::hazard_rate_impl)
    /// is the rate *derived* from the density, and a curve implementing this
    /// adapter is the one *quoting* it.
    fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate>;

    /// Numerically derives survival from the quoted hazard rate using
    /// QuantLib's 48-point Gauss-Chebyshev rule on `[0, t]`.
    ///
    /// Wire this into `survival_probability_impl` only when no closed form is
    /// available. Like other implementation hooks, it assumes the public curve
    /// method has checked the time range, and excludes jumps. Evaluation errors
    /// propagate immediately, including at zero time.
    fn survival_probability_from_hazard_rate(&self, t: Time) -> QlResult<Probability> {
        static INTEGRAL: LazyLock<QlResult<GaussianQuadrature>> =
            LazyLock::new(|| GaussianQuadrature::chebyshev(48));
        let integral = INTEGRAL.as_ref().map_err(Clone::clone)?;
        let mut sum = 0.0;
        for (&weight, &x) in integral
            .weights()
            .iter()
            .zip(integral.abscissas().iter())
            .rev()
        {
            sum += weight * self.hazard_rate_curve_impl((x + 1.0) * t / 2.0)?;
        }
        Ok((-sum * t / 2.0).exp())
    }

    /// The default density calculated from the hazard rate as
    /// `h(t) S(t)` (C++'s `defaultDensityImpl`,
    /// `hazardratestructure.hpp:106-108`).
    ///
    /// Like C++ this multiplies by
    /// [`survival_probability_impl`](DefaultProbabilityTermStructure::survival_probability_impl)
    /// rather than the public survival probability, so it neither range-checks
    /// twice nor folds in jumps.
    fn default_density_from_hazard_rate(&self, t: Time) -> QlResult<Real> {
        let hazard_rate = self.hazard_rate_curve_impl(t)?;
        Ok(hazard_rate * self.survival_probability_impl(t)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fail;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::{TermStructure, TermStructureBase};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use std::cell::Cell;

    const INTENSITY: Real = 0.04;

    /// A curve wired through the adapter that leaves the base's derived
    /// [`DefaultProbabilityTermStructure::hazard_rate_impl`] in place, so the
    /// public hazard rate round-trips through the density.
    struct DerivedHazardCurve {
        base: TermStructureBase,
        failing: bool,
    }

    impl DerivedHazardCurve {
        fn new(failing: bool) -> DerivedHazardCurve {
            DerivedHazardCurve {
                base: TermStructureBase::with_reference_date(
                    Date::new(15, Month::June, 2026),
                    None,
                    Some(Actual360::new()),
                ),
                failing,
            }
        }
    }

    impl AsObservable for DerivedHazardCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for DerivedHazardCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl HazardRateStructure for DerivedHazardCurve {
        fn hazard_rate_curve_impl(&self, _t: Time) -> QlResult<Rate> {
            if self.failing {
                fail!("no hazard rate available");
            }
            Ok(INTENSITY)
        }
    }

    impl DefaultProbabilityTermStructure for DerivedHazardCurve {
        fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
            Ok((-INTENSITY * t).exp())
        }

        fn default_density_impl(&self, t: Time) -> QlResult<Real> {
            self.default_density_from_hazard_rate(t)
        }
    }

    /// The same curve with the base hazard rate overridden by the hook, the
    /// shape `FlatHazardRate` takes (`flathazardrate.hpp:59`).
    struct QuotedHazardCurve {
        inner: DerivedHazardCurve,
    }

    impl AsObservable for QuotedHazardCurve {
        fn observable(&self) -> &Observable {
            self.inner.observable()
        }
    }

    impl TermStructure for QuotedHazardCurve {
        fn base(&self) -> &TermStructureBase {
            self.inner.base()
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl HazardRateStructure for QuotedHazardCurve {
        fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate> {
            self.inner.hazard_rate_curve_impl(t)
        }
    }

    impl DefaultProbabilityTermStructure for QuotedHazardCurve {
        fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
            self.inner.survival_probability_impl(t)
        }

        fn default_density_impl(&self, t: Time) -> QlResult<Real> {
            self.default_density_from_hazard_rate(t)
        }

        fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
            self.hazard_rate_curve_impl(t)
        }
    }

    fn survival(t: Time) -> Probability {
        (-INTENSITY * t).exp()
    }

    #[test]
    fn default_density_is_the_hazard_rate_times_the_survival_probability() {
        let curve = DerivedHazardCurve::new(false);
        for t in [0.0_f64, 0.25, 1.0, 2.5] {
            let expected = INTENSITY * survival(t);
            assert!((curve.default_density(t, false).unwrap() - expected).abs() < 1.0e-15);
            assert!(
                (curve.default_density_from_hazard_rate(t).unwrap() - expected).abs() < 1.0e-15
            );
        }
    }

    #[test]
    fn the_derived_hazard_rate_closes_back_on_the_quoted_one() {
        let curve = DerivedHazardCurve::new(false);
        for t in [0.25_f64, 1.0, 2.5] {
            assert!((curve.hazard_rate(t, false).unwrap() - INTENSITY).abs() < 1.0e-15);
        }
    }

    #[test]
    fn overriding_the_base_hazard_rate_agrees_with_the_derived_one() {
        let quoted = QuotedHazardCurve {
            inner: DerivedHazardCurve::new(false),
        };
        let derived = DerivedHazardCurve::new(false);
        for t in [0.25_f64, 1.0, 2.5] {
            assert_eq!(quoted.hazard_rate(t, false).unwrap(), INTENSITY);
            assert!(
                (quoted.hazard_rate(t, false).unwrap() - derived.hazard_rate(t, false).unwrap())
                    .abs()
                    < 1.0e-15
            );
            assert!(
                (quoted.default_density(t, false).unwrap()
                    - derived.default_density(t, false).unwrap())
                .abs()
                    < 1.0e-15
            );
        }
    }

    #[test]
    fn the_survival_probability_is_left_to_the_curve() {
        let curve = DerivedHazardCurve::new(false);
        assert!((curve.survival_probability(2.5, false).unwrap() - survival(2.5)).abs() < 1.0e-15);
        assert!(
            (curve.default_probability(2.5, false).unwrap() - (1.0 - survival(2.5))).abs()
                < 1.0e-15
        );
    }

    #[test]
    fn hazard_rate_errors_propagate_through_the_default_density() {
        let curve = DerivedHazardCurve::new(true);
        let err = curve.default_density(1.0, false).unwrap_err();
        assert!(err.message().contains("no hazard rate available"));
        let err = curve.hazard_rate(1.0, false).unwrap_err();
        assert!(err.message().contains("no hazard rate available"));
        assert!((curve.survival_probability(1.0, false).unwrap() - survival(1.0)).abs() < 1.0e-15);
    }

    struct QuadratureCurve {
        inner: DerivedHazardCurve,
        hazard: fn(Time) -> QlResult<Rate>,
        evaluations: Cell<usize>,
    }

    impl QuadratureCurve {
        fn new(hazard: fn(Time) -> QlResult<Rate>) -> Self {
            Self {
                inner: DerivedHazardCurve::new(false),
                hazard,
                evaluations: Cell::new(0),
            }
        }
    }

    impl AsObservable for QuadratureCurve {
        fn observable(&self) -> &Observable {
            self.inner.observable()
        }
    }

    impl TermStructure for QuadratureCurve {
        fn base(&self) -> &TermStructureBase {
            self.inner.base()
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl HazardRateStructure for QuadratureCurve {
        fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate> {
            self.evaluations.set(self.evaluations.get() + 1);
            (self.hazard)(t)
        }
    }

    impl DefaultProbabilityTermStructure for QuadratureCurve {
        fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
            self.survival_probability_from_hazard_rate(t)
        }

        fn default_density_impl(&self, t: Time) -> QlResult<Real> {
            self.default_density_from_hazard_rate(t)
        }
    }

    #[test]
    fn quadrature_matches_quantlib_and_analytic_survival() {
        let constant = QuadratureCurve::new(|_| Ok(0.04));
        let varying = QuadratureCurve::new(|t| Ok(0.04 + 0.01 * t + 0.002 * t * t));
        let cases = [
            (0.0, 1.0, 1.0),
            (0.25, 0.9900480664219722, 0.9897283570410018),
            (1.0, 0.9607825787915585, 0.9553525176808173),
            (2.5, 0.9048212660113185, 0.867887755838896),
            (5.0, 0.8187015234263251, 0.6647038533745758),
        ];
        for (t, constant_ql, varying_ql) in cases {
            let flat = constant.survival_probability(t, false).unwrap();
            let curved = varying.survival_probability(t, false).unwrap();
            assert!((flat - constant_ql).abs() < 1.0e-13);
            assert!((curved - varying_ql).abs() < 1.0e-13);
            assert!((flat - (-0.04 * t).exp()).abs() < 3.0e-5);
            let integrated = 0.04 * t + 0.005 * t * t + 0.002 * t.powi(3) / 3.0;
            assert!((curved - (-integrated).exp()).abs() < 6.0e-5);
            assert!((constant.hazard_rate(t, false).unwrap() - 0.04).abs() < 1.0e-15);
        }
    }

    #[test]
    fn quadrature_maps_all_48_nodes_into_the_time_interval() {
        let curve = QuadratureCurve::new(|t| {
            assert!((0.0..2.5).contains(&t));
            Ok(0.04)
        });
        curve.survival_probability(2.5, false).unwrap();
        assert_eq!(curve.evaluations.get(), 48);
        curve.evaluations.set(0);
        assert_eq!(curve.survival_probability(0.0, false).unwrap(), 1.0);
        assert_eq!(curve.evaluations.get(), 48);
    }

    #[test]
    fn quadrature_propagates_hazard_errors_and_rejects_invalid_public_times() {
        let curve = QuadratureCurve::new(|_| fail!("hazard unavailable"));
        for t in [0.0, 1.0] {
            let err = curve.survival_probability(t, false).unwrap_err();
            assert_eq!(err.message(), "hazard unavailable");
        }
        assert_eq!(curve.evaluations.get(), 2);
        for t in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(curve.survival_probability(t, true).is_err());
        }
        assert_eq!(curve.evaluations.get(), 2);
        let late_failure = QuadratureCurve::new(|t| {
            if t > 0.5 {
                fail!("late hazard failure");
            }
            Ok(0.04)
        });
        let err = late_failure.survival_probability(1.0, false).unwrap_err();
        assert_eq!(err.message(), "late hazard failure");
        assert!(late_failure.evaluations.get() > 1);
        assert!(late_failure.evaluations.get() < 48);
    }
}
