//! Checked Hull-White short-rate dynamics under a validated forward measure.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::processes::{ForwardMeasureProcess1D, ForwardMeasureTime, OrnsteinUhlenbeckProcess};
use crate::require;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::frequency::Frequency;
use crate::types::{Real, Time};

/// Hull-White short-rate process under the `T`-forward measure.
///
/// Follows `ql/processes/hullwhiteprocess.{hpp,cpp}`. The initial short rate is
/// captured at construction; drift and fitting shifts read the live curve.
/// Curve changes and horizon changes notify observers. The initial horizon is
/// zero and may be replaced through [`ForwardMeasureProcess1D`]. Times need not
/// precede the horizon. Date conversion remains unsupported, as in QuantLib.
///
/// Stable algebra supplies the zero-mean-reversion drift limit and avoids the
/// native small-`a` cancellation in `B`, `alpha` and `M_T`. Transition variance
/// retains QuantLib's Ornstein-Uhlenbeck small-speed branch.
pub struct HullWhiteForwardProcess {
    process: OrnsteinUhlenbeckProcess,
    curve: Handle<dyn YieldTermStructure>,
    a: Real,
    sigma: Real,
    horizon: ForwardMeasureTime,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl HullWhiteForwardProcess {
    /// Constructs a process with initial forward-measure horizon zero.
    ///
    /// # Errors
    ///
    /// Rejects nonfinite or negative parameters and unavailable/nonfinite
    /// initial curve forwards. Curve queries do not request extrapolation.
    pub fn new(curve: Handle<dyn YieldTermStructure>, a: Real, sigma: Real) -> QlResult<Self> {
        require!(
            a.is_finite() && a >= 0.0,
            "a must be finite and nonnegative"
        );
        require!(
            sigma.is_finite() && sigma >= 0.0,
            "sigma must be finite and nonnegative"
        );
        let initial = Self::forward(&curve, 0.0)?;
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::forwarding(observable.clone());
        curve.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        Ok(Self {
            process: OrnsteinUhlenbeckProcess::new(a, sigma, initial, 0.0)?,
            curve,
            a,
            sigma,
            horizon: ForwardMeasureTime::new(0.0)?,
            observable,
            _listener: listener,
        })
    }

    /// Returns the constant mean-reversion speed.
    pub fn a(&self) -> Real {
        self.a
    }

    /// Returns the constant short-rate volatility.
    pub fn sigma(&self) -> Real {
        self.sigma
    }

    /// Returns the curve-fitting shift `f(t,t) + sigma² B(0,t)² / 2`.
    ///
    /// # Errors
    ///
    /// Rejects invalid times, unavailable curve forwards and nonfinite results.
    pub fn alpha(&self, t: Time) -> QlResult<Real> {
        Self::time_argument(t)?;
        let shift = if self.sigma == 0.0 {
            0.0
        } else {
            let scaled = self.sigma * self.b_interval(t)?;
            0.5 * scaled * scaled
        };
        Self::finite(shift + Self::forward(&self.curve, t)?)
    }

    /// Returns the affine factor `B(t,T) = (1-exp(-a(T-t)))/a`.
    ///
    /// # Errors
    ///
    /// Rejects nonfinite/negative endpoint times and nonfinite results. `T < t`
    /// is allowed, yielding a negative factor; at `a = 0`, returns `T-t`.
    pub fn b(&self, t: Time, maturity: Time) -> QlResult<Real> {
        Self::time_argument(t)?;
        Self::time_argument(maturity)?;
        self.b_interval(maturity - t)
    }

    /// Returns the integrated forward-measure adjustment `M_T(s,t,T)`.
    ///
    /// Evaluates `sigma² ∫[s,t] exp(-a(t-u)) B(u,T) du` using stable algebra.
    /// At zero `a` this is `sigma² (t-s) ((T-t) + (t-s)/2)`.
    ///
    /// # Errors
    ///
    /// Requires finite nonnegative times, `s <= t`, and a finite result. The
    /// measure horizon may precede either endpoint, as in QuantLib.
    pub fn m_t(&self, s: Time, t: Time, maturity: Time) -> QlResult<Real> {
        Self::time_argument(s)?;
        Self::time_argument(t)?;
        Self::time_argument(maturity)?;
        require!(
            s.partial_cmp(&t).is_some_and(|order| order.is_le()),
            "M_T requires s <= t"
        );
        if s == t || self.sigma == 0.0 {
            return Ok(0.0);
        }
        let duration = self.b_interval(t - s)?;
        let remaining = self.b_interval(maturity - t)?;
        Self::finite(
            self.sigma
                * self.sigma
                * duration
                * (remaining + 0.5 * (-self.a * (maturity - t)).exp() * duration),
        )
    }

    fn b_interval(&self, interval: Time) -> QlResult<Real> {
        let exponent = -self.a * interval;
        Self::finite(if self.a == 0.0 || exponent == 0.0 {
            interval
        } else {
            -exponent.exp_m1() / self.a
        })
    }

    fn forward(curve: &Handle<dyn YieldTermStructure>, t: Time) -> QlResult<Real> {
        Self::finite(
            curve
                .current_link()?
                .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)?
                .rate(),
        )
    }

    fn time_argument(t: Time) -> QlResult<()> {
        require!(
            t.is_finite() && t >= 0.0,
            "time must be finite and nonnegative"
        );
        Ok(())
    }

    fn state(t: Time, x: Real) -> QlResult<()> {
        Self::time_argument(t)?;
        require!(x.is_finite(), "state must be finite");
        Ok(())
    }

    fn step(t: Time, x: Real, dt: Time) -> QlResult<Time> {
        Self::state(t, x)?;
        Self::time_argument(dt)?;
        Self::finite(t + dt)
    }

    fn finite(value: Real) -> QlResult<Real> {
        require!(value.is_finite(), "process produced a nonfinite value");
        Ok(value)
    }
}

impl AsObservable for HullWhiteForwardProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl ForwardMeasureProcess1D for HullWhiteForwardProcess {
    fn forward_measure_state(&self) -> &ForwardMeasureTime {
        &self.horizon
    }
}

impl StochasticProcess1D for HullWhiteForwardProcess {
    fn x0(&self) -> QlResult<Real> {
        self.process.x0()
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        Self::state(t, x)?;
        let shift = 0.0001;
        let shifted_time = Self::finite(t + shift)?;
        let forward = Self::forward(&self.curve, t)?;
        let derivative = (Self::forward(&self.curve, shifted_time)? - forward) / shift;
        let volatility_drift = if self.sigma == 0.0 {
            0.0
        } else {
            self.sigma
                * self.sigma
                * (0.5 * self.b_interval(t)? * (1.0 + (-self.a * t).exp())
                    - self.b(t, self.forward_measure_time())?)
        };
        Self::finite(self.process.drift(t, x)? + volatility_drift + self.a * forward + derivative)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        Self::state(t, x)?;
        Ok(self.sigma)
    }

    fn expectation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        let end = Self::step(t, x, dt)?;
        if dt == 0.0 {
            return Ok(x);
        }
        Self::finite(
            self.process.expectation(t, x, dt)? + self.alpha(end)?
                - self.alpha(t)? * (-self.a * dt).exp()
                - self.m_t(t, end, self.forward_measure_time())?,
        )
    }

    fn variance(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        if dt == 0.0 || self.sigma == 0.0 {
            return Ok(0.0);
        }
        Self::finite(self.process.variance(t, x, dt)?)
    }

    fn std_deviation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Ok(self.variance(t, x, dt)?.sqrt())
    }

    fn evolve(&self, t: Time, x: Real, dt: Time, dw: Real) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        require!(dw.is_finite(), "shock must be finite");
        if dt == 0.0 {
            return Ok(x);
        }
        Self::finite(self.expectation(t, x, dt)? + self.std_deviation(t, x, dt)? * dw)
    }
}
