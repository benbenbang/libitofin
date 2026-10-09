//! Standalone ABCD instantaneous and integrated rate covariance.

use crate::errors::QlResult;
use crate::fail;
use crate::math::abcdmathfunction::{AbcdMathFunction, finite, non_negative};
use crate::types::{Real, Time};

/// A checked ABCD rate-volatility shape, not a calibrated market model.
///
/// Unlike the math curve, defaults are `(-0.06, 0.17, 0.54, 0.17)`.
#[derive(Clone, Copy, Debug)]
pub struct AbcdFunction {
    math: AbcdMathFunction,
}

impl AbcdFunction {
    /// Constructs a shape with QuantLib's coefficient constraints.
    ///
    /// # Errors
    /// As [`AbcdMathFunction::new`].
    pub fn new(a: Real, b: Real, c: Real, d: Real) -> QlResult<Self> {
        Ok(Self {
            math: AbcdMathFunction::new(a, b, c, d)?,
        })
    }

    /// Constructs QuantLib's volatility defaults.
    pub fn with_defaults() -> QlResult<Self> {
        Self::new(-0.06, 0.17, 0.54, 0.17)
    }
    /// Exponential intercept coefficient.
    pub fn a(&self) -> Real {
        self.math.a()
    }
    /// Exponential slope coefficient.
    pub fn b(&self) -> Real {
        self.math.b()
    }
    /// Decay coefficient.
    pub fn c(&self) -> Real {
        self.math.c()
    }
    /// Long-term volatility level.
    pub fn d(&self) -> Real {
        self.math.d()
    }
    /// Coefficients in `(a, b, c, d)` order.
    pub fn coefficients(&self) -> &[Real; 4] {
        self.math.coefficients()
    }

    /// Evaluates `f(t)`, with zero for finite negative time.
    ///
    /// # Errors
    /// As [`AbcdMathFunction::value`].
    pub fn value(&self, t: Time) -> QlResult<Real> {
        self.math.value(t)
    }

    /// Instantaneous covariance `f(T-t)*f(S-t)`.
    ///
    /// # Errors
    /// Rejects non-finite times, unrepresentable lags or computed covariance.
    pub fn instantaneous_covariance(&self, t: Time, t_fix: Time, s_fix: Time) -> QlResult<Real> {
        for value in [t, t_fix, s_fix] {
            finite(value)?;
        }
        non_negative(self.value(t_fix - t)? * self.value(s_fix - t)?)
    }

    /// Integral of instantaneous covariance over `[t1, t2]`.
    ///
    /// The upper bound is clipped at `min(T,S)`. Signed finite observation and
    /// fixing times are allowed; only their differences enter the calculation.
    /// Stable analytic exponential moments avoid subtracting large primitives.
    ///
    /// # Errors
    /// Rejects non-finite times, reversed bounds, unrepresentable intermediate
    /// arithmetic or a negative/non-finite result. No negative result is clamped.
    pub fn covariance(&self, t1: Time, t2: Time, t_fix: Time, s_fix: Time) -> QlResult<Real> {
        for value in [t1, t2, t_fix, s_fix] {
            finite(value)?;
        }
        if t1 > t2 {
            fail!("ABCD integration bounds are reversed");
        }
        let end = t2.min(t_fix.min(s_fix));
        if t1 >= end {
            return Ok(0.0);
        }
        let length = finite(end - t1)?;
        let lag_t = finite(t_fix - end)?;
        let lag_s = finite(s_fix - end)?;
        let et = (-self.c() * lag_t).exp();
        let es = (-self.c() * lag_s).exp();
        let at = finite(self.a() + self.b() * lag_t)?;
        let as_ = finite(self.a() + self.b() * lag_s)?;
        let z = finite(self.c() * length)?;
        if z < 0.5 {
            let ft = local_series(self.value(lag_t)?, at, self.b(), et, z, length)?;
            let fs = local_series(self.value(lag_s)?, as_, self.b(), es, z, length)?;
            let mut sum = 0.0;
            for (i, ti) in ft.iter().enumerate() {
                for (j, sj) in fs.iter().enumerate() {
                    sum += ti * sj / (i + j + 1) as Real;
                }
            }
            return non_negative(length * sum);
        }
        let m = moments(self.c(), length)?;
        let m2 = moments(finite(2.0 * self.c())?, length)?;
        let exponential = et
            * es
            * (at * as_ * m2[0] + self.b() * (at + as_) * m2[1] + self.b() * self.b() * m2[2]);
        let mixed =
            self.d() * (et * (at * m[0] + self.b() * m[1]) + es * (as_ * m[0] + self.b() * m[1]));
        non_negative(exponential + mixed + self.d() * self.d() * length)
    }

    /// Integrated variance `integral f(T-t)^2 dt`, without duration normalization.
    ///
    /// # Errors
    /// As [`Self::covariance`].
    pub fn variance(&self, t_min: Time, t_max: Time, t_fix: Time) -> QlResult<Real> {
        self.covariance(t_min, t_max, t_fix, t_fix)
    }
}

impl Default for AbcdFunction {
    fn default() -> Self {
        Self {
            math: AbcdMathFunction::new(-0.06, 0.17, 0.54, 0.17)
                .expect("fixed ABCD volatility defaults are valid"),
        }
    }
}

/// Immutable integrand `t -> f(T-t)*f(S-t)`; `T` and `S` need not coincide.
#[derive(Clone, Copy, Debug)]
pub struct AbcdSquared {
    abcd: AbcdFunction,
    t_fix: Time,
    s_fix: Time,
}

impl AbcdSquared {
    /// Constructs an integrand with finite fixing times.
    ///
    /// # Errors
    /// As [`AbcdFunction::new`], plus non-finite fixing times.
    pub fn new(a: Real, b: Real, c: Real, d: Real, t_fix: Time, s_fix: Time) -> QlResult<Self> {
        finite(t_fix)?;
        finite(s_fix)?;
        Ok(Self {
            abcd: AbcdFunction::new(a, b, c, d)?,
            t_fix,
            s_fix,
        })
    }

    /// Evaluates instantaneous covariance at `t`.
    ///
    /// # Errors
    /// As [`AbcdFunction::instantaneous_covariance`].
    pub fn value(&self, t: Time) -> QlResult<Real> {
        self.abcd
            .instantaneous_covariance(t, self.t_fix, self.s_fix)
    }
}

fn moments(rate: Real, length: Real) -> QlResult<[Real; 3]> {
    let z = finite(rate * length)?;
    let mut result = [0.0; 3];
    if z < 0.5 {
        for (n, value) in result.iter_mut().enumerate() {
            let mut term = 1.0;
            let mut sum = 1.0 / (n + 1) as Real;
            for k in 1..=32 {
                term *= -z / k as Real;
                sum += term / (n + k + 1) as Real;
            }
            *value = length.powi((n + 1) as i32) * sum;
        }
    } else {
        let decay = (-z).exp();
        result[0] = -(-z).exp_m1() / rate;
        result[1] = (result[0] - length * decay) / rate;
        result[2] = (2.0 * result[1] - length * length * decay) / rate;
    }
    for value in result {
        non_negative(value)?;
    }
    Ok(result)
}

fn local_series(
    initial: Real,
    affine: Real,
    slope: Real,
    decay: Real,
    z: Real,
    length: Real,
) -> QlResult<[Real; 33]> {
    let mut coefficients = [0.0; 33];
    coefficients[0] = initial;
    let scaled_slope = finite(slope * length)?;
    let mut power = 1.0;
    for (n, value) in coefficients.iter_mut().enumerate().skip(1) {
        let next = power * (-z) / n as Real;
        *value = finite(decay * (affine * next + scaled_slope * power))?;
        power = next;
    }
    Ok(coefficients)
}
