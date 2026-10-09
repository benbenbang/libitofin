//! Stateless Greek conversion helpers from `ql/pricingengines/greeks.cpp`.

use crate::errors::QlResult;
use crate::interestrate::Compounding;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::require;
use crate::time::frequency::Frequency;
use crate::types::Real;

/// Recovers annual Black-Scholes theta from a process and supplied Greeks.
///
/// Uses the current spot, continuously compounded zero rates at time zero and
/// local volatility at `(0, spot)`, matching QuantLib's `blackScholesTheta`:
/// `r * value - (r - q) * spot * delta - 0.5 * vol * vol * spot * spot * gamma`.
/// At zero, the curves use their standard short-time zero-rate convention.
/// An external local-volatility handle takes precedence over Black volatility.
/// No maturity, day counter conversion or repricing of the supplied Greeks is
/// performed. Callers must supply value, delta and gamma for the current market.
///
/// # Errors
/// Returns an error for non-finite arguments or market inputs, non-positive
/// spot, negative local volatility, unreadable/empty market handles, unsupported
/// derived local volatility, curve range errors or a non-finite formula result.
/// These checks deliberately replace QuantLib's unchecked IEEE propagation.
/// Signed values, deltas, gammas and negative interest rates remain supported.
pub fn black_scholes_theta(
    process: &GeneralizedBlackScholesProcess,
    value: Real,
    delta: Real,
    gamma: Real,
) -> QlResult<Real> {
    for (name, input) in [("value", value), ("delta", delta), ("gamma", gamma)] {
        require!(input.is_finite(), "{name} ({input}) must be finite");
    }
    let spot = process.state_variable().current_link()?.value()?;
    require!(
        spot.is_finite() && spot > 0.0,
        "spot ({spot}) must be positive and finite"
    );
    let r = process
        .risk_free_rate()
        .current_link()?
        .zero_rate(0.0, Compounding::Continuous, Frequency::NoFrequency, false)?
        .rate();
    let q = process
        .dividend_yield()
        .current_link()?
        .zero_rate(0.0, Compounding::Continuous, Frequency::NoFrequency, false)?
        .rate();
    require!(r.is_finite() && q.is_finite(), "zero rates must be finite");
    let vol = process
        .local_volatility()?
        .current_link()?
        .local_vol(0.0, spot, false)?;
    require!(
        vol.is_finite() && vol >= 0.0,
        "local volatility ({vol}) must be non-negative and finite"
    );
    let theta = r * value - (r - q) * spot * delta - 0.5 * vol * vol * spot * spot * gamma;
    require!(theta.is_finite(), "Black-Scholes theta is not finite");
    Ok(theta)
}

/// Converts annual theta to theta per day using a fixed 365-day year.
///
/// Matches QuantLib's `defaultThetaPerDay`: no calendar or day counter is used,
/// even in leap years. IEEE-754 zeros, infinities and NaNs propagate normally.
///
/// # Examples
/// ```
/// use libitofin::pricingengines::default_theta_per_day;
/// assert_eq!(default_theta_per_day(-365.0), -1.0);
/// ```
pub fn default_theta_per_day(theta: Real) -> Real {
    theta / 365.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theta_per_day_uses_fixed_365_and_preserves_sign() {
        for (theta, expected) in [(365.0, 1.0), (-730.0, -2.0), (182.5, 0.5)] {
            assert_eq!(default_theta_per_day(theta), expected);
        }
        assert_eq!(default_theta_per_day(366.0), 366.0 / 365.0);
        assert_ne!(default_theta_per_day(366.0), 1.0);
        assert_eq!(default_theta_per_day(0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(default_theta_per_day(-0.0).to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn theta_per_day_preserves_ieee_nonfinite_behavior() {
        assert_eq!(default_theta_per_day(f64::INFINITY), f64::INFINITY);
        assert_eq!(default_theta_per_day(f64::NEG_INFINITY), f64::NEG_INFINITY);
        assert!(default_theta_per_day(f64::NAN).is_nan());
        assert_eq!(default_theta_per_day(f64::MAX), f64::MAX / 365.0);
        assert_eq!(
            default_theta_per_day(f64::MIN_POSITIVE),
            f64::MIN_POSITIVE / 365.0
        );
    }
}
