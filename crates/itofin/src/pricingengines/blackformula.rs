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
