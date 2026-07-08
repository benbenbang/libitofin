//! Forward-rate based yield term structure.
//!
//! Port of `ql/termstructures/yield/forwardstructure.{hpp,cpp}`: the
//! [`ForwardRateStructure`] adapter lets a curve implement only
//! [`forward_impl`](ForwardRateStructure::forward_impl); zero yields are
//! derived by integrating the instantaneous forwards and discounts follow
//! through [`ZeroYieldStructure`].
//!
//! Forward rates are assumed to be annual continuous compounding.
//!
//! QuantLib deprecates this class (since 1.42) because the default
//! integration is a slow 1000-step trapezoid rule: implement
//! [`ZeroYieldStructure`] directly when a closed-form zero yield exists, or
//! override [`zero_yield_impl`](ZeroYieldStructure::zero_yield_impl) with an
//! exact integral as the interpolated forward curve does.
//!
//! ## Divergences from QuantLib
//!
//! As with [`ZeroYieldStructure`], the C++ `zeroYieldImpl` override becomes
//! the provided
//! [`zero_yield_from_forwards`](ForwardRateStructure::zero_yield_from_forwards)
//! that each concrete curve wires in:
//!
//! ```ignore
//! impl ZeroYieldStructure for MyCurve {
//!     fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
//!         self.zero_yield_from_forwards(t)
//!     }
//! }
//! ```

use crate::errors::QlResult;
use crate::termstructures::yields::ZeroYieldStructure;
use crate::types::{Rate, Time};

/// Forward-rate term structure: implement
/// [`forward_impl`](Self::forward_impl) and wire
/// [`zero_yield_from_forwards`](Self::zero_yield_from_forwards) into
/// [`zero_yield_impl`](ZeroYieldStructure::zero_yield_impl).
pub trait ForwardRateStructure: ZeroYieldStructure {
    /// Instantaneous forward-rate calculation, called after range checking;
    /// it must assume extrapolation is required.
    fn forward_impl(&self, t: Time) -> QlResult<Rate>;

    /// The zero yield calculated as the average of the instantaneous forward
    /// over `[0, t]` (C++'s `zeroYieldImpl`), by a 1000-step trapezoid rule;
    /// at `t = 0` it returns `forward_impl(0.0)`.
    fn zero_yield_from_forwards(&self, t: Time) -> QlResult<Rate> {
        if t == 0.0 {
            return self.forward_impl(0.0);
        }
        let mut sum = 0.5 * self.forward_impl(0.0)?;
        let dt = t / 1000.0;
        let mut i = dt;
        while i < t {
            sum += self.forward_impl(i)?;
            i += dt;
        }
        sum += 0.5 * self.forward_impl(t)?;
        Ok(sum * dt / t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::termstructures::{TermStructure, TermStructureBase};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::types::{DiscountFactor, Real};

    struct LinearForwardCurve {
        base: TermStructureBase,
        a: Real,
        b: Real,
        exact_zero_yield: bool,
    }

    impl LinearForwardCurve {
        fn new(a: Real, b: Real, exact_zero_yield: bool) -> LinearForwardCurve {
            LinearForwardCurve {
                base: TermStructureBase::with_reference_date(
                    Date::new(15, Month::June, 2026),
                    None,
                    Some(Actual360::new()),
                ),
                a,
                b,
                exact_zero_yield,
            }
        }
    }

    impl AsObservable for LinearForwardCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for LinearForwardCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl ForwardRateStructure for LinearForwardCurve {
        fn forward_impl(&self, t: Time) -> QlResult<Rate> {
            Ok(self.a + self.b * t)
        }
    }

    impl ZeroYieldStructure for LinearForwardCurve {
        fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
            if self.exact_zero_yield {
                Ok(self.a + 0.5 * self.b * t)
            } else {
                self.zero_yield_from_forwards(t)
            }
        }
    }

    impl YieldTermStructure for LinearForwardCurve {
        fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
            self.discount_from_zero_yield(t)
        }
    }

    #[test]
    fn zero_yield_at_zero_is_the_instantaneous_forward() {
        let curve = LinearForwardCurve::new(0.03, 0.01, false);
        assert_eq!(curve.zero_yield_from_forwards(0.0).unwrap(), 0.03);
    }

    #[test]
    fn flat_forwards_integrate_to_the_flat_zero_yield() {
        let curve = LinearForwardCurve::new(0.05, 0.0, false);
        // t = 1000 * 2^-10 makes the trapezoid step exactly representable, so
        // the integration hits exactly 999 interior nodes and is exact.
        let t = 0.9765625;
        assert!((curve.zero_yield_impl(t).unwrap() - 0.05).abs() < 1.0e-14);
        let df = curve.discount(t, false).unwrap();
        assert!((df - (-0.05 * t).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn linear_forwards_integrate_to_their_average() {
        let curve = LinearForwardCurve::new(0.03, 0.01, false);
        let t = 0.9765625;
        let expected = 0.03 + 0.5 * 0.01 * t;
        assert!((curve.zero_yield_impl(t).unwrap() - expected).abs() < 1.0e-14);

        // At a step that is not exactly representable the node count can be
        // off by one, bounding the error by roughly f(t) / 1000.
        let t = 2.0;
        let expected = 0.03 + 0.5 * 0.01 * t;
        assert!((curve.zero_yield_impl(t).unwrap() - expected).abs() < 1.0e-4);
    }

    #[test]
    fn discount_flows_through_the_derived_zero_yield() {
        let curve = LinearForwardCurve::new(0.03, 0.01, false);
        let t = 0.9765625;
        let zero = 0.03 + 0.5 * 0.01 * t;
        let df = curve.discount(t, false).unwrap();
        assert!((df - (-zero * t).exp()).abs() < 1.0e-15);
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn overriding_the_zero_yield_bypasses_the_integration() {
        let exact = LinearForwardCurve::new(0.03, 0.01, true);
        let t = 2.7;
        let zero = 0.03 + 0.5 * 0.01 * t;
        let df = exact.discount(t, false).unwrap();
        assert!((df - (-zero * t).exp()).abs() < 1.0e-15);

        let integrated = LinearForwardCurve::new(0.03, 0.01, false);
        let integrated_df = integrated.discount(t, false).unwrap();
        assert!((integrated_df - df).abs() < 1.0e-3);
        assert!((integrated_df - df).abs() > 1.0e-9);
    }
}
