//! Stateless Greek conversion helpers from `ql/pricingengines/greeks.cpp`.

use crate::types::Real;

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
