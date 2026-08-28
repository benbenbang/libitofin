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

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
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
pub struct ConvexMonotone4Helper {
    x_prev: Real,
    x_scaling: Real,
    g_prev: Real,
    g_next: Real,
    f_average: Real,
    eta4: Real,
    prev_primitive: Real,
    a: Real,
}

impl ConvexMonotone4Helper {
    /// Builds the section over `[x_prev, x_next]` from the boundary gradients
    /// `g_prev`/`g_next` around the discrete forward `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        g_prev: Real,
        g_next: Real,
        f_average: Real,
        eta4: Real,
        prev_primitive: Real,
    ) -> Self {
        ConvexMonotone4Helper {
            x_prev,
            x_scaling: x_next - x_prev,
            g_prev,
            g_next,
            f_average,
            eta4,
            prev_primitive,
            a: -0.5 * (eta4 * g_prev + (1.0 - eta4) * g_next),
        }
    }
}

impl SectionHelper for ConvexMonotone4Helper {
    fn value(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta4 {
            self.f_average
                + self.a
                + (self.g_prev - self.a) * (self.eta4 - x_val) * (self.eta4 - x_val)
                    / (self.eta4 * self.eta4)
        } else {
            self.f_average
                + self.a
                + (self.g_next - self.a) * (x_val - self.eta4) * (x_val - self.eta4)
                    / ((1.0 - self.eta4) * (1.0 - self.eta4))
        }
    }

    fn primitive(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        if x_val <= self.eta4 {
            self.prev_primitive
                + self.x_scaling
                    * (self.f_average
                        + self.a
                        + (self.g_prev - self.a) / (self.eta4 * self.eta4)
                            * (self.eta4 * self.eta4 - self.eta4 * x_val
                                + 1.0 / 3.0 * x_val * x_val))
                    * x_val
        } else {
            self.prev_primitive
                + self.x_scaling
                    * (self.f_average * x_val
                        + self.a * x_val
                        + (self.g_prev - self.a) * (1.0 / 3.0 * self.eta4)
                        + (self.g_next - self.a) / ((1.0 - self.eta4) * (1.0 - self.eta4))
                            * (1.0 / 3.0 * x_val * x_val * x_val - self.eta4 * x_val * x_val
                                + self.eta4 * self.eta4 * x_val
                                - 1.0 / 3.0 * self.eta4 * self.eta4 * self.eta4))
        }
    }

    fn f_next(&self) -> Real {
        self.f_average + self.g_next
    }
}

/// The positivity-preserving variant of the "g4" section
/// (`detail::ConvexMonotone4MinHelper`): when the two quadratics would dip
/// below zero, the section is squeezed to the interval ends with a flat zero
/// region in between; otherwise it delegates to [`ConvexMonotone4Helper`].
pub struct ConvexMonotone4MinHelper {
    base: ConvexMonotone4Helper,
    split_region: bool,
    x_ratio: Real,
    x2: Real,
    x3: Real,
}

impl ConvexMonotone4MinHelper {
    /// Builds the section over `[x_prev, x_next]` from the boundary gradients
    /// `g_prev`/`g_next` around the discrete forward `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        g_prev: Real,
        g_next: Real,
        f_average: Real,
        eta4: Real,
        prev_primitive: Real,
    ) -> Self {
        let mut base = ConvexMonotone4Helper::new(
            x_prev,
            x_next,
            g_prev,
            g_next,
            f_average,
            eta4,
            prev_primitive,
        );
        let mut split_region = false;
        let mut x_ratio = 0.0;
        let mut x2 = 0.0;
        let mut x3 = 0.0;
        if base.a + base.f_average <= 0.0 {
            split_region = true;
            let f_prev = base.g_prev + base.f_average;
            let f_next = base.g_next + base.f_average;
            let reqd_shift =
                (base.eta4 * f_prev + (1.0 - base.eta4) * f_next) / 3.0 - base.f_average;
            let reqd_period = reqd_shift * base.x_scaling / (base.f_average + reqd_shift);
            let x_adjust = base.x_scaling - reqd_period;
            x_ratio = x_adjust / base.x_scaling;

            base.f_average += reqd_shift;
            base.g_next = f_next - base.f_average;
            base.g_prev = f_prev - base.f_average;
            base.a = -(base.eta4 * base.g_prev + (1.0 - base.eta4) * base.g_next) / 2.0;
            x2 = base.x_prev + x_adjust * base.eta4;
            x3 = base.x_prev + base.x_scaling - x_adjust * (1.0 - base.eta4);
        }
        ConvexMonotone4MinHelper {
            base,
            split_region,
            x_ratio,
            x2,
            x3,
        }
    }
}

impl SectionHelper for ConvexMonotone4MinHelper {
    fn value(&self, x: Real) -> Real {
        if !self.split_region {
            return self.base.value(x);
        }

        let b = &self.base;
        let mut x_val = (x - b.x_prev) / b.x_scaling;
        if x <= self.x2 {
            x_val /= self.x_ratio;
            b.f_average
                + b.a
                + (b.g_prev - b.a) * (b.eta4 - x_val) * (b.eta4 - x_val) / (b.eta4 * b.eta4)
        } else if x < self.x3 {
            0.0
        } else {
            x_val = 1.0 - (1.0 - x_val) / self.x_ratio;
            b.f_average
                + b.a
                + (b.g_next - b.a) * (x_val - b.eta4) * (x_val - b.eta4)
                    / ((1.0 - b.eta4) * (1.0 - b.eta4))
        }
    }

    fn primitive(&self, x: Real) -> Real {
        if !self.split_region {
            return self.base.primitive(x);
        }

        let b = &self.base;
        let mut x_val = (x - b.x_prev) / b.x_scaling;
        if x <= self.x2 {
            x_val /= self.x_ratio;
            b.prev_primitive
                + b.x_scaling
                    * self.x_ratio
                    * (b.f_average
                        + b.a
                        + (b.g_prev - b.a) / (b.eta4 * b.eta4)
                            * (b.eta4 * b.eta4 - b.eta4 * x_val + 1.0 / 3.0 * x_val * x_val))
                    * x_val
        } else if x <= self.x3 {
            b.prev_primitive
                + b.x_scaling
                    * self.x_ratio
                    * (b.f_average * b.eta4
                        + b.a * b.eta4
                        + (b.g_prev - b.a) / (b.eta4 * b.eta4)
                            * (1.0 / 3.0 * b.eta4 * b.eta4 * b.eta4))
        } else {
            x_val = 1.0 - (1.0 - x_val) / self.x_ratio;
            b.prev_primitive
                + b.x_scaling
                    * self.x_ratio
                    * (b.f_average * x_val
                        + b.a * x_val
                        + (b.g_prev - b.a) * (1.0 / 3.0 * b.eta4)
                        + (b.g_next - b.a) / ((1.0 - b.eta4) * (1.0 - b.eta4))
                            * (1.0 / 3.0 * x_val * x_val * x_val - b.eta4 * x_val * x_val
                                + b.eta4 * b.eta4 * x_val
                                - 1.0 / 3.0 * b.eta4 * b.eta4 * b.eta4))
        }
    }

    fn f_next(&self) -> Real {
        self.base.f_next()
    }
}

/// A single quadratic section matching the endpoint forwards and the average
/// (`detail::QuadraticHelper`).
pub struct QuadraticHelper {
    x_prev: Real,
    x_scaling: Real,
    a: Real,
    b: Real,
    c: Real,
    f_next: Real,
    prev_primitive: Real,
}

impl QuadraticHelper {
    /// Builds the quadratic through `f_prev` at `x_prev` and `f_next` at
    /// `x_next` whose average over the interval is `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        f_prev: Real,
        f_next: Real,
        f_average: Real,
        prev_primitive: Real,
    ) -> Self {
        QuadraticHelper {
            x_prev,
            x_scaling: x_next - x_prev,
            a: 3.0 * f_prev + 3.0 * f_next - 6.0 * f_average,
            b: -(4.0 * f_prev + 2.0 * f_next - 6.0 * f_average),
            c: f_prev,
            f_next,
            prev_primitive,
        }
    }
}

impl SectionHelper for QuadraticHelper {
    fn value(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        self.a * x_val * x_val + self.b * x_val + self.c
    }

    fn primitive(&self, x: Real) -> Real {
        let x_val = (x - self.x_prev) / self.x_scaling;
        self.prev_primitive
            + self.x_scaling
                * (self.a / 3.0 * x_val * x_val + self.b / 2.0 * x_val + self.c)
                * x_val
    }

    fn f_next(&self) -> Real {
        self.f_next
    }
}

/// The positivity-preserving variant of the quadratic section
/// (`detail::QuadraticMinHelper`): when the quadratic would go negative, the
/// section is rescaled to the interval ends with a flat zero region in
/// between.
pub struct QuadraticMinHelper {
    split_region: bool,
    x1: Real,
    x2: Real,
    x3: Real,
    x4: Real,
    a: Real,
    b: Real,
    c: Real,
    primitive1: Real,
    primitive2: Real,
    f_next: Real,
    x_scaling: Real,
    x_ratio: Real,
}

impl QuadraticMinHelper {
    /// Builds the quadratic through `f_prev` at `x_prev` and `f_next` at
    /// `x_next` whose average over the interval is `f_average`.
    pub fn new(
        x_prev: Real,
        x_next: Real,
        f_prev: Real,
        f_next: Real,
        f_average: Real,
        prev_primitive: Real,
    ) -> Self {
        let mut split_region = false;
        let mut a = 3.0 * f_prev + 3.0 * f_next - 6.0 * f_average;
        let mut b = -(4.0 * f_prev + 2.0 * f_next - 6.0 * f_average);
        let mut c = f_prev;
        let d = b * b - 4.0 * a * c;
        let mut x_scaling = x_next - x_prev;
        let mut x_ratio = 1.0;
        let mut x2 = 0.0;
        let mut x3 = 0.0;
        let mut primitive2 = 0.0;
        if d > 0.0 {
            let a_av = 36.0;
            let b_av = -24.0 * (f_prev + f_next);
            let c_av = 4.0 * (f_prev * f_prev + f_prev * f_next + f_next * f_next);
            let d_av = b_av * b_av - 4.0 * a_av * c_av;
            if d_av >= 0.0 {
                split_region = true;
                let av_root = (-b_av - d_av.sqrt()) / (2.0 * a_av);

                x_ratio = f_average / av_root;
                x_scaling *= x_ratio;

                a = 3.0 * f_prev + 3.0 * f_next - 6.0 * av_root;
                b = -(4.0 * f_prev + 2.0 * f_next - 6.0 * av_root);
                c = f_prev;
                let x_root = -b / (2.0 * a);
                x2 = x_prev + x_ratio * (x_next - x_prev) * x_root;
                x3 = x_next - x_ratio * (x_next - x_prev) * (1.0 - x_root);
                primitive2 = prev_primitive
                    + x_scaling * (a / 3.0 * x_root * x_root + b / 2.0 * x_root + c) * x_root;
            }
        }
        QuadraticMinHelper {
            split_region,
            x1: x_prev,
            x2,
            x3,
            x4: x_next,
            a,
            b,
            c,
            primitive1: prev_primitive,
            primitive2,
            f_next,
            x_scaling,
            x_ratio,
        }
    }
}

impl SectionHelper for QuadraticMinHelper {
    fn value(&self, x: Real) -> Real {
        let mut x_val = (x - self.x1) / (self.x4 - self.x1);
        if self.split_region {
            if x <= self.x2 {
                x_val /= self.x_ratio;
            } else if x < self.x3 {
                return 0.0;
            } else {
                x_val = 1.0 - (1.0 - x_val) / self.x_ratio;
            }
        }
        self.c + self.b * x_val + self.a * x_val * x_val
    }

    fn primitive(&self, x: Real) -> Real {
        let mut x_val = (x - self.x1) / (self.x4 - self.x1);
        if self.split_region {
            if x < self.x2 {
                x_val /= self.x_ratio;
            } else if x < self.x3 {
                return self.primitive2;
            } else {
                x_val = 1.0 - (1.0 - x_val) / self.x_ratio;
            }
        }
        self.primitive1
            + self.x_scaling
                * (self.a / 3.0 * x_val * x_val + self.b / 2.0 * x_val + self.c)
                * x_val
    }

    fn f_next(&self) -> Real {
        self.f_next
    }
}

/// A convex combination of a quadratic section and a convex-monotone section
/// (`detail::ComboHelper`), weighted by the quadraticity.
pub struct ComboHelper {
    quadraticity: Real,
    quadratic_helper: Box<dyn SectionHelper>,
    conv_mono_helper: Box<dyn SectionHelper>,
}

impl ComboHelper {
    /// Combines `quadraticity` parts of `quadratic_helper` with
    /// `1 - quadraticity` parts of `conv_mono_helper`; `quadraticity` must lie
    /// strictly between 0 and 1 (the pure cases store a single helper).
    pub fn new(
        quadratic_helper: Box<dyn SectionHelper>,
        conv_mono_helper: Box<dyn SectionHelper>,
        quadraticity: Real,
    ) -> Self {
        debug_assert!(
            quadraticity < 1.0 && quadraticity > 0.0,
            "quadratic value must lie between 0 and 1"
        );
        ComboHelper {
            quadraticity,
            quadratic_helper,
            conv_mono_helper,
        }
    }
}

impl SectionHelper for ComboHelper {
    fn value(&self, x: Real) -> Real {
        self.quadraticity * self.quadratic_helper.value(x)
            + (1.0 - self.quadraticity) * self.conv_mono_helper.value(x)
    }

    fn primitive(&self, x: Real) -> Real {
        self.quadraticity * self.quadratic_helper.primitive(x)
            + (1.0 - self.quadraticity) * self.conv_mono_helper.primitive(x)
    }

    fn f_next(&self) -> Real {
        self.quadraticity * self.quadratic_helper.f_next()
            + (1.0 - self.quadraticity) * self.conv_mono_helper.f_next()
    }
}

/// Convex-monotone interpolation over strictly increasing `x` nodes
/// (QuantLib's `ConvexMonotoneInterpolation`).
///
/// The `y` values are reinterpreted as the discrete forwards over each node
/// interval, so **the first `y` value is ignored** and the curve does not pass
/// through `y[0]` (`convexmonotoneinterpolation.hpp:172`); the boundary
/// forward is derived as `f[0] = 1.5 * y[1] - 0.5 * f[1]` instead. Each
/// interval `(x[i-1], x[i]]` is covered by the section helper the Hagan-West
/// region logic picks for it, and evaluation past the last node continues
/// flat at the curve's terminal value.
///
/// The incremental build path over pre-existing helpers (`localInterpolate` /
/// `getExistingHelpers` and the `constantLastPeriod` flag), used only by
/// QuantLib's `LocalBootstrap`, is deferred; this port always builds the full
/// section set (the main `interpolate()` path, which passes no pre-existing
/// helpers and `flatFinalPeriod = false`).
pub struct ConvexMonotoneInterpolation {
    x: Vec<Real>,
    section_helpers: Vec<Box<dyn SectionHelper>>,
    extrapolation_helper: Box<dyn SectionHelper>,
    allow_extrapolation: bool,
}

impl ConvexMonotoneInterpolation {
    /// Builds an interpolation through the forwards `y` over the nodes `x`.
    /// The `x` values must be strictly increasing with at least two points
    /// (the first `y` is ignored, so a single point carries no data), and
    /// `quadraticity` and `monotonicity` must both lie in `[0, 1]`.
    pub fn new(
        x: Vec<Real>,
        y: Vec<Real>,
        quadraticity: Real,
        monotonicity: Real,
        force_positive: bool,
    ) -> QlResult<Self> {
        if x.len() != y.len() {
            fail!(
                "x and y must have equal length ({} vs {})",
                x.len(),
                y.len()
            );
        }
        if !(0.0..=1.0).contains(&monotonicity) {
            fail!("monotonicity must lie between 0 and 1, got {monotonicity}");
        }
        if !(0.0..=1.0).contains(&quadraticity) {
            fail!("quadraticity must lie between 0 and 1, got {quadraticity}");
        }
        if x.len() < 2 {
            fail!(
                "single point provided, not supported by convex monotone method as first point is ignored"
            );
        }
        for &xi in &x {
            if !xi.is_finite() {
                fail!("x values must be finite, got {xi}");
            }
        }
        for &yi in &y {
            if !yi.is_finite() {
                fail!("y values must be finite, got {yi}");
            }
        }
        for w in x.windows(2) {
            if w[1] <= w[0] {
                fail!("x values must be strictly increasing");
            }
        }

        let (section_helpers, extrapolation_helper) =
            build_sections(&x, &y, quadraticity, monotonicity, force_positive);
        Ok(ConvexMonotoneInterpolation {
            x,
            section_helpers,
            extrapolation_helper,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted (flat
    /// beyond the last node) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The section covering `x`: the first whose right endpoint is above it,
    /// clamped to the first section below the domain (the port of the
    /// `sectionHelpers_.upper_bound(x)` dispatch). Only called for
    /// `x < x[last]`; above that the extrapolation helper takes over.
    fn section(&self, x: Real) -> &dyn SectionHelper {
        let i = self.x[1..self.x.len() - 1].partition_point(|&k| k <= x);
        self.section_helpers[i].as_ref()
    }

    fn check_range(&self, x: Real) -> QlResult<()> {
        if x.is_nan() {
            fail!("interpolation cannot be evaluated at NaN");
        }
        if !self.allow_extrapolation && !self.is_in_range(x) {
            fail!(
                "interpolation range is [{}, {}]: extrapolation at {x} not allowed",
                self.x_min(),
                self.x_max()
            );
        }
        Ok(())
    }
}

impl Interpolation for ConvexMonotoneInterpolation {
    fn value(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        if x >= self.x[self.x.len() - 1] {
            Ok(self.extrapolation_helper.value(x))
        } else {
            Ok(self.section(x).value(x))
        }
    }

    /// A faithful port of the C++ `QL_FAIL("Convex-monotone spline derivative
    /// not implemented")`: the derivative is not defined for this method, so
    /// this is a deliberate `Err`, not an unfinished stub. (The C++
    /// `secondDerivative` fails the same way; the Rust trait has no second
    /// derivative, so there is nothing to port for it.)
    fn derivative(&self, _x: Real) -> QlResult<Real> {
        fail!("Convex-monotone spline derivative not implemented")
    }

    fn primitive(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        if x >= self.x[self.x.len() - 1] {
            Ok(self.extrapolation_helper.primitive(x))
        } else {
            Ok(self.section(x).primitive(x))
        }
    }

    fn x_min(&self) -> Real {
        self.x[0]
    }

    fn x_max(&self) -> Real {
        self.x[self.x.len() - 1]
    }

    fn is_in_range(&self, x: Real) -> bool {
        x >= self.x_min() && x <= self.x_max()
    }
}

/// The section-building algorithm (`ConvexMonotoneImpl::update()`): derives
/// the boundary forwards from the discrete ones, then covers each interval
/// with the section the Hagan-West region logic picks. Returns the sections
/// (one per interval, in node order) and the flat extrapolation section.
fn build_sections(
    x: &[Real],
    y: &[Real],
    quadraticity_param: Real,
    monotonicity: Real,
    force_positive: bool,
) -> (Vec<Box<dyn SectionHelper>>, Box<dyn SectionHelper>) {
    let n = x.len();
    if n == 2 {
        let section: Box<dyn SectionHelper> =
            Box::new(EverywhereConstantHelper::new(y[1], 0.0, x[0]));
        let extrapolation = Box::new(EverywhereConstantHelper::new(y[1], 0.0, x[0]));
        return (vec![section], extrapolation);
    }

    let mut f = vec![0.0; n];
    for i in 1..n - 1 {
        let dx_prev = x[i] - x[i - 1];
        let dx = x[i + 1] - x[i];
        f[i] = dx / (dx + dx_prev) * y[i] + dx_prev / (dx + dx_prev) * y[i + 1];
    }
    f[0] = 1.5 * y[1] - 0.5 * f[1];
    f[n - 1] = 1.5 * y[n - 1] - 0.5 * f[n - 2];
    if force_positive {
        if f[0] < 0.0 {
            f[0] = 0.0;
        }
        if f[n - 1] < 0.0 {
            f[n - 1] = 0.0;
        }
    }

    let mut primitive = 0.0;
    let mut section_helpers: Vec<Box<dyn SectionHelper>> = Vec::with_capacity(n - 1);
    for i in 1..n {
        let g_prev = f[i - 1] - y[i];
        let g_next = f[i] - y[i];
        let helper = if g_prev.abs() < 1.0e-14 && g_next.abs() < 1.0e-14 {
            Box::new(ConstantGradHelper::new(
                f[i - 1],
                primitive,
                x[i - 1],
                x[i],
                f[i],
            )) as Box<dyn SectionHelper>
        } else {
            let quadratic = |min: bool| -> Box<dyn SectionHelper> {
                if min {
                    Box::new(QuadraticMinHelper::new(
                        x[i - 1],
                        x[i],
                        f[i - 1],
                        f[i],
                        y[i],
                        primitive,
                    ))
                } else {
                    Box::new(QuadraticHelper::new(
                        x[i - 1],
                        x[i],
                        f[i - 1],
                        f[i],
                        y[i],
                        primitive,
                    ))
                }
            };
            let convex4 = |eta: Real| -> Box<dyn SectionHelper> {
                if force_positive {
                    Box::new(ConvexMonotone4MinHelper::new(
                        x[i - 1],
                        x[i],
                        g_prev,
                        g_next,
                        y[i],
                        eta,
                        primitive,
                    ))
                } else {
                    Box::new(ConvexMonotone4Helper::new(
                        x[i - 1],
                        x[i],
                        g_prev,
                        g_next,
                        y[i],
                        eta,
                        primitive,
                    ))
                }
            };

            let mut quadraticity = quadraticity_param;
            let mut quadratic_helper: Option<Box<dyn SectionHelper>> = None;
            let mut conv_mono_helper: Option<Box<dyn SectionHelper>> = None;
            if quadraticity_param > 0.0 {
                quadratic_helper = Some(quadratic(
                    g_prev >= -2.0 * g_next && g_prev > -0.5 * g_next && force_positive,
                ));
            }
            if quadraticity_param < 1.0 {
                if (g_prev > 0.0 && -0.5 * g_prev >= g_next && g_next >= -2.0 * g_prev)
                    || (g_prev < 0.0 && -0.5 * g_prev <= g_next && g_next <= -2.0 * g_prev)
                {
                    quadraticity = 1.0;
                    if quadraticity_param == 0.0 {
                        quadratic_helper = Some(quadratic(force_positive));
                    }
                } else if (g_prev < 0.0 && g_next > -2.0 * g_prev)
                    || (g_prev > 0.0 && g_next < -2.0 * g_prev)
                {
                    let eta = (g_next + 2.0 * g_prev) / (g_next - g_prev);
                    let b2 = (1.0 + monotonicity) / 2.0;
                    if eta < b2 {
                        conv_mono_helper = Some(Box::new(ConvexMonotone2Helper::new(
                            x[i - 1],
                            x[i],
                            g_prev,
                            g_next,
                            y[i],
                            eta,
                            primitive,
                        )));
                    } else {
                        conv_mono_helper = Some(convex4(b2));
                    }
                } else if (g_prev > 0.0 && g_next < 0.0 && g_next > -0.5 * g_prev)
                    || (g_prev < 0.0 && g_next > 0.0 && g_next < -0.5 * g_prev)
                {
                    let eta = g_next / (g_next - g_prev) * 3.0;
                    let b3 = (1.0 - monotonicity) / 2.0;
                    if eta > b3 {
                        conv_mono_helper = Some(Box::new(ConvexMonotone3Helper::new(
                            x[i - 1],
                            x[i],
                            g_prev,
                            g_next,
                            y[i],
                            eta,
                            primitive,
                        )));
                    } else {
                        conv_mono_helper = Some(convex4(b3));
                    }
                } else {
                    let mut eta = g_next / (g_prev + g_next);
                    let b2 = (1.0 + monotonicity) / 2.0;
                    let b3 = (1.0 - monotonicity) / 2.0;
                    if eta > b2 {
                        eta = b2;
                    }
                    if eta < b3 {
                        eta = b3;
                    }
                    conv_mono_helper = Some(convex4(eta));
                }
            }

            if quadraticity == 1.0 {
                quadratic_helper.expect("a quadratic helper is built whenever quadraticity is 1")
            } else if quadraticity == 0.0 {
                conv_mono_helper
                    .expect("a convex-monotone helper is built whenever quadraticity is 0")
            } else {
                Box::new(ComboHelper::new(
                    quadratic_helper.expect("a quadratic helper is built for a mixed quadraticity"),
                    conv_mono_helper
                        .expect("a convex-monotone helper is built for a mixed quadraticity"),
                    quadraticity,
                ))
            }
        };
        section_helpers.push(helper);
        primitive += y[i] * (x[i] - x[i - 1]);
    }

    let last_value = section_helpers[n - 2].value(x[n - 1]);
    let extrapolation = Box::new(EverywhereConstantHelper::new(
        last_value,
        primitive,
        x[n - 1],
    ));
    (section_helpers, extrapolation)
}

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

    #[test]
    fn convex_monotone4_helper() {
        let h = ConvexMonotone4Helper::new(1.0, 2.0, 0.02, 0.03, 0.04, 0.6, 0.2);
        check(
            &h,
            &[
                (1.3, 3.6e-2, 2.140_000_000_000_000_2e-1),
                (1.8, 3.850_000_000_000_000_6e-2, 2.295_000_000_000_000_4e-1),
            ],
        );
        assert!((h.f_next() - 0.07).abs() < 1e-14);
    }

    #[test]
    fn convex_monotone4_min_helper_split_region() {
        // A + fAverage = -0.015 <= 0: the section is squeezed to the ends
        // around a flat zero region on [1.25, 1.75].
        let h = ConvexMonotone4MinHelper::new(1.0, 2.0, 0.04, 0.06, 0.01, 0.5, 0.0);
        check(
            &h,
            &[
                (1.1, 1.799_999_999_999_998_5e-2, 3.266_666_666_666_669e-3),
                (1.5, 0.0, 4.166_666_666_666_667_5e-3),
                (1.9, 2.519_999_999_999_996_5e-2, 5.426_666_666_666_668e-3),
            ],
        );
    }

    #[test]
    fn convex_monotone4_min_helper_delegates_when_positive() {
        // A + fAverage > 0: identical to the plain ConvexMonotone4Helper.
        let h = ConvexMonotone4MinHelper::new(1.0, 2.0, -0.01, 0.02, 0.04, 0.4, 0.0);
        check(
            &h,
            &[
                (1.2, 3.45e-2, 6.499_999_999_999_999e-3),
                (1.7, 4.2e-2, 2.5e-2),
            ],
        );
        let base = ConvexMonotone4Helper::new(1.0, 2.0, -0.01, 0.02, 0.04, 0.4, 0.0);
        assert_eq!(h.value(1.2), base.value(1.2));
        assert_eq!(h.primitive(1.7), base.primitive(1.7));
    }

    #[test]
    fn quadratic_helper() {
        let h = QuadraticHelper::new(1.0, 2.0, 0.02, 0.04, 0.05, 0.1);
        check(
            &h,
            &[
                (1.25, 4.75e-2, 1.087_500_000_000_000_1e-1),
                (1.75, 5.749_999_999_999_999_6e-2, 1.375e-1),
            ],
        );
        assert_eq!(h.f_next(), 0.04);
    }

    #[test]
    fn quadratic_min_helper_split_region() {
        // The quadratic through 0.05/0.05 with average 0.01 dips below zero:
        // rescaled to the ends around a flat zero region on [1.3, 1.7].
        let h = QuadraticMinHelper::new(1.0, 2.0, 0.05, 0.05, 0.01, 0.0);
        check(
            &h,
            &[
                (1.1, 2.222_222_222_222_22e-2, 3.518_518_518_518_52e-3),
                (1.5, 0.0, 4.999_999_999_999_997_5e-3),
                (1.9, 2.222_222_222_222_218_5e-2, 6.481_481_481_481_481e-3),
            ],
        );
    }

    #[test]
    fn quadratic_min_helper_stays_quadratic_when_positive() {
        let h = QuadraticMinHelper::new(1.0, 2.0, 0.05, 0.05, 0.045, 0.0);
        check(
            &h,
            &[(1.4, 4.279_999_999_999_999e-2, 1.823_999_999_999_999_2e-2)],
        );
    }

    #[test]
    fn combo_helper_weights_its_parts() {
        let quadratic = Box::new(QuadraticHelper::new(1.0, 2.0, 0.02, 0.04, 0.05, 0.1));
        let conv_mono = Box::new(ConvexMonotone2Helper::new(
            1.0, 2.0, 0.02, -0.06, 0.03, 0.25, 0.1,
        ));
        let h = ComboHelper::new(quadratic, conv_mono, 0.3);
        check(
            &h,
            &[(1.6, 4.104_444_444_444_444e-2, 1.281_451_851_851_852e-1)],
        );
        assert!((h.f_next() - -8.999_999_999_999_998e-3).abs() < 1e-14);
    }

    // The reference fixture of #943: extracted from the built QuantLib 1.43.0
    // dylib with the default settings (quadraticity 0.3, monotonicity 0.7,
    // forcePositive true). The dip to 0.0084 exercises the positive-preserving
    // sections, the rise to 0.0528 a convex-monotone/quadratic combination.
    fn sample() -> ConvexMonotoneInterpolation {
        ConvexMonotoneInterpolation::new(
            vec![0.0, 1.0, 2.0, 3.5, 4.0, 6.0],
            vec![0.02, 0.04, 0.015, 0.05, 0.048, 0.03],
            0.3,
            0.7,
            true,
        )
        .unwrap()
    }

    #[test]
    fn reference_values_match_quantlib() {
        let f = sample();
        let expected: [(Real, Real, Real); 11] = [
            (0.0, 0.04625, 0.0),
            (0.5, 0.0415625, 0.02234375),
            (1.0, 0.0275, 0.04),
            (1.5, 0.008428236607143, 0.047486997767857),
            (2.0, 0.029, 0.055),
            (2.75, 0.052795631487889, 0.091268923010381),
            (3.5, 0.0485, 0.13),
            (3.75, 0.0485825, 0.142164375),
            (4.0, 0.0444, 0.154),
            (5.0, 0.0282, 0.1894),
            (6.0, 0.0228, 0.214),
        ];
        for (x, value, primitive) in expected {
            assert!(
                (f.value(x).unwrap() - value).abs() < 1e-12,
                "value({x}): {} vs {value}",
                f.value(x).unwrap()
            );
            assert!(
                (f.primitive(x).unwrap() - primitive).abs() < 1e-12,
                "primitive({x}): {} vs {primitive}",
                f.primitive(x).unwrap()
            );
        }
    }
}
