//! Concrete SABR smile calibration.
//!
//! Port of `ql/math/interpolations/sabrinterpolation.hpp` (the
//! `XABRInterpolation<SABRSpecs>` template instantiated for SABR) as a concrete
//! calibrator, per D10: the generic XABR framework and the other smile models
//! it can host are deferred to #586.
//!
//! [`SABRInterpolation`] fits the four SABR parameters (alpha, beta, nu, rho)
//! to a strike/vol smile by least squares, optionally holding any subset fixed
//! and vega-weighting the fit, with a Halton random-restart loop to escape
//! local minima. It is driven by the SABR reparametrization transforms
//! ([`sabr_direct`], [`sabr_inverse`], [`sabr_guess`]) ported verbatim from
//! `SABRSpecs` (sabrinterpolation.hpp:82-130) and the vol-error cost function
//! ([`SabrCostFunction`]). The exact sqrt/asin/exp transform forms matter: a
//! simpler reparametrization changes which local minima the restarts escape.
//!
//! ## Scope and divergences
//!
//! - Shift-zero only: `addParams`/`shift` are 0, so [`sabr_guess`]'s
//!   `pow(forward + shift, ...)` becomes `pow(forward, ...)`. Displaced SABR
//!   follows with the shifted vol formula (deferred to #586).
//! - `VolatilityType::ShiftedLognormal` only; the Normal arm is deferred to
//!   #586 in [`unsafe_sabr_volatility`].
//! - `useMaxError` (ranking restarts by max rather than RMS error) is deferred
//!   to #586; the RMS arm is used.
//! - `HaltonRsg::new` is the deterministic (`randomStart=false`) arm (#587), so
//!   the restart points are the plain low-discrepancy sequence, not QuantLib's
//!   `HaltonRsg(free, 42)` Mersenne-offset stream. The optimizer is passed to
//!   [`SABRInterpolation::update`] rather than stored in the constructor, since
//!   `OptimizationMethod::minimize` needs `&mut self`.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::math::optimization::projectedcostfunction::ProjectedCostFunction;
use crate::math::randomnumbers::haltonrsg::HaltonRsg;
use crate::pricingengines::blackformula::black_formula_std_dev_derivative;
use crate::termstructures::volatility::{VolatilityType, unsafe_sabr_volatility};
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

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

/// The RMS interpolation error `sqrt(n * sum_i w_i (v_i - m_i)^2 / (n - 1))`
/// over real (already `direct`ed) `params`, matching
/// `XABRInterpolationImpl::interpolationError`.
fn interpolation_rms_error(
    params: &Array,
    strikes: &[Real],
    vols: &[Real],
    weights: &[Real],
    forward: Rate,
    expiry_time: Time,
    volatility_type: VolatilityType,
) -> Real {
    let n = strikes.len();
    let squared: Real = (0..n)
        .map(|i| {
            let error =
                model_vol(params, strikes[i], forward, expiry_time, volatility_type) - vols[i];
            error * error * weights[i]
        })
        .sum();
    let denom = if n == 1 { 1.0 } else { (n - 1) as Real };
    (n as Real * squared / denom).sqrt()
}

/// The maximum absolute vol error over `params`, matching
/// `XABRInterpolationImpl::interpolationMaxError`.
fn interpolation_max_error(
    params: &Array,
    strikes: &[Real],
    vols: &[Real],
    forward: Rate,
    expiry_time: Time,
    volatility_type: VolatilityType,
) -> Real {
    strikes
        .iter()
        .zip(vols)
        .map(|(&k, &v)| (model_vol(params, k, forward, expiry_time, volatility_type) - v).abs())
        .fold(Real::MIN, Real::max)
}

/// SABR smile calibrated to a strike/vol table.
///
/// Concrete port of `SABRInterpolation` (sabrinterpolation.hpp:150).
/// [`new`](SABRInterpolation::new) stores the smile and the guesses;
/// [`update`](SABRInterpolation::update) runs the fit, holding any subset of
/// {alpha, beta, nu, rho} fixed at its guess. Per-restart errors are measured
/// at the returned optimum, where QuantLib reads the stale model left by the
/// last cost evaluation; this keeps [`SabrCostFunction`] a stateless borrow.
pub struct SABRInterpolation {
    strikes: Vec<Real>,
    vols: Vec<Real>,
    expiry_time: Time,
    forward: Rate,
    params: [Real; 4],
    param_is_fixed: [bool; 4],
    vega_weighted: bool,
    end_criteria: EndCriteria,
    error_accept: Real,
    max_guesses: usize,
    volatility_type: VolatilityType,
    weights: Vec<Real>,
    rms_error: Real,
    max_error: Real,
    end_criteria_result: EndCriteriaType,
}

impl SABRInterpolation {
    /// Builds an uncalibrated SABR smile over `strikes`/`vols`. The four
    /// `*_guess` values seed the fit; a fixed parameter stays pinned at its
    /// guess. Call [`update`](SABRInterpolation::update) to calibrate.
    ///
    /// # Errors
    ///
    /// Fails if `strikes`/`vols` differ in length or are empty, if `expiry_time`
    /// or `forward` is not positive, or if `volatility_type` is
    /// [`VolatilityType::Normal`] (deferred to #586).
    #[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(
        strikes: Vec<Real>,
        vols: Vec<Real>,
        expiry_time: Time,
        forward: Rate,
        alpha_guess: Real,
        beta_guess: Real,
        nu_guess: Real,
        rho_guess: Real,
        alpha_is_fixed: bool,
        beta_is_fixed: bool,
        nu_is_fixed: bool,
        rho_is_fixed: bool,
        vega_weighted: bool,
        end_criteria: EndCriteria,
        error_accept: Real,
        max_guesses: usize,
        volatility_type: VolatilityType,
    ) -> QlResult<Self> {
        require!(!strikes.is_empty(), "strikes must not be empty");
        require!(
            strikes.len() == vols.len(),
            "strikes and volatilities must have the same length"
        );
        require!(
            expiry_time > 0.0,
            "expiry time must be positive: {expiry_time} not allowed"
        );
        require!(
            forward > 0.0,
            "forward must be positive: {forward} not allowed"
        );
        if volatility_type == VolatilityType::Normal {
            fail!("normal (Bachelier) SABR calibration is not yet ported (deferred to #586)");
        }
        Ok(SABRInterpolation {
            strikes,
            vols,
            expiry_time,
            forward,
            params: [alpha_guess, beta_guess, nu_guess, rho_guess],
            param_is_fixed: [alpha_is_fixed, beta_is_fixed, nu_is_fixed, rho_is_fixed],
            vega_weighted,
            end_criteria,
            error_accept,
            max_guesses,
            volatility_type,
            weights: Vec::new(),
            rms_error: Real::NAN,
            max_error: Real::NAN,
            end_criteria_result: EndCriteriaType::None,
        })
    }

    /// Runs the vega-weight, least-squares calibration with the Halton
    /// random-restart loop, using `method` as the optimizer and keeping the
    /// lowest-error restart. A fully fixed parameter set skips optimization.
    ///
    /// # Errors
    ///
    /// Propagates weight-computation and optimizer failures.
    pub fn update(&mut self, method: &mut dyn OptimizationMethod) -> QlResult<()> {
        let n = self.strikes.len();
        let weights = if self.vega_weighted {
            let mut w = Vec::with_capacity(n);
            let mut sum = 0.0;
            for i in 0..n {
                let std_dev = (self.vols[i] * self.vols[i] * self.expiry_time).sqrt();
                let weight = black_formula_std_dev_derivative(
                    self.strikes[i],
                    self.forward,
                    std_dev,
                    1.0,
                    0.0,
                )?;
                w.push(weight);
                sum += weight;
            }
            for weight in &mut w {
                *weight /= sum;
            }
            w
        } else {
            vec![1.0 / n as Real; n]
        };
        self.weights = weights.clone();

        if self.param_is_fixed.iter().all(|&fixed| fixed) {
            let params = Array::from(self.params);
            self.rms_error = interpolation_rms_error(
                &params,
                &self.strikes,
                &self.vols,
                &weights,
                self.forward,
                self.expiry_time,
                self.volatility_type,
            );
            self.max_error = interpolation_max_error(
                &params,
                &self.strikes,
                &self.vols,
                self.forward,
                self.expiry_time,
                self.volatility_type,
            );
            self.end_criteria_result = EndCriteriaType::None;
            return Ok(());
        }

        let cost = SabrCostFunction {
            strikes: &self.strikes,
            vols: &self.vols,
            weights: &weights,
            forward: self.forward,
            expiry_time: self.expiry_time,
            volatility_type: self.volatility_type,
        };
        let free = self.param_is_fixed.iter().filter(|&&fixed| !fixed).count();
        let mut halton = HaltonRsg::new(free)?;
        let mut guess = Array::from(self.params);
        let mut best_error = Real::MAX;
        let mut best_params = Array::from(self.params);
        let mut best_end = EndCriteriaType::None;
        let mut iterations = 0usize;
        loop {
            if iterations > 0 {
                let draw = halton.next_sequence();
                sabr_guess(&mut guess, &self.param_is_fixed, self.forward, draw);
                for i in 0..4 {
                    if self.param_is_fixed[i] {
                        guess[i] = self.params[i];
                    }
                }
            }
            let inversed = sabr_inverse(&guess);
            let projected_cost =
                ProjectedCostFunction::new(&cost, &inversed, self.param_is_fixed.to_vec())?;
            let projected_guess = projected_cost.project(&inversed);
            let constraint = NoConstraint;
            let mut problem = Problem::new(&projected_cost, &constraint, projected_guess);
            let end = method.minimize(&mut problem, &self.end_criteria)?;
            let result = sabr_direct(&projected_cost.include(problem.current_value()));
            let error = interpolation_rms_error(
                &result,
                &self.strikes,
                &self.vols,
                &weights,
                self.forward,
                self.expiry_time,
                self.volatility_type,
            );
            if error < best_error {
                best_error = error;
                best_params = result;
                best_end = end;
            }
            iterations += 1;
            if iterations >= self.max_guesses || error <= self.error_accept {
                break;
            }
        }

        let final_rms = interpolation_rms_error(
            &best_params,
            &self.strikes,
            &self.vols,
            &weights,
            self.forward,
            self.expiry_time,
            self.volatility_type,
        );
        let final_max = interpolation_max_error(
            &best_params,
            &self.strikes,
            &self.vols,
            self.forward,
            self.expiry_time,
            self.volatility_type,
        );
        self.params = [
            best_params[0],
            best_params[1],
            best_params[2],
            best_params[3],
        ];
        self.rms_error = final_rms;
        self.max_error = final_max;
        self.end_criteria_result = best_end;
        Ok(())
    }

    /// The calibrated (or pre-`update` guessed) alpha.
    pub fn alpha(&self) -> Real {
        self.params[0]
    }

    /// The calibrated beta.
    pub fn beta(&self) -> Real {
        self.params[1]
    }

    /// The calibrated nu.
    pub fn nu(&self) -> Real {
        self.params[2]
    }

    /// The calibrated rho.
    pub fn rho(&self) -> Real {
        self.params[3]
    }

    /// The RMS calibration error (`NaN` before `update`).
    pub fn rms_error(&self) -> Real {
        self.rms_error
    }

    /// The maximum absolute vol error (`NaN` before `update`).
    pub fn max_error(&self) -> Real {
        self.max_error
    }

    /// The end criterion of the retained restart.
    pub fn end_criteria(&self) -> EndCriteriaType {
        self.end_criteria_result
    }

    /// The option expiry, in years.
    pub fn expiry(&self) -> Time {
        self.expiry_time
    }

    /// The forward.
    pub fn forward(&self) -> Rate {
        self.forward
    }

    /// The normalized fit weights (empty before `update`).
    pub fn interpolation_weights(&self) -> &[Real] {
        &self.weights
    }

    /// The SABR implied vol at `strike` for the current parameters.
    pub fn volatility(&self, strike: Rate) -> Real {
        model_vol(
            &Array::from(self.params),
            strike,
            self.forward,
            self.expiry_time,
            self.volatility_type,
        )
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

#[cfg(test)]
mod calibration_tests {
    use super::*;
    use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
    use crate::math::optimization::simplex::Simplex;

    const FORWARD: Rate = 0.039;
    const EXPIRY: Time = 1.0;
    const ALPHA: Real = 0.3;
    const BETA: Real = 0.6;
    const NU: Real = 0.02;
    const RHO: Real = 0.01;

    /// The 31-strike/vol smile generated at `(0.3, 0.6, 0.02, 0.01)` from
    /// `interpolations.cpp testSabrInterpolation:1377-1402`; the same table
    /// checked analytically in `termstructures::volatility::sabr`'s tests,
    /// duplicated here to keep that fixture test-private.
    fn smile() -> ([Real; 31], [Real; 31]) {
        let strikes = [
            0.03, 0.032, 0.034, 0.036, 0.038, 0.04, 0.042, 0.044, 0.046, 0.048, 0.05, 0.052, 0.054,
            0.056, 0.058, 0.06, 0.062, 0.064, 0.066, 0.068, 0.07, 0.072, 0.074, 0.076, 0.078, 0.08,
            0.082, 0.084, 0.086, 0.088, 0.09,
        ];
        let vols = [
            1.16725837321531,
            1.15226075991385,
            1.13829711098834,
            1.12524190877505,
            1.11299079244474,
            1.10145609357162,
            1.09056348513411,
            1.08024942745106,
            1.07045919457758,
            1.06114533019077,
            1.05226642581503,
            1.04378614411707,
            1.03567243073732,
            1.0278968727451,
            1.02043417226345,
            1.01326171139321,
            1.00635919013311,
            0.999708323124949,
            0.993292584155381,
            0.987096989695393,
            0.98110791455717,
            0.975312934134512,
            0.969700688771689,
            0.964260766651027,
            0.958983602256592,
            0.953860388001395,
            0.948882997029509,
            0.944043915545469,
            0.939336183299237,
            0.934753341079515,
            0.930289384251337,
        ];
        (strikes, vols)
    }

    /// Oracle port of `interpolations.cpp testSabrInterpolation:1424-1524`:
    /// recover the four generating parameters to `5e-8` from deliberately wrong
    /// guesses that land in a local minimum without the random-restart loop,
    /// across both optimizers, both vega-weighting flags, and all 16
    /// fixed/free subsets (64 calibrations).
    #[test]
    fn recovers_the_generating_parameters_across_all_64_combinations() {
        let (strikes, vols) = smile();
        let alpha_guess = 0.2_f64.sqrt();
        let beta_guess = 0.5;
        let nu_guess = 0.4_f64.sqrt();
        let rho_guess = 0.0;
        let end_criteria = EndCriteria::new(100_000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap();
        let tolerance = 5e-8;

        let mut simplex = Simplex::new(0.01);
        let mut lm = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
        let mut worst = 0.0_f64;
        for method in [
            &mut simplex as &mut dyn OptimizationMethod,
            &mut lm as &mut dyn OptimizationMethod,
        ] {
            for &vega in &[true, false] {
                for &k_a in &[true, false] {
                    for &k_b in &[true, false] {
                        for &k_n in &[true, false] {
                            for &k_r in &[true, false] {
                                let mut interp = SABRInterpolation::new(
                                    strikes.to_vec(),
                                    vols.to_vec(),
                                    EXPIRY,
                                    FORWARD,
                                    if k_a { ALPHA } else { alpha_guess },
                                    if k_b { BETA } else { beta_guess },
                                    if k_n { NU } else { nu_guess },
                                    if k_r { RHO } else { rho_guess },
                                    k_a,
                                    k_b,
                                    k_n,
                                    k_r,
                                    vega,
                                    end_criteria,
                                    1e-10,
                                    50,
                                    VolatilityType::ShiftedLognormal,
                                )
                                .unwrap();
                                interp.update(&mut *method).unwrap();
                                for (got, want, name) in [
                                    (interp.alpha(), ALPHA, "alpha"),
                                    (interp.beta(), BETA, "beta"),
                                    (interp.nu(), NU, "nu"),
                                    (interp.rho(), RHO, "rho"),
                                ] {
                                    let error = (got - want).abs();
                                    worst = worst.max(error);
                                    assert!(
                                        error < tolerance,
                                        "{name} not recovered (vega={vega} a{k_a} b{k_b} n{k_n} r{k_r}): got {got}, want {want}, err {error}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(worst < tolerance, "worst parameter error {worst}");
    }

    /// Confirm-by-stubbing: with the restart loop disabled (`max_guesses = 1`),
    /// the Simplex fit of the nu-fixed, alpha/beta/rho-free subset stays trapped
    /// in a local minimum and misses the `5e-8` recovery. It is the one combo of
    /// the 64 that fails without restarts, so it proves the restart machinery -
    /// not the initial guess - recovers the parameters. QuantLib's comment
    /// attributes the trap to fixed-subset cases; the all-free fit is recovered
    /// directly by Levenberg-Marquardt and does not exercise the restarts.
    #[test]
    fn without_restarts_a_fixed_subset_stays_in_a_local_minimum() {
        let (strikes, vols) = smile();
        let end_criteria = EndCriteria::new(100_000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap();
        let mut simplex = Simplex::new(0.01);
        let mut interp = SABRInterpolation::new(
            strikes.to_vec(),
            vols.to_vec(),
            EXPIRY,
            FORWARD,
            0.2_f64.sqrt(),
            0.5,
            NU,
            0.0,
            false,
            false,
            true,
            false,
            false,
            end_criteria,
            1e-10,
            1,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap();
        interp.update(&mut simplex).unwrap();
        let worst = [
            (interp.alpha() - ALPHA).abs(),
            (interp.beta() - BETA).abs(),
            (interp.nu() - NU).abs(),
            (interp.rho() - RHO).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max);
        assert!(
            worst > 5e-8,
            "expected the restart-free fixed-nu Simplex fit to miss recovery, but worst error was {worst}"
        );
    }
}
