//! Constant-parameter Gaussian short-rate state process in a forward measure.
//!
//! This is the standalone process, not a GSR model or calibration engine.

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable};
use crate::processes::forwardmeasureprocess::{ForwardMeasureProcess1D, ForwardMeasureTime};
use crate::require;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Time};

/// Constant-volatility, constant-reversion GSR process under a `T`-forward measure.
///
/// Dynamics are `dx = (y(t) - sigma² G(t,T) - a x) dt + sigma dW`, with
/// `y(t) = sigma² (1-exp(-2at))/(2a)` and `G(t,T) = (1-exp(-a(T-t)))/a`.
/// Zero reversion uses the continuous limits, without a small-reversion cutoff.
/// All dynamics require finite state and times in `[0,T]`. Date conversion is
/// unsupported. Parameter grids must be constant; changing the horizon through
/// [`ForwardMeasureProcess1D`] immediately changes drift and transition mean.
pub struct GsrProcess {
    a: Real,
    sigma: Real,
    measure: ForwardMeasureTime,
    observable: Observable,
}

impl GsrProcess {
    /// Constructs the constant-parameter subset of QuantLib's grid-shaped API.
    ///
    /// Repeated identical parameters are accepted. Redundant grid boundaries do
    /// not restrict subsequent horizon changes, including changes to zero.
    ///
    /// # Errors
    /// Rejects invalid grid lengths, nonfinite/negative or non-increasing times,
    /// nonfinite/negative volatility, nonfinite reversion, varying parameters,
    /// and nonfinite/negative measure maturity. Signed reversion is supported.
    pub fn new(
        times: Vec<Time>,
        vols: Vec<Real>,
        reversions: Vec<Real>,
        t_measure: Time,
    ) -> QlResult<Self> {
        require!(
            vols.len().checked_sub(1) == Some(times.len()),
            "number of volatilities must exceed number of times by one"
        );
        require!(
            reversions.len() == 1 || reversions.len() == vols.len(),
            "reversions must contain one value or match the volatility count"
        );
        require!(
            times.iter().all(|t| t.is_finite() && *t >= 0.0)
                && times.windows(2).all(|p| p[0] < p[1]),
            "grid times must be finite, nonnegative and strictly increasing"
        );
        let sigma = vols[0];
        let a = reversions[0];
        require!(
            sigma.is_finite() && sigma >= 0.0 && vols.iter().all(|v| *v == sigma),
            "only finite nonnegative constant volatility is supported"
        );
        require!(
            a.is_finite() && reversions.iter().all(|v| *v == a),
            "only finite constant reversion is supported"
        );
        Ok(Self {
            a,
            sigma,
            measure: ForwardMeasureTime::new(t_measure)?,
            observable: Observable::new(),
        })
    }

    /// Returns constant instantaneous volatility.
    ///
    /// # Errors
    /// Rejects time outside the finite interval `[0,T]`.
    pub fn sigma(&self, t: Time) -> QlResult<Real> {
        self.check_t(t)?;
        Ok(self.sigma)
    }

    /// Returns constant signed mean reversion.
    ///
    /// # Errors
    /// Rejects time outside the finite interval `[0,T]`.
    pub fn reversion(&self, t: Time) -> QlResult<Real> {
        self.check_t(t)?;
        Ok(self.a)
    }

    /// Returns the auxiliary variance `y(t)`.
    ///
    /// # Errors
    /// Rejects invalid time or a nonfinite result.
    pub fn y(&self, t: Time) -> QlResult<Real> {
        self.check_t(t)?;
        self.integrated_variance(t)
    }

    /// Returns the affine kernel `G(t,end)` independently of state.
    ///
    /// # Errors
    /// Requires `0 <= t <= end <= T` and a finite result.
    pub fn g(&self, t: Time, end: Time) -> QlResult<Real> {
        self.check_t(t)?;
        self.check_t(end)?;
        require!(
            end.partial_cmp(&t).is_some_and(|order| order.is_ge()),
            "G requires ordered endpoints"
        );
        finite(kernel(self.a, end - t))
    }

    fn check_t(&self, t: Time) -> QlResult<()> {
        require!(
            t.is_finite() && t >= 0.0 && t <= self.measure.get(),
            "time must be finite and within the forward-measure horizon"
        );
        Ok(())
    }

    fn state(&self, t: Time, x: Real) -> QlResult<()> {
        self.check_t(t)?;
        require!(x.is_finite(), "state must be finite");
        Ok(())
    }

    fn step(&self, t: Time, x: Real, dt: Time) -> QlResult<Time> {
        self.state(t, x)?;
        require!(
            dt.is_finite() && dt >= 0.0,
            "step must be finite and nonnegative"
        );
        let end = t + dt;
        self.check_t(end)?;
        Ok(end)
    }

    fn integrated_variance(&self, dt: Time) -> QlResult<Real> {
        if dt == 0.0 || self.sigma == 0.0 {
            return Ok(0.0);
        }
        let z = (-self.a * dt) * 2.0;
        let log_integral = if self.a == 0.0 || z == 0.0 {
            dt.ln()
        } else if self.a > 0.0 {
            (-z.exp_m1()).ln() - self.a.ln() - 2.0_f64.ln()
        } else {
            z + (-(-z).exp_m1()).ln() - (-self.a).ln() - 2.0_f64.ln()
        };
        let integral = if z == 0.0 {
            dt
        } else {
            (-z.exp_m1() / self.a) * 0.5
        };
        let value = self.sigma * (self.sigma * integral);
        if value.is_normal() && integral.is_normal() && (self.sigma * integral).is_normal() {
            return Ok(value);
        }
        finite((2.0 * self.sigma.ln() + log_integral).exp())
    }
}

impl AsObservable for GsrProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl ForwardMeasureProcess1D for GsrProcess {
    fn forward_measure_state(&self) -> &ForwardMeasureTime {
        &self.measure
    }
}

impl StochasticProcess1D for GsrProcess {
    fn x0(&self) -> QlResult<Real> {
        Ok(0.0)
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        self.state(t, x)?;
        let adjustment = if self.sigma == 0.0 {
            0.0
        } else {
            self.sigma * (self.sigma * self.g(t, self.measure.get())?)
        };
        finite(self.y(t)? - adjustment - self.a * x)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        self.state(t, x)?;
        Ok(self.sigma)
    }

    fn expectation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        let end = self.step(t, x, dt)?;
        if dt == 0.0 {
            return Ok(x);
        }
        let decay = (-self.a * dt).exp();
        let initial = if x == 0.0 { 0.0 } else { x * decay };
        if self.sigma == 0.0 {
            return finite(initial);
        }
        let duration = kernel(self.a, dt);
        let remaining = self.g(end, self.measure.get())?;
        let adjustment = (self.sigma * duration) * (self.sigma * remaining) * (0.5 + 0.5 * decay);
        finite(initial + duration * (decay * self.y(t)?) - adjustment)
    }

    fn variance(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        self.step(t, x, dt)?;
        self.integrated_variance(dt)
    }

    fn std_deviation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Ok(self.variance(t, x, dt)?.sqrt())
    }

    fn evolve(&self, t: Time, x: Real, dt: Time, dw: Real) -> QlResult<Real> {
        require!(dw.is_finite(), "Gaussian increment must be finite");
        finite(self.expectation(t, x, dt)? + self.std_deviation(t, x, dt)? * dw)
    }
}

fn kernel(a: Real, dt: Time) -> Real {
    let z = -a * dt;
    if a == 0.0 || z == 0.0 {
        dt
    } else {
        -z.exp_m1() / a
    }
}

fn finite(value: Real) -> QlResult<Real> {
    require!(
        value.is_finite(),
        "process result is not representable as finite binary64"
    );
    Ok(value)
}
