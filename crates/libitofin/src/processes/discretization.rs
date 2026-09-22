//! Pluggable discretization of scalar and multifactor stochastic processes.
//!
//! Strategy contracts and Euler formulas follow `ql/stochasticprocess.hpp` and
//! `ql/processes/eulerdiscretization.cpp`. Explicit adapters select a strategy
//! without changing the exact transition overrides of the original process.
//! Adapter transitions reject invalid time steps, nonfinite values and mismatched
//! strategy dimensions with errors. They retain their inputs and share the
//! source observable. Generic strategies and adapters are Rust-only; existing
//! concrete Python/C/Go processes are unchanged.
//!
//! ```
//! use libitofin::processes::{DiscretizedProcess1D, OrnsteinUhlenbeckProcess};
//! use libitofin::shared::shared;
//! use libitofin::stochasticprocess::StochasticProcess1D;
//! # fn main() -> libitofin::errors::QlResult<()> {
//! let exact = shared(OrnsteinUhlenbeckProcess::new(0.5, 0.3, 1.0, 2.0)?);
//! let euler = DiscretizedProcess1D::new(exact.clone());
//! assert_eq!(euler.expectation(0.0, 1.0, 1.0)?, 1.5);
//! assert_ne!(exact.expectation(0.0, 1.0, 1.0)?, 1.5);
//! # Ok(())
//! # }
//! ```

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::{StochasticProcess, StochasticProcess1D};
use crate::time::date::Date;
use crate::types::{Real, Size, Time};

fn finite(value: Real) -> QlResult<Real> {
    require!(
        value.is_finite(),
        "discretization produced a nonfinite value"
    );
    Ok(value)
}

fn step(t: Time, dt: Time) -> QlResult<()> {
    require!(
        t.is_finite() && dt.is_finite() && dt >= 0.0,
        "invalid discretization time step"
    );
    Ok(())
}

fn state(value: &Array, size: Size) -> QlResult<()> {
    require!(
        value.size() == size,
        "discretization state dimension mismatch"
    );
    require!(
        value.iter().all(|x| x.is_finite()),
        "nonfinite discretization state"
    );
    Ok(())
}

fn matrix(value: Matrix, rows: Size, columns: Size) -> QlResult<Matrix> {
    require!(
        value.rows() == rows && value.columns() == columns,
        "discretization matrix dimension mismatch"
    );
    require!(
        (0..rows).all(|i| value.row(i).iter().all(|x| x.is_finite())),
        "nonfinite discretization matrix"
    );
    Ok(value)
}

/// Finite-step scalar drift, standard deviation and variance.
///
/// Implementations receive the original process and propagate coefficient errors.
pub trait ProcessDiscretization1D {
    /// Returns the drift increment over `dt`.
    fn drift(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real>;
    /// Returns the standard deviation over `dt`.
    fn diffusion(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real>;
    /// Returns the variance over `dt`.
    fn variance(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real>;
}

/// Finite-step multifactor drift, standard-deviation matrix and covariance.
///
/// Diffusion matrices have one row per state and one column per random factor.
pub trait ProcessDiscretization {
    /// Returns the drift increment over `dt`.
    fn drift(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Array>;
    /// Returns the standard-deviation matrix over `dt`.
    fn diffusion(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix>;
    /// Returns the covariance over `dt`.
    fn covariance(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix>;
}

/// Euler drift `mu * dt`, diffusion `sigma * sqrt(dt)` and covariance `sigma sigma^T * dt`.
#[derive(Clone, Copy, Debug, Default)]
pub struct EulerDiscretization;

impl ProcessDiscretization1D for EulerDiscretization {
    fn drift(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real> {
        Ok(process.drift(t, x)? * dt)
    }
    fn diffusion(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real> {
        Ok(process.diffusion(t, x)? * dt.sqrt())
    }
    fn variance(
        &self,
        process: &dyn StochasticProcess1D,
        t: Time,
        x: Real,
        dt: Time,
    ) -> QlResult<Real> {
        let sigma = process.diffusion(t, x)?;
        Ok(sigma * sigma * dt)
    }
}

impl ProcessDiscretization for EulerDiscretization {
    fn drift(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Array> {
        Ok(&process.drift(t, x)? * dt)
    }
    fn diffusion(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix> {
        Ok(&process.diffusion(t, x)? * dt.sqrt())
    }
    fn covariance(
        &self,
        process: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix> {
        let sigma = process.diffusion(t, x)?;
        Ok(&(&sigma * &sigma.transpose()) * dt)
    }
}

/// A process using an explicitly selected transition strategy.
///
/// Wrapping intentionally replaces the source's expectation, standard deviation,
/// variance/covariance and evolution overrides. Instantaneous coefficients,
/// state composition, date conversion and observable remain the source's.
pub struct DiscretizedProcess1D {
    process: Shared<dyn StochasticProcess1D>,
    discretization: Shared<dyn ProcessDiscretization1D>,
}

impl DiscretizedProcess1D {
    /// Selects Euler discretization, retaining the original process.
    pub fn new(process: Shared<dyn StochasticProcess1D>) -> Self {
        Self::with_discretization(process, shared(EulerDiscretization))
    }

    /// Selects a custom strategy, retaining both inputs.
    pub fn with_discretization(
        process: Shared<dyn StochasticProcess1D>,
        discretization: Shared<dyn ProcessDiscretization1D>,
    ) -> Self {
        Self {
            process,
            discretization,
        }
    }
}

impl AsObservable for DiscretizedProcess1D {
    fn observable(&self) -> &Observable {
        self.process.observable()
    }
}

impl StochasticProcess1D for DiscretizedProcess1D {
    fn x0(&self) -> QlResult<Real> {
        self.process.x0()
    }
    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        self.process.drift(t, x)
    }
    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        self.process.diffusion(t, x)
    }
    fn expectation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        step(t, dt)?;
        finite(x)?;
        let drift = finite(self.discretization.drift(self.process.as_ref(), t, x, dt)?)?;
        finite(self.apply(x, drift))
    }
    fn std_deviation(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        step(t, dt)?;
        finite(x)?;
        finite(
            self.discretization
                .diffusion(self.process.as_ref(), t, x, dt)?,
        )
    }
    fn variance(&self, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        step(t, dt)?;
        finite(x)?;
        let variance = finite(
            self.discretization
                .variance(self.process.as_ref(), t, x, dt)?,
        )?;
        if variance < 0.0 {
            crate::fail!("negative discretization variance");
        }
        Ok(variance)
    }
    fn evolve(&self, t: Time, x: Real, dt: Time, dw: Real) -> QlResult<Real> {
        finite(dw)?;
        let expectation = self.expectation(t, x, dt)?;
        let diffusion = finite(self.std_deviation(t, x, dt)? * dw)?;
        finite(self.apply(expectation, diffusion))
    }
    fn apply(&self, x: Real, dx: Real) -> Real {
        self.process.apply(x, dx)
    }
    fn time(&self, date: &Date) -> QlResult<Time> {
        self.process.time(date)
    }
}

/// A process using an explicitly selected transition strategy.
///
/// Wrapping intentionally replaces the source's expectation, standard deviation,
/// variance/covariance and evolution overrides. Instantaneous coefficients,
/// state composition, date conversion and observable remain the source's.
pub struct DiscretizedProcess {
    process: Shared<dyn StochasticProcess>,
    discretization: Shared<dyn ProcessDiscretization>,
}

impl DiscretizedProcess {
    /// Selects Euler discretization, retaining the original process.
    pub fn new(process: Shared<dyn StochasticProcess>) -> Self {
        Self::with_discretization(process, shared(EulerDiscretization))
    }

    /// Selects a custom strategy, retaining both inputs.
    pub fn with_discretization(
        process: Shared<dyn StochasticProcess>,
        discretization: Shared<dyn ProcessDiscretization>,
    ) -> Self {
        Self {
            process,
            discretization,
        }
    }
}

impl AsObservable for DiscretizedProcess {
    fn observable(&self) -> &Observable {
        self.process.observable()
    }
}

impl StochasticProcess for DiscretizedProcess {
    fn size(&self) -> Size {
        self.process.size()
    }
    fn factors(&self) -> Size {
        self.process.factors()
    }
    fn initial_values(&self) -> QlResult<Array> {
        self.process.initial_values()
    }
    fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        self.process.drift(t, x)
    }
    fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
        self.process.diffusion(t, x)
    }
    fn expectation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Array> {
        step(t, dt)?;
        state(x, self.size())?;
        let drift = self.discretization.drift(self.process.as_ref(), t, x, dt)?;
        state(&drift, self.size())?;
        let result = self.apply(x, &drift);
        state(&result, self.size())?;
        Ok(result)
    }
    fn std_deviation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        step(t, dt)?;
        state(x, self.size())?;
        matrix(
            self.discretization
                .diffusion(self.process.as_ref(), t, x, dt)?,
            self.size(),
            self.factors(),
        )
    }
    fn covariance(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        step(t, dt)?;
        state(x, self.size())?;
        matrix(
            self.discretization
                .covariance(self.process.as_ref(), t, x, dt)?,
            self.size(),
            self.size(),
        )
    }
    fn evolve(&self, t: Time, x: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        state(dw, self.factors())?;
        let expectation = self.expectation(t, x, dt)?;
        let increment = &self.std_deviation(t, x, dt)? * dw;
        state(&increment, self.size())?;
        let result = self.apply(&expectation, &increment);
        state(&result, self.size())?;
        Ok(result)
    }
    fn apply(&self, x: &Array, dx: &Array) -> Array {
        self.process.apply(x, dx)
    }
    fn time(&self, date: &Date) -> QlResult<Time> {
        self.process.time(date)
    }
}

#[cfg(test)]
#[path = "discretization_tests.rs"]
mod tests;
