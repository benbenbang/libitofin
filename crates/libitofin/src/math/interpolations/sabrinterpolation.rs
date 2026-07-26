//! Concrete SABR smile calibration.
//!
//! Port of `ql/math/interpolations/sabrinterpolation.hpp` (the
//! `XABRInterpolation<SABRSpecs>` template instantiated for SABR) as a concrete
//! calibrator, per D10: the generic XABR framework and the other smile models
//! it can host are deferred to #586.
//!
//! This first layer holds the machinery the optimizer drives: the SABR
//! reparametrization transforms ([`sabr_direct`], [`sabr_inverse`],
//! [`sabr_guess`]) ported verbatim from `SABRSpecs` (sabrinterpolation.hpp:
//! 82-130), and the vol-error cost function ([`SabrCostFunction`]). The exact
//! sqrt/asin/exp transform forms matter: a simpler reparametrization changes
//! which local minima the random restarts escape.
//!
//! ## Scope and divergences
//!
//! - Shift-zero only: `addParams`/`shift` are 0, so [`sabr_guess`]'s
//!   `pow(forward + shift, ...)` becomes `pow(forward, ...)`. Displaced SABR
//!   follows with the shifted vol formula (deferred to #586).
//! - `VolatilityType::ShiftedLognormal` only; the Normal arm is deferred to
//!   #586 in [`unsafe_sabr_volatility`].

// The transforms and cost function below are exercised by this module's tests
// and driven by `SABRInterpolation`, which lands in the next commit of this
// stack; the allow is removed there once the driver consumes them.
#![allow(dead_code)]

use crate::math::array::Array;
use crate::math::optimization::costfunction::CostFunction;
use crate::termstructures::volatility::{VolatilityType, unsafe_sabr_volatility};
use crate::types::{Rate, Real, Time};

const EPS1: Real = 1.0e-7;
const EPS2: Real = 0.9999;

/// Map unconstrained optimizer coordinates to SABR parameters.
///
/// Verbatim port of `SABRSpecs::direct` (sabrinterpolation.hpp:115-130): the
/// image is always a valid SABR point (`alpha > 0`, `beta in (0, 1]`,
/// `nu > 0`, `|rho| <= eps2 < 1`), so the optimizer never leaves the feasible
/// region.
pub(crate) fn sabr_direct(x: &Array) -> Array {
    let y0 = if x[0].abs() < 5.0 {
        x[0] * x[0] + EPS1
    } else {
        10.0 * x[0].abs() - 25.0 + EPS1
    };
    let y1 = if x[1].abs() < (-EPS1.ln()).sqrt() {
        (-(x[1] * x[1])).exp()
    } else {
        EPS1
    };
    let y2 = if x[2].abs() < 5.0 {
        x[2] * x[2] + EPS1
    } else {
        10.0 * x[2].abs() - 25.0 + EPS1
    };
    let y3 = if x[3].abs() < 2.5 * std::f64::consts::PI {
        EPS2 * x[3].sin()
    } else {
        EPS2 * if x[3] > 0.0 { 1.0 } else { -1.0 }
    };
    Array::from([y0, y1, y2, y3])
}

/// Map SABR parameters to unconstrained optimizer coordinates.
///
/// Verbatim port of `SABRSpecs::inverse` (sabrinterpolation.hpp:103-113); the
/// left inverse of [`sabr_direct`] on the valid SABR domain.
pub(crate) fn sabr_inverse(y: &Array) -> Array {
    let x0 = if y[0] < 25.0 + EPS1 {
        (y[0] - EPS1).sqrt()
    } else {
        (y[0] - EPS1 + 25.0) / 10.0
    };
    let x1 = (-(y[1].ln())).sqrt();
    let x2 = if y[2] < 25.0 + EPS1 {
        (y[2] - EPS1).sqrt()
    } else {
        (y[2] - EPS1 + 25.0) / 10.0
    };
    let x3 = (y[3] / EPS2).asin();
    Array::from([x0, x1, x2, x3])
}

/// Draw a random SABR starting point from a low-discrepancy sample.
///
/// Verbatim port of `SABRSpecs::guess` (sabrinterpolation.hpp:82-99) at shift
/// zero. `r` supplies one value per free parameter, consumed in the order
/// beta, alpha, nu, rho; fixed entries of `values` are left untouched (the
/// caller re-pins them). `values[1]` (beta) is read while adapting alpha, so
/// the beta-before-alpha consumption order is load-bearing.
pub(crate) fn sabr_guess(values: &mut Array, param_is_fixed: &[bool], forward: Rate, r: &[Real]) {
    let mut draws = r.iter().copied();
    let mut next = || {
        draws
            .next()
            .expect("guess needs one draw per free parameter")
    };
    if !param_is_fixed[1] {
        values[1] = (1.0 - 2.0e-6) * next() + 1.0e-6;
    }
    if !param_is_fixed[0] {
        values[0] = (1.0 - 2.0e-6) * next() + 1.0e-6;
        if values[1] < 0.999 {
            values[0] *= forward.powf(1.0 - values[1]);
        }
    }
    if !param_is_fixed[2] {
        values[2] = 1.5 * next() + 1.0e-6;
    }
    if !param_is_fixed[3] {
        values[3] = (2.0 * next() - 1.0) * (1.0 - 1.0e-6);
    }
}

/// The SABR implied vol at `strike` for the given real (already `direct`ed)
/// parameters `[alpha, beta, nu, rho]`.
///
/// The `expect` holds because [`sabr_direct`]'s image is always a valid SABR
/// point and the shifted-lognormal branch of [`unsafe_sabr_volatility`] is
/// infallible (only the deferred Normal branch returns `Err`).
pub(crate) fn model_vol(
    params: &Array,
    strike: Rate,
    forward: Rate,
    expiry_time: Time,
    volatility_type: VolatilityType,
) -> Real {
    unsafe_sabr_volatility(
        strike,
        forward,
        expiry_time,
        params[0],
        params[1],
        params[2],
        params[3],
        volatility_type,
    )
    .expect("shifted-lognormal SABR volatility is infallible")
}

/// Least-squares vol-error cost over the SABR parameters.
///
/// Port of `XABRInterpolationImpl::XABRError` for SABR. The optimizer coordinate
/// `x` is the unconstrained (inverse-transformed) full parameter vector;
/// [`values`](CostFunction::values) applies [`sabr_direct`] to recover the real
/// parameters, then returns the weighted vol residuals
/// `(model_vol_i - vol_i) * sqrt(weight_i)`. Unlike QuantLib's `XABRError` it
/// stores no state: the driver recomputes the final parameters from the
/// optimizer's result, so the cost function is a plain immutable borrow.
pub(crate) struct SabrCostFunction<'a> {
    pub(crate) strikes: &'a [Real],
    pub(crate) vols: &'a [Real],
    pub(crate) weights: &'a [Real],
    pub(crate) forward: Rate,
    pub(crate) expiry_time: Time,
    pub(crate) volatility_type: VolatilityType,
}

impl SabrCostFunction<'_> {
    fn model_vols(&self, x: &Array) -> Array {
        let params = sabr_direct(x);
        self.strikes
            .iter()
            .map(|&k| {
                model_vol(
                    &params,
                    k,
                    self.forward,
                    self.expiry_time,
                    self.volatility_type,
                )
            })
            .collect()
    }
}

impl CostFunction for SabrCostFunction<'_> {
    fn values(&self, x: &Array) -> Array {
        let model = self.model_vols(x);
        (0..self.strikes.len())
            .map(|i| (model[i] - self.vols[i]) * self.weights[i].sqrt())
            .collect()
    }

    fn value(&self, x: &Array) -> Real {
        let model = self.model_vols(x);
        (0..self.strikes.len())
            .map(|i| {
                let error = model[i] - self.vols[i];
                error * error * self.weights[i]
            })
            .sum()
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    const FORWARD: Rate = 0.039;
    const EXPIRY: Time = 1.0;
    const TRUE_PARAMS: [Real; 4] = [0.3, 0.6, 0.02, 0.01];

    #[test]
    fn direct_is_a_left_inverse_of_inverse_on_valid_params() {
        let params = Array::from(TRUE_PARAMS);
        let round_trip = sabr_direct(&sabr_inverse(&params));
        for i in 0..4 {
            assert!(
                (round_trip[i] - params[i]).abs() < 1e-12,
                "param {i}: {} vs {}",
                round_trip[i],
                params[i]
            );
        }
    }

    #[test]
    fn direct_maps_into_the_valid_sabr_domain() {
        for x in [
            Array::from([0.4, 0.9, 0.14, 0.0]),
            Array::from([12.0, 3.0, -20.0, 10.0]),
            Array::from([-8.0, -0.2, 6.0, -9.0]),
        ] {
            let y = sabr_direct(&x);
            assert!(y[0] > 0.0, "alpha must be positive: {}", y[0]);
            assert!(y[1] > 0.0 && y[1] <= 1.0, "beta out of (0, 1]: {}", y[1]);
            assert!(y[2] > 0.0, "nu must be positive: {}", y[2]);
            assert!(y[3] * y[3] < 1.0, "rho^2 must be < 1: {}", y[3]);
        }
    }

    #[test]
    fn guess_consumes_draws_in_beta_alpha_nu_rho_order() {
        let mut values = Array::from([0.0, 0.0, 0.0, 0.0]);
        let all_free = [false, false, false, false];
        let r = [0.1, 0.2, 0.3, 0.4];
        sabr_guess(&mut values, &all_free, FORWARD, &r);
        let beta = (1.0 - 2.0e-6) * r[0] + 1.0e-6;
        let mut alpha = (1.0 - 2.0e-6) * r[1] + 1.0e-6;
        alpha *= FORWARD.powf(1.0 - beta);
        let nu = 1.5 * r[2] + 1.0e-6;
        let rho = (2.0 * r[3] - 1.0) * (1.0 - 1.0e-6);
        assert!((values[0] - alpha).abs() < 1e-15);
        assert!((values[1] - beta).abs() < 1e-15);
        assert!((values[2] - nu).abs() < 1e-15);
        assert!((values[3] - rho).abs() < 1e-15);
    }

    #[test]
    fn guess_leaves_fixed_entries_untouched_and_reads_fixed_beta() {
        let mut values = Array::from([0.0, 0.6, 0.0, 0.01]);
        let fixed = [false, true, false, true];
        let r = [0.2, 0.3];
        sabr_guess(&mut values, &fixed, FORWARD, &r);
        assert_eq!(values[1], 0.6);
        assert_eq!(values[3], 0.01);
        let alpha = ((1.0 - 2.0e-6) * r[0] + 1.0e-6) * FORWARD.powf(1.0 - 0.6);
        assert!((values[0] - alpha).abs() < 1e-15);
        assert!((values[2] - (1.5 * r[1] + 1.0e-6)).abs() < 1e-15);
    }

    #[test]
    fn cost_residuals_vanish_at_the_generating_parameters() {
        let strikes = [0.03, 0.05, 0.07, 0.09];
        let vols: Vec<Real> = strikes
            .iter()
            .map(|&k| {
                model_vol(
                    &Array::from(TRUE_PARAMS),
                    k,
                    FORWARD,
                    EXPIRY,
                    VolatilityType::ShiftedLognormal,
                )
            })
            .collect();
        let weights = vec![0.25; 4];
        let cost = SabrCostFunction {
            strikes: &strikes,
            vols: &vols,
            weights: &weights,
            forward: FORWARD,
            expiry_time: EXPIRY,
            volatility_type: VolatilityType::ShiftedLognormal,
        };
        let residuals = cost.values(&sabr_inverse(&Array::from(TRUE_PARAMS)));
        for r in residuals.iter() {
            assert!(r.abs() < 1e-12, "residual not zero at true params: {r}");
        }
        assert!(cost.value(&sabr_inverse(&Array::from(TRUE_PARAMS))) < 1e-20);
    }
}
