//! Bare-factor two-dimensional Gaussian G2 process with exact OU transitions.
//!
//! State is `(x,y)`, not a shifted short rate. No curve fitting or forward measure
//! is provided. Instantaneous diffusion uses `rho * eta` in its lower-left entry,
//! correcting the unequal-volatility inconsistency in QuantLib's G2 diffusion.

use crate::errors::QlResult;
use crate::math::{array::Array, matrix::Matrix};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::stochasticprocess::StochasticProcess;
use crate::types::{Real, Size, Time};

/// Two correlated zero-level OU factors `dx=-a*x*dt+sigma*dW1`,
/// `dy=-b*y*dt+eta*dW2`, with instantaneous correlation `rho`.
///
/// Both factors initially equal zero. Signed finite reversion is supported.
/// Dynamics require finite two-element state vectors, finite nonnegative times
/// and steps, and finite results. The exact transition root maps two independent
/// standard normals to correlated Gaussian increments. Date conversion is not
/// supported, and the process never emits input-change notifications.
///
/// # Examples
/// ```
/// use libitofin::{math::array::Array, processes::G2Process,
///     stochasticprocess::StochasticProcess};
/// let process = G2Process::new(0.1, 0.01, 0.2, 0.008, -0.75)?;
/// let next = process.evolve(0.0, &process.initial_values()?, 0.25,
///     &Array::from([0.5, -0.3]))?;
/// assert_eq!(next.len(), 2);
/// # Ok::<(), libitofin::errors::QlError>(())
/// ```
pub struct G2Process {
    a: Real,
    sigma: Real,
    b: Real,
    eta: Real,
    rho: Real,
    observable: Observable,
}

impl G2Process {
    /// Constructs the bare-factor process, without a term-structure handle.
    ///
    /// # Errors
    /// Rejects nonfinite parameters, negative volatility, or correlation outside
    /// `[-1,1]`. Zero volatility and zero reversion are supported.
    pub fn new(a: Real, sigma: Real, b: Real, eta: Real, rho: Real) -> QlResult<Self> {
        require!(
            [a, sigma, b, eta, rho].iter().all(|v| v.is_finite()),
            "G2 parameters must be finite"
        );
        require!(
            sigma >= 0.0 && eta >= 0.0,
            "G2 volatility must be nonnegative"
        );
        require!((-1.0..=1.0).contains(&rho), "G2 rho must be in [-1,1]");
        Ok(Self {
            a,
            sigma,
            b,
            eta,
            rho,
            observable: Observable::new(),
        })
    }

    /// Initial first factor value, always zero.
    pub fn x0(&self) -> Real {
        0.0
    }
    /// Initial second factor value, always zero.
    pub fn y0(&self) -> Real {
        0.0
    }
    /// First factor signed mean reversion.
    pub fn a(&self) -> Real {
        self.a
    }
    /// First factor volatility.
    pub fn sigma(&self) -> Real {
        self.sigma
    }
    /// Second factor signed mean reversion.
    pub fn b(&self) -> Real {
        self.b
    }
    /// Second factor volatility.
    pub fn eta(&self) -> Real {
        self.eta
    }
    /// Instantaneous factor correlation.
    pub fn rho(&self) -> Real {
        self.rho
    }

    fn state(t: Time, x: &Array) -> QlResult<()> {
        require!(
            t.is_finite() && t >= 0.0,
            "G2 time must be finite and nonnegative"
        );
        require!(
            x.len() == 2 && x.iter().all(|v| v.is_finite()),
            "G2 state must contain two finite values"
        );
        Ok(())
    }

    fn step(t: Time, x: &Array, dt: Time) -> QlResult<()> {
        Self::state(t, x)?;
        require!(
            dt.is_finite() && dt >= 0.0 && (t + dt).is_finite(),
            "G2 step and endpoint must be finite and nonnegative"
        );
        Ok(())
    }

    fn transition_root(&self, dt: Time) -> QlResult<Matrix> {
        if dt == 0.0 {
            return Ok(Matrix::with_size(2, 2));
        }
        let lx = log_kernel(self.a, dt);
        let ly = log_kernel(self.b, dt);
        let sx = scaled_root(self.sigma, lx)?;
        let sy = scaled_root(self.eta, ly)?;
        let (corr, residual) = if sx == 0.0 || sy == 0.0 || self.rho == 0.0 {
            (0.0, 1.0)
        } else {
            let lc = log_kernel(self.a * 0.5 + self.b * 0.5, dt);
            let log_corr = integrated_log_correlation(self.a, self.b, dt, lc - lx * 0.5 - ly * 0.5);
            require!(
                log_corr.is_finite() && log_corr <= 32.0 * f64::EPSILON,
                "G2 transition correlation is invalid"
            );
            let log_corr = log_corr.min(0.0);
            let c = self.rho * log_corr.exp();
            let residual = (1.0 - self.rho) * (1.0 + self.rho)
                + self.rho * self.rho * (-(2.0 * log_corr).exp_m1());
            (c, residual)
        };
        Ok(Matrix::from([[sx, 0.0], [sy * corr, sy * residual.sqrt()]]))
    }
}

fn finite(v: Real) -> QlResult<Real> {
    require!(v.is_finite(), "G2 result is not finite");
    Ok(v)
}

fn log_kernel(r: Real, dt: Time) -> Real {
    let z = (-r * dt) * 2.0;
    if r == 0.0 || z == 0.0 {
        dt.ln()
    } else if r > 0.0 {
        (-z.exp_m1()).ln() - r.ln() - 2.0_f64.ln()
    } else {
        z + (-(-z).exp_m1()).ln() - (-r).ln() - 2.0_f64.ln()
    }
}

fn integrated_log_correlation(a: Real, b: Real, dt: Time, fallback: Real) -> Real {
    let x = a * dt;
    let y = b * dt;
    if x.abs().max(y.abs()) <= 0.125 {
        let m2 = (x * 0.5 + y * 0.5).powi(2);
        let d2 = (x * 0.5 - y * 0.5).powi(2);
        d2 * (-1.0 / 6.0 + (6.0 * m2 + d2) / 180.0
            - (15.0 * m2 * m2 + 15.0 * m2 * d2 + d2 * d2) / 2835.0
            + (28.0 * m2.powi(3) + 70.0 * m2 * m2 * d2 + 28.0 * m2 * d2 * d2 + d2.powi(3))
                / 37800.0
            - (45.0 * m2.powi(4)
                + 210.0 * m2.powi(3) * d2
                + 210.0 * m2 * m2 * d2 * d2
                + 45.0 * m2 * d2.powi(3)
                + d2.powi(4))
                / 467775.0
            + 691.0
                * (66.0 * m2.powi(5)
                    + 495.0 * m2.powi(4) * d2
                    + 924.0 * m2.powi(3) * d2 * d2
                    + 495.0 * m2 * m2 * d2.powi(3)
                    + 66.0 * m2 * d2.powi(4)
                    + d2.powi(5))
                / 3831077250.0)
    } else {
        let m = x * 0.5 + y * 0.5;
        let d = x * 0.5 - y * 0.5;
        if m.is_finite() && d.is_finite() && m != 0.0 && (d / m).abs() <= 0.5 {
            let log_ratio = 2.0 * (log_sinhc(d) - log_sinhc(m));
            let ratio = (d / m).powi(2);
            let residual = ratio * (-log_ratio.exp_m1()) / (1.0 - ratio * log_ratio.exp());
            0.5 * (-residual).ln_1p()
        } else if x.abs().max(y.abs()) <= 16.0 {
            log_sinhc(m) - 0.5 * log_sinhc(x) - 0.5 * log_sinhc(y)
        } else if a > 0.0 && b > 0.0 && x.is_infinite() && y.is_infinite() {
            let ratio = ((a * 0.5 - b * 0.5) / (a * 0.5 + b * 0.5)).powi(2);
            0.5 * (-ratio).ln_1p()
        } else {
            fallback
        }
    }
}

fn log_sinhc(x: Real) -> Real {
    let v = x.abs();
    if v <= 0.125 {
        let z = v * v;
        z * (1.0 / 6.0
            + z * (-1.0 / 180.0
                + z * (1.0 / 2835.0
                    + z * (-1.0 / 37800.0 + z * (1.0 / 467775.0 - z * 691.0 / 3831077250.0)))))
    } else {
        v - 2.0_f64.ln() - v.ln() + (-(-2.0 * v).exp_m1()).ln()
    }
}

fn scaled_root(vol: Real, log_integral: Real) -> QlResult<Real> {
    if vol == 0.0 {
        return Ok(0.0);
    }
    finite((vol.ln() + 0.5 * log_integral).exp())
}

impl AsObservable for G2Process {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess for G2Process {
    fn size(&self) -> Size {
        2
    }

    fn initial_values(&self) -> QlResult<Array> {
        Ok(Array::from([0.0, 0.0]))
    }

    fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        Self::state(t, x)?;
        Ok(Array::from([
            finite(-self.a * x[0])?,
            finite(-self.b * x[1])?,
        ]))
    }

    fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
        Self::state(t, x)?;
        Ok(Matrix::from([
            [self.sigma, 0.0],
            [
                self.rho * self.eta,
                self.eta * ((1.0 - self.rho) * (1.0 + self.rho)).sqrt(),
            ],
        ]))
    }

    fn expectation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Array> {
        Self::step(t, x, dt)?;
        let mean = |v: Real, r: Real| {
            if v == 0.0 {
                Ok(v)
            } else {
                let z = -r * dt;
                let result = v * z.exp();
                if result.is_finite() {
                    Ok(result)
                } else {
                    finite(v.signum() * (v.abs().ln() + z).exp())
                }
            }
        };
        Ok(Array::from([mean(x[0], self.a)?, mean(x[1], self.b)?]))
    }

    fn std_deviation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        Self::step(t, x, dt)?;
        self.transition_root(dt)
    }

    fn covariance(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        let s = self.std_deviation(t, x, dt)?;
        let xx = finite(s[(0, 0)] * s[(0, 0)])?;
        let yy = finite(s[(1, 0)] * s[(1, 0)] + s[(1, 1)] * s[(1, 1)])?;
        let xy = finite(s[(0, 0)] * s[(1, 0)])?;
        Ok(Matrix::from([[xx, xy], [xy, yy]]))
    }

    fn evolve(&self, t: Time, x: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        require!(
            dw.len() == 2 && dw.iter().all(|v| v.is_finite()),
            "G2 noise must contain two finite values"
        );
        let e = self.expectation(t, x, dt)?;
        let s = self.std_deviation(t, x, dt)?;
        Ok(Array::from([
            finite(e[0] + s[(0, 0)] * dw[0])?,
            finite(e[1] + s[(1, 0)] * dw[0] + s[(1, 1)] * dw[1])?,
        ]))
    }
}
