//! Standalone Markov-functional state process with exact Gaussian transitions.
//!
//! Preserves the native raw-volatility diffusion and the empty-grid unit-volatility
//! variance convention. This is not a Markov-functional interest-rate model.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Time};

/// Zero-drift Gaussian state process with piecewise-constant volatility.
///
/// Transition variance integrates `sigma(s)^2 exp(2 a s)`. Native `diffusion`
/// instead returns raw `sigma(t)`, including when `a` is nonzero. With no bucket
/// times, variance uses unit volatility irrespective of the supplied volatility.
pub struct MfStateProcess {
    reversion: Real,
    times: Array,
    vols: Array,
    observable: Observable,
}

impl MfStateProcess {
    /// Constructs an immutable process. Bucket boundaries are right-continuous.
    ///
    /// # Errors
    /// Rejects nonfinite reversion, negative/nonfinite times or volatilities,
    /// non-increasing times, and any volatility count other than `times.len()+1`.
    pub fn new(reversion: Real, times: Array, vols: Array) -> QlResult<Self> {
        require!(reversion.is_finite(), "reversion must be finite");
        require!(
            vols.size().checked_sub(1) == Some(times.size()),
            "number of volatilities must exceed number of times by one"
        );
        for &t in times.iter() {
            validate_time(t)?;
        }
        require!(
            times.windows(2).all(|pair| pair[0] < pair[1]),
            "times must be strictly increasing"
        );
        require!(
            vols.iter().all(|v| v.is_finite() && *v >= 0.0),
            "volatilities must be finite and nonnegative"
        );
        Ok(Self {
            reversion,
            times,
            vols,
            observable: Observable::new(),
        })
    }

    /// Returns the signed exponential reversion parameter.
    pub fn reversion(&self) -> Real {
        self.reversion
    }

    /// Returns the bucket boundaries in process years.
    pub fn times(&self) -> &[Time] {
        &self.times
    }

    /// Returns the raw bucket volatilities, including the terminal bucket.
    pub fn volatilities(&self) -> &[Real] {
        &self.vols
    }

    fn bucket_variance(&self, start: Time, end: Time, vol: Real) -> QlResult<Real> {
        if vol == 0.0 || end == start {
            return Ok(0.0);
        }
        let length = end - start;
        let a = if self.reversion.abs() < Real::EPSILON {
            0.0
        } else {
            self.reversion
        };
        let anchor = if a > 0.0 { end } else { start };
        let z = (a.abs() * length) * 2.0;
        let ratio = if z == 0.0 { 1.0 } else { -(-z).exp_m1() / z };
        let exponent = (a * anchor) * 2.0;
        let squared_vol = vol * vol;
        let scale = exponent.exp();
        let scaled = squared_vol * scale;
        let integrated = scaled * length;
        let direct = integrated * ratio;
        if [squared_vol, scale, scaled, integrated, direct]
            .iter()
            .all(|value| value.is_normal())
        {
            return Ok(direct);
        }
        let log_integral = if a == 0.0 || z == 0.0 {
            length.ln()
        } else {
            (-(-z).exp_m1()).ln() - a.abs().ln() - 2.0_f64.ln()
        };
        let value = (2.0 * vol.ln() + exponent + log_integral).exp();
        require!(value.is_finite(), "transition variance overflow");
        Ok(value)
    }
}

impl AsObservable for MfStateProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess1D for MfStateProcess {
    fn x0(&self) -> QlResult<Real> {
        Ok(0.0)
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        validate_state(t, x)?;
        Ok(0.0)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        validate_state(t, x)?;
        Ok(self.vols[self.times.partition_point(|boundary| *boundary <= t)])
    }

    fn expectation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        validate_interval(t0, x0, dt)?;
        Ok(x0)
    }

    fn variance(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        let end = validate_interval(t0, x0, dt)?;
        if dt < Real::EPSILON {
            return Ok(0.0);
        }
        if self.times.is_empty() {
            if self.reversion.abs() < Real::EPSILON {
                return Ok(dt);
            }
            return self.bucket_variance(t0, end, 1.0);
        }
        let mut bucket = self.times.partition_point(|boundary| *boundary <= t0);
        let mut start = t0;
        let mut variance = 0.0;
        while start < end {
            let stop = self.times.get(bucket).copied().unwrap_or(end).min(end);
            variance += self.bucket_variance(start, stop, self.vols[bucket])?;
            require!(variance.is_finite(), "transition variance overflow");
            start = stop;
            bucket += 1;
        }
        Ok(variance)
    }

    fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        Ok(self.variance(t0, x0, dt)?.sqrt())
    }

    fn evolve(&self, t0: Time, x0: Real, dt: Time, dw: Real) -> QlResult<Real> {
        require!(dw.is_finite(), "Gaussian increment must be finite");
        let value = self.expectation(t0, x0, dt)? + self.std_deviation(t0, x0, dt)? * dw;
        require!(value.is_finite(), "evolved state overflow");
        Ok(value)
    }
}

fn validate_time(t: Time) -> QlResult<()> {
    require!(
        t.is_finite() && t >= 0.0,
        "time must be finite and nonnegative"
    );
    Ok(())
}

fn validate_state(t: Time, x: Real) -> QlResult<()> {
    validate_time(t)?;
    require!(x.is_finite(), "state must be finite");
    Ok(())
}

fn validate_interval(t0: Time, x0: Real, dt: Time) -> QlResult<Time> {
    validate_state(t0, x0)?;
    validate_time(dt)?;
    let end = t0 + dt;
    require!(end.is_finite(), "transition end time overflow");
    Ok(end)
}
