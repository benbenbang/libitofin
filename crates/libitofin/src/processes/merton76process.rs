//! Live market and lognormal-jump inputs for Merton's 1976 pricing model.
//!
//! Jump intensity is the event rate; log mean and volatility parameterize
//! `ln(J) ~ N(mu, delta^2)`. Pricing compensates the drift by
//! `intensity * (exp(mu + delta^2 / 2) - 1)`.
//! Like QuantLib, this carrier does not implement path evolution. Its fallible
//! unsupported methods return errors instead of exposing an infallible generator.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::termstructures::volatility::BlackVolTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{Real, Time};
use crate::{fail, require};

use super::GeneralizedBlackScholesProcess;

/// Observable market carrier for Merton76 European option pricing.
pub struct Merton76Process {
    process: Shared<GeneralizedBlackScholesProcess>,
    intensity: Handle<dyn Quote>,
    log_mean: Handle<dyn Quote>,
    log_vol: Handle<dyn Quote>,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl Merton76Process {
    /// Retains a Black-Scholes process and three live jump quote handles.
    ///
    /// # Errors
    /// Rejects empty or invalid quotes, nonpositive spot, negative intensity or
    /// jump dispersion, and unrepresentable jump compensation.
    pub fn from_black_scholes(
        process: Shared<GeneralizedBlackScholesProcess>,
        intensity: Handle<dyn Quote>,
        log_mean: Handle<dyn Quote>,
        log_vol: Handle<dyn Quote>,
    ) -> QlResult<Self> {
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::broadcasting(Shared::clone(&observable), || {});
        let observer = listener.clone() as SharedMut<dyn Observer>;
        process.observable().register_observer(&observer);
        intensity.register_observer(&observer);
        log_mean.register_observer(&observer);
        log_vol.register_observer(&observer);
        let result = Self {
            process,
            intensity,
            log_mean,
            log_vol,
            observable,
            _listener: listener,
        };
        let spot = result.x0()?;
        result.risk_free_rate().current_link()?;
        result.dividend_yield().current_link()?;
        let volatility = result
            .black_volatility()
            .current_link()?
            .black_vol(0.0, spot, false)?;
        require!(
            volatility.is_finite() && volatility >= 0.0,
            "Black volatility must be finite and non-negative"
        );
        result.jump_parameters()?;
        Ok(result)
    }

    /// Builds the process in QuantLib's conventional seven-handle order.
    ///
    /// # Errors
    /// Returns the same validation failures as [`Self::from_black_scholes`].
    pub fn new(
        spot: Handle<dyn Quote>,
        dividend: Handle<dyn YieldTermStructure>,
        risk_free: Handle<dyn YieldTermStructure>,
        volatility: Handle<dyn BlackVolTermStructure>,
        intensity: Handle<dyn Quote>,
        log_mean: Handle<dyn Quote>,
        log_vol: Handle<dyn Quote>,
    ) -> QlResult<Self> {
        Self::from_black_scholes(
            shared(GeneralizedBlackScholesProcess::new(
                spot, dividend, risk_free, volatility,
            )),
            intensity,
            log_mean,
            log_vol,
        )
    }

    /// The retained underlying spot quote handle.
    pub fn state_variable(&self) -> Handle<dyn Quote> {
        self.process.state_variable()
    }
    /// The retained dividend curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.process.dividend_yield()
    }
    /// The retained risk-free curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.process.risk_free_rate()
    }
    /// The retained Black volatility handle.
    pub fn black_volatility(&self) -> Handle<dyn BlackVolTermStructure> {
        self.process.black_volatility()
    }
    /// The event intensity, before the pricing measure adjustment.
    pub fn jump_intensity(&self) -> Handle<dyn Quote> {
        self.intensity.clone()
    }
    /// The normal mean of the logarithmic jump multiplier.
    pub fn log_mean_jump(&self) -> Handle<dyn Quote> {
        self.log_mean.clone()
    }
    /// The normal standard deviation of the logarithmic jump multiplier.
    pub fn log_jump_volatility(&self) -> Handle<dyn Quote> {
        self.log_vol.clone()
    }

    /// Reads the current positive finite spot.
    ///
    /// # Errors
    /// Rejects missing, invalid, nonfinite or nonpositive spot quotes.
    pub fn x0(&self) -> QlResult<Real> {
        let spot = self.process.x0()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "Merton76 spot must be finite and positive"
        );
        Ok(spot)
    }

    /// Converts a date using the underlying risk-free curve's clock.
    ///
    /// # Errors
    /// Propagates missing curve or invalid reference-date errors.
    pub fn time(&self, date: &Date) -> QlResult<Time> {
        self.process.time(date)
    }

    /// Path drift is unsupported by this pricing-only process.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn drift(&self, _time: Time, _spot: Real) -> QlResult<Real> {
        fail!("Merton76Process does not provide path drift")
    }
    /// Path diffusion is unsupported by this pricing-only process.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn diffusion(&self, _time: Time, _spot: Real) -> QlResult<Real> {
        fail!("Merton76Process does not provide path diffusion")
    }
    /// Path increments are unsupported by this pricing-only process.
    ///
    /// # Errors
    /// Always returns an unsupported-operation error.
    pub fn apply(&self, _spot: Real, _increment: Real) -> QlResult<Real> {
        fail!("Merton76Process does not provide path apply")
    }

    pub(crate) fn jump_parameters(&self) -> QlResult<(Real, Real, Real, Real, Real)> {
        let intensity = self.intensity.current_link()?.value()?;
        let mean = self.log_mean.current_link()?.value()?;
        let vol = self.log_vol.current_link()?.value()?;
        require!(
            intensity.is_finite() && intensity >= 0.0,
            "jump intensity must be finite and non-negative"
        );
        require!(mean.is_finite(), "log-jump mean must be finite");
        require!(
            vol.is_finite() && vol >= 0.0,
            "log-jump volatility must be finite and non-negative"
        );
        let variance = vol * vol;
        let exponent = mean + 0.5 * variance;
        let multiplier = exponent.exp();
        let compensation = intensity * exponent.exp_m1();
        let effective_intensity = intensity * multiplier;
        require!(
            variance.is_finite()
                && exponent.is_finite()
                && multiplier.is_finite()
                && multiplier > 0.0
                && compensation.is_finite()
                && effective_intensity.is_finite()
                && (intensity == 0.0 || effective_intensity > 0.0),
            "jump compensation is not representable"
        );
        Ok((
            intensity,
            variance,
            exponent,
            compensation,
            effective_intensity,
        ))
    }
}

impl AsObservable for Merton76Process {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}
