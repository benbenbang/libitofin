//! Black 1976 formula family.
//!
//! Port of the value and undiscounted-form subset of
//! `ql/pricingengines/blackformula.{hpp,cpp}`: [`black_formula`], its forward
//! derivative, the cash/asset in-the-money probabilities and the standard
//! deviation first and second derivatives. Every function takes the *standard
//! deviation* over the option life, `volatility * sqrt(time_to_maturity)`,
//! not the volatility itself, and an optional lognormal `displacement`
//! shifting both forward and strike.
//!
//! Out of scope, left as follow-ups with the quotes that need them: the
//! implied-standard-deviation family (approximations and solvers) and the
//! Bachelier (normal-model) family.
//!
//! One deviation from the C++ reference: at `std_dev == 0` the reference's
//! `blackFormulaAssetItmProbability` tests `forward * sign < strike * sign`,
//! which contradicts its own `std_dev -> 0` limit (`N(sign * d1) -> 1` exactly
//! when `sign * (forward - strike) > 0`) and the cash probability next to it.
//! The port uses the limit; a test locks continuity with a tiny standard
//! deviation.

use crate::errors::QlResult;
use crate::fail;
use crate::math::distributions::normal::{CumulativeNormalDistribution, NormalDistribution};
use crate::option::OptionType;
use crate::types::Real;

fn check_parameters(strike: Real, forward: Real, displacement: Real) -> QlResult<()> {
    if displacement.is_nan() || displacement < 0.0 {
        fail!("displacement ({displacement}) must be non-negative");
    }
    let shifted_strike = strike + displacement;
    if shifted_strike.is_nan() || shifted_strike < 0.0 {
        fail!("strike + displacement ({strike} + {displacement}) must be non-negative");
    }
    let shifted_forward = forward + displacement;
    if shifted_forward.is_nan() || shifted_forward <= 0.0 {
        fail!("forward + displacement ({forward} + {displacement}) must be positive");
    }
    Ok(())
}

fn check_std_dev_and_discount(std_dev: Real, discount: Real) -> QlResult<()> {
    if std_dev.is_nan() || std_dev < 0.0 {
        fail!("stdDev ({std_dev}) must be non-negative");
    }
    if discount.is_nan() || discount <= 0.0 {
        fail!("discount ({discount}) must be positive");
    }
    Ok(())
}

fn sign_of(option_type: OptionType) -> Real {
    Real::from(option_type as i32)
}

/// Black 1976 value of a European option on the given forward.
pub fn black_formula(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;
    check_std_dev_and_discount(std_dev, discount)?;

    let sign = sign_of(option_type);

    if std_dev == 0.0 {
        let intrinsic = (forward - strike) * sign;
        let intrinsic = if intrinsic < 0.0 { 0.0 } else { intrinsic };
        return Ok(intrinsic * discount);
    }

    let forward = forward + displacement;
    let strike = strike + displacement;

    if strike == 0.0 {
        return Ok(match option_type {
            OptionType::Call => forward * discount,
            OptionType::Put => 0.0,
        });
    }

    let d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
    let d2 = d1 - std_dev;
    let phi = CumulativeNormalDistribution::standard();
    let nd1 = phi.value(sign * d1);
    let nd2 = phi.value(sign * d2);
    let result = discount * sign * (forward * nd1 - strike * nd2);
    if result.is_nan() || result < 0.0 {
        fail!(
            "negative value ({result}) for {std_dev} stdDev, {option_type} option, \
             {strike} strike, {forward} forward"
        );
    }
    Ok(result)
}

/// Derivative of [`black_formula`] with respect to the forward.
pub fn black_formula_forward_derivative(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;
    check_std_dev_and_discount(std_dev, discount)?;

    let sign = sign_of(option_type);

    if std_dev == 0.0 {
        let moneyness = (forward - strike) * sign;
        return Ok(if moneyness > 0.0 {
            sign * discount
        } else {
            0.0
        });
    }

    let forward = forward + displacement;
    let strike = strike + displacement;

    if strike == 0.0 {
        return Ok(match option_type {
            OptionType::Call => discount,
            OptionType::Put => 0.0,
        });
    }

    let d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(sign * phi.value(sign * d1) * discount)
}

/// Risk-neutral probability of exercise in the bond martingale measure, `N(d2)`.
pub fn black_formula_cash_itm_probability(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;

    let sign = sign_of(option_type);

    if std_dev == 0.0 {
        return Ok(if forward * sign > strike * sign {
            1.0
        } else {
            0.0
        });
    }

    let forward = forward + displacement;
    let strike = strike + displacement;
    if strike == 0.0 {
        return Ok(match option_type {
            OptionType::Call => 1.0,
            OptionType::Put => 0.0,
        });
    }
    let d2 = (forward / strike).ln() / std_dev - 0.5 * std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(phi.value(sign * d2))
}

/// Risk-neutral probability of exercise in the asset martingale measure, `N(d1)`.
pub fn black_formula_asset_itm_probability(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;

    let sign = sign_of(option_type);

    if std_dev == 0.0 {
        return Ok(if forward * sign > strike * sign {
            1.0
        } else {
            0.0
        });
    }

    let forward = forward + displacement;
    let strike = strike + displacement;
    if strike == 0.0 {
        return Ok(match option_type {
            OptionType::Call => 1.0,
            OptionType::Put => 0.0,
        });
    }
    let d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(phi.value(sign * d1))
}

/// Derivative of [`black_formula`] with respect to the standard deviation.
///
/// Multiplying by `sqrt(time_to_maturity)` turns this into the Black vega.
pub fn black_formula_std_dev_derivative(
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;
    check_std_dev_and_discount(std_dev, discount)?;

    let forward = forward + displacement;
    let strike = strike + displacement;

    if std_dev == 0.0 || strike == 0.0 {
        return Ok(0.0);
    }

    let d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(discount * forward * phi.derivative(d1))
}

/// Derivative of [`black_formula`] with respect to the implied volatility.
///
/// This is the Black vega: [`black_formula_std_dev_derivative`] times
/// `sqrt(expiry)`.
pub fn black_formula_vol_derivative(
    strike: Real,
    forward: Real,
    std_dev: Real,
    expiry: Real,
    discount: Real,
    displacement: Real,
) -> QlResult<Real> {
    let derivative =
        black_formula_std_dev_derivative(strike, forward, std_dev, discount, displacement)?;
    Ok(derivative * expiry.sqrt())
}

/// Second derivative of [`black_formula`] with respect to the standard deviation.
pub fn black_formula_std_dev_second_derivative(
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
    displacement: Real,
) -> QlResult<Real> {
    check_parameters(strike, forward, displacement)?;
    check_std_dev_and_discount(std_dev, discount)?;

    let forward = forward + displacement;
    let strike = strike + displacement;

    if std_dev == 0.0 || strike == 0.0 {
        return Ok(0.0);
    }

    let d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
    let d1_prime = -(forward / strike).ln() / (std_dev * std_dev) + 0.5;
    let density = NormalDistribution::standard();
    Ok(discount * forward * density.derivative(d1) * d1_prime)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HULL_FORWARD: Real = 44.15338604779301;
    const HULL_DISCOUNT: Real = 0.951229424500714;
    const HULL_STD_DEV: Real = 0.14142135623730953;

    fn assert_close(actual: Real, expected: Real, tolerance: Real) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual {actual} vs expected {expected} (tolerance {tolerance})"
        );
    }

    #[test]
    fn known_values_match_black_scholes() {
        let call = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let put = black_formula(
            OptionType::Put,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        assert_close(call, 4.759422392871536, 1e-10);
        assert_close(put, 0.8085993729000926, 1e-10);
    }

    #[test]
    fn put_call_parity_holds() {
        for strike in [20.0, 40.0, 44.15338604779301, 60.0] {
            let call = black_formula(
                OptionType::Call,
                strike,
                HULL_FORWARD,
                HULL_STD_DEV,
                HULL_DISCOUNT,
                0.0,
            )
            .expect("valid inputs");
            let put = black_formula(
                OptionType::Put,
                strike,
                HULL_FORWARD,
                HULL_STD_DEV,
                HULL_DISCOUNT,
                0.0,
            )
            .expect("valid inputs");
            assert_close(call - put, HULL_DISCOUNT * (HULL_FORWARD - strike), 1e-12);
        }
    }

    #[test]
    fn zero_std_dev_returns_discounted_intrinsic() {
        let call = black_formula(OptionType::Call, 40.0, 44.0, 0.0, 0.95, 0.0).expect("valid");
        assert_close(call, 0.95 * 4.0, 1e-15);
        let put = black_formula(OptionType::Put, 40.0, 44.0, 0.0, 0.95, 0.0).expect("valid");
        assert_close(put, 0.0, 0.0);
    }

    #[test]
    fn zero_strike_prices_the_forward() {
        let call = black_formula(OptionType::Call, 0.0, 44.0, 0.2, 0.95, 0.0).expect("valid");
        assert_close(call, 44.0 * 0.95, 1e-15);
        let put = black_formula(OptionType::Put, 0.0, 44.0, 0.2, 0.95, 0.0).expect("valid");
        assert_close(put, 0.0, 0.0);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(black_formula(OptionType::Call, 40.0, 44.0, -0.1, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, 44.0, 0.1, 0.0, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, 44.0, Real::NAN, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, Real::NAN, 0.1, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, -1.0, 44.0, 0.1, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, -44.0, 0.1, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, 44.0, 0.1, 0.95, -0.01).is_err());
    }

    fn assert_forward_derivative_consistency(option_type: OptionType, strikes: &[Real], vol: Real) {
        let forward = 1.0;
        let tte: Real = 10.0;
        let std_dev = vol * tte.sqrt();
        let discount = 0.95;
        let displacement = 0.01;
        let bump = 0.0001;
        let epsilon = 1.0e-10;

        for &strike in strikes {
            let delta = black_formula_forward_derivative(
                option_type,
                strike,
                forward,
                std_dev,
                discount,
                displacement,
            )
            .expect("valid inputs");
            let bumped_delta = black_formula_forward_derivative(
                option_type,
                strike,
                forward + bump,
                std_dev,
                discount,
                displacement,
            )
            .expect("valid inputs");

            let base_premium = black_formula(
                option_type,
                strike,
                forward,
                std_dev,
                discount,
                displacement,
            )
            .expect("valid inputs");
            let bumped_premium = black_formula(
                option_type,
                strike,
                forward + bump,
                std_dev,
                discount,
                displacement,
            )
            .expect("valid inputs");
            let delta_approx = (bumped_premium - base_premium) / bump;

            assert!(
                delta.max(bumped_delta) + epsilon > delta_approx
                    && delta_approx > delta.min(bumped_delta) - epsilon,
                "forward derivative inconsistent with bump for {option_type} at strike {strike}: \
                 analytical {delta}, approximated {delta_approx}"
            );
        }
    }

    #[test]
    fn forward_derivative_is_consistent_with_bumping() {
        let strikes = [0.1, 0.5, 1.0, 2.0, 3.0];
        assert_forward_derivative_consistency(OptionType::Call, &strikes, 0.1);
        assert_forward_derivative_consistency(OptionType::Put, &strikes, 0.1);
    }

    #[test]
    fn forward_derivative_is_consistent_with_bumping_at_zero_strike() {
        assert_forward_derivative_consistency(OptionType::Call, &[0.0], 0.1);
        assert_forward_derivative_consistency(OptionType::Put, &[0.0], 0.1);
    }

    #[test]
    fn forward_derivative_is_consistent_with_bumping_at_zero_volatility() {
        let strikes = [0.1, 0.5, 1.0, 2.0, 3.0];
        assert_forward_derivative_consistency(OptionType::Call, &strikes, 0.0);
        assert_forward_derivative_consistency(OptionType::Put, &strikes, 0.0);
    }

    #[test]
    fn std_dev_derivative_matches_bumped_value() {
        let bump = 1.0e-6;
        let up = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV + bump,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let down = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV - bump,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let analytical =
            black_formula_std_dev_derivative(40.0, HULL_FORWARD, HULL_STD_DEV, HULL_DISCOUNT, 0.0)
                .expect("valid inputs");
        assert_close((up - down) / (2.0 * bump), analytical, 1e-6);

        let wide = 1.0e-4;
        let up_wide = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV + wide,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let down_wide = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV - wide,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let second = black_formula_std_dev_second_derivative(
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        let base = black_formula(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV,
            HULL_DISCOUNT,
            0.0,
        )
        .expect("valid inputs");
        assert_close(
            (up_wide - 2.0 * base + down_wide) / (wide * wide),
            second,
            1e-4,
        );
    }

    #[test]
    fn vol_derivative_scales_by_sqrt_expiry() {
        let std_dev_derivative =
            black_formula_std_dev_derivative(40.0, HULL_FORWARD, HULL_STD_DEV, HULL_DISCOUNT, 0.0)
                .expect("valid inputs");
        let vol_derivative =
            black_formula_vol_derivative(40.0, HULL_FORWARD, HULL_STD_DEV, 0.5, HULL_DISCOUNT, 0.0)
                .expect("valid inputs");
        assert_close(vol_derivative, std_dev_derivative * 0.5_f64.sqrt(), 1e-12);
        assert_close(vol_derivative, 8.813415059602862, 1e-10);
    }

    #[test]
    fn itm_probabilities_match_normal_quantiles() {
        let cash = black_formula_cash_itm_probability(
            OptionType::Call,
            40.0,
            HULL_FORWARD,
            HULL_STD_DEV,
            0.0,
        )
        .expect("valid inputs");
        assert_close(cash, 0.7349460368459086, 1e-10);
    }

    #[test]
    fn asset_itm_probability_is_continuous_at_zero_std_dev() {
        for (option_type, forward) in [
            (OptionType::Call, 44.0),
            (OptionType::Call, 36.0),
            (OptionType::Put, 44.0),
            (OptionType::Put, 36.0),
        ] {
            let limit = black_formula_asset_itm_probability(option_type, 40.0, forward, 1e-12, 0.0)
                .expect("valid inputs");
            let at_zero = black_formula_asset_itm_probability(option_type, 40.0, forward, 0.0, 0.0)
                .expect("valid inputs");
            assert_close(at_zero, limit, 1e-9);
        }
    }
}
