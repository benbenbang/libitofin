//! Convex-monotone interpolation (Hagan-West).
//!
//! Port of `ql/math/interpolations/convexmonotoneinterpolation.hpp`, the
//! enhanced convex monotone method of Hagan & West, "Interpolation Methods for
//! Curve Construction" (AMF Vol 13, No 2, 2006). The curve is piecewise: each
//! node interval is covered by a section helper chosen by the region logic of
//! the build algorithm, and evaluation dispatches to the covering section.
//!
//! Reference values for the tests were extracted from the locally built
//! QuantLib 1.43.0 (`libQuantLib.dylib`) by constructing the C++ classes
//! directly.

use crate::types::Real;

/// One section of a piecewise convex-monotone curve (QuantLib's
/// `detail::SectionHelper`): the curve value and its antiderivative over the
/// interval the section covers, plus the forward the section ends on.
pub trait SectionHelper {
    /// The section's value at `x`.
    fn value(&self, x: Real) -> Real;

    /// The section's antiderivative at `x` (continued from the previous
    /// section's primitive).
    fn primitive(&self, x: Real) -> Real;

    /// The forward at the section's right endpoint.
    ///
    /// Consumed by the incremental (pre-existing helpers) build path used by
    /// `LocalBootstrap`, which is deferred; kept so each helper's surface is
    /// complete.
    #[allow(dead_code)]
    fn f_next(&self) -> Real;
}

/// A constant section: the value everywhere, with a linear primitive
/// (`detail::EverywhereConstantHelper`). Used for the single-period curve and
/// for flat extrapolation past the last node.
pub struct EverywhereConstantHelper {
    value: Real,
    prev_primitive: Real,
    x_prev: Real,
}

impl EverywhereConstantHelper {
    /// Builds the section from its constant value, the primitive accumulated
    /// up to `x_prev`, and `x_prev` itself.
    pub fn new(value: Real, prev_primitive: Real, x_prev: Real) -> Self {
        EverywhereConstantHelper {
            value,
            prev_primitive,
            x_prev,
        }
    }
}

impl SectionHelper for EverywhereConstantHelper {
    fn value(&self, _x: Real) -> Real {
        self.value
    }

    fn primitive(&self, x: Real) -> Real {
        self.prev_primitive + (x - self.x_prev) * self.value
    }

    fn f_next(&self) -> Real {
        self.value
    }
}

/// A constant-gradient (linear) section (`detail::ConstantGradHelper`).
pub struct ConstantGradHelper {
    f_prev: Real,
    prev_primitive: Real,
    x_prev: Real,
    f_grad: Real,
    f_next: Real,
}

impl ConstantGradHelper {
    /// Builds the section running linearly from `f_prev` at `x_prev` to
    /// `f_next` at `x_next`.
    pub fn new(
        f_prev: Real,
        prev_primitive: Real,
        x_prev: Real,
        x_next: Real,
        f_next: Real,
    ) -> Self {
        ConstantGradHelper {
            f_prev,
            prev_primitive,
            x_prev,
            f_grad: (f_next - f_prev) / (x_next - x_prev),
            f_next,
        }
    }
}

impl SectionHelper for ConstantGradHelper {
    fn value(&self, x: Real) -> Real {
        self.f_prev + (x - self.x_prev) * self.f_grad
    }

    fn primitive(&self, x: Real) -> Real {
        self.prev_primitive
            + (x - self.x_prev) * (self.f_prev + 0.5 * (x - self.x_prev) * self.f_grad)
    }

    fn f_next(&self) -> Real {
        self.f_next
    }
}

/// The Hagan-West "g2" section: flat at `f_average + g_prev` up to `eta2`,
/// then quadratic to the right endpoint (`detail::ConvexMonotone2Helper`).
pub struct ConvexMonotone2Helper {
    x_prev: Real,
    x_scaling: Real,
    g_prev: Real,
    g_next: Real,
    f_average: Real,
    eta2: Real,
    prev_primitive: Real,
}

impl ConvexMonotone2Helper {
    /// Builds the section over `[x_prev, x_next]` from the boundary gradients
    /// `g_prev`/`g_next` around the discrete forward `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        g_prev: Real,
        g_next: Real,
        f_average: Real,
        eta2: Real,
        prev_primitive: Real,
    ) -> Self {
        ConvexMonotone2Helper {
            x_prev,
            x_scaling: x_next - x_prev,
            g_prev,
            g_next,
            f_average,
            eta2,
            prev_primitive,
        }
    }
}

impl SectionHelper for ConvexMonotone2Helper {
    fn value(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta2 {
            self.f_average + self.g_prev
        } else {
            self.f_average
                + self.g_prev
                + (self.g_next - self.g_prev) / ((1.0 - self.eta2) * (1.0 - self.eta2))
                    * (x_val - self.eta2)
                    * (x_val - self.eta2)
        }
    }

    fn primitive(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta2 {
            self.prev_primitive + self.x_scaling * (self.f_average * x_val + self.g_prev * x_val)
        } else {
            self.prev_primitive
                + self.x_scaling
                    * (self.f_average * x_val
                        + self.g_prev * x_val
                        + (self.g_next - self.g_prev) / ((1.0 - self.eta2) * (1.0 - self.eta2))
                            * (1.0 / 3.0
                                * (x_val * x_val * x_val - self.eta2 * self.eta2 * self.eta2)
                                - self.eta2 * x_val * x_val
                                + self.eta2 * self.eta2 * x_val))
        }
    }

    fn f_next(&self) -> Real {
        self.f_average + self.g_next
    }
}

/// The Hagan-West "g3" section: quadratic up to `eta3`, then flat at
/// `f_average + g_next` (`detail::ConvexMonotone3Helper`).
pub struct ConvexMonotone3Helper {
    x_prev: Real,
    x_scaling: Real,
    g_prev: Real,
    g_next: Real,
    f_average: Real,
    eta3: Real,
    prev_primitive: Real,
}

impl ConvexMonotone3Helper {
    /// Builds the section over `[x_prev, x_next]` from the boundary gradients
    /// `g_prev`/`g_next` around the discrete forward `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        g_prev: Real,
        g_next: Real,
        f_average: Real,
        eta3: Real,
        prev_primitive: Real,
    ) -> Self {
        ConvexMonotone3Helper {
            x_prev,
            x_scaling: x_next - x_prev,
            g_prev,
            g_next,
            f_average,
            eta3,
            prev_primitive,
        }
    }
}

impl SectionHelper for ConvexMonotone3Helper {
    fn value(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta3 {
            self.f_average
                + self.g_next
                + (self.g_prev - self.g_next) / (self.eta3 * self.eta3)
                    * (self.eta3 - x_val)
                    * (self.eta3 - x_val)
        } else {
            self.f_average + self.g_next
        }
    }

    fn primitive(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta3 {
            self.prev_primitive
                + self.x_scaling
                    * (self.f_average * x_val
                        + self.g_next * x_val
                        + (self.g_prev - self.g_next) / (self.eta3 * self.eta3)
                            * (1.0 / 3.0 * x_val * x_val * x_val - self.eta3 * x_val * x_val
                                + self.eta3 * self.eta3 * x_val))
        } else {
            self.prev_primitive
                + self.x_scaling
                    * (self.f_average * x_val
                        + self.g_next * x_val
                        + (self.g_prev - self.g_next) / (self.eta3 * self.eta3)
                            * (1.0 / 3.0 * self.eta3 * self.eta3 * self.eta3))
        }
    }

    fn f_next(&self) -> Real {
        self.f_average + self.g_next
    }
}

/// The Hagan-West "g4" section: two quadratics meeting at `eta4`
/// (`detail::ConvexMonotone4Helper`).
#[cfg(test)]
mod tests {
    use super::*;

    // Expected (x, value, primitive) triples come from constructing the C++
    // detail:: classes directly against the built QuantLib 1.43.0 dylib.
    fn check(helper: &dyn SectionHelper, expected: &[(Real, Real, Real)]) {
        for &(x, value, primitive) in expected {
            assert!(
                (helper.value(x) - value).abs() < 1e-14,
                "value({x}): {} vs {value}",
                helper.value(x)
            );
            assert!(
                (helper.primitive(x) - primitive).abs() < 1e-14,
                "primitive({x}): {} vs {primitive}",
                helper.primitive(x)
            );
        }
    }

    #[test]
    fn everywhere_constant_helper() {
        let h = EverywhereConstantHelper::new(0.03, 0.2, 6.0);
        check(&h, &[(7.5, 0.03, 0.245)]);
        assert_eq!(h.f_next(), 0.03);
    }

    #[test]
    fn constant_grad_helper() {
        let h = ConstantGradHelper::new(0.02, 0.05, 1.0, 2.0, 0.03);
        check(&h, &[(1.4, 0.024, 0.0588)]);
        assert_eq!(h.f_next(), 0.03);
    }

    #[test]
    fn convex_monotone2_helper() {
        let h = ConvexMonotone2Helper::new(1.0, 2.0, 0.02, -0.06, 0.03, 0.25, 0.1);
        check(
            &h,
            &[
                (1.1, 5e-2, 1.050_000_000_000_000_1e-1),
                (1.6, 3.257_777_777_777_777_5e-2, 1.279_674_074_074_074_2e-1),
                (2.0, -2.999_999_999_999_999_2e-2, 1.3e-1),
            ],
        );
        assert!((h.f_next() - -0.03).abs() < 1e-14);
    }

    #[test]
    fn convex_monotone3_helper() {
        let h = ConvexMonotone3Helper::new(1.0, 2.0, 0.03, -0.01, 0.025, 0.75, 0.05);
        check(
            &h,
            &[
                (1.5, 1.944_444_444_444_444_5e-2, 6.712_962_962_962_964e-2),
                (1.9, 1.500_000_000_000_000_1e-2, 7.350_000_000_000_001e-2),
            ],
        );
        assert!((h.f_next() - 0.015).abs() < 1e-14);
    }
}
