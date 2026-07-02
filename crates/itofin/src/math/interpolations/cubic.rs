//! Cubic interpolation between discrete points.
//!
//! Port of `CubicInterpolation` from
//! `ql/math/interpolations/cubicinterpolation.hpp`. On each segment
//! `P_i(x) = y_i + a_i (x-x_i) + b_i (x-x_i)^2 + c_i (x-x_i)^3`, with the node
//! first-derivatives supplied by a [`CubicDerivativeApprox`] scheme. This layer
//! ports the coefficient/evaluation engine and the two simplest local schemes,
//! Parabolic and Kruger. The non-local spline schemes and the Hyman
//! monotonicity filter land in later layers; a private `CubicConfig` already
//! carries their knobs so those layers slot in without reshaping the pipeline.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
use crate::types::{Real, Size};

/// First-derivative approximation scheme. Variants are added as they are ported,
/// so it is `#[non_exhaustive]`: downstream matches must include a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CubicDerivativeApprox {
    /// Parabolic approximation (local, non-monotonic): fits a local parabola,
    /// so it reproduces quadratic data exactly.
    Parabolic,
    /// Kruger approximation (local, monotonic): a harmonic-style blend that
    /// zeroes the derivative where the slope changes sign.
    Kruger,
    /// Fritsch-Butland approximation (local, non-linear): a weighted harmonic
    /// mean of adjacent slopes. Monotone for monotone data, but the raw
    /// approximation is not monotone at extrema (QuantLib's convenience class
    /// pairs it with the Hyman filter, which lands in a later layer).
    FritschButland,
    /// Weighted harmonic mean approximation (local, monotonic, non-linear):
    /// distance-weighted harmonic mean of adjacent slopes, zeroed where the
    /// slope changes sign, with the end slopes clamped against overshoot.
    Harmonic,
    /// Akima approximation (local, non-monotonic): a weighted average of
    /// adjacent slopes that favors the less-curved side, falling back to an
    /// exact-equality special case where consecutive slopes coincide. Needs at
    /// least 4 points; the two nodes on each end use distinct bespoke formulas.
    Akima,
    /// Cubic spline (non-local, `C^2`): node derivatives solve a tridiagonal
    /// system, so the second derivative is continuous. Boundary conditions are
    /// chosen with [`CubicInterpolation::with_boundary_conditions`]
    /// (`SecondDerivative` value 0 - the default - gives the natural spline;
    /// `FirstDerivative` gives the clamped spline; `NotAKnot` and `Lagrange`
    /// reproduce cubic polynomials exactly).
    Spline,
}

/// Boundary condition for the (non-local) spline schemes. Variants are added as
/// they are ported, so it is `#[non_exhaustive]`: downstream matches must
/// include a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CubicBoundaryCondition {
    /// Match the second derivative at the end; the boundary value is that second
    /// derivative (0 gives the natural spline).
    SecondDerivative,
    /// Match the first derivative (end slope) at the end; the boundary value is
    /// that slope (the clamped spline).
    FirstDerivative,
    /// Not-a-knot: make the second (or second-to-last) point an inactive knot,
    /// so the first two (or last two) segments share one cubic. Reproduces cubic
    /// polynomials exactly. The boundary value is ignored. A not-a-knot end needs
    /// at least 3 points; using it on both ends needs at least 4.
    NotAKnot,
    /// Lagrange: match the end slope to that of the cubic through the four
    /// nearest points, so it also reproduces cubic polynomials exactly. The
    /// boundary value is ignored, and either Lagrange end needs at least 4 points.
    Lagrange,
}

/// The full cubic configuration, retained on the interpolation so a
/// [`CubicInterpolation::with_boundary_conditions`] change can rebuild the
/// coefficients from the original data.
#[derive(Clone, Copy, Debug)]
struct CubicConfig {
    da: CubicDerivativeApprox,
    /// Carried through but not yet consumed; the Hyman layer will read it.
    #[allow(dead_code)]
    monotonic: bool,
    left_cond: CubicBoundaryCondition,
    left_value: Real,
    right_cond: CubicBoundaryCondition,
    right_value: Real,
}

impl CubicConfig {
    /// The QuantLib `Cubic` defaults, specialized to a derivative scheme
    /// (natural `SecondDerivative` boundaries, value 0, non-monotonic).
    fn defaults(da: CubicDerivativeApprox) -> Self {
        CubicConfig {
            da,
            monotonic: false,
            left_cond: CubicBoundaryCondition::SecondDerivative,
            left_value: 0.0,
            right_cond: CubicBoundaryCondition::SecondDerivative,
            right_value: 0.0,
        }
    }
}

/// Piecewise-cubic interpolation over strictly increasing `x` nodes.
pub struct CubicInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    a: Vec<Real>,
    b: Vec<Real>,
    c: Vec<Real>,
    primitive_const: Vec<Real>,
    config: CubicConfig,
    allow_extrapolation: bool,
}

impl CubicInterpolation {
    /// Builds a cubic interpolation through `(x, y)` using the derivative scheme
    /// `da`. The `x` values must be strictly increasing with at least two
    /// points.
    pub fn new(x: Vec<Real>, y: Vec<Real>, da: CubicDerivativeApprox) -> QlResult<Self> {
        Self::build(x, y, CubicConfig::defaults(da))
    }

    fn build(x: Vec<Real>, y: Vec<Real>, config: CubicConfig) -> QlResult<Self> {
        if x.len() != y.len() {
            fail!(
                "x and y must have equal length ({} vs {})",
                x.len(),
                y.len()
            );
        }
        if x.len() < 2 {
            fail!(
                "cubic interpolation needs at least 2 points, got {}",
                x.len()
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
        if config.da == CubicDerivativeApprox::Akima && x.len() < 4 {
            fail!(
                "Akima approximation requires at least 4 points, got {}",
                x.len()
            );
        }

        let n = x.len();
        let mut dx = vec![0.0; n - 1];
        let mut s = vec![0.0; n - 1];
        for i in 0..n - 1 {
            dx[i] = x[i + 1] - x[i];
            s[i] = (y[i + 1] - y[i]) / dx[i];
        }

        // The spline is non-local: it solves a tridiagonal system (and needs the
        // node coordinates for the Lagrange boundary), so it is dispatched
        // separately from the infallible per-node local schemes.
        let d = if config.da == CubicDerivativeApprox::Spline {
            spline_node_derivatives(&config, &x, &y, &dx, &s)?
        } else {
            node_derivatives(&config, &dx, &s)
        };

        let mut a = vec![0.0; n - 1];
        let mut b = vec![0.0; n - 1];
        let mut c = vec![0.0; n - 1];
        for i in 0..n - 1 {
            a[i] = d[i];
            b[i] = (3.0 * s[i] - d[i + 1] - 2.0 * d[i]) / dx[i];
            c[i] = (d[i + 1] + d[i] - 2.0 * s[i]) / (dx[i] * dx[i]);
        }

        let mut primitive_const = vec![0.0; n - 1];
        for i in 1..n - 1 {
            primitive_const[i] = primitive_const[i - 1]
                + dx[i - 1]
                    * (y[i - 1]
                        + dx[i - 1]
                            * (a[i - 1] / 2.0
                                + dx[i - 1] * (b[i - 1] / 3.0 + dx[i - 1] * c[i - 1] / 4.0)));
        }

        Ok(CubicInterpolation {
            x,
            y,
            a,
            b,
            c,
            primitive_const,
            config,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted (extending
    /// the end segments' cubics) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// Rebuilds the interpolation with the given spline boundary conditions,
    /// preserving the derivative scheme and extrapolation setting. The boundary
    /// conditions only affect the `Spline` scheme; for the local schemes they
    /// are carried through but leave the coefficients unchanged, matching
    /// QuantLib.
    ///
    /// # Errors
    ///
    /// Returns an error if either boundary value is not finite, which would
    /// otherwise poison the spline's right-hand side and yield `NaN` results.
    pub fn with_boundary_conditions(
        self,
        left_cond: CubicBoundaryCondition,
        left_value: Real,
        right_cond: CubicBoundaryCondition,
        right_value: Real,
    ) -> QlResult<Self> {
        if !left_value.is_finite() || !right_value.is_finite() {
            fail!("boundary values must be finite, got left {left_value}, right {right_value}");
        }
        let config = CubicConfig {
            left_cond,
            left_value,
            right_cond,
            right_value,
            ..self.config
        };
        let allow_extrapolation = self.allow_extrapolation;
        let mut rebuilt = Self::build(self.x, self.y, config)?;
        rebuilt.allow_extrapolation = allow_extrapolation;
        Ok(rebuilt)
    }

    /// The second derivative at `x`.
    pub fn second_derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(2.0 * self.b[j] + 6.0 * self.c[j] * dx)
    }

    /// The index of the segment containing `x`, clamped to the end segments.
    fn locate(&self, x: Real) -> Size {
        let n = self.x.len();
        if x < self.x[0] {
            0
        } else if x > self.x[n - 1] {
            n - 2
        } else {
            self.x[..n - 1].partition_point(|&xi| xi <= x) - 1
        }
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

/// The node first-derivatives for a local scheme, given segment widths `dx` and
/// secant slopes `s` (both length `n-1`). Returns a length-`n` vector. The
/// non-local spline is handled separately by [`spline_node_derivatives`].
fn node_derivatives(config: &CubicConfig, dx: &[Real], s: &[Real]) -> Vec<Real> {
    let n = dx.len() + 1;
    // Two points degenerate to the single secant slope for the local schemes.
    if n == 2 {
        return vec![s[0], s[0]];
    }

    let mut d = vec![0.0; n];
    match config.da {
        CubicDerivativeApprox::Spline => unreachable!("spline handled by the caller"),
        CubicDerivativeApprox::Parabolic => {
            for i in 1..n - 1 {
                d[i] = (dx[i - 1] * s[i] + dx[i] * s[i - 1]) / (dx[i] + dx[i - 1]);
            }
            d[0] = ((2.0 * dx[0] + dx[1]) * s[0] - dx[0] * s[1]) / (dx[0] + dx[1]);
            d[n - 1] = ((2.0 * dx[n - 2] + dx[n - 3]) * s[n - 2] - dx[n - 2] * s[n - 3])
                / (dx[n - 2] + dx[n - 3]);
        }
        CubicDerivativeApprox::Kruger => {
            for i in 1..n - 1 {
                d[i] = if s[i - 1] * s[i] < 0.0 {
                    // slope changes sign at the point
                    0.0
                } else {
                    2.0 / (1.0 / s[i - 1] + 1.0 / s[i])
                };
            }
            d[0] = (3.0 * s[0] - d[1]) / 2.0;
            d[n - 1] = (3.0 * s[n - 2] - d[n - 2]) / 2.0;
        }
        CubicDerivativeApprox::FritschButland => {
            for i in 1..n - 1 {
                let s_min = s[i - 1].min(s[i]);
                let s_max = s[i - 1].max(s[i]);
                d[i] = if s_max + 2.0 * s_min == 0.0 {
                    // Degenerate denominator: QuantLib falls back to signed
                    // extremes (QL_MIN_REAL / QL_MAX_REAL) or zero.
                    if s_min * s_max < 0.0 {
                        Real::MIN
                    } else if s_min * s_max == 0.0 {
                        0.0
                    } else {
                        Real::MAX
                    }
                } else {
                    3.0 * s_min * s_max / (s_max + 2.0 * s_min)
                };
            }
            // end points reuse the parabolic estimate
            d[0] = ((2.0 * dx[0] + dx[1]) * s[0] - dx[0] * s[1]) / (dx[0] + dx[1]);
            d[n - 1] = ((2.0 * dx[n - 2] + dx[n - 3]) * s[n - 2] - dx[n - 2] * s[n - 3])
                / (dx[n - 2] + dx[n - 3]);
        }
        CubicDerivativeApprox::Harmonic => {
            for i in 1..n - 1 {
                let w1 = 2.0 * dx[i] + dx[i - 1];
                let w2 = dx[i] + 2.0 * dx[i - 1];
                d[i] = if s[i - 1] * s[i] <= 0.0 {
                    // slope changes sign at the point
                    0.0
                } else {
                    (w1 + w2) / (w1 / s[i - 1] + w2 / s[i])
                };
            }
            // end points: parabolic estimate, clamped against overshoot
            d[0] = ((2.0 * dx[0] + dx[1]) * s[0] - dx[0] * s[1]) / (dx[1] + dx[0]);
            if d[0] * s[0] < 0.0 {
                d[0] = 0.0;
            } else if s[0] * s[1] < 0.0 && d[0].abs() > (3.0 * s[0]).abs() {
                d[0] = 3.0 * s[0];
            }
            d[n - 1] = ((2.0 * dx[n - 2] + dx[n - 3]) * s[n - 2] - dx[n - 2] * s[n - 3])
                / (dx[n - 3] + dx[n - 2]);
            if d[n - 1] * s[n - 2] < 0.0 {
                d[n - 1] = 0.0;
            } else if s[n - 2] * s[n - 3] < 0.0 && d[n - 1].abs() > (3.0 * s[n - 2]).abs() {
                d[n - 1] = 3.0 * s[n - 2];
            }
        }
        // Literal port of QuantLib's endpoint formulas, including the
        // s[i-2] == s[i-1] exact-equality branches: both are meaningful
        // comparisons in the source algorithm (flagging perfectly straight
        // segments), not incidental floating-point drift.
        #[allow(clippy::float_cmp)]
        CubicDerivativeApprox::Akima => {
            // Hardening deviation from QuantLib: each endpoint blend divides by a
            // sum of absolute weights that is exactly zero for flat data and for
            // a constant slope of 0.5 (numerator zero too, so the raw formula
            // yields NaN even at a knot). In every such case the adjacent slopes
            // are equal, so we fall back to the relevant secant slope, which is
            // the value the blend would take. Only an exact zero triggers this;
            // a tiny nonzero denominator still yields a finite ratio.
            let w0 = (s[1] - s[0]).abs();
            let w0b = (2.0 * s[0] * s[1] - 4.0 * s[0] * s[0] * s[1]).abs();
            d[0] = if w0 + w0b == 0.0 {
                s[0]
            } else {
                (w0 * 2.0 * s[0] * s[1] + w0b * s[0]) / (w0 + w0b)
            };

            let w1 = (s[2] - s[1]).abs();
            let w1b = (s[0] - 2.0 * s[0] * s[1]).abs();
            d[1] = if w1 + w1b == 0.0 {
                s[1]
            } else {
                (w1 * s[0] + w1b * s[1]) / (w1 + w1b)
            };

            for i in 2..n - 2 {
                // Two guards below assign the same s[i], but the guard
                // conditions are distinct branches of QuantLib's original
                // if/else-if chain, kept separate for faithfulness.
                #[allow(clippy::if_same_then_else)]
                let di = if s[i - 2] == s[i - 1] && s[i] != s[i + 1] {
                    s[i - 1]
                } else if s[i - 2] != s[i - 1] && s[i] == s[i + 1] {
                    s[i]
                } else if s[i] == s[i - 1] {
                    s[i]
                } else if s[i - 2] == s[i - 1] && s[i - 1] != s[i] && s[i] == s[i + 1] {
                    (s[i - 1] + s[i]) / 2.0
                } else {
                    let wl = (s[i + 1] - s[i]).abs();
                    let wr = (s[i - 1] - s[i - 2]).abs();
                    (wl * s[i - 1] + wr * s[i]) / (wl + wr)
                };
                d[i] = di;
            }

            let wn2 = (2.0 * s[n - 2] * s[n - 3] - s[n - 2]).abs();
            let wn2b = (s[n - 3] - s[n - 4]).abs();
            d[n - 2] = if wn2 + wn2b == 0.0 {
                s[n - 3]
            } else {
                (wn2 * s[n - 3] + wn2b * s[n - 2]) / (wn2 + wn2b)
            };

            let wn1 = (4.0 * s[n - 2] * s[n - 2] * s[n - 3] - 2.0 * s[n - 2] * s[n - 3]).abs();
            let wn1b = (s[n - 2] - s[n - 3]).abs();
            d[n - 1] = if wn1 + wn1b == 0.0 {
                s[n - 2]
            } else {
                (wn1 * s[n - 2] + wn1b * 2.0 * s[n - 2] * s[n - 3]) / (wn1 + wn1b)
            };
        }
    }
    d
}

/// Node first-derivatives for a cubic spline: build and solve the tridiagonal
/// system `L d = rhs` (QuantLib's `TridiagonalOperator`). Interior rows are
/// standard; the first and last rows encode the boundary conditions. The
/// `SecondDerivative`, `FirstDerivative`, `NotAKnot`, and `Lagrange` conditions
/// are supported here (the last needs the node coordinates).
fn spline_node_derivatives(
    config: &CubicConfig,
    x: &[Real],
    y: &[Real],
    dx: &[Real],
    s: &[Real],
) -> QlResult<Vec<Real>> {
    let n = dx.len() + 1;
    // A not-a-knot end row references dx[1] / dx[n-3], so each not-a-knot side
    // needs at least 3 points; with both ends not-a-knot the two conditions
    // coincide on a single interior knot and the system is singular below 4
    // points. Each boundary row is otherwise independent, so a mixed 3-point
    // case (one not-a-knot end) is valid. QuantLib leaves both unchecked
    // (out-of-bounds / singular); we reject them up front with a clear error.
    let left_nak = config.left_cond == CubicBoundaryCondition::NotAKnot;
    let right_nak = config.right_cond == CubicBoundaryCondition::NotAKnot;
    if (left_nak || right_nak) && n < 3 {
        fail!("the not-a-knot spline boundary condition requires at least 3 points, got {n}");
    }
    if left_nak && right_nak && n < 4 {
        fail!("two not-a-knot spline boundary conditions require at least 4 points, got {n}");
    }
    // A Lagrange end row fits the cubic through the four nearest points, so
    // either Lagrange side needs at least 4 points (matching QuantLib's check).
    if (config.left_cond == CubicBoundaryCondition::Lagrange
        || config.right_cond == CubicBoundaryCondition::Lagrange)
        && n < 4
    {
        fail!("the Lagrange spline boundary condition requires at least 4 points, got {n}");
    }
    let mut lower = vec![0.0; n - 1];
    let mut diag = vec![0.0; n];
    let mut upper = vec![0.0; n - 1];
    let mut rhs = vec![0.0; n];

    // interior rows: L[i] = (dx[i], 2(dx[i]+dx[i-1]), dx[i-1])
    for i in 1..n - 1 {
        lower[i - 1] = dx[i];
        diag[i] = 2.0 * (dx[i] + dx[i - 1]);
        upper[i] = dx[i - 1];
        rhs[i] = 3.0 * (dx[i] * s[i - 1] + dx[i - 1] * s[i]);
    }

    // Left boundary row, from left_cond (value is the target derivative).
    match config.left_cond {
        CubicBoundaryCondition::SecondDerivative => {
            diag[0] = 2.0;
            upper[0] = 1.0;
            rhs[0] = 3.0 * s[0] - config.left_value * dx[0] / 2.0;
        }
        CubicBoundaryCondition::FirstDerivative => {
            diag[0] = 1.0;
            upper[0] = 0.0;
            rhs[0] = config.left_value;
        }
        CubicBoundaryCondition::NotAKnot => {
            diag[0] = dx[1] * (dx[1] + dx[0]);
            upper[0] = (dx[0] + dx[1]) * (dx[0] + dx[1]);
            rhs[0] = s[0] * dx[1] * (2.0 * dx[1] + 3.0 * dx[0]) + s[1] * dx[0] * dx[0];
        }
        CubicBoundaryCondition::Lagrange => {
            diag[0] = 1.0;
            upper[0] = 0.0;
            rhs[0] = cubic_interpolating_polynomial_derivative(
                [x[0], x[1], x[2], x[3]],
                [y[0], y[1], y[2], y[3]],
                x[0],
            );
        }
    }

    // Right boundary row, from right_cond.
    match config.right_cond {
        CubicBoundaryCondition::SecondDerivative => {
            lower[n - 2] = 1.0;
            diag[n - 1] = 2.0;
            rhs[n - 1] = 3.0 * s[n - 2] + config.right_value * dx[n - 2] / 2.0;
        }
        CubicBoundaryCondition::FirstDerivative => {
            lower[n - 2] = 0.0;
            diag[n - 1] = 1.0;
            rhs[n - 1] = config.right_value;
        }
        CubicBoundaryCondition::NotAKnot => {
            lower[n - 2] = -(dx[n - 2] + dx[n - 3]) * (dx[n - 2] + dx[n - 3]);
            diag[n - 1] = -dx[n - 3] * (dx[n - 3] + dx[n - 2]);
            rhs[n - 1] = -s[n - 3] * dx[n - 2] * dx[n - 2]
                - s[n - 2] * dx[n - 3] * (3.0 * dx[n - 2] + 2.0 * dx[n - 3]);
        }
        CubicBoundaryCondition::Lagrange => {
            lower[n - 2] = 0.0;
            diag[n - 1] = 1.0;
            rhs[n - 1] = cubic_interpolating_polynomial_derivative(
                [x[n - 4], x[n - 3], x[n - 2], x[n - 1]],
                [y[n - 4], y[n - 3], y[n - 2], y[n - 1]],
                x[n - 1],
            );
        }
    }

    solve_tridiagonal(&lower, &diag, &upper, &rhs)
}

/// The derivative at `x` of the cubic polynomial interpolating the four points
/// `(xs[i], ys[i])`. A literal port of QuantLib's
/// `cubicInterpolatingPolynomialDerivative`, used to set the Lagrange end slope.
fn cubic_interpolating_polynomial_derivative(xs: [Real; 4], ys: [Real; 4], x: Real) -> Real {
    let [a, b, c, d] = xs;
    let [u, v, w, z] = ys;
    let num = (((a - c) * (b - c) * (c - x) * z - (a - d) * (b - d) * (d - x) * w)
        * (a - x + b - x)
        + ((a - c) * (b - c) * z - (a - d) * (b - d) * w) * (a - x) * (b - x))
        * (a - b)
        + ((a - c) * (a - d) * v - (b - c) * (b - d) * u) * (c - d) * (c - x) * (d - x)
        + ((a - c) * (a - d) * (a - x) * v - (b - c) * (b - d) * (b - x) * u)
            * (c - x + d - x)
            * (c - d);
    let den = (a - b) * (a - c) * (a - d) * (b - c) * (b - d) * (c - d);
    -num / den
}

/// Solves the tridiagonal system `M x = rhs` by the Thomas algorithm, where `M`
/// has sub-diagonal `lower` (length `n-1`), main diagonal `diag` (length `n`),
/// and super-diagonal `upper` (length `n-1`). Fails on a zero pivot (singular or
/// ill-conditioned system).
fn solve_tridiagonal(
    lower: &[Real],
    diag: &[Real],
    upper: &[Real],
    rhs: &[Real],
) -> QlResult<Vec<Real>> {
    let n = diag.len();
    // Forward elimination, carrying modified super-diagonal `cp` and rhs `dp`.
    let mut cp = vec![0.0; n];
    let mut dp = vec![0.0; n];
    if diag[0] == 0.0 {
        fail!("tridiagonal system is singular (zero pivot at row 0)");
    }
    cp[0] = upper[0] / diag[0];
    dp[0] = rhs[0] / diag[0];
    for i in 1..n {
        let pivot = diag[i] - lower[i - 1] * cp[i - 1];
        if pivot == 0.0 {
            fail!("tridiagonal system is singular (zero pivot at row {i})");
        }
        if i < n - 1 {
            cp[i] = upper[i] / pivot;
        }
        dp[i] = (rhs[i] - lower[i - 1] * dp[i - 1]) / pivot;
    }

    // Back substitution.
    let mut x = vec![0.0; n];
    x[n - 1] = dp[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = dp[i] - cp[i] * x[i + 1];
    }
    Ok(x)
}

impl Interpolation for CubicInterpolation {
    fn value(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.y[j] + dx * (self.a[j] + dx * (self.b[j] + dx * self.c[j])))
    }

    fn derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.a[j] + (2.0 * self.b[j] + 3.0 * self.c[j] * dx) * dx)
    }

    fn primitive(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.primitive_const[j]
            + dx * (self.y[j]
                + dx * (self.a[j] / 2.0 + dx * (self.b[j] / 3.0 + dx * self.c[j] / 4.0))))
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

/// Parabolic cubic interpolation (local, non-monotonic).
pub struct ParabolicInterpolation;

impl ParabolicInterpolation {
    /// Builds a parabolic cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Parabolic)
    }
}

/// Kruger cubic interpolation (local, monotonic).
pub struct KrugerCubicInterpolation;

impl KrugerCubicInterpolation {
    /// Builds a Kruger cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Kruger)
    }
}

/// Harmonic cubic interpolation (local, monotonic).
///
/// QuantLib's `FritschButlandCubic` convenience type pairs the FritschButland
/// scheme with the Hyman monotonicity filter, so its factory lands with the
/// Hyman layer; the raw scheme is reachable now via
/// [`CubicDerivativeApprox::FritschButland`].
pub struct HarmonicCubicInterpolation;

impl HarmonicCubicInterpolation {
    /// Builds a Harmonic cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Harmonic)
    }
}

/// Akima cubic interpolation (local, non-monotonic). Needs at least 4 points.
pub struct AkimaCubicInterpolation;

impl AkimaCubicInterpolation {
    /// Builds an Akima cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Akima)
    }
}

/// Natural cubic spline (non-local, `C^2`): the spline scheme with zero second
/// derivative at both ends.
pub struct CubicNaturalSpline;

impl CubicNaturalSpline {
    /// Builds a natural cubic spline through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Spline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parabolic reproduces q(x) = 1 + 2x + 3x^2 exactly. Non-uniform nodes.
    fn q(x: Real) -> Real {
        1.0 + 2.0 * x + 3.0 * x * x
    }

    // Not-a-knot reproduces this cubic exactly (derivative 2 + 6x + 12x^2).
    fn cubic(x: Real) -> Real {
        1.0 + 2.0 * x + 3.0 * x * x + 4.0 * x * x * x
    }

    fn parabolic_sample() -> CubicInterpolation {
        let x = vec![0.0, 1.0, 3.0, 4.0];
        let y = x.iter().map(|&xi| q(xi)).collect();
        ParabolicInterpolation::new(x, y).unwrap()
    }

    fn assert_close(got: Real, expected: Real) {
        let tol = 1e-11 * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn parabolic_reproduces_quadratic() {
        // value = q, derivative = q' = 2 + 6x, second derivative = q'' = 6, and
        // the primitive from x_min = 0 is x + x^2 + x^3.
        let f = parabolic_sample();
        for &x in &[0.0, 0.5, 1.0, 2.0, 3.0, 3.7, 4.0_f64] {
            assert_close(f.value(x).unwrap(), q(x));
            assert_close(f.derivative(x).unwrap(), 2.0 + 6.0 * x);
            assert_close(f.second_derivative(x).unwrap(), 6.0);
            assert_close(f.primitive(x).unwrap(), x + x * x + x * x * x);
        }
    }

    #[test]
    fn two_points_is_linear() {
        // n == 2 returns the single secant slope before the scheme match, so it
        // is scheme-independent. y = [1, 5] over [0, 2]: value = 1 + 2x.
        let f = ParabolicInterpolation::new(vec![0.0, 2.0], vec![1.0, 5.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 3.0);
        assert_close(f.derivative(1.5).unwrap(), 2.0);
        assert_close(f.second_derivative(0.7).unwrap(), 0.0);
    }

    #[test]
    fn kruger_node_derivatives() {
        // y = [0, 1, 0]: passes through the nodes; S[0]=1, S[1]=-1 (product < 0)
        // so the Kruger derivative at the peak is 0.
        let f = KrugerCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 1.0);
        assert_close(f.derivative(1.0).unwrap(), 0.0);
        // y = [0, 0.5, 2]: S = [0.5, 1.5] same sign, so the interior derivative
        // is the harmonic mean 2/(1/0.5 + 1/1.5) = 0.75.
        let g = KrugerCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 0.5, 2.0]).unwrap();
        assert_close(g.derivative(1.0).unwrap(), 0.75);
    }

    #[test]
    fn harmonic_and_fritsch_butland_node_derivatives() {
        // x = [0,1,3], y = [0,1,4]: dx = [1,2], S = [1, 1.5]. At the interior
        // node the Harmonic weights are w1 = 5, w2 = 4, giving
        // (5+4)/(5/1 + 4/1.5) = 27/23; Fritsch-Butland gives 3*1*1.5/3.5 = 9/7.
        let h = HarmonicCubicInterpolation::new(vec![0.0, 1.0, 3.0], vec![0.0, 1.0, 4.0]).unwrap();
        assert_close(h.value(1.0).unwrap(), 1.0);
        assert_close(h.derivative(1.0).unwrap(), 27.0 / 23.0);
        let fb = CubicInterpolation::new(
            vec![0.0, 1.0, 3.0],
            vec![0.0, 1.0, 4.0],
            CubicDerivativeApprox::FritschButland,
        )
        .unwrap();
        assert_close(fb.value(1.0).unwrap(), 1.0);
        assert_close(fb.derivative(1.0).unwrap(), 9.0 / 7.0);
    }

    #[test]
    fn harmonic_zeroes_derivative_at_sign_change() {
        // y = [0, 1, 0]: S[0]=1, S[1]=-1 (product < 0), so the interior derivative
        // is zeroed and the sample passes through the nodes.
        let h = HarmonicCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.0]).unwrap();
        assert_close(h.value(1.0).unwrap(), 1.0);
        assert_close(h.derivative(1.0).unwrap(), 0.0);
    }

    #[test]
    fn akima_reproduces_reference_node_derivatives() {
        // x = [0,1,2,4,7], y = [0,1,0,2,3] -> S = [1,-1,1,1/3]. Exercises the
        // generic interior branch (node 2) and all four bespoke endpoint
        // formulas; oracle cross-checked with an independent transcription of
        // the same formula.
        let x = vec![0.0, 1.0, 2.0, 4.0, 7.0];
        let y = vec![0.0, 1.0, 0.0, 2.0, 3.0];
        let f = AkimaCubicInterpolation::new(x.clone(), y.clone()).unwrap();
        let oracle = [-0.5, -0.2, 0.5, 3.0 / 7.0, 7.0 / 12.0];
        for i in 0..x.len() {
            assert_close(f.value(x[i]).unwrap(), y[i]);
            assert_close(f.derivative(x[i]).unwrap(), oracle[i]);
        }
    }

    #[test]
    fn akima_interior_special_case_branches() {
        // node_derivatives is exercised directly (unit spacing, so S = the
        // secant slopes below) to pin the three exact-equality interior
        // branches without the indirection of a full x/y dataset.
        // S = [1,2,3,3,1,0.5]: branch "S[i-2]!=S[i-1], S[i]==S[i+1]" at i=2,
        // "S[i]==S[i-1]" at i=3, "S[i-2]==S[i-1], S[i]!=S[i+1]" at i=4.
        let cfg = CubicConfig::defaults(CubicDerivativeApprox::Akima);
        let dx = vec![1.0; 6];
        let s = vec![1.0, 2.0, 3.0, 3.0, 1.0, 0.5];
        let d = node_derivatives(&cfg, &dx, &s);
        let oracle = [1.6, 1.75, 3.0, 3.0, 3.0, 0.6, 1.0];
        for i in 0..d.len() {
            assert_close(d[i], oracle[i]);
        }

        // S = [1,1,1,2,2,-1] hits the remaining branch, the average
        // "S[i-2]==S[i-1], S[i-1]!=S[i], S[i]==S[i+1]", at i=3.
        let s2 = vec![1.0, 1.0, 1.0, 2.0, 2.0, -1.0];
        let d2 = node_derivatives(&cfg, &dx, &s2);
        assert_close(d2[3], 1.5);
    }

    #[test]
    fn akima_handles_zero_denominator_endpoints() {
        // Flat and constant-slope-0.5 data zero every endpoint denominator; the
        // raw formula would return NaN even at a knot. The secant fallback keeps
        // the interpolant exact (flat data reproduced, and the 0.5 line
        // reproduced with a constant derivative of 0.5).
        let flat = AkimaCubicInterpolation::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0; 4]).unwrap();
        for &x in &[0.0, 0.5, 1.7, 3.0_f64] {
            assert_close(flat.value(x).unwrap(), 0.0);
            assert_close(flat.derivative(x).unwrap(), 0.0);
        }
        let line = AkimaCubicInterpolation::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 0.5, 1.0, 1.5])
            .unwrap();
        for &x in &[0.0, 0.75, 2.4, 3.0_f64] {
            assert_close(line.value(x).unwrap(), 0.5 * x);
            assert_close(line.derivative(x).unwrap(), 0.5);
        }
    }

    #[test]
    fn akima_requires_at_least_4_points() {
        assert!(AkimaCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.0]).is_err());
        assert!(
            AkimaCubicInterpolation::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 0.0, 2.0])
                .is_ok()
        );
    }

    #[test]
    fn domain_range_and_extrapolation() {
        let f = parabolic_sample();
        assert_eq!(f.x_min(), 0.0);
        assert_eq!(f.x_max(), 4.0);
        assert!(f.is_in_range(2.0));
        assert!(!f.is_in_range(-0.1));
        assert!(f.value(-1.0).is_err());
        assert!(f.value(5.0).is_err());

        // With extrapolation the end segment's cubic (still q) extends exactly.
        let g = parabolic_sample().with_extrapolation(true);
        assert!(g.allows_extrapolation());
        assert_close(g.value(5.0).unwrap(), q(5.0));
        assert_close(g.value(-1.0).unwrap(), q(-1.0));
    }

    #[test]
    fn rejects_nan_eval_and_bad_construction() {
        let f = parabolic_sample().with_extrapolation(true);
        assert!(f.value(Real::NAN).is_err());
        assert!(f.derivative(Real::NAN).is_err());
        assert!(f.second_derivative(Real::NAN).is_err());

        let da = CubicDerivativeApprox::Parabolic;
        assert!(CubicInterpolation::new(vec![0.0], vec![1.0], da).is_err());
        assert!(CubicInterpolation::new(vec![0.0, 1.0], vec![1.0], da).is_err());
        assert!(CubicInterpolation::new(vec![1.0, 1.0], vec![1.0, 2.0], da).is_err());
        assert!(CubicInterpolation::new(vec![0.0, Real::NAN], vec![1.0, 2.0], da).is_err());
    }

    #[test]
    fn natural_spline_matches_reference() {
        // x = [0,1,2,3], y = [0,1,0,1]. Reference values from an independent
        // Thomas solve of the natural-spline system: node derivatives
        // [5/3, -1/3, -1/3, 5/3], giving value(0.5)=0.75, value(1.5)=0.5,
        // value(2.5)=0.25.
        let f =
            CubicNaturalSpline::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 0.0, 1.0]).unwrap();
        for (x, y) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0), (3.0, 1.0)] {
            assert_close(f.value(x).unwrap(), y);
        }
        assert_close(f.value(0.5).unwrap(), 0.75);
        assert_close(f.value(1.5).unwrap(), 0.5);
        assert_close(f.value(2.5).unwrap(), 0.25);
        assert_close(f.derivative(0.0).unwrap(), 5.0 / 3.0);
    }

    #[test]
    fn natural_spline_is_c2_with_zero_end_curvature() {
        // Defining properties: zero second derivative at both ends, and the
        // second derivative is continuous across an interior knot.
        let f =
            CubicNaturalSpline::new(vec![0.0, 1.0, 3.0, 4.0], vec![0.0, 2.0, 1.0, 3.0]).unwrap();
        assert_close(f.second_derivative(0.0).unwrap(), 0.0);
        assert_close(f.second_derivative(4.0).unwrap(), 0.0);
        let below = f.second_derivative(1.0 - 1e-7).unwrap();
        let above = f.second_derivative(1.0 + 1e-7).unwrap();
        assert!(
            (below - above).abs() < 1e-5,
            "2nd derivative jumps: {below} vs {above}"
        );
    }

    #[test]
    fn spline_reproduces_linear_data() {
        // A line has zero curvature everywhere, matching the natural boundary,
        // so the natural spline reproduces it exactly.
        let f =
            CubicNaturalSpline::new(vec![0.0, 1.0, 2.0, 4.0], vec![1.0, 3.0, 5.0, 9.0]).unwrap();
        for &x in &[0.3, 1.7, 3.5_f64] {
            assert_close(f.value(x).unwrap(), 1.0 + 2.0 * x);
            assert_close(f.derivative(x).unwrap(), 2.0);
        }
    }

    #[test]
    fn notaknot_spline_reproduces_cubic() {
        // Not-a-knot makes the end segments share a cubic, so a cubic-sampled
        // dataset is reproduced exactly - value and derivative - on any grid.
        let x = vec![0.0, 1.0, 2.5, 4.0, 6.0];
        let y: Vec<Real> = x.iter().map(|&xi| cubic(xi)).collect();
        let f = CubicNaturalSpline::new(x, y)
            .unwrap()
            .with_boundary_conditions(
                CubicBoundaryCondition::NotAKnot,
                0.0,
                CubicBoundaryCondition::NotAKnot,
                0.0,
            )
            .unwrap();
        for &xx in &[0.0, 0.7, 2.5, 3.3, 5.1, 6.0_f64] {
            assert_close(f.value(xx).unwrap(), cubic(xx));
            assert_close(f.derivative(xx).unwrap(), 2.0 + 6.0 * xx + 12.0 * xx * xx);
        }
    }

    #[test]
    fn lagrange_spline_reproduces_cubic() {
        // Lagrange matches each end slope to the cubic through the four nearest
        // points, so a cubic-sampled dataset is reproduced exactly.
        let x = vec![0.0, 1.0, 2.5, 4.0, 6.0];
        let y: Vec<Real> = x.iter().map(|&xi| cubic(xi)).collect();
        let f = CubicNaturalSpline::new(x, y)
            .unwrap()
            .with_boundary_conditions(
                CubicBoundaryCondition::Lagrange,
                0.0,
                CubicBoundaryCondition::Lagrange,
                0.0,
            )
            .unwrap();
        for &xx in &[0.0, 0.7, 2.5, 3.3, 5.1, 6.0_f64] {
            assert_close(f.value(xx).unwrap(), cubic(xx));
            assert_close(f.derivative(xx).unwrap(), 2.0 + 6.0 * xx + 12.0 * xx * xx);
        }
    }

    #[test]
    fn cubic_interpolating_polynomial_derivative_matches_cubic() {
        // The helper returns the exact derivative of the cubic through 4 points.
        let (a, b, c, d) = (0.0, 1.0, 2.5, 4.0);
        let (u, v, w, z) = (cubic(a), cubic(b), cubic(c), cubic(d));
        for &xx in &[0.0, 1.3, 2.5, 4.0, -0.5_f64] {
            assert_close(
                cubic_interpolating_polynomial_derivative([a, b, c, d], [u, v, w, z], xx),
                2.0 + 6.0 * xx + 12.0 * xx * xx,
            );
        }
    }

    #[test]
    fn lagrange_requires_at_least_4_points() {
        // Either Lagrange end fits the four nearest points, so 3 points are
        // rejected up front.
        let three = || CubicNaturalSpline::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.5]).unwrap();
        assert!(
            three()
                .with_boundary_conditions(
                    CubicBoundaryCondition::Lagrange,
                    0.0,
                    CubicBoundaryCondition::SecondDerivative,
                    0.0,
                )
                .is_err()
        );
    }

    #[test]
    fn notaknot_point_count_guards() {
        // Two not-a-knot ends coincide on a single interior knot at 3 points
        // (singular), and either not-a-knot end indexes out of bounds at 2
        // points, so both are rejected. A mixed 3-point case (one not-a-knot
        // end) is well posed and accepted, matching QuantLib's independent rows.
        let three = || CubicNaturalSpline::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.5]).unwrap();
        assert!(
            three()
                .with_boundary_conditions(
                    CubicBoundaryCondition::NotAKnot,
                    0.0,
                    CubicBoundaryCondition::NotAKnot,
                    0.0,
                )
                .is_err()
        );
        // One not-a-knot end at 3 points is valid and passes through the nodes.
        let mixed = three()
            .with_boundary_conditions(
                CubicBoundaryCondition::NotAKnot,
                0.0,
                CubicBoundaryCondition::SecondDerivative,
                0.0,
            )
            .unwrap();
        assert_close(mixed.value(0.0).unwrap(), 0.0);
        assert_close(mixed.value(1.0).unwrap(), 1.0);
        assert_close(mixed.value(2.0).unwrap(), 0.5);
        // Either not-a-knot end at 2 points indexes out of bounds, so rejected.
        let two = || {
            CubicInterpolation::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                CubicDerivativeApprox::Spline,
            )
            .unwrap()
        };
        assert!(
            two()
                .with_boundary_conditions(
                    CubicBoundaryCondition::NotAKnot,
                    0.0,
                    CubicBoundaryCondition::SecondDerivative,
                    0.0,
                )
                .is_err()
        );
    }

    #[test]
    fn solve_tridiagonal_known_system_and_singular() {
        // [[2,1,0],[1,4,1],[0,1,2]] x = [3,0,3] has solution [2,-1,2].
        let x = solve_tridiagonal(&[1.0, 1.0], &[2.0, 4.0, 2.0], &[1.0, 1.0], &[3.0, 0.0, 3.0])
            .unwrap();
        assert_close(x[0], 2.0);
        assert_close(x[1], -1.0);
        assert_close(x[2], 2.0);
        // A zero leading pivot makes the system singular.
        assert!(solve_tridiagonal(&[1.0], &[0.0, 1.0], &[1.0], &[1.0, 1.0]).is_err());
        // A zero pivot produced during elimination is also rejected: row 1
        // pivot = 1 - 1*(1/1) = 0.
        assert!(solve_tridiagonal(&[1.0], &[1.0, 1.0], &[1.0], &[1.0, 1.0]).is_err());
    }

    #[test]
    fn clamped_spline_matches_end_slopes_and_reference() {
        // FirstDerivative both ends (clamped) with slopes 1 and -1. The
        // FirstDerivative row sets d[0]/d[n-1] directly, so the end derivatives
        // equal the clamps exactly. Interior value cross-checked with an
        // independent Thomas solve (value(0.5) = 2/3).
        let f = CubicNaturalSpline::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 0.0, 1.0])
            .unwrap()
            .with_boundary_conditions(
                CubicBoundaryCondition::FirstDerivative,
                1.0,
                CubicBoundaryCondition::FirstDerivative,
                -1.0,
            )
            .unwrap();
        for (x, y) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0), (3.0, 1.0)] {
            assert_close(f.value(x).unwrap(), y);
        }
        assert_close(f.derivative(0.0).unwrap(), 1.0);
        assert_close(f.derivative(3.0).unwrap(), -1.0);
        assert_close(f.value(0.5).unwrap(), 2.0 / 3.0);
    }

    #[test]
    fn clamped_spline_two_points() {
        // n == 2 now flows through the spline solver: a clamped two-point spline
        // takes the end slopes directly, so its node derivatives are [0.5, 3.0].
        let f = CubicInterpolation::new(
            vec![0.0, 2.0],
            vec![1.0, 5.0],
            CubicDerivativeApprox::Spline,
        )
        .unwrap()
        .with_boundary_conditions(
            CubicBoundaryCondition::FirstDerivative,
            0.5,
            CubicBoundaryCondition::FirstDerivative,
            3.0,
        )
        .unwrap();
        assert_close(f.derivative(0.0).unwrap(), 0.5);
        assert_close(f.derivative(2.0).unwrap(), 3.0);
        assert_close(f.value(0.0).unwrap(), 1.0);
        assert_close(f.value(2.0).unwrap(), 5.0);
    }

    #[test]
    fn boundary_conditions_reject_non_finite_values() {
        // A NaN or infinite boundary value would flow into the spline RHS and
        // silently poison later value/derivative results, so it is rejected.
        let spline =
            || CubicNaturalSpline::new(vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 0.0, 1.0]).unwrap();
        assert!(
            spline()
                .with_boundary_conditions(
                    CubicBoundaryCondition::FirstDerivative,
                    Real::NAN,
                    CubicBoundaryCondition::FirstDerivative,
                    -1.0,
                )
                .is_err()
        );
        assert!(
            spline()
                .with_boundary_conditions(
                    CubicBoundaryCondition::SecondDerivative,
                    0.0,
                    CubicBoundaryCondition::SecondDerivative,
                    Real::INFINITY,
                )
                .is_err()
        );
    }

    #[test]
    fn boundary_conditions_are_noop_for_local_schemes() {
        // Local schemes ignore the boundary conditions, so rebuilding a Parabolic
        // interpolation with different ends leaves its values unchanged.
        let base =
            ParabolicInterpolation::new(vec![0.0, 1.0, 3.0, 4.0], vec![1.0, 6.0, 34.0, 57.0])
                .unwrap();
        let before = base.value(2.0).unwrap();
        let rebuilt =
            ParabolicInterpolation::new(vec![0.0, 1.0, 3.0, 4.0], vec![1.0, 6.0, 34.0, 57.0])
                .unwrap()
                .with_boundary_conditions(
                    CubicBoundaryCondition::FirstDerivative,
                    99.0,
                    CubicBoundaryCondition::SecondDerivative,
                    -7.0,
                )
                .unwrap();
        assert_close(rebuilt.value(2.0).unwrap(), before);
    }
}
