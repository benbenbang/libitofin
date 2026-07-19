//! Heston square-root stochastic-volatility process.
//!
//! Port of the analytic surface of `ql/processes/hestonprocess.{hpp,cpp}`:
//! [`HestonProcess`] describes the 2-factor square-root stochastic-volatility
//! model
//!
//! ```text
//! dS = mu S dt + sqrt(v) S dW_1
//! dv = kappa (theta - v) dt + sigma sqrt(v) dW_2
//! dW_1 dW_2 = rho dt
//! ```
//!
//! implementing the multi-factor
//! [`StochasticProcess`](crate::stochasticprocess::StochasticProcess) trait.
//! Only the analytic surface - `drift`, `diffusion`, `apply`,
//! `initial_values`, plus the getters and curves - is ported here; it is
//! everything the (next batch) `AnalyticHestonEngine` reads, and that engine
//! never evolves the process.
//!
//! Deferred, visibly (tracked by #410):
//! - The C++ `Discretization` enum (PartialTruncation / Reflection /
//!   QuadraticExponential* / BroadieKaya*) is omitted entirely. Only the
//!   default path is ported: `drift`/`diffusion` hard-code the branch used by
//!   every scheme except Reflection and PartialTruncation, so the analytic
//!   surface is unambiguous without the enum.
//! - The Monte Carlo exact-sampling surface (`evolve`'s QE/BroadieKaya
//!   scheme, `pdf`, `varianceDistribution`, `Phi`) needs the modified Bessel
//!   function and complex exact sampling. QuantLib overrides `evolve`
//!   (`hestonprocess.cpp:396`) with that scheme, so the [`StochasticProcess`]
//!   base's Euler `evolve` would return numbers QuantLib's `HestonProcess`
//!   never produces. This port therefore overrides `evolve` to fail rather
//!   than silently diverge. The base `expectation` / `std_deviation` /
//!   `covariance` are kept: QuantLib's own ctor installs `EulerDiscretization`
//!   (`hestonprocess.cpp:46`), so those three genuinely are the Euler defaults.

use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Size, Time};

/// Square-root stochastic-volatility Heston process (analytic surface).
pub struct HestonProcess {
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    s0: Handle<dyn Quote>,
    v0: Real,
    kappa: Real,
    theta: Real,
    sigma: Real,
    rho: Real,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl HestonProcess {
    /// Builds a Heston process registered with its three market inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        s0: Handle<dyn Quote>,
        v0: Real,
        kappa: Real,
        theta: Real,
        sigma: Real,
        rho: Real,
    ) -> HestonProcess {
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
        let observer = listener.clone() as SharedMut<dyn Observer>;
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);
        s0.register_observer(&observer);
        HestonProcess {
            risk_free_rate,
            dividend_yield,
            s0,
            v0,
            kappa,
            theta,
            sigma,
            rho,
            observable,
            _listener: listener,
        }
    }

    /// The initial variance `v0`.
    pub fn v0(&self) -> Real {
        self.v0
    }

    /// The mean-reversion speed `kappa`.
    pub fn kappa(&self) -> Real {
        self.kappa
    }

    /// The long-run variance `theta`.
    pub fn theta(&self) -> Real {
        self.theta
    }

    /// The volatility of variance `sigma`.
    pub fn sigma(&self) -> Real {
        self.sigma
    }

    /// The spot/variance correlation `rho`.
    pub fn rho(&self) -> Real {
        self.rho
    }

    /// The spot quote handle.
    pub fn s0(&self) -> Handle<dyn Quote> {
        self.s0.clone()
    }

    /// The dividend-yield curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.dividend_yield.clone()
    }

    /// The risk-free-rate curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.risk_free_rate.clone()
    }

    /// The instantaneous forward `forwardRate(t, t, Continuous)` of a curve.
    fn instantaneous_forward(
        &self,
        curve: &Handle<dyn YieldTermStructure>,
        t: Time,
    ) -> QlResult<Rate> {
        Ok(curve
            .current_link()?
            .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)?
            .rate())
    }
}

impl AsObservable for HestonProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess for HestonProcess {
    fn size(&self) -> Size {
        2
    }

    fn factors(&self) -> Size {
        2
    }

    fn initial_values(&self) -> QlResult<Array> {
        Ok(Array::from([self.s0.current_link()?.value()?, self.v0]))
    }

    fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        let vol = if x[1] > 0.0 { x[1].sqrt() } else { 0.0 };
        let r = self.instantaneous_forward(&self.risk_free_rate, t)?;
        let q = self.instantaneous_forward(&self.dividend_yield, t)?;
        Ok(Array::from([
            r - q - 0.5 * vol * vol,
            self.kappa * (self.theta - vol * vol),
        ]))
    }

    fn diffusion(&self, _t: Time, x: &Array) -> QlResult<Matrix> {
        let vol = if x[1] > 0.0 { x[1].sqrt() } else { 1e-8 };
        let sigma2 = self.sigma * vol;
        let sqrhov = (1.0 - self.rho * self.rho).sqrt();
        Ok(Matrix::from([
            [vol, 0.0],
            [self.rho * sigma2, sqrhov * sigma2],
        ]))
    }

    fn apply(&self, x0: &Array, dx: &Array) -> Array {
        Array::from([x0[0] * dx[0].exp(), x0[1] + dx[1]])
    }

    fn evolve(&self, _t0: Time, _x0: &Array, _dt: Time, _dw: &Array) -> QlResult<Array> {
        fail!(
            "HestonProcess::evolve requires the QE/BroadieKaya exact-sampling scheme, deferred to \
             the Monte Carlo batch (#410); only the analytic surface (drift, diffusion, apply, \
             initial_values) is ported"
        );
    }

    fn time(&self, date: &Date) -> QlResult<Time> {
        self.risk_free_rate
            .current_link()?
            .time_from_reference(*date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::{SimpleQuote, make_quote_handle};
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    const S0: Real = 100.0;
    const V0: Real = 0.04;
    const KAPPA: Real = 1.2;
    const THETA: Real = 0.06;
    const SIGMA: Real = 0.3;
    const RHO: Real = -0.5;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const TOL: Real = 1e-12;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn make_process() -> HestonProcess {
        HestonProcess::new(
            flat_yield(R),
            flat_yield(Q),
            make_quote_handle(S0).handle(),
            V0,
            KAPPA,
            THETA,
            SIGMA,
            RHO,
        )
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
    fn dimensions_and_getters() {
        let p = make_process();
        assert_eq!(p.size(), 2);
        assert_eq!(p.factors(), 2);
        assert_eq!(p.v0(), V0);
        assert_eq!(p.kappa(), KAPPA);
        assert_eq!(p.theta(), THETA);
        assert_eq!(p.sigma(), SIGMA);
        assert_eq!(p.rho(), RHO);
        assert_eq!(p.s0().current_link().unwrap().value().unwrap(), S0);
    }

    #[test]
    fn initial_values_are_spot_and_v0() {
        let p = make_process();
        assert_array_close(&p.initial_values().unwrap(), &[S0, V0]);
    }

    /// The flat curve fixes every forward at the flat rate, so this pins the
    /// `-0.5 vol^2` term and the `kappa (theta - vol^2)` term and the vol
    /// branch, but NOT drift's `r - q` time-dependence (that is pinned later by
    /// `testAnalyticVsCached` at the AnalyticHestonEngine batch).
    #[test]
    fn drift_positive_variance_branch() {
        let p = make_process();
        let vol = V0.sqrt();
        let x = Array::from([S0, V0]);
        let d = p.drift(0.5, &x).unwrap();
        assert_close(d[0], R - Q - 0.5 * vol * vol);
        assert_close(d[1], KAPPA * (THETA - vol * vol));
    }

    #[test]
    fn drift_nonpositive_variance_uses_zero_vol() {
        let p = make_process();
        let x = Array::from([S0, -0.01]);
        let d = p.drift(0.5, &x).unwrap();
        assert_close(d[0], R - Q);
        assert_close(d[1], KAPPA * THETA);
    }

    #[test]
    fn diffusion_is_the_root_correlation_matrix() {
        let p = make_process();
        let vol = V0.sqrt();
        let sigma2 = SIGMA * vol;
        let sqrhov = (1.0 - RHO * RHO).sqrt();
        let m = p.diffusion(0.5, &Array::from([S0, V0])).unwrap();
        assert_matrix_close(&m, &[&[vol, 0.0], &[RHO * sigma2, sqrhov * sigma2]]);
        assert_eq!(m[(0, 1)], 0.0);
    }

    #[test]
    fn diffusion_nonpositive_variance_floors_vol() {
        let p = make_process();
        let vol = 1e-8;
        let sigma2 = SIGMA * vol;
        let sqrhov = (1.0 - RHO * RHO).sqrt();
        let m = p.diffusion(0.5, &Array::from([S0, 0.0])).unwrap();
        assert_matrix_close(&m, &[&[vol, 0.0], &[RHO * sigma2, sqrhov * sigma2]]);
    }

    /// Confirm-by-stubbing: `diffusion[1][1]` carries the `sqrt(1 - rho^2)`
    /// factor, so a different rho moves it (and only it among the sqrhov path).
    #[test]
    fn diffusion_bottom_right_tracks_sqrt_one_minus_rho_squared() {
        let vol = V0.sqrt();
        let x = Array::from([S0, V0]);

        let correlated = make_process();
        let uncorrelated = HestonProcess::new(
            flat_yield(R),
            flat_yield(Q),
            make_quote_handle(S0).handle(),
            V0,
            KAPPA,
            THETA,
            SIGMA,
            0.0,
        );

        let m_rho = correlated.diffusion(0.5, &x).unwrap();
        let m_zero = uncorrelated.diffusion(0.5, &x).unwrap();

        assert_close(m_zero[(1, 1)], SIGMA * vol);
        assert_close(m_rho[(1, 1)], (1.0 - RHO * RHO).sqrt() * SIGMA * vol);
        assert!((m_rho[(1, 1)] - m_zero[(1, 1)]).abs() > 1e-6);
    }

    #[test]
    fn apply_is_multiplicative_in_spot_additive_in_variance() {
        let p = make_process();
        let x0 = Array::from([S0, V0]);
        let dx = Array::from([0.1, -0.005]);
        let out = p.apply(&x0, &dx);
        assert_close(out[0], S0 * 0.1_f64.exp());
        assert_close(out[1], V0 - 0.005);
    }

    /// The inherited Euler `expectation` routes through Heston's `apply`, so
    /// element 0 is multiplicative `S exp(drift0 dt)` and element 1 additive
    /// `v + drift1 dt`.
    #[test]
    fn expectation_composes_euler_drift_through_apply() {
        let p = make_process();
        let x0 = Array::from([S0, V0]);
        let dt: Time = 0.5;
        let drift = p.drift(0.0, &x0).unwrap();
        let e = p.expectation(0.0, &x0, dt).unwrap();
        assert_close(e[0], S0 * (drift[0] * dt).exp());
        assert_close(e[1], V0 + drift[1] * dt);
    }

    #[test]
    fn std_deviation_is_diffusion_scaled_by_sqrt_dt() {
        let p = make_process();
        let x0 = Array::from([S0, V0]);
        let dt: Time = 0.5;
        let d = p.diffusion(0.0, &x0).unwrap();
        let sd = p.std_deviation(0.0, &x0, dt).unwrap();
        let s = dt.sqrt();
        assert_matrix_close(
            &sd,
            &[
                &[d[(0, 0)] * s, d[(0, 1)] * s],
                &[d[(1, 0)] * s, d[(1, 1)] * s],
            ],
        );
    }

    #[test]
    fn covariance_is_diffusion_gram_scaled_by_dt() {
        let p = make_process();
        let x0 = Array::from([S0, V0]);
        let dt: Time = 0.5;
        let vol = V0.sqrt();
        let a = RHO * SIGMA * vol;
        let b = (1.0 - RHO * RHO).sqrt() * SIGMA * vol;
        let cov = p.covariance(0.0, &x0, dt).unwrap();
        assert_matrix_close(
            &cov,
            &[
                &[vol * vol * dt, vol * a * dt],
                &[vol * a * dt, (a * a + b * b) * dt],
            ],
        );
    }

    #[test]
    fn evolve_is_deferred_to_the_monte_carlo_batch() {
        let p = make_process();
        let x0 = Array::from([S0, V0]);
        let dw = Array::from([0.5, -0.5]);
        let err = p.evolve(0.0, &x0, 0.5, &dw).unwrap_err();
        assert!(
            err.message().contains("#410"),
            "evolve error must reference the #410 deferral: {}",
            err.message()
        );
    }

    #[test]
    fn time_uses_the_risk_free_day_counter() {
        let p = make_process();
        assert_eq!(p.time(&(reference() + 180)).unwrap(), 0.5);
    }

    #[test]
    fn input_changes_notify_process_observers() {
        let spot = shared(SimpleQuote::new(S0));
        let s0_handle = Handle::new(Shared::clone(&spot) as Shared<dyn Quote>);
        let p = HestonProcess::new(
            flat_yield(R),
            flat_yield(Q),
            s0_handle,
            V0,
            KAPPA,
            THETA,
            SIGMA,
            RHO,
        );
        let flag = Flag::new();
        p.observable().register_observer(&as_observer(&flag));

        spot.set_value(105.0);

        assert!(
            Flag::is_up(&flag),
            "a spot change must notify process observers"
        );
    }

    #[test]
    fn trait_is_object_safe() {
        let p = make_process();
        let dynamic: &dyn StochasticProcess = &p;
        assert_eq!(dynamic.size(), 2);
    }
}
