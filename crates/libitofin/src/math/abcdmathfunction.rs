//! Checked ABCD curve values, with QuantLib's non-negative support contract.

use crate::errors::QlResult;
use crate::types::{Real, Time};
use crate::{fail, require};

/// The immutable curve `f(t) = (a + b*t)*exp(-c*t) + d`, zero for `t < 0`.
///
/// This scoped port provides values and coefficients, not QuantLib's primitive,
/// derivatives or extrema helpers. Numerical queries reject non-finite results.
#[derive(Clone, Copy, Debug)]
pub struct AbcdMathFunction {
    coefficients: [Real; 4],
}

impl AbcdMathFunction {
    /// Constructs a globally non-negative ABCD curve.
    ///
    /// # Errors
    /// Returns an error for non-finite coefficients, `c <= 0`, `d < 0`, a
    /// negative initial value or a negative stationary minimum.
    pub fn new(a: Real, b: Real, c: Real, d: Real) -> QlResult<Self> {
        validate(a, b, c, d)?;
        Ok(Self {
            coefficients: [a, b, c, d],
        })
    }

    /// Constructs QuantLib's math defaults: `(0.002, 0.001, 0.16, 0.0005)`.
    pub fn with_defaults() -> QlResult<Self> {
        Self::new(0.002, 0.001, 0.16, 0.0005)
    }

    /// Exponential intercept coefficient.
    pub fn a(&self) -> Real {
        self.coefficients[0]
    }
    /// Exponential slope coefficient.
    pub fn b(&self) -> Real {
        self.coefficients[1]
    }
    /// Strictly positive decay coefficient.
    pub fn c(&self) -> Real {
        self.coefficients[2]
    }
    /// Non-negative long-term level.
    pub fn d(&self) -> Real {
        self.coefficients[3]
    }
    /// Coefficients in `(a, b, c, d)` order.
    pub fn coefficients(&self) -> &[Real; 4] {
        &self.coefficients
    }

    /// Evaluates the curve, including `f(0) = a + d`.
    ///
    /// # Errors
    /// Rejects non-finite time or a non-finite/negative computed value. Finite
    /// negative time returns zero, matching QuantLib's support cutoff.
    pub fn value(&self, t: Time) -> QlResult<Real> {
        finite(t)?;
        if t < 0.0 {
            return Ok(0.0);
        }
        let decay = -self.c() * t;
        let affine = finite(self.a() + self.b() * t)?;
        let result = if decay > -0.5 {
            (self.a() + self.d()) + self.b() * t + affine * decay.exp_m1()
        } else {
            affine * decay.exp() + self.d()
        };
        non_negative(result)
    }
}

impl Default for AbcdMathFunction {
    fn default() -> Self {
        Self {
            coefficients: [0.002, 0.001, 0.16, 0.0005],
        }
    }
}

/// Validates QuantLib's globally non-negative ABCD coefficient constraints.
///
/// # Errors
/// Rejects non-finite coefficients and invalid shapes. The stationary-minimum
/// comparison uses logarithms rather than an overflow-prone exponential bound.
/// An exactly zero long-term level with `b < 0` is invalid even when its eventual
/// negative tail would underflow in machine arithmetic.
pub fn validate(a: Real, b: Real, c: Real, d: Real) -> QlResult<()> {
    for value in [a, b, c, d] {
        finite(value)?;
    }
    if c <= 0.0 {
        fail!("ABCD c must be positive");
    }
    if d < 0.0 {
        fail!("ABCD d must be non-negative");
    }
    non_negative(a + d)?;
    if b < 0.0 {
        let ratio = a / b;
        let first = c * ratio;
        let stationary = if first.is_finite() && (ratio != 0.0 || a == 0.0) {
            first
        } else {
            let product = c * a;
            if product != 0.0 && product.is_finite() {
                product / b
            } else {
                a * (c / b)
            }
        };
        require!(
            !stationary.is_nan(),
            "ABCD stationary minimum is indeterminate"
        );
        if stationary <= 1.0 {
            if d == 0.0 {
                fail!("ABCD negative slope has a negative tail with d=0");
            }
            let log_depth = (-b).ln() - c.ln() + stationary - 1.0;
            if log_depth > d.ln() {
                fail!("ABCD stationary minimum is negative");
            }
        }
    }
    Ok(())
}

pub(crate) fn finite(value: Real) -> QlResult<Real> {
    require!(
        value.is_finite(),
        "ABCD quantity must be finite, got {value}"
    );
    Ok(value)
}

pub(crate) fn non_negative(value: Real) -> QlResult<Real> {
    finite(value)?;
    if value < 0.0 {
        fail!("ABCD quantity must be non-negative, got {value}");
    }
    Ok(value)
}
