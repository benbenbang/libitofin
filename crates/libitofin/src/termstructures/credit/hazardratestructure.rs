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
//! - The numeric `survivalProbabilityImpl` fallback
//!   (`hazardratestructure.cpp:77-82`), which integrates the hazard rate under
//!   a 48-point Gauss-Chebyshev quadrature with a `[-1, 1]` to `[0, t]`
//!   remapping and its `t / 2` Jacobian, is not ported:
//!   [`survival_probability_impl`](DefaultProbabilityTermStructure::survival_probability_impl)
//!   stays a required method of the base trait. Curves with a closed-form
//!   survival probability supply it directly and never reach the quadrature
//!   (`flathazardrate.hpp:64,72-74`); the interpolated hazard curve that does
//!   need it arrives with the bootstrapped credit curves in EPIC Credit
//!   (#676), for which
//!   [`GaussianQuadrature::chebyshev`](crate::math::integrals::gaussianquadratures::GaussianQuadrature::chebyshev)
//!   is already available.
//!
//! - The three C++ constructors (`hazardratestructure.cpp:52-71`) only forward
//!   the day counter, jumps and jump dates to the base, so this adapter is a
//!   stateless trait. The jump machinery is deferred with the rest of it; see
//!   the
//!   [`defaulttermstructure`](crate::termstructures::credit::defaulttermstructure)
//!   module documentation.

use crate::errors::QlResult;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::types::{Rate, Real, Time};

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
    use crate::types::Probability;

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
}
