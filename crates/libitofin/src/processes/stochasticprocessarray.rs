//! Array of correlated 1-D stochastic processes.
//!
//! Port of `ql/processes/stochasticprocessarray.{hpp,cpp}`:
//! [`StochasticProcessArray`] bundles `N`
//! [`StochasticProcess1D`](crate::stochasticprocess::StochasticProcess1D)
//! constituents under a correlation matrix and exposes them as one
//! multi-factor
//! [`StochasticProcess`](crate::stochasticprocess::StochasticProcess). The
//! constructor factors the correlation into
//! `sqrt_correlation = pseudo_sqrt(correlation, Spectral)`
//! (`ql/math/matrixutilities/pseudosqrt.cpp`); `diffusion` / `std_deviation`
//! scale row `i` of that root by the `i`-th constituent's own scalar, and
//! `evolve` correlates the independent Brownian increments through
//! `dz = sqrt_correlation . dw` before delegating to each constituent.
//!
//! Divergences from QuantLib:
//! - C++ `QL_REQUIRE(process, "null 1-D stochastic process")` guards a raw
//!   `shared_ptr`; a [`Shared`] cannot be null, so that check is a no-op here
//!   and is omitted.
//! - C++ `time(d)` delegates to `processes_[0]->time(d)`, but the Rust
//!   [`StochasticProcess1D`](crate::stochasticprocess::StochasticProcess1D)
//!   trait deliberately has no `time`. This port therefore inherits the
//!   multi-factor trait's default `time`, which fails; growing the 1D trait a
//!   `time` method just to forward it is out of scope.
//! - `covariance` is overridden to route through this type's `std_deviation`
//!   (matching C++ `stdDeviation * transpose(stdDeviation)`) rather than the
//!   trait default, which composes from `diffusion`. The two agree whenever the
//!   constituents keep the Euler `std_deviation = diffusion * sqrt(dt)`, but a
//!   constituent free to redefine `std_deviation` makes them diverge, and C++
//!   uses the per-constituent `std_deviation`.
//!
//! Deferred, visibly (tracked by #411): the pluggable discretization-strategy
//! object; only the Euler surface QuantLib installs by default is ported.

use crate::errors::QlResult;
use crate::fail;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::math::matrixutilities::pseudosqrt::{SalvagingAlgorithm, pseudo_sqrt};
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::{StochasticProcess, StochasticProcess1D};
use crate::types::{Size, Time};

/// A container of correlated 1-D stochastic processes.
pub struct StochasticProcessArray {
    processes: Vec<Shared<dyn StochasticProcess1D>>,
    sqrt_correlation: Matrix,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl StochasticProcessArray {
    /// Builds the array from its constituents and a `correlation` matrix,
    /// registering with each constituent so their notifications propagate.
    ///
    /// Fails when `processes` is empty or when `correlation` is not square with
    /// side equal to the number of processes.
    pub fn new(
        processes: Vec<Shared<dyn StochasticProcess1D>>,
        correlation: &Matrix,
    ) -> QlResult<StochasticProcessArray> {
        if processes.is_empty() {
            fail!("no processes given");
        }
        if correlation.rows() != processes.len() {
            fail!("mismatch between number of processes and size of correlation matrix");
        }
        let sqrt_correlation = pseudo_sqrt(correlation, SalvagingAlgorithm::Spectral);
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
        let observer = listener.clone() as SharedMut<dyn Observer>;
        for process in &processes {
            process.observable().register_observer(&observer);
        }
        Ok(StochasticProcessArray {
            processes,
            sqrt_correlation,
            observable,
            _listener: listener,
        })
    }

    /// The `i`-th constituent process.
    pub fn process(&self, i: Size) -> Shared<dyn StochasticProcess1D> {
        Shared::clone(&self.processes[i])
    }

    /// The correlation matrix `sqrt_correlation . sqrt_correlation^T`.
    ///
    /// For a positive-definite input this reproduces the constructor argument;
    /// the `Spectral` salvaging otherwise returns the nearest positive-semi-
    /// definite matrix.
    pub fn correlation(&self) -> Matrix {
        &self.sqrt_correlation * &self.sqrt_correlation.transpose()
    }
}

impl AsObservable for StochasticProcessArray {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess for StochasticProcessArray {
    fn size(&self) -> Size {
        self.processes.len()
    }

    fn initial_values(&self) -> QlResult<Array> {
        let mut tmp = Array::with_size(self.size());
        for (i, process) in self.processes.iter().enumerate() {
            tmp[i] = process.x0()?;
        }
        Ok(tmp)
    }

    fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        let mut tmp = Array::with_size(self.size());
        for (i, process) in self.processes.iter().enumerate() {
            tmp[i] = process.drift(t, x[i])?;
        }
        Ok(tmp)
    }

    fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
        let mut tmp = self.sqrt_correlation.clone();
        for (i, process) in self.processes.iter().enumerate() {
            let sigma = process.diffusion(t, x[i])?;
            for value in tmp.row_mut(i) {
                *value *= sigma;
            }
        }
        Ok(tmp)
    }

    fn expectation(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Array> {
        let mut tmp = Array::with_size(self.size());
        for (i, process) in self.processes.iter().enumerate() {
            tmp[i] = process.expectation(t0, x0[i], dt)?;
        }
        Ok(tmp)
    }

    fn std_deviation(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Matrix> {
        let mut tmp = self.sqrt_correlation.clone();
        for (i, process) in self.processes.iter().enumerate() {
            let sigma = process.std_deviation(t0, x0[i], dt)?;
            for value in tmp.row_mut(i) {
                *value *= sigma;
            }
        }
        Ok(tmp)
    }

    fn covariance(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Matrix> {
        let sd = self.std_deviation(t0, x0, dt)?;
        Ok(&sd * &sd.transpose())
    }

    fn evolve(&self, t0: Time, x0: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        let dz = &self.sqrt_correlation * dw;
        let mut tmp = Array::with_size(self.size());
        for (i, process) in self.processes.iter().enumerate() {
            tmp[i] = process.evolve(t0, x0[i], dt, dz[i])?;
        }
        Ok(tmp)
    }

    fn apply(&self, x0: &Array, dx: &Array) -> Array {
        let mut tmp = Array::with_size(self.size());
        for (i, process) in self.processes.iter().enumerate() {
            tmp[i] = process.apply(x0[i], dx[i]);
        }
        tmp
    }
}
