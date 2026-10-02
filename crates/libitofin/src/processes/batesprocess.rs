//! Analytic Heston market carrier with independent constant-intensity lognormal jumps.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::{array::Array, matrix::Matrix};
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::Quote;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

use super::HestonProcess;

/// Pricing-only Bates process. Generic Gaussian evolution is deliberately unavailable.
pub struct BatesProcess {
    heston: Shared<HestonProcess>,
    lambda: Real,
    nu: Real,
    delta: Real,
}

impl BatesProcess {
    /// Retains live market handles and validates all eight parameters.
    ///
    /// # Errors
    /// Rejects empty handles, invalid spot, parameter domains or jump compensation.
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
        lambda: Real,
        nu: Real,
        delta: Real,
    ) -> QlResult<Self> {
        Self::validate_parameters(&Array::from([
            theta, kappa, sigma, rho, v0, nu, delta, lambda,
        ]))?;
        require!(
            risk_free_rate.current_link()?.reference_date()? != Date::null()
                && dividend_yield.current_link()?.reference_date()? != Date::null(),
            "Bates curves require non-null reference dates"
        );
        let spot = s0.current_link()?.value()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "Bates spot must be finite and positive"
        );
        Ok(Self {
            heston: shared(HestonProcess::new(
                risk_free_rate,
                dividend_yield,
                s0,
                v0,
                kappa,
                theta,
                sigma,
                rho,
            )),
            lambda,
            nu,
            delta,
        })
    }

    pub(crate) fn validate_parameters(params: &Array) -> QlResult<()> {
        require!(params.size() == 8, "Bates model requires eight parameters");
        require!(
            params.iter().all(|p| p.is_finite()),
            "Bates parameters must be finite"
        );
        require!(
            [0, 1, 2, 4].iter().all(|&i| params[i] > 0.0),
            "Bates theta, kappa, sigma and v0 must be positive"
        );
        require!(
            (-1.0..=1.0).contains(&params[3]),
            "Bates rho must lie in [-1, 1]"
        );
        require!(
            params[6] >= 0.0 && params[7] >= 0.0,
            "Bates delta and lambda must be non-negative"
        );
        let variance = params[6] * params[6];
        let exponent = params[5] + 0.5 * variance;
        let compensator = exponent.exp_m1();
        let exponential_moment = exponent.exp();
        require!(
            variance.is_finite()
                && exponent.is_finite()
                && compensator.is_finite()
                && exponential_moment.is_finite()
                && exponential_moment > 0.0
                && (params[7] * compensator).is_finite(),
            "Bates jump compensation is not representable"
        );
        Ok(())
    }

    /// The retained Heston component, for analytic parameter access only.
    pub fn heston_process(&self) -> Shared<HestonProcess> {
        self.heston.clone()
    }
    /// Initial variance.
    pub fn v0(&self) -> Real {
        self.heston.v0()
    }
    /// Variance mean-reversion speed.
    pub fn kappa(&self) -> Real {
        self.heston.kappa()
    }
    /// Long-run variance.
    pub fn theta(&self) -> Real {
        self.heston.theta()
    }
    /// Volatility of variance.
    pub fn sigma(&self) -> Real {
        self.heston.sigma()
    }
    /// Spot/variance correlation.
    pub fn rho(&self) -> Real {
        self.heston.rho()
    }
    /// Original jump event intensity.
    pub fn lambda(&self) -> Real {
        self.lambda
    }
    /// Mean logarithmic jump multiplier.
    pub fn nu(&self) -> Real {
        self.nu
    }
    /// Standard deviation of the logarithmic jump multiplier.
    pub fn delta(&self) -> Real {
        self.delta
    }
    /// Live risk-free curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.heston.risk_free_rate()
    }
    /// Live dividend curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.heston.dividend_yield()
    }
    /// Live spot quote handle.
    pub fn s0(&self) -> Handle<dyn Quote> {
        self.heston.s0()
    }
    /// State dimension: spot and variance.
    pub fn size(&self) -> Size {
        2
    }
    /// Upstream factor count, including jump randomness, not a Gaussian evolution contract.
    pub fn factors(&self) -> Size {
        4
    }
    /// Reads the current positive finite spot and initial variance.
    ///
    /// # Errors
    /// Rejects missing or invalid live spot quotes.
    pub fn initial_values(&self) -> QlResult<Array> {
        let spot = self.s0().current_link()?.value()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "Bates spot must be finite and positive"
        );
        Ok(Array::from([spot, self.v0()]))
    }
    /// Uses the risk-free curve's date clock.
    ///
    /// # Errors
    /// Propagates curve and date errors.
    pub fn time(&self, date: &Date) -> QlResult<Time> {
        require!(*date != Date::null(), "Bates time requires a non-null date");
        require!(
            self.risk_free_rate().current_link()?.reference_date()? != Date::null(),
            "Bates risk-free curve requires a non-null reference date"
        );
        self.heston.time(date)
    }
    /// Unsupported generic path drift.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn drift(&self, _time: Time, _state: &Array) -> QlResult<Array> {
        fail!("BatesProcess does not provide path drift")
    }
    /// Unsupported generic path diffusion.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn diffusion(&self, _time: Time, _state: &Array) -> QlResult<Matrix> {
        fail!("BatesProcess does not provide path diffusion")
    }
    /// Unsupported generic state increments.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn apply(&self, _state: &Array, _increment: &Array) -> QlResult<Array> {
        fail!("BatesProcess does not provide path apply")
    }
    /// Unsupported generic Gaussian expectation.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn expectation(&self, _time: Time, _state: &Array, _dt: Time) -> QlResult<Array> {
        fail!("BatesProcess does not provide Gaussian expectation")
    }
    /// Unsupported generic Gaussian standard deviation.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn std_deviation(&self, _time: Time, _state: &Array, _dt: Time) -> QlResult<Matrix> {
        fail!("BatesProcess does not provide Gaussian standard deviation")
    }
    /// Unsupported generic Gaussian covariance.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn covariance(&self, _time: Time, _state: &Array, _dt: Time) -> QlResult<Matrix> {
        fail!("BatesProcess does not provide Gaussian covariance")
    }
    /// Unsupported generic Gaussian evolution.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn evolve(&self, _time: Time, _state: &Array, _dt: Time, _dw: &Array) -> QlResult<Array> {
        fail!("BatesProcess does not provide Gaussian evolution")
    }
}

impl AsObservable for BatesProcess {
    fn observable(&self) -> &Observable {
        self.heston.observable()
    }
}
