//! Stochastic processes.
//!
//! Port of the 1-D subset of `ql/stochasticprocess.{hpp,cpp}` that
//! Black-Scholes needs: [`StochasticProcess1D`] with `x0`, `drift`,
//! `diffusion`, the discretized `expectation` / `std_deviation` / `variance` /
//! `evolve` / `apply` interface and the `time` utility. The provided methods
//! hard-code the Euler scheme QuantLib installs as the default
//! `discretization` strategy (`ql/processes/eulerdiscretization.cpp`);
//! implementors override them to hard-code a better discretization, exactly
//! like the C++ virtuals. The multi-factor [`StochasticProcess`] base
//! (Array/Matrix interface) lives alongside it below; the two traits are
//! deliberately independent (C++ derives 1D from the base through private
//! bridges, a unification nothing in this crate needs). Out of scope, noted as
//! follow-up (#411): the pluggable `discretization` strategy objects and
//! `StochasticProcessArray`.
//!
//! C++ `StochasticProcess` inherits `Observer` and `Observable`, with
//! `update()` forwarding every input notification to its own observers. The
//! trait carries the [`AsObservable`] half; a concrete process embeds an
//! [`Observable`](crate::patterns::observable::Observable) and, at
//! construction, registers with each of its inputs an
//! [`Observer`](crate::patterns::observable::Observer) whose `update`
//! forwards the notification to that embedded observable. Processes inside
//! this crate reuse the crate-internal `ResetThenNotify` (via its
//! `forwarding` constructor) for that observer, as
//! [`Handle`](crate::handle::Handle) and `DeltaVolQuote` already do.

use crate::errors::QlResult;
use crate::fail;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::patterns::observable::AsObservable;
use crate::time::date::Date;
use crate::types::{Real, Size, Time};

/// 1-dimensional stochastic process `dx_t = mu(t, x_t) dt + sigma(t, x_t) dW_t`.
///
/// Mirrors QuantLib's `StochasticProcess1D`. The required methods are the
/// process-defining ones; the provided methods discretize the process over a
/// finite interval with the Euler scheme and route every state change through
/// [`apply`](StochasticProcess1D::apply), so a process redefining the state
/// composition (e.g. log-space) overrides `apply` once and `expectation` /
/// `evolve` follow.
///
/// `x0`, `drift` and `diffusion` are fallible because they typically read
/// quotes and term structures whose lookups can fail (D4: `QL_REQUIRE` maps
/// to `Err`).
pub trait StochasticProcess1D: AsObservable {
    /// Returns the initial value of the state variable.
    fn x0(&self) -> QlResult<Real>;

    /// Returns the drift part of the equation, i.e. `mu(t, x_t)`.
    fn drift(&self, t: Time, x: Real) -> QlResult<Real>;

    /// Returns the diffusion part of the equation, i.e. `sigma(t, x_t)`.
    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real>;

    /// Returns the expectation `E(x_{t0 + dt} | x_{t0} = x0)` of the process
    /// after a time interval `dt`.
    fn expectation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        Ok(self.apply(x0, self.drift(t0, x0)? * dt))
    }

    /// Returns the standard deviation `S(x_{t0 + dt} | x_{t0} = x0)` of the
    /// process after a time interval `dt`.
    fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        Ok(self.diffusion(t0, x0)? * dt.sqrt())
    }

    /// Returns the variance `V(x_{t0 + dt} | x_{t0} = x0)` of the process
    /// after a time interval `dt`.
    fn variance(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        let sigma = self.diffusion(t0, x0)?;
        Ok(sigma * sigma * dt)
    }

    /// Returns the asset value after a time interval `dt`, by default
    /// `E(x0, t0, dt) + S(x0, t0, dt) * dw` composed through
    /// [`apply`](StochasticProcess1D::apply).
    fn evolve(&self, t0: Time, x0: Real, dt: Time, dw: Real) -> QlResult<Real> {
        Ok(self.apply(
            self.expectation(t0, x0, dt)?,
            self.std_deviation(t0, x0, dt)? * dw,
        ))
    }

    /// Applies a change to the asset value, by default `x0 + dx`.
    fn apply(&self, x0: Real, dx: Real) -> Real {
        x0 + dx
    }

    /// Returns the time value corresponding to the given date in the
    /// reference system of the process.
    ///
    /// As in C++, processes that do not need this functionality inherit the
    /// default, which fails (`QL_FAIL` maps to `Err`).
    fn time(&self, _date: &Date) -> QlResult<Time> {
        fail!("date/time conversion not supported");
    }
}

/// Multi-dimensional stochastic process
/// `dx_t = mu(t, x_t) dt + sigma(t, x_t) . dW_t`.
///
/// Mirrors QuantLib's multi-factor `StochasticProcess` base, the sibling of
/// [`StochasticProcess1D`] above. The required methods are the
/// process-defining ones; the provided methods discretize the process over a
/// finite interval with the Euler scheme
/// (`ql/processes/eulerdiscretization.cpp`) and route every state change
/// through [`apply`](StochasticProcess::apply), so a process redefining the
/// state composition (e.g. log-space) overrides `apply` once and `expectation`
/// / `evolve` follow.
///
/// `initial_values`, `drift` and `diffusion` are fallible because they
/// typically read quotes and term structures whose lookups can fail (D4:
/// `QL_REQUIRE` maps to `Err`).
///
/// Deferred (#411): the pluggable `discretization` strategy object and
/// `StochasticProcessArray`.
pub trait StochasticProcess: AsObservable {
    /// Returns the number of dimensions of the process.
    fn size(&self) -> Size;

    /// Returns the number of independent factors of the process. Defaults to
    /// [`size`](StochasticProcess::size).
    fn factors(&self) -> Size {
        self.size()
    }

    /// Returns the initial values of the state variables.
    fn initial_values(&self) -> QlResult<Array>;

    /// Returns the drift part of the equation, i.e. `mu(t, x_t)`.
    fn drift(&self, t: Time, x: &Array) -> QlResult<Array>;

    /// Returns the diffusion part of the equation, i.e. `sigma(t, x_t)`.
    fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix>;

    /// Returns the expectation `E(x_{t0 + dt} | x_{t0} = x0)` of the process
    /// after a time interval `dt`.
    fn expectation(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Array> {
        let drift = self.drift(t0, x0)?;
        Ok(self.apply(x0, &(&drift * dt)))
    }

    /// Returns the standard deviation `S(x_{t0 + dt} | x_{t0} = x0)` of the
    /// process after a time interval `dt`.
    fn std_deviation(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Matrix> {
        Ok(&self.diffusion(t0, x0)? * dt.sqrt())
    }

    /// Returns the covariance `V(x_{t0 + dt} | x_{t0} = x0)` of the process
    /// after a time interval `dt`, i.e. `sigma . sigma^T . dt`.
    fn covariance(&self, t0: Time, x0: &Array, dt: Time) -> QlResult<Matrix> {
        let sigma = self.diffusion(t0, x0)?;
        let sigma_t = sigma.transpose();
        Ok(&(&sigma * &sigma_t) * dt)
    }

    /// Returns the asset values after a time interval `dt`, by default
    /// `E(x0, t0, dt) + S(x0, t0, dt) . dw` composed through
    /// [`apply`](StochasticProcess::apply).
    fn evolve(&self, t0: Time, x0: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        let expectation = self.expectation(t0, x0, dt)?;
        let std_deviation = self.std_deviation(t0, x0, dt)?;
        Ok(self.apply(&expectation, &(&std_deviation * dw)))
    }

    /// Applies a change to the asset values, by default `x0 + dx` element-wise.
    fn apply(&self, x0: &Array, dx: &Array) -> Array {
        x0 + dx
    }

    /// Returns the time value corresponding to the given date in the reference
    /// system of the process.
    ///
    /// As in C++, processes that do not need this functionality inherit the
    /// default, which fails (`QL_FAIL` maps to `Err`).
    fn time(&self, _date: &Date) -> QlResult<Time> {
        fail!("date/time conversion not supported");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::{Shared, SharedMut, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;

    struct ConstantProcess {
        mu: Real,
        sigma: Real,
        quote: Shared<SimpleQuote>,
        observable: Shared<Observable>,
        _listener: SharedMut<ResetThenNotify>,
    }

    impl ConstantProcess {
        fn new(initial: Real, mu: Real, sigma: Real) -> Self {
            let quote = shared(SimpleQuote::new(initial));
            let observable = shared(Observable::new());
            let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
            quote
                .observable()
                .register_observer(&(listener.clone() as SharedMut<dyn Observer>));
            ConstantProcess {
                mu,
                sigma,
                quote,
                observable,
                _listener: listener,
            }
        }
    }

    impl AsObservable for ConstantProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess1D for ConstantProcess {
        fn x0(&self) -> QlResult<Real> {
            self.quote.value()
        }

        fn drift(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(self.mu)
        }

        fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(self.sigma)
        }
    }

    struct LogProcess {
        inner: ConstantProcess,
    }

    impl AsObservable for LogProcess {
        fn observable(&self) -> &Observable {
            self.inner.observable()
        }
    }

    impl StochasticProcess1D for LogProcess {
        fn x0(&self) -> QlResult<Real> {
            self.inner.x0()
        }

        fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
            self.inner.drift(t, x)
        }

        fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
            self.inner.diffusion(t, x)
        }

        fn apply(&self, x0: Real, dx: Real) -> Real {
            x0 * dx.exp()
        }
    }

    #[test]
    fn defaults_follow_the_euler_discretization() {
        let process = ConstantProcess::new(100.0, 0.05, 0.20);
        let (t0, x0, dt, dw): (Time, Real, Time, Real) = (1.0, 100.0, 0.25, 0.5);

        assert_eq!(process.x0().unwrap(), 100.0);
        assert_eq!(process.expectation(t0, x0, dt).unwrap(), x0 + 0.05 * dt);
        assert_eq!(process.std_deviation(t0, x0, dt).unwrap(), 0.20 * dt.sqrt());
        assert_eq!(process.variance(t0, x0, dt).unwrap(), 0.20 * 0.20 * dt);
        assert_eq!(
            process.evolve(t0, x0, dt, dw).unwrap(),
            (x0 + 0.05 * dt) + 0.20 * dt.sqrt() * dw
        );
        assert_eq!(process.apply(x0, 2.5), x0 + 2.5);
    }

    #[test]
    fn expectation_and_evolve_route_through_apply() {
        let process = LogProcess {
            inner: ConstantProcess::new(100.0, 0.05, 0.20),
        };
        let (t0, x0, dt, dw): (Time, Real, Time, Real) = (0.0, 100.0, 0.25, -1.0);

        let expectation = x0 * (0.05 * dt).exp();
        assert_eq!(process.expectation(t0, x0, dt).unwrap(), expectation);
        assert_eq!(
            process.evolve(t0, x0, dt, dw).unwrap(),
            expectation * (0.20 * dt.sqrt() * dw).exp()
        );
    }

    #[test]
    fn time_defaults_to_an_error() {
        let process = ConstantProcess::new(100.0, 0.05, 0.20);
        let date = Date::new(15, Month::May, 2026);
        let err = process.time(&date).unwrap_err();
        assert_eq!(err.message(), "date/time conversion not supported");
    }

    #[test]
    fn input_notifications_are_forwarded_to_process_observers() {
        let process = ConstantProcess::new(100.0, 0.05, 0.20);
        let flag = Flag::new();
        process.observable().register_observer(&as_observer(&flag));

        process.quote.set_value(101.0);

        assert!(
            Flag::is_up(&flag),
            "process observers were not notified of an input change"
        );
    }

    #[test]
    fn trait_is_object_safe() {
        let process = ConstantProcess::new(100.0, 0.05, 0.20);
        let dynamic: &dyn StochasticProcess1D = &process;
        assert_eq!(dynamic.x0().unwrap(), 100.0);
    }
}

#[cfg(test)]
mod multifactor_tests {
    use super::*;
    use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::{Shared, SharedMut, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;

    const TOL: Real = 1e-12;

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

    struct TwoFactorProcess {
        initial: Array,
        mu: Array,
        sigma: Matrix,
        quote: Shared<SimpleQuote>,
        observable: Shared<Observable>,
        _listener: SharedMut<ResetThenNotify>,
    }

    impl TwoFactorProcess {
        fn new() -> Self {
            let quote = shared(SimpleQuote::new(100.0));
            let observable = shared(Observable::new());
            let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
            quote
                .observable()
                .register_observer(&(listener.clone() as SharedMut<dyn Observer>));
            TwoFactorProcess {
                initial: Array::from([100.0, 50.0]),
                mu: Array::from([0.05, -0.03]),
                sigma: Matrix::from([[0.20, 0.0], [0.10, 0.15]]),
                quote,
                observable,
                _listener: listener,
            }
        }
    }

    impl AsObservable for TwoFactorProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess for TwoFactorProcess {
        fn size(&self) -> Size {
            2
        }

        fn initial_values(&self) -> QlResult<Array> {
            Ok(Array::from([self.quote.value()?, self.initial[1]]))
        }

        fn drift(&self, _t: Time, _x: &Array) -> QlResult<Array> {
            Ok(self.mu.clone())
        }

        fn diffusion(&self, _t: Time, _x: &Array) -> QlResult<Matrix> {
            Ok(self.sigma.clone())
        }
    }

    struct LogFirstProcess {
        inner: TwoFactorProcess,
    }

    impl AsObservable for LogFirstProcess {
        fn observable(&self) -> &Observable {
            self.inner.observable()
        }
    }

    impl StochasticProcess for LogFirstProcess {
        fn size(&self) -> Size {
            self.inner.size()
        }

        fn initial_values(&self) -> QlResult<Array> {
            self.inner.initial_values()
        }

        fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
            self.inner.drift(t, x)
        }

        fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
            self.inner.diffusion(t, x)
        }

        fn apply(&self, x0: &Array, dx: &Array) -> Array {
            let mut out = x0 + dx;
            out[0] = x0[0] * dx[0].exp();
            out
        }
    }

    #[test]
    fn dimensions_and_initial_values() {
        let p = TwoFactorProcess::new();
        assert_eq!(p.size(), 2);
        assert_eq!(p.factors(), 2);
        assert_array_close(&p.initial_values().unwrap(), &[100.0, 50.0]);
    }

    #[test]
    fn drift_and_diffusion_shapes_and_values() {
        let p = TwoFactorProcess::new();
        let x = Array::from([100.0, 50.0]);
        assert_array_close(&p.drift(1.0, &x).unwrap(), &[0.05, -0.03]);
        assert_matrix_close(
            &p.diffusion(1.0, &x).unwrap(),
            &[&[0.20, 0.0], &[0.10, 0.15]],
        );
    }

    #[test]
    fn euler_defaults_are_hand_computable() {
        let p = TwoFactorProcess::new();
        let x0 = Array::from([100.0, 50.0]);
        let dt: Time = 0.25;

        assert_array_close(&p.expectation(0.0, &x0, dt).unwrap(), &[100.0125, 49.9925]);
        assert_matrix_close(
            &p.std_deviation(0.0, &x0, dt).unwrap(),
            &[&[0.10, 0.0], &[0.05, 0.075]],
        );
        assert_matrix_close(
            &p.covariance(0.0, &x0, dt).unwrap(),
            &[&[0.01, 0.005], &[0.005, 0.008125]],
        );

        let dw = Array::from([0.5, -1.0]);
        assert_array_close(&p.evolve(0.0, &x0, dt, &dw).unwrap(), &[100.0625, 49.9425]);
    }

    #[test]
    fn apply_default_is_elementwise_sum() {
        let p = TwoFactorProcess::new();
        let x0 = Array::from([1.0, 2.0]);
        let dx = Array::from([10.0, 20.0]);
        assert_array_close(&p.apply(&x0, &dx), &[11.0, 22.0]);
    }

    #[test]
    fn expectation_and_evolve_route_through_apply() {
        let p = LogFirstProcess {
            inner: TwoFactorProcess::new(),
        };
        let x0 = Array::from([100.0, 50.0]);
        let dt: Time = 0.25;

        let e0 = 100.0 * (0.05 * dt).exp();
        assert_array_close(&p.expectation(0.0, &x0, dt).unwrap(), &[e0, 49.9925]);

        let dw = Array::from([0.5, -1.0]);
        assert_array_close(
            &p.evolve(0.0, &x0, dt, &dw).unwrap(),
            &[e0 * 0.05_f64.exp(), 49.9425],
        );
    }

    #[test]
    fn time_defaults_to_an_error() {
        let p = TwoFactorProcess::new();
        let date = Date::new(15, Month::May, 2026);
        let err = p.time(&date).unwrap_err();
        assert_eq!(err.message(), "date/time conversion not supported");
    }

    #[test]
    fn input_notifications_are_forwarded_to_process_observers() {
        let p = TwoFactorProcess::new();
        let flag = Flag::new();
        p.observable().register_observer(&as_observer(&flag));

        p.quote.set_value(101.0);

        assert!(
            Flag::is_up(&flag),
            "process observers were not notified of an input change"
        );
    }

    #[test]
    fn trait_is_object_safe() {
        let p = TwoFactorProcess::new();
        let dynamic: &dyn StochasticProcess = &p;
        assert_eq!(dynamic.size(), 2);
    }
}
