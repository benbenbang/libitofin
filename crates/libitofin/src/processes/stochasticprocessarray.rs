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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::{Date, Month};
    use crate::types::Real;

    const TOL: Real = 1e-12;

    /// A configurable 1-D constituent. `diffusion` returns `sigma`; the
    /// optional `std_dev_override` and `evolve_sentinel` let a test drive
    /// `std_deviation` and `evolve` off their Euler defaults so a test can tell
    /// delegation from inheritance. `log_apply` switches `apply` to `x0 exp(dx)`.
    struct Stub1D {
        mu: Real,
        sigma: Real,
        std_dev_override: Option<Real>,
        evolve_sentinel: Option<Real>,
        log_apply: bool,
        quote: Shared<SimpleQuote>,
        observable: Shared<Observable>,
        _listener: SharedMut<ResetThenNotify>,
    }

    impl Stub1D {
        fn new(initial: Real, mu: Real, sigma: Real) -> Self {
            let quote = shared(SimpleQuote::new(initial));
            let observable = shared(Observable::new());
            let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
            quote
                .observable()
                .register_observer(&(listener.clone() as SharedMut<dyn Observer>));
            Stub1D {
                mu,
                sigma,
                std_dev_override: None,
                evolve_sentinel: None,
                log_apply: false,
                quote,
                observable,
                _listener: listener,
            }
        }

        fn with_std_dev(mut self, value: Real) -> Self {
            self.std_dev_override = Some(value);
            self
        }

        fn with_evolve(mut self, value: Real) -> Self {
            self.evolve_sentinel = Some(value);
            self
        }

        fn log_apply(mut self) -> Self {
            self.log_apply = true;
            self
        }
    }

    impl AsObservable for Stub1D {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess1D for Stub1D {
        fn x0(&self) -> QlResult<Real> {
            self.quote.value()
        }

        fn drift(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(self.mu)
        }

        fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(self.sigma)
        }

        fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
            match self.std_dev_override {
                Some(value) => Ok(value),
                None => Ok(self.diffusion(t0, x0)? * dt.sqrt()),
            }
        }

        fn evolve(&self, t0: Time, x0: Real, dt: Time, dw: Real) -> QlResult<Real> {
            match self.evolve_sentinel {
                Some(value) => Ok(value),
                None => Ok(self.apply(
                    self.expectation(t0, x0, dt)?,
                    self.std_deviation(t0, x0, dt)? * dw,
                )),
            }
        }

        fn apply(&self, x0: Real, dx: Real) -> Real {
            if self.log_apply {
                x0 * dx.exp()
            } else {
                x0 + dx
            }
        }
    }

    fn corr_2x2() -> Matrix {
        Matrix::from([[1.0, 0.5], [0.5, 1.0]])
    }

    fn root_2x2() -> Matrix {
        pseudo_sqrt(&corr_2x2(), SalvagingAlgorithm::Spectral)
    }

    fn array_of(stubs: Vec<Stub1D>) -> StochasticProcessArray {
        let processes = stubs
            .into_iter()
            .map(|s| shared(s) as Shared<dyn StochasticProcess1D>)
            .collect();
        StochasticProcessArray::new(processes, &corr_2x2()).unwrap()
    }

    fn assert_close(actual: Real, expected: Real) {
        assert!((actual - expected).abs() < TOL, "{actual} != {expected}");
    }

    fn assert_array_close(actual: &Array, expected: &[Real]) {
        assert_eq!(actual.size(), expected.len(), "array size mismatch");
        for (i, e) in expected.iter().enumerate() {
            assert!(
                (actual[i] - e).abs() < TOL,
                "element {i}: {} != {e}",
                actual[i]
            );
        }
    }

    fn assert_matrix_close(actual: &Matrix, expected: &[&[Real]]) {
        assert_eq!(actual.rows(), expected.len(), "row count mismatch");
        for (i, row) in expected.iter().enumerate() {
            assert_eq!(actual.columns(), row.len(), "column count mismatch");
            for (j, e) in row.iter().enumerate() {
                assert!(
                    (actual[(i, j)] - e).abs() < TOL,
                    "element ({i},{j}): {} != {e}",
                    actual[(i, j)]
                );
            }
        }
    }

    #[test]
    fn assembles_size_initial_values_drift_expectation() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.05, 0.20),
            Stub1D::new(50.0, -0.03, 0.10),
        ]);
        assert_eq!(a.size(), 2);
        assert_eq!(a.factors(), 2);
        assert_array_close(&a.initial_values().unwrap(), &[100.0, 50.0]);

        let x = Array::from([100.0, 50.0]);
        assert_array_close(&a.drift(1.0, &x).unwrap(), &[0.05, -0.03]);

        let dt: Time = 0.25;
        assert_array_close(
            &a.expectation(0.0, &x, dt).unwrap(),
            &[100.0 + 0.05 * dt, 50.0 - 0.03 * dt],
        );
    }

    #[test]
    fn diffusion_scales_root_correlation_rows_by_sigma() {
        let (sig0, sig1) = (0.20, 0.10);
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, sig0),
            Stub1D::new(50.0, 0.0, sig1),
        ]);
        let root = root_2x2();
        let d = a.diffusion(1.0, &Array::from([100.0, 50.0])).unwrap();
        assert_matrix_close(
            &d,
            &[
                &[root[(0, 0)] * sig0, root[(0, 1)] * sig0],
                &[root[(1, 0)] * sig1, root[(1, 1)] * sig1],
            ],
        );
    }

    #[test]
    fn correlation_round_trips_positive_definite_input() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let c = a.correlation();
        for (i, j, e) in [(0, 0, 1.0), (0, 1, 0.5), (1, 0, 0.5), (1, 1, 1.0)] {
            assert!(
                (c[(i, j)] - e).abs() < 1e-14,
                "correlation({i},{j}) = {} != {e}",
                c[(i, j)]
            );
        }
    }

    #[test]
    fn evolve_correlates_increments_through_sqrt_correlation() {
        let (mu0, sig0, mu1, sig1) = (0.05, 0.20, -0.03, 0.10);
        let a = array_of(vec![
            Stub1D::new(100.0, mu0, sig0),
            Stub1D::new(50.0, mu1, sig1),
        ]);
        let root = root_2x2();
        let x0 = Array::from([100.0, 50.0]);
        let dt: Time = 0.25;
        let dw = Array::from([1.0, 0.0]);

        let dz0 = root[(0, 0)] * 1.0 + root[(0, 1)] * 0.0;
        let dz1 = root[(1, 0)] * 1.0 + root[(1, 1)] * 0.0;
        assert!(
            dz1.abs() > 1e-9,
            "off-diagonal correlation must mix dw[0] into component 1"
        );

        let e0 = (100.0 + mu0 * dt) + sig0 * dt.sqrt() * dz0;
        let e1 = (50.0 + mu1 * dt) + sig1 * dt.sqrt() * dz1;
        assert_array_close(&a.evolve(0.0, &x0, dt, &dw).unwrap(), &[e0, e1]);
    }

    #[test]
    fn apply_delegates_to_constituent_apply() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20).log_apply(),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let out = a.apply(&Array::from([100.0, 50.0]), &Array::from([0.1, 0.2]));
        assert_close(out[0], 100.0 * 0.1_f64.exp());
        assert_close(out[1], 50.2);
        assert!(
            (out[0] - 100.1).abs() > 1e-6,
            "array.apply must delegate, not fall back to x0 + dx"
        );
    }

    /// Confirm-by-stubbing the `covariance` override: constituent 0's
    /// `std_deviation` (0.5) is deliberately unequal to its Euler value
    /// `diffusion * sqrt(dt)` (0.2 * 0.5 = 0.1). The trait default `covariance`
    /// composes from `diffusion` and would use 0.1; this port routes through
    /// `std_deviation` and uses 0.5, so the two disagree measurably.
    #[test]
    fn covariance_routes_through_std_deviation_not_diffusion() {
        let (sd0, diff0, sig1) = (0.5, 0.2, 0.10);
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, diff0).with_std_dev(sd0),
            Stub1D::new(50.0, 0.0, sig1),
        ]);
        let root = root_2x2();
        let dt: Time = 0.25;
        let sd1 = sig1 * dt.sqrt();

        let sd = [
            [root[(0, 0)] * sd0, root[(0, 1)] * sd0],
            [root[(1, 0)] * sd1, root[(1, 1)] * sd1],
        ];
        let cov00 = sd[0][0] * sd[0][0] + sd[0][1] * sd[0][1];
        let cov01 = sd[0][0] * sd[1][0] + sd[0][1] * sd[1][1];
        let cov11 = sd[1][0] * sd[1][0] + sd[1][1] * sd[1][1];

        let c = a.covariance(0.0, &Array::from([100.0, 50.0]), dt).unwrap();
        assert_matrix_close(&c, &[&[cov00, cov01], &[cov01, cov11]]);

        let default_sd0 = diff0 * dt.sqrt();
        let default_cov00 =
            (root[(0, 0)] * default_sd0).powi(2) + (root[(0, 1)] * default_sd0).powi(2);
        assert!(
            (cov00 - default_cov00).abs() > 1e-6,
            "covariance must use the constituent std_deviation, not diffusion * sqrt(dt)"
        );
    }

    /// Confirm-by-stubbing that `evolve` delegates to each constituent's own
    /// `evolve`: constituent 0 returns a sentinel that the multi-factor Euler
    /// default could never compose.
    #[test]
    fn evolve_delegates_to_constituent_evolve() {
        let sentinel = 12_345.0;
        let a = array_of(vec![
            Stub1D::new(100.0, 0.05, 0.20).with_evolve(sentinel),
            Stub1D::new(50.0, -0.03, 0.10),
        ]);
        let out = a
            .evolve(
                0.0,
                &Array::from([100.0, 50.0]),
                0.25,
                &Array::from([1.0, 0.0]),
            )
            .unwrap();
        assert_close(out[0], sentinel);
    }

    #[test]
    fn changing_one_constituent_diffusion_scales_only_its_row() {
        let x = Array::from([100.0, 50.0]);
        let base = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let bumped = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.40),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let d_base = base.diffusion(1.0, &x).unwrap();
        let d_bump = bumped.diffusion(1.0, &x).unwrap();

        assert_close(d_bump[(1, 0)], d_base[(1, 0)]);
        assert_close(d_bump[(1, 1)], d_base[(1, 1)]);
        assert_close(d_bump[(0, 0)], 2.0 * d_base[(0, 0)]);
        assert_close(d_bump[(0, 1)], 2.0 * d_base[(0, 1)]);
    }

    #[test]
    fn ctor_errors_on_empty() {
        let err = StochasticProcessArray::new(vec![], &corr_2x2())
            .err()
            .unwrap();
        assert_eq!(err.message(), "no processes given");
    }

    #[test]
    fn ctor_errors_on_correlation_size_mismatch() {
        let processes = vec![
            shared(Stub1D::new(100.0, 0.0, 0.20)) as Shared<dyn StochasticProcess1D>,
            shared(Stub1D::new(50.0, 0.0, 0.10)) as Shared<dyn StochasticProcess1D>,
        ];
        let corr3 = Matrix::from([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let err = StochasticProcessArray::new(processes, &corr3)
            .err()
            .unwrap();
        assert_eq!(
            err.message(),
            "mismatch between number of processes and size of correlation matrix"
        );
    }

    #[test]
    fn input_notifications_are_forwarded_to_array_observers() {
        let stub = Stub1D::new(100.0, 0.05, 0.20);
        let quote = Shared::clone(&stub.quote);
        let processes = vec![
            shared(stub) as Shared<dyn StochasticProcess1D>,
            shared(Stub1D::new(50.0, -0.03, 0.10)) as Shared<dyn StochasticProcess1D>,
        ];
        let a = StochasticProcessArray::new(processes, &corr_2x2()).unwrap();

        let flag = Flag::new();
        a.observable().register_observer(&as_observer(&flag));
        quote.set_value(101.0);

        assert!(
            Flag::is_up(&flag),
            "a constituent input change must notify array observers"
        );
    }

    #[test]
    fn process_accessor_returns_constituent() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        assert_close(a.process(0).x0().unwrap(), 100.0);
        assert_close(a.process(1).x0().unwrap(), 50.0);
    }

    #[test]
    fn time_inherits_the_multifactor_default_error() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let err = a.time(&Date::new(15, Month::May, 2026)).unwrap_err();
        assert_eq!(err.message(), "date/time conversion not supported");
    }

    #[test]
    fn trait_is_object_safe() {
        let a = array_of(vec![
            Stub1D::new(100.0, 0.0, 0.20),
            Stub1D::new(50.0, 0.0, 0.10),
        ]);
        let dynamic: &dyn StochasticProcess = &a;
        assert_eq!(dynamic.size(), 2);
    }
}
