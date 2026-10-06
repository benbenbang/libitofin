//! Standalone constant-coefficient geometric Brownian motion.
//!
//! Follows `ql/processes/geometricbrownianprocess.hpp`: instantaneous drift
//! and diffusion are `mu * x` and `sigma * x`. Finite transitions use the
//! generic Euler strategy, not the exact lognormal scheme used by simulation
//! helpers. Euler states may be negative and their diffusion retains its sign.

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable};
use crate::processes::discretization::{EulerDiscretization, ProcessDiscretization1D};
use crate::require;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Time};

/// The scalar process `dx = mu * x * dt + sigma * x * dW` with Euler transitions.
///
/// Parameters are immutable scalars, so no market inputs are observed. Signed
/// initial values and states are supported. Times and steps must be finite and
/// nonnegative; states, shocks and returned values must be finite. A zero step
/// preserves the state exactly without evaluating potentially overflowing
/// coefficients. Date-to-time conversion is unsupported.
pub struct GeometricBrownianMotionProcess {
    initial: Real,
    mu: Real,
    volatility: Real,
    observable: Shared<Observable>,
}

impl GeometricBrownianMotionProcess {
    /// Builds a process from its initial state, drift rate and volatility.
    ///
    /// # Errors
    ///
    /// Returns an error for nonfinite parameters or negative volatility.
    pub fn new(initial: Real, mu: Real, volatility: Real) -> QlResult<Self> {
        require!(initial.is_finite(), "initial state must be finite");
        require!(mu.is_finite(), "drift rate must be finite");
        require!(
            volatility.is_finite() && volatility >= 0.0,
            "volatility must be finite and nonnegative"
        );
        Ok(Self {
            initial,
            mu,
            volatility,
            observable: shared(Observable::new()),
        })
    }

    /// Returns the constant drift rate.
    pub fn mu(&self) -> Real {
        self.mu
    }

    /// Returns the constant volatility.
    pub fn volatility(&self) -> Real {
        self.volatility
    }

    fn state(t: Time, x: Real) -> QlResult<()> {
        require!(
            t.is_finite() && t >= 0.0,
            "time must be finite and nonnegative"
        );
        require!(x.is_finite(), "state must be finite");
        Ok(())
    }

    fn step(t: Time, x: Real, dt: Time) -> QlResult<()> {
        Self::state(t, x)?;
        require!(
            dt.is_finite() && dt >= 0.0,
            "time step must be finite and nonnegative"
        );
        Ok(())
    }

    fn finite(value: Real) -> QlResult<Real> {
        require!(value.is_finite(), "process produced a nonfinite value");
        Ok(value)
    }
}

impl AsObservable for GeometricBrownianMotionProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess1D for GeometricBrownianMotionProcess {
    fn x0(&self) -> QlResult<Real> {
        Ok(self.initial)
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        Self::state(t, x)?;
        Self::finite(self.mu * x)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        Self::state(t, x)?;
        Self::finite(self.volatility * x)
    }

    fn expectation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        if dt == 0.0 {
            return Ok(x);
        }
        Self::finite(self.apply(x, EulerDiscretization.drift(self, t, x, dt)?))
    }

    fn variance(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        if dt == 0.0 {
            return Ok(0.0);
        }
        Self::finite(EulerDiscretization.variance(self, t, x, dt)?)
    }

    fn std_deviation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        if dt == 0.0 {
            return Ok(0.0);
        }
        Self::finite(EulerDiscretization.diffusion(self, t, x, dt)?)
    }

    fn evolve(&self, t: Time, x: Real, dt: Time, dw: Real) -> QlResult<Real> {
        Self::step(t, x, dt)?;
        require!(dw.is_finite(), "shock must be finite");
        if dt == 0.0 {
            return Ok(x);
        }
        Self::finite(self.apply(
            self.expectation(t, x, dt)?,
            self.std_deviation(t, x, dt)? * dw,
        ))
    }
}

#[cfg(test)]
#[path = "geometricbrownianprocess_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "geometricbrownianprocess_oracle_tests.rs"]
mod oracle_tests;
