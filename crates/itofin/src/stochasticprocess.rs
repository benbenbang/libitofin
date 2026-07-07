//! Stochastic processes.
//!
//! Port of the 1-D subset of `ql/stochasticprocess.{hpp,cpp}` that
//! Black-Scholes needs: [`StochasticProcess1D`] with `x0`, `drift`,
//! `diffusion`, the discretized `expectation` / `std_deviation` / `variance` /
//! `evolve` / `apply` interface and the `time` utility. The provided methods
//! hard-code the Euler scheme QuantLib installs as the default
//! `discretization` strategy (`ql/processes/eulerdiscretization.cpp`);
//! implementors override them to hard-code a better discretization, exactly
//! like the C++ virtuals. Out of scope, noted as follow-up: the multi-asset
//! `StochasticProcess` base (Array/Matrix interface) and the pluggable
//! `discretization` strategy objects.
//!
//! C++ `StochasticProcess` inherits `Observer` and `Observable`, with
//! `update()` forwarding every input notification to its own observers. The
//! trait carries the [`AsObservable`] half; a concrete process embeds an
//! [`Observable`](crate::patterns::observable::Observable) and, at
//! construction, registers with each of its inputs an
//! [`Observer`](crate::patterns::observable::Observer) whose `update`
//! forwards the notification to that embedded observable. Processes inside
//! this crate reuse the crate-internal `Forwarder` for that observer, as
//! [`Handle`](crate::handle::Handle) and `DeltaVolQuote` already do.

use crate::errors::QlResult;
use crate::fail;
use crate::patterns::observable::AsObservable;
use crate::time::date::Date;
use crate::types::{Real, Time};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{Forwarder, Observable, Observer};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;

    struct ConstantProcess {
        mu: Real,
        sigma: Real,
        quote: Shared<SimpleQuote>,
        observable: Shared<Observable>,
        _listener: SharedMut<Forwarder>,
    }

    impl ConstantProcess {
        fn new(initial: Real, mu: Real, sigma: Real) -> Self {
            let quote = shared(SimpleQuote::new(initial));
            let observable = shared(Observable::new());
            let listener = shared_mut(Forwarder {
                observable: Shared::clone(&observable),
            });
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
