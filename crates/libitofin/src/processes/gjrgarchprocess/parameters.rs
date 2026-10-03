use crate::errors::QlResult;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::require;

/// Daily GJR-GARCH constants and the annualization convention.
///
/// Finite-horizon simulation does not require stationary persistence. This
/// process requires nonnegative daily variance, omega, alpha, beta, and
/// alpha + gamma, unlike the separate QuantLib calibrated-model constraints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GjrGarchParameters {
    /// Initial daily variance.
    pub v0: f64,
    /// Daily variance intercept.
    pub omega: f64,
    /// Symmetric innovation coefficient.
    pub alpha: f64,
    /// Lagged variance coefficient.
    pub beta: f64,
    /// Additional negative-innovation coefficient.
    pub gamma: f64,
    /// Innovation displacement used by the diffusion approximation.
    pub lambda: f64,
    /// Number of model days in a year.
    pub days_per_year: f64,
}

/// Variance treatment in the GJR-GARCH diffusion approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GjrGarchDiscretization {
    /// Truncate diffusion while preserving raw variance in the drift.
    PartialTruncation = 0,
    /// Truncate variance in both drift and diffusion.
    FullTruncation = 1,
    /// Reflect variance before evolving, with QuantLib's signed diffusion.
    Reflection = 2,
}

/// Validated, precomputed coefficients shared by scalar and batch evolution.
#[derive(Debug, Clone, Copy)]
pub struct GjrGarchCoefficients {
    initial_variance: f64,
    intercept: f64,
    mean_reversion: f64,
    rho1: f64,
    rho2: f64,
}

impl GjrGarchParameters {
    /// Validates daily parameter domains and representable annual coefficients.
    ///
    /// # Errors
    /// Rejects nonfinite values, invalid domains, overflow and a negative
    /// residual diffusion variance. Persistence at or above one is allowed.
    pub fn validate(&self) -> QlResult<()> {
        self.coefficients().map(|_| ())
    }

    /// Precomputes annual coefficients without applying a stationarity test.
    ///
    /// # Errors
    /// Returns the same failures as [`Self::validate`].
    pub fn coefficients(&self) -> QlResult<GjrGarchCoefficients> {
        require!(
            [
                self.v0,
                self.omega,
                self.alpha,
                self.beta,
                self.gamma,
                self.lambda,
                self.days_per_year
            ]
            .iter()
            .all(|x| x.is_finite()),
            "GJR-GARCH parameters must be finite"
        );
        require!(
            self.v0 >= 0.0
                && self.omega >= 0.0
                && self.alpha >= 0.0
                && self.beta >= 0.0
                && self.alpha + self.gamma >= 0.0
                && self.days_per_year > 0.0,
            "invalid GJR-GARCH parameter domain"
        );
        let l = self.lambda;
        let n_cdf = CumulativeNormalDistribution::standard().value(l);
        let density = (-l * l / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
        let q2 = 1.0 + l * l;
        let q3 = l * density + n_cdf + l * l * n_cdf;
        let sigma2 = 2.0 + 4.0 * l * l;
        let e4 = l * l * l * density
            + 5.0 * l * density
            + 3.0 * n_cdf
            + l * l * l * l * n_cdf
            + 6.0 * l * l * n_cdf;
        let sigma3 = e4 - q3 * q3;
        let sigma12 = -2.0 * l;
        let sigma13 = -2.0 * density - 2.0 * l * n_cdf;
        let sigma23 = 2.0 * n_cdf + sigma12 * sigma13;
        let residual = self.alpha * self.alpha * (sigma2 - sigma12 * sigma12)
            + self.gamma * self.gamma * (sigma3 - sigma13 * sigma13)
            + 2.0 * self.alpha * self.gamma * (sigma23 - sigma12 * sigma13);
        require!(
            residual.is_finite() && residual >= 0.0,
            "GJR-GARCH residual diffusion variance is not representable"
        );
        let days = self.days_per_year;
        let result = GjrGarchCoefficients {
            initial_variance: days * self.v0,
            intercept: days * days * self.omega,
            mean_reversion: days * (self.beta + self.alpha * q2 + self.gamma * q3 - 1.0),
            rho1: days.sqrt() * (self.alpha * sigma12 + self.gamma * sigma13),
            rho2: days.sqrt() * residual.sqrt(),
        };
        require!(
            [
                result.initial_variance,
                result.intercept,
                result.mean_reversion,
                result.rho1,
                result.rho2
            ]
            .iter()
            .all(|x| x.is_finite()),
            "GJR-GARCH annual coefficients are not representable"
        );
        Ok(result)
    }

    /// Evolves spot and annual variance from two independent normal variates.
    ///
    /// # Errors
    /// Rejects invalid parameters, state, time step, rates, shocks or output.
    pub fn evolve(
        &self,
        state: [f64; 2],
        dt: f64,
        r_minus_q: f64,
        dw: [f64; 2],
        scheme: GjrGarchDiscretization,
    ) -> QlResult<[f64; 2]> {
        self.coefficients()?
            .evolve(state, dt, r_minus_q, dw, scheme)
    }
}

impl GjrGarchCoefficients {
    /// Initial annual variance, equal to daily variance times days per year.
    pub fn initial_variance(&self) -> f64 {
        self.initial_variance
    }

    pub(super) fn drift(
        &self,
        variance: f64,
        r_minus_q: f64,
        scheme: GjrGarchDiscretization,
    ) -> QlResult<[f64; 2]> {
        let vol = volatility(variance, scheme, 0.0);
        let drift_variance = if scheme == GjrGarchDiscretization::PartialTruncation {
            variance
        } else {
            vol * vol
        };
        let result = [
            r_minus_q - 0.5 * vol * vol,
            self.intercept + self.mean_reversion * drift_variance,
        ];
        require!(
            result.iter().all(|x| x.is_finite()),
            "GJR-GARCH drift overflow"
        );
        Ok(result)
    }

    pub(super) fn diffusion(
        &self,
        variance: f64,
        scheme: GjrGarchDiscretization,
    ) -> QlResult<[[f64; 2]; 2]> {
        let vol = volatility(variance, scheme, 1e-8);
        let result = [[vol, 0.0], [self.rho1 * vol * vol, vol * vol * self.rho2]];
        require!(
            result.iter().flatten().all(|x| x.is_finite()),
            "GJR-GARCH diffusion overflow"
        );
        Ok(result)
    }

    /// Evolves spot and annual variance using precomputed daily parameters.
    ///
    /// Negative variance is retained by truncation schemes. Reflection starts
    /// from squared absolute-volatility, but does not floor the resulting state.
    ///
    /// # Errors
    /// Rejects nonpositive/nonfinite spot, nonfinite variance, negative/nonfinite
    /// step, nonfinite rates or shocks, and unrepresentable output.
    pub fn evolve(
        &self,
        state: [f64; 2],
        dt: f64,
        r_minus_q: f64,
        dw: [f64; 2],
        scheme: GjrGarchDiscretization,
    ) -> QlResult<[f64; 2]> {
        validate_state(state)?;
        validate_step(dt)?;
        require!(
            r_minus_q.is_finite() && dw.iter().all(|x| x.is_finite()),
            "GJR-GARCH rates and shocks must be finite"
        );
        let vol = if scheme == GjrGarchDiscretization::Reflection {
            state[1].abs().sqrt()
        } else {
            state[1].max(0.0).sqrt()
        };
        let drift_variance = if scheme == GjrGarchDiscretization::PartialTruncation {
            state[1]
        } else {
            vol * vol
        };
        let nu = self.intercept + self.mean_reversion * drift_variance;
        let base = if scheme == GjrGarchDiscretization::Reflection {
            vol * vol
        } else {
            state[1]
        };
        let result = [
            state[0] * ((r_minus_q - 0.5 * vol * vol) * dt + vol * dw[0] * dt.sqrt()).exp(),
            base + nu * dt + dt.sqrt() * vol * vol * (self.rho1 * dw[0] + self.rho2 * dw[1]),
        ];
        validate_state(result)?;
        Ok(result)
    }
}

pub(super) fn validate_state(state: [f64; 2]) -> QlResult<()> {
    require!(
        state[0].is_finite() && state[0] > 0.0 && state[1].is_finite(),
        "GJR-GARCH state requires positive finite spot and finite variance"
    );
    Ok(())
}

pub(super) fn validate_step(dt: f64) -> QlResult<()> {
    require!(
        dt.is_finite() && dt >= 0.0,
        "GJR-GARCH step must be finite and non-negative"
    );
    Ok(())
}

fn volatility(variance: f64, scheme: GjrGarchDiscretization, floor: f64) -> f64 {
    if variance > 0.0 {
        variance.sqrt()
    } else if scheme == GjrGarchDiscretization::Reflection {
        -(-variance).sqrt()
    } else {
        floor
    }
}
