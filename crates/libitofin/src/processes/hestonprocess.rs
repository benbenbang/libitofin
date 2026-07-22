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
//! `evolve` ports the ctor-default Andersen Quadratic-Exponential scheme
//! ([`Discretization::QuadraticExponential`] /
//! [`Discretization::QuadraticExponentialMartingale`],
//! `hestonprocess.cpp:461-516`), reading the two Gaussian factors `dw[0]`
//! (spot) and `dw[1]` (variance).
//!
//! Deferred, visibly (tracked by #410):
//! - The remaining seven [`Discretization`] variants (PartialTruncation,
//!   FullTruncation, Reflection, NonCentralChiSquareVariance, and the three
//!   BroadieKaya exact schemes) make `evolve` fail loudly rather than silently
//!   misprice; the BroadieKaya schemes additionally need the modified Bessel
//!   function and complex exact sampling. `drift` / `diffusion` are scheme
//!   independent (they hard-code the branch every scheme except Reflection and
//!   PartialTruncation shares), so the analytic surface stays unambiguous.
//! - `pdf` and `varianceDistribution` (the Fokker-Planck density) are not
//!   ported. The base `expectation` / `std_deviation` / `covariance` are kept:
//!   QuantLib's own ctor installs `EulerDiscretization` (`hestonprocess.cpp:46`),
//!   so those three genuinely are the Euler defaults.

use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::math::matrix::Matrix;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Size, Time};

/// Variance-discretization scheme for the Monte Carlo `evolve`.
///
/// Mirrors `HestonProcess::Discretization` (`hestonprocess.hpp:47-55`). Only
/// the ctor-default [`QuadraticExponential`](Discretization::QuadraticExponential)
/// and
/// [`QuadraticExponentialMartingale`](Discretization::QuadraticExponentialMartingale)
/// are ported; every other variant makes `evolve` fail loudly (deferred, #410).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discretization {
    /// Lord-Koekkoek-van Dijk partial truncation (deferred, #410).
    PartialTruncation,
    /// Lord-Koekkoek-van Dijk full truncation (deferred, #410).
    FullTruncation,
    /// Lord-Koekkoek-van Dijk reflection (deferred, #410).
    Reflection,
    /// Alan Lewis exact non-central chi-square variance (deferred, #410).
    NonCentralChiSquareVariance,
    /// Andersen quadratic-exponential scheme.
    QuadraticExponential,
    /// Andersen quadratic-exponential scheme with the martingale correction
    /// (the ctor default).
    QuadraticExponentialMartingale,
    /// Broadie-Kaya exact scheme, Gauss-Lobatto quadrature (deferred, #410).
    BroadieKayaExactSchemeLobatto,
    /// Broadie-Kaya exact scheme, Gauss-Laguerre quadrature (deferred, #410).
    BroadieKayaExactSchemeLaguerre,
    /// Broadie-Kaya exact scheme, trapezoidal quadrature (deferred, #410).
    BroadieKayaExactSchemeTrapezoidal,
}

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
    discretization: Discretization,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl HestonProcess {
    /// Builds a Heston process registered with its three market inputs, using
    /// the ctor-default [`Discretization::QuadraticExponentialMartingale`]
    /// (`hestonprocess.hpp:65`).
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
        Self::with_discretization(
            risk_free_rate,
            dividend_yield,
            s0,
            v0,
            kappa,
            theta,
            sigma,
            rho,
            Discretization::QuadraticExponentialMartingale,
        )
    }

    /// Builds a Heston process with an explicit variance-discretization scheme.
    #[allow(clippy::too_many_arguments)]
    pub fn with_discretization(
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        s0: Handle<dyn Quote>,
        v0: Real,
        kappa: Real,
        theta: Real,
        sigma: Real,
        rho: Real,
        discretization: Discretization,
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
            discretization,
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

    /// The interval forward `forwardRate(t0, t0+dt, Continuous)` of a curve.
    ///
    /// The QE scheme drifts the spot by the INTERVAL forward over `[t0, t0+dt]`
    /// (`hestonprocess.cpp:510-511`), NOT the instantaneous forward
    /// `forwardRate(t, t)` that [`drift`](Self::drift) uses. On the flat curves
    /// exercised here the two coincide, but the interval form is the faithful
    /// port of the C++.
    fn forward_interval(
        &self,
        curve: &Handle<dyn YieldTermStructure>,
        t0: Time,
        dt: Time,
    ) -> QlResult<Rate> {
        Ok(curve
            .current_link()?
            .forward_rate(
                t0,
                t0 + dt,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )?
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

    fn evolve(&self, t0: Time, x0: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        let martingale = match self.discretization {
            Discretization::QuadraticExponential => false,
            Discretization::QuadraticExponentialMartingale => true,
            other => fail!(
                "HestonProcess::evolve: the {other:?} variance scheme is deferred (#410); only \
                 QuadraticExponential and QuadraticExponentialMartingale are ported"
            ),
        };

        let ex = (-self.kappa * dt).exp();
        let m = self.theta + (x0[1] - self.theta) * ex;
        let s2 = x0[1] * self.sigma * self.sigma * ex / self.kappa * (1.0 - ex)
            + self.theta * self.sigma * self.sigma / (2.0 * self.kappa) * (1.0 - ex) * (1.0 - ex);
        let psi = s2 / (m * m);

        let g1 = 0.5;
        let g2 = 0.5;
        let mut k0 = -self.rho * self.kappa * self.theta * dt / self.sigma;
        let k1 = g1 * dt * (self.kappa * self.rho / self.sigma - 0.5) - self.rho / self.sigma;
        let k2 = g2 * dt * (self.kappa * self.rho / self.sigma - 0.5) + self.rho / self.sigma;
        let k3 = g1 * dt * (1.0 - self.rho * self.rho);
        let k4 = g2 * dt * (1.0 - self.rho * self.rho);
        let a = k2 + 0.5 * k4;

        let next_v = if psi < 1.5 {
            let b2 = 2.0 / psi - 1.0 + (2.0 / psi * (2.0 / psi - 1.0)).sqrt();
            let b = b2.sqrt();
            let a_qe = m / (1.0 + b2);
            if martingale {
                if a >= 1.0 / (2.0 * a_qe) {
                    fail!(
                        "HestonProcess::evolve QEM martingale correction: illegal value \
                         (A >= 1/(2a))"
                    );
                }
                k0 = -a * b2 * a_qe / (1.0 - 2.0 * a * a_qe) + 0.5 * (1.0 - 2.0 * a * a_qe).ln()
                    - (k1 + 0.5 * k3) * x0[1];
            }
            a_qe * (b + dw[1]) * (b + dw[1])
        } else {
            let p = (psi - 1.0) / (psi + 1.0);
            let beta = (1.0 - p) / m;
            let u = CumulativeNormalDistribution::standard().value(dw[1]);
            if martingale {
                if a >= beta {
                    fail!(
                        "HestonProcess::evolve QEM martingale correction: illegal value \
                         (A >= beta)"
                    );
                }
                k0 = -(p + beta * (1.0 - p) / (beta - a)).ln() - (k1 + 0.5 * k3) * x0[1];
            }
            if u <= p {
                0.0
            } else {
                ((1.0 - p) / (1.0 - u)).ln() / beta
            }
        };

        let mu = self.forward_interval(&self.risk_free_rate, t0, dt)?
            - self.forward_interval(&self.dividend_yield, t0, dt)?;

        let next_s = x0[0]
            * (mu * dt + k0 + k1 * x0[1] + k2 * next_v + (k3 * x0[1] + k4 * next_v).sqrt() * dw[0])
                .exp();

        Ok(Array::from([next_s, next_v]))
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
    use crate::math::distributions::normal::InverseCumulativeNormal;
    use crate::math::randomnumbers::{MersenneTwisterUniformRng, UniformRng};
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

    /// N standard-normal draws from a fixed-seed Mersenne twister pushed
    /// through the inverse cumulative normal (deterministic).
    fn standard_normals(n: usize, seed: u32) -> Vec<Real> {
        let mut rng = MersenneTwisterUniformRng::new(seed);
        (0..n)
            .map(|_| InverseCumulativeNormal::standard_value(rng.next_real()).unwrap())
            .collect()
    }

    /// Sample mean and unbiased variance, with the standard error of each: the
    /// mean's `sqrt(var/N)` and the variance's `sqrt((mu4 - var^2)/N)` computed
    /// from the sample's own fourth central moment (robust to non-normal tails,
    /// unlike the Gaussian `sqrt(2) var / sqrt(N)`).
    fn sample_moments(xs: &[Real]) -> (Real, Real, Real, Real) {
        let n = xs.len() as Real;
        let mean = xs.iter().sum::<Real>() / n;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<Real>() / (n - 1.0);
        let mu4 = xs.iter().map(|x| (x - mean).powi(4)).sum::<Real>() / n;
        let se_mean = (var / n).sqrt();
        let se_var = ((mu4 - var * var) / n).max(0.0).sqrt();
        (mean, var, se_mean, se_var)
    }

    #[allow(clippy::too_many_arguments)]
    fn heston(
        v0: Real,
        kappa: Real,
        theta: Real,
        sigma: Real,
        rho: Real,
        disc: Discretization,
    ) -> HestonProcess {
        HestonProcess::with_discretization(
            flat_yield(R),
            flat_yield(Q),
            make_quote_handle(S0).handle(),
            v0,
            kappa,
            theta,
            sigma,
            rho,
            disc,
        )
    }

    /// The QE variance leg (`retVal[1]`) is a moment-matched draw of the CIR
    /// conditional law: over many fixed-seed `dw[1]` its sample mean converges
    /// to `m` and its sample variance to `s2` (`hestonprocess.cpp:469-471`).
    /// `m` / `s2` here are an INDEPENDENT transcription of the formula (never
    /// read back from the implementation), so the `s2` confirm-by-stubbing edit
    /// can actually make this fail. `retVal[1]` ignores `dw[0]`, so the spot
    /// draw is fixed at 0. This fixture exercises the `psi < 1.5` branch.
    #[test]
    fn qe_variance_leg_matches_cir_moments_psi_below_one_and_a_half() {
        const V0F: Real = 0.09;
        const KAPPAF: Real = 1.0;
        const THETAF: Real = 0.09;
        const SIGMAF: Real = 0.3;
        const DT: Time = 0.1;
        const N: usize = 40_000;

        let ex = (-KAPPAF * DT).exp();
        let m = THETAF + (V0F - THETAF) * ex;
        let s2 = V0F * SIGMAF * SIGMAF * ex / KAPPAF * (1.0 - ex)
            + THETAF * SIGMAF * SIGMAF / (2.0 * KAPPAF) * (1.0 - ex) * (1.0 - ex);
        let psi = s2 / (m * m);
        assert!(psi < 1.5, "fixture must hit the psi<1.5 branch: psi={psi}");

        let p = heston(
            V0F,
            KAPPAF,
            THETAF,
            SIGMAF,
            RHO,
            Discretization::QuadraticExponential,
        );
        let x0 = Array::from([S0, V0F]);
        let vs: Vec<Real> = standard_normals(N, 0x51E9_0001)
            .into_iter()
            .map(|z| p.evolve(0.0, &x0, DT, &Array::from([0.0, z])).unwrap()[1])
            .collect();

        let (mean, var, se_mean, se_var) = sample_moments(&vs);
        assert!(
            (mean - m).abs() < 4.0 * se_mean,
            "mean {mean} vs m {m}: {:.2} se (bound 4.0), psi={psi}",
            (mean - m).abs() / se_mean
        );
        assert!(
            (var - s2).abs() < 5.0 * se_var,
            "var {var} vs s2 {s2}: {:.2} se (bound 5.0), psi={psi}",
            (var - s2).abs() / se_var
        );
    }

    /// The `psi >= 1.5` branch of the same CIR moment pin: `retVal[1]` is the
    /// point-mass / exponential mixture (`hestonprocess.cpp:496-508`), whose
    /// mean is `m` and variance `s2`. The empirical `se` (from the sample's own
    /// fourth moment) absorbs the mixture's heavy tail.
    #[test]
    fn qe_variance_leg_matches_cir_moments_psi_above_one_and_a_half() {
        const V0F: Real = 0.01;
        const KAPPAF: Real = 0.5;
        const THETAF: Real = 0.01;
        const SIGMAF: Real = 1.0;
        const DT: Time = 1.0;
        const N: usize = 40_000;

        let ex = (-KAPPAF * DT).exp();
        let m = THETAF + (V0F - THETAF) * ex;
        let s2 = V0F * SIGMAF * SIGMAF * ex / KAPPAF * (1.0 - ex)
            + THETAF * SIGMAF * SIGMAF / (2.0 * KAPPAF) * (1.0 - ex) * (1.0 - ex);
        let psi = s2 / (m * m);
        assert!(
            psi >= 1.5,
            "fixture must hit the psi>=1.5 branch: psi={psi}"
        );

        let p = heston(
            V0F,
            KAPPAF,
            THETAF,
            SIGMAF,
            RHO,
            Discretization::QuadraticExponential,
        );
        let x0 = Array::from([S0, V0F]);
        let vs: Vec<Real> = standard_normals(N, 0x51E9_0002)
            .into_iter()
            .map(|z| p.evolve(0.0, &x0, DT, &Array::from([0.0, z])).unwrap()[1])
            .collect();

        let (mean, var, se_mean, se_var) = sample_moments(&vs);
        assert!(
            (mean - m).abs() < 4.0 * se_mean,
            "mean {mean} vs m {m}: {:.2} se (bound 4.0), psi={psi}",
            (mean - m).abs() / se_mean
        );
        assert!(
            (var - s2).abs() < 5.0 * se_var,
            "var {var} vs s2 {s2}: {:.2} se (bound 5.0), psi={psi}",
            (var - s2).abs() / se_var
        );
    }

    /// The reason the ctor default is *Martingale*: over many draws the
    /// discounted spot is a martingale, `mean(retVal[0]) == x0[0] exp((r-q) dt)`
    /// (`hestonprocess.cpp:513-514` with the QEM `k0` correction). `dw[0]` and
    /// `dw[1]` are INDEPENDENT standard normals (the `rho = -0.8` correlation is
    /// carried by `k1..k4`, not the draws); the identity holds only for that.
    /// The fixture is chosen so the `k0` martingale correction is load-bearing:
    /// dropping it (confirm-by-stubbing) throws the mean tens of `se` off.
    #[test]
    fn qem_discounted_spot_is_a_martingale() {
        const V0F: Real = 0.3;
        const KAPPAF: Real = 1.5;
        const THETAF: Real = 0.1;
        const SIGMAF: Real = 0.6;
        const RHOF: Real = -0.8;
        const DT: Time = 1.0;
        const N: usize = 50_000;

        let p = heston(
            V0F,
            KAPPAF,
            THETAF,
            SIGMAF,
            RHOF,
            Discretization::QuadraticExponentialMartingale,
        );
        let x0 = Array::from([S0, V0F]);
        let mut rng = MersenneTwisterUniformRng::new(0x51E9_0003);
        let spots: Vec<Real> = (0..N)
            .map(|_| {
                let z0 = InverseCumulativeNormal::standard_value(rng.next_real()).unwrap();
                let z1 = InverseCumulativeNormal::standard_value(rng.next_real()).unwrap();
                p.evolve(0.0, &x0, DT, &Array::from([z0, z1])).unwrap()[0]
            })
            .collect();

        let (mean, _var, se_mean, _se_var) = sample_moments(&spots);
        let target = S0 * ((R - Q) * DT).exp();
        assert!(
            (mean - target).abs() < 5.0 * se_mean,
            "discounted-spot mean {mean} vs target {target}: {:.2} se (bound 5.0)",
            (mean - target).abs() / se_mean
        );
    }

    /// A deferred variance scheme must fail loudly (naming #410), never silently
    /// misprice via a wrong branch.
    #[test]
    fn deferred_variance_scheme_evolve_errs_naming_410() {
        let p = heston(
            V0,
            KAPPA,
            THETA,
            SIGMA,
            RHO,
            Discretization::PartialTruncation,
        );
        let err = p
            .evolve(0.0, &Array::from([S0, V0]), 0.5, &Array::from([0.1, 0.2]))
            .unwrap_err();
        assert!(
            err.message().contains("#410"),
            "deferred scheme must name the #410 deferral: {}",
            err.message()
        );
    }

    /// The QEM martingale correction requires `A < beta` on the `psi >= 1.5`
    /// branch (`hestonprocess.cpp:503`); an extreme fixture (`v0=8, kappa=0.8,
    /// theta=1, sigma=2.5, rho=0.9, dt=1.5` gives `psi~=1.56`, `A~=0.272 >=
    /// beta~=0.251`) trips it, and evolve fails with "illegal value".
    #[test]
    fn qem_martingale_require_violation_errs() {
        let p = heston(
            8.0,
            0.8,
            1.0,
            2.5,
            0.9,
            Discretization::QuadraticExponentialMartingale,
        );
        let err = p
            .evolve(0.0, &Array::from([S0, 8.0]), 1.5, &Array::from([0.0, 0.0]))
            .unwrap_err();
        assert!(
            err.message().contains("illegal value"),
            "QEM require violation must report the illegal value: {}",
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
