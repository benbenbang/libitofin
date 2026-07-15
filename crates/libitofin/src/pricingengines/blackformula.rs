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
//! The Bachelier (normal-model) pricing pair, [`bachelier_black_formula`] and
//! its forward derivative [`bachelier_black_formula_forward_derivative`], sit
//! beside the Black family; the optionlet volatility surface selects between
//! the two through its volatility type. Unlike the lognormal family the normal
//! model admits negative forwards and strikes and applies no displacement, so
//! these two functions deliberately skip [`check_parameters`]: they validate
//! only the standard deviation and discount, exactly as the C++ reference does.
//!
//! Out of scope, left as follow-ups with the quotes that need them: the
//! implied-standard-deviation family (approximations and solvers), including
//! its Bachelier variants.
//!
//! One deviation from the C++ reference: at `std_dev == 0` the reference's
//! `blackFormulaAssetItmProbability` tests `forward * sign < strike * sign`,
//! which inverts its own `std_dev -> 0` limit (`N(sign * d1) -> 1` exactly
//! when `sign * (forward - strike) > 0`) and the cash probability next to it.
//! The port uses the limit off the money; exactly at the money it returns
//! 0.0 like both C++ probability branches, where the limit would be 0.5.
//! Tests lock the off-the-money continuity and the at-the-money convention.

use crate::errors::QlResult;
use crate::fail;
use crate::math::distributions::normal::{CumulativeNormalDistribution, NormalDistribution};
use crate::option::OptionType;
use crate::types::Real;

/// QuantLib's `checkParameters` (`blackformula.cpp:44,47,50`): the
/// `displacement >= 0`, `strike + displacement >= 0` and
/// `forward + displacement > 0` requirements, kept intact.
///
/// Divergence: the standalone finiteness checks on `strike` and `forward`, and
/// the `!is_finite()` clauses replacing C++'s implicit NaN handling. In C++ a
/// NaN argument fails every comparison, so `QL_REQUIRE(x >= 0.0)` already
/// throws; an infinite one does not, and `+inf - inf` in the shifted sums then
/// yields NaN downstream. Rejecting both here keeps the failure at the boundary.
fn check_parameters(strike: Real, forward: Real, displacement: Real) -> QlResult<()> {
    if !displacement.is_finite() || displacement < 0.0 {
        fail!("displacement ({displacement}) must be non-negative");
    }
    if !strike.is_finite() {
        fail!("strike ({strike}) must be finite");
    }
    if !forward.is_finite() {
        fail!("forward ({forward}) must be finite");
    }
    let shifted_strike = strike + displacement;
    if !shifted_strike.is_finite() || shifted_strike < 0.0 {
        fail!("strike + displacement ({strike} + {displacement}) must be non-negative");
    }
    let shifted_forward = forward + displacement;
    if !shifted_forward.is_finite() || shifted_forward <= 0.0 {
        fail!("forward + displacement ({forward} + {displacement}) must be positive");
    }
    Ok(())
}

fn check_std_dev_and_discount(std_dev: Real, discount: Real) -> QlResult<()> {
    check_std_dev(std_dev)?;
    if !discount.is_finite() || discount <= 0.0 {
        fail!("discount ({discount}) must be positive");
    }
    Ok(())
}

/// QuantLib's `QL_REQUIRE(stdDev >= 0.0)` (`blackformula.cpp:67`), extended to
/// reject `+inf`.
///
/// Divergence: `blackFormulaCashItmProbability` and
/// `blackFormulaAssetItmProbability` call only `checkParameters` and never
/// validate `stdDev`, so a negative one silently flips the sign of `d2`. This
/// port applies the same check there as in `black_formula`.
fn check_std_dev(std_dev: Real) -> QlResult<()> {
    if !std_dev.is_finite() || std_dev < 0.0 {
        fail!("stdDev ({std_dev}) must be non-negative");
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
    check_std_dev(std_dev)?;

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
    check_std_dev(std_dev)?;

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

/// Bachelier (normal-model) value of a European option on the given forward.
///
/// Port of `bachelierBlackFormula` (`blackformula.cpp:705`). With
/// `d = (forward - strike) * sign` and `h = d / std_dev`, the premium is
/// `discount * (std_dev * phi(h) + d * Phi(h))`, where `phi`/`Phi` are the
/// standard normal density and cumulative distribution. The normal model
/// prices negative forwards and strikes, so only `std_dev` and `discount` are
/// validated; the terminal non-negativity check mirrors C++'s `QL_ENSURE`.
pub fn bachelier_black_formula(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
) -> QlResult<Real> {
    check_std_dev_and_discount(std_dev, discount)?;

    let sign = sign_of(option_type);
    let d = (forward - strike) * sign;

    if std_dev == 0.0 {
        return Ok(discount * d.max(0.0));
    }

    let h = d / std_dev;
    let phi = CumulativeNormalDistribution::standard();
    let result = discount * (std_dev * phi.derivative(h) + d * phi.value(h));
    if result.is_nan() || result < 0.0 {
        fail!(
            "negative value ({result}) for {std_dev} stdDev, {option_type} option, \
             {strike} strike, {forward} forward"
        );
    }
    Ok(result)
}

/// Derivative of [`bachelier_black_formula`] with respect to the forward.
///
/// Port of `bachelierBlackFormulaForwardDerivative` (`blackformula.cpp:738`),
/// which equals `sign * Phi(h) * discount`. At `std_dev == 0` the reference
/// collapses this to `sign * max(boost_sign((forward - strike) * sign), 0) *
/// discount`; `boost::math::sign` yields `0` at the money, so the derivative
/// is `0` exactly when `forward == strike` (distinct from `f64::signum`, which
/// never returns zero).
pub fn bachelier_black_formula_forward_derivative(
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
) -> QlResult<Real> {
    check_std_dev_and_discount(std_dev, discount)?;

    let sign = sign_of(option_type);
    let moneyness = (forward - strike) * sign;

    if std_dev == 0.0 {
        let boost_sign: Real = if moneyness > 0.0 {
            1.0
        } else if moneyness < 0.0 {
            -1.0
        } else {
            0.0
        };
        return Ok(sign * boost_sign.max(0.0) * discount);
    }

    let h = moneyness / std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(sign * phi.value(h) * discount)
}

/// Derivative of [`bachelier_black_formula`] with respect to the standard
/// deviation.
///
/// Port of `bachelierBlackFormulaStdDevDerivative` (`blackformula.cpp:923`),
/// which equals `discount * phi((forward - strike) / std_dev)` where `phi` is
/// the standard normal density. The normal model prices negative forwards and
/// strikes, so only `std_dev` and `discount` are validated. Multiplying by
/// `sqrt(exercise_time)` turns this into the Bachelier vega.
pub fn bachelier_black_formula_std_dev_derivative(
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
) -> QlResult<Real> {
    check_std_dev_and_discount(std_dev, discount)?;

    if std_dev == 0.0 {
        return Ok(0.0);
    }

    let d1 = (forward - strike) / std_dev;
    let phi = CumulativeNormalDistribution::standard();
    Ok(discount * phi.derivative(d1))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::pricingengines::hull_fixture::{
        DISCOUNT as HULL_DISCOUNT, FORWARD as HULL_FORWARD, STD_DEV as HULL_STD_DEV,
    };

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

    #[test]
    fn itm_probabilities_at_the_money_keep_the_zero_convention() {
        for option_type in [OptionType::Call, OptionType::Put] {
            let asset = black_formula_asset_itm_probability(option_type, 40.0, 40.0, 0.0, 0.0)
                .expect("valid inputs");
            assert_close(asset, 0.0, 0.0);
            let cash = black_formula_cash_itm_probability(option_type, 40.0, 40.0, 0.0, 0.0)
                .expect("valid inputs");
            assert_close(cash, 0.0, 0.0);
        }
    }

    #[test]
    fn itm_probabilities_reject_invalid_standard_deviation() {
        assert!(
            black_formula_cash_itm_probability(OptionType::Call, 40.0, 44.0, -0.1, 0.0).is_err()
        );
        assert!(
            black_formula_asset_itm_probability(OptionType::Call, 40.0, 44.0, -0.1, 0.0).is_err()
        );
        assert!(
            black_formula_cash_itm_probability(OptionType::Call, 40.0, 44.0, Real::NAN, 0.0)
                .is_err()
        );
        assert!(
            black_formula_asset_itm_probability(OptionType::Call, 40.0, 44.0, Real::NAN, 0.0)
                .is_err()
        );
        assert!(
            black_formula_cash_itm_probability(OptionType::Call, 40.0, 44.0, Real::INFINITY, 0.0)
                .is_err()
        );
    }

    #[test]
    fn black_formula_rejects_non_finite_inputs() {
        assert!(black_formula(OptionType::Call, Real::INFINITY, 44.0, 0.2, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, Real::INFINITY, 0.2, 0.95, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, 44.0, 0.2, Real::INFINITY, 0.0).is_err());
        assert!(black_formula(OptionType::Call, 40.0, 44.0, 0.2, 0.95, Real::INFINITY).is_err());
    }

    #[test]
    fn bachelier_put_call_parity_is_exact() {
        // C - P == discount * (forward - strike) for the normal model, derived from
        // C - P = discount * [d*Phi(h) + d*(1 - Phi(h))] with d = forward - strike.
        for (forward, strike) in [(0.03, 0.02), (0.02, 0.02), (-0.01, 0.01), (0.05, -0.02)] {
            for std_dev in [0.001, 0.02, 0.1] {
                let call =
                    bachelier_black_formula(OptionType::Call, strike, forward, std_dev, 0.95)
                        .expect("valid inputs");
                let put = bachelier_black_formula(OptionType::Put, strike, forward, std_dev, 0.95)
                    .expect("valid inputs");
                assert_close(call - put, 0.95 * (forward - strike), 1e-15);
            }
        }
    }

    #[test]
    fn bachelier_zero_volatility_is_discounted_intrinsic() {
        for (forward, strike) in [(0.03, 0.02), (0.01, 0.02), (-0.02, -0.01)] {
            let call = bachelier_black_formula(OptionType::Call, strike, forward, 0.0, 0.95)
                .expect("valid inputs");
            let put = bachelier_black_formula(OptionType::Put, strike, forward, 0.0, 0.95)
                .expect("valid inputs");
            assert_close(call, 0.95 * (forward - strike).max(0.0), 0.0);
            assert_close(put, 0.95 * (strike - forward).max(0.0), 0.0);
        }
    }

    #[test]
    fn bachelier_at_the_money_matches_the_closed_form() {
        // At forward == strike, d = 0 and h = 0, so the premium collapses to
        // discount * std_dev * phi(0) = discount * std_dev / sqrt(2*pi).
        for std_dev in [0.001, 0.05, 0.2] {
            let expected = 0.95 * std_dev / (2.0 * std::f64::consts::PI).sqrt();
            for option_type in [OptionType::Call, OptionType::Put] {
                let premium = bachelier_black_formula(option_type, 1.0, 1.0, std_dev, 0.95)
                    .expect("valid inputs");
                assert_close(premium, expected, 1e-16);
            }
        }
    }

    #[test]
    fn bachelier_matches_the_normal_pricing_definition() {
        // premium = discount * (std_dev * phi(h) + d * Phi(h)), d = (forward-strike)*sign,
        // h = d / std_dev, reconstructed from the normal primitives directly.
        let phi = CumulativeNormalDistribution::standard();
        for (forward, strike) in [(0.03, 0.025), (-0.01, 0.005), (0.02, 0.02)] {
            for std_dev in [0.001, 0.01, 0.05] {
                for option_type in [OptionType::Call, OptionType::Put] {
                    let sign = Real::from(option_type as i32);
                    let d = (forward - strike) * sign;
                    let h = d / std_dev;
                    let expected = 0.95 * (std_dev * phi.derivative(h) + d * phi.value(h));
                    let actual =
                        bachelier_black_formula(option_type, strike, forward, std_dev, 0.95)
                            .expect("valid inputs");
                    assert_close(actual, expected, 1e-16);
                }
            }
        }
    }

    #[test]
    fn bachelier_prices_negative_forward_and_strike() {
        let premium = bachelier_black_formula(OptionType::Call, -0.005, -0.01, 0.01, 1.0)
            .expect("normal model admits negative rates");
        assert!(premium > 0.0);
    }

    #[test]
    fn bachelier_rejects_invalid_std_dev_and_discount() {
        assert!(bachelier_black_formula(OptionType::Call, 1.0, 1.0, -0.1, 0.95).is_err());
        assert!(bachelier_black_formula(OptionType::Call, 1.0, 1.0, Real::INFINITY, 0.95).is_err());
        assert!(bachelier_black_formula(OptionType::Call, 1.0, 1.0, 0.1, 0.0).is_err());
        assert!(bachelier_black_formula(OptionType::Call, 1.0, 1.0, 0.1, -1.0).is_err());
        assert!(
            bachelier_black_formula_forward_derivative(OptionType::Call, 1.0, 1.0, -0.1, 0.95)
                .is_err()
        );
        assert!(
            bachelier_black_formula_forward_derivative(OptionType::Call, 1.0, 1.0, 0.1, 0.0)
                .is_err()
        );
    }

    fn assert_bachelier_forward_derivative(option_type: OptionType, strikes: &[Real], bpvol: Real) {
        // Mean-value-theorem check ported from blackformula.cpp:357: the analytical
        // forward derivative must bracket the bumped finite difference of the premium.
        let forward = 1.0;
        let tte: Real = 10.0;
        let std_dev = bpvol * tte.sqrt();
        let discount = 0.95;
        let bump = 0.0001;
        let epsilon = 1.0e-10;
        for &strike in strikes {
            let delta = bachelier_black_formula_forward_derivative(
                option_type,
                strike,
                forward,
                std_dev,
                discount,
            )
            .expect("valid inputs");
            let bumped_delta = bachelier_black_formula_forward_derivative(
                option_type,
                strike,
                forward + bump,
                std_dev,
                discount,
            )
            .expect("valid inputs");
            let base = bachelier_black_formula(option_type, strike, forward, std_dev, discount)
                .expect("valid inputs");
            let bumped =
                bachelier_black_formula(option_type, strike, forward + bump, std_dev, discount)
                    .expect("valid inputs");
            let delta_approx = (bumped - base) / bump;
            assert!(delta.max(bumped_delta) + epsilon > delta_approx);
            assert!(delta_approx > delta.min(bumped_delta) - epsilon);
        }
    }

    #[test]
    fn bachelier_forward_derivative_brackets_the_bumped_premium() {
        let strikes = [-3.0, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 3.0];
        assert_bachelier_forward_derivative(OptionType::Call, &strikes, 0.001);
        assert_bachelier_forward_derivative(OptionType::Put, &strikes, 0.001);
    }

    #[test]
    fn bachelier_forward_derivative_brackets_the_bumped_premium_zero_vol() {
        let strikes = [-3.0, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 3.0];
        assert_bachelier_forward_derivative(OptionType::Call, &strikes, 0.0);
        assert_bachelier_forward_derivative(OptionType::Put, &strikes, 0.0);
    }

    #[test]
    fn bachelier_std_dev_derivative_matches_bumped_value() {
        let forward = 1.0;
        let std_dev = 0.001 * 10.0_f64.sqrt();
        let discount = 0.95;
        let bump = 1.0e-6;
        for strike in [-1.0, 0.0, 0.9, 1.0, 1.1, 2.0] {
            let up = bachelier_black_formula(
                OptionType::Call,
                strike,
                forward,
                std_dev + bump,
                discount,
            )
            .expect("valid inputs");
            let down = bachelier_black_formula(
                OptionType::Call,
                strike,
                forward,
                std_dev - bump,
                discount,
            )
            .expect("valid inputs");
            let analytical =
                bachelier_black_formula_std_dev_derivative(strike, forward, std_dev, discount)
                    .expect("valid inputs");
            assert_close((up - down) / (2.0 * bump), analytical, 1e-6);
        }
    }

    #[test]
    fn bachelier_std_dev_derivative_edge_cases() {
        assert_eq!(
            bachelier_black_formula_std_dev_derivative(1.0, 1.0, 0.0, 0.95).expect("zero std dev"),
            0.0
        );
        assert!(bachelier_black_formula_std_dev_derivative(1.0, 1.0, -0.1, 0.95).is_err());
        assert!(bachelier_black_formula_std_dev_derivative(1.0, 1.0, 0.1, 0.0).is_err());
    }
}
