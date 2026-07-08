//! Black 1976 calculator.
//!
//! Port of `ql/pricingengines/blackcalculator.{hpp,cpp}`: a
//! [`BlackCalculator`] prices a European payoff on a forward and exposes the
//! full greek set. Only the plain-vanilla payoff is supported; the C++
//! visitor dispatch on the other striked payoffs follows those payoffs as a
//! follow-up, as do `strike_gamma`, `vanna` and `volga`.
//!
//! One deviation from the C++ reference: its zero-volatility branches detect
//! the option type with `alpha_ >= 0`, which misreads an out-of-the-money put
//! (`alpha_ == 0` exactly) as a call and hands it in-the-money call greeks.
//! The port dispatches on the stored option type, implementing the values the
//! reference's own comments state; tests lock them.

use crate::errors::QlResult;
use crate::fail;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
use crate::math::comparison::close;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::option::OptionType;
use crate::types::{Real, Time};

/// Black 1976 pricing and greeks for a plain-vanilla European payoff.
#[derive(Clone, Debug)]
pub struct BlackCalculator {
    option_type: OptionType,
    strike: Real,
    forward: Real,
    std_dev: Real,
    discount: Real,
    variance: Real,
    d1: Real,
    d2: Real,
    alpha: Real,
    beta: Real,
    dalpha_dd1: Real,
    dbeta_dd2: Real,
    cum_d1: Real,
    cum_d2: Real,
    x: Real,
    dx_ds: Real,
    dx_dstrike: Real,
}

impl BlackCalculator {
    /// Builds a calculator for the given option type and strike.
    pub fn new(
        option_type: OptionType,
        strike: Real,
        forward: Real,
        std_dev: Real,
        discount: Real,
    ) -> QlResult<BlackCalculator> {
        if strike.is_nan() || strike < 0.0 {
            fail!("strike ({strike}) must be non-negative");
        }
        if forward.is_nan() || forward <= 0.0 {
            fail!("forward ({forward}) must be positive");
        }
        if std_dev.is_nan() || std_dev < 0.0 {
            fail!("stdDev ({std_dev}) must be non-negative");
        }
        if discount.is_nan() || discount <= 0.0 {
            fail!("discount ({discount}) must be positive");
        }

        let (d1, d2, cum_d1, cum_d2, n_d1, n_d2);
        if std_dev >= Real::EPSILON {
            if close(strike, 0.0) {
                d1 = Real::MAX;
                d2 = Real::MAX;
                cum_d1 = 1.0;
                cum_d2 = 1.0;
                n_d1 = 0.0;
                n_d2 = 0.0;
            } else {
                d1 = (forward / strike).ln() / std_dev + 0.5 * std_dev;
                d2 = d1 - std_dev;
                let f = CumulativeNormalDistribution::standard();
                cum_d1 = f.value(d1);
                cum_d2 = f.value(d2);
                n_d1 = f.derivative(d1);
                n_d2 = f.derivative(d2);
            }
        } else if close(forward, strike) {
            d1 = 0.0;
            d2 = 0.0;
            cum_d1 = 0.5;
            cum_d2 = 0.5;
            n_d1 = (2.0 * std::f64::consts::PI).sqrt().recip();
            n_d2 = n_d1;
        } else if forward > strike {
            d1 = Real::MAX;
            d2 = Real::MAX;
            cum_d1 = 1.0;
            cum_d2 = 1.0;
            n_d1 = 0.0;
            n_d2 = 0.0;
        } else {
            d1 = Real::MIN;
            d2 = Real::MIN;
            cum_d1 = 0.0;
            cum_d2 = 0.0;
            n_d1 = 0.0;
            n_d2 = 0.0;
        }

        let (alpha, dalpha_dd1, beta, dbeta_dd2) = match option_type {
            OptionType::Call => (cum_d1, n_d1, -cum_d2, -n_d2),
            OptionType::Put => (-1.0 + cum_d1, n_d1, 1.0 - cum_d2, -n_d2),
        };

        Ok(BlackCalculator {
            option_type,
            strike,
            forward,
            std_dev,
            discount,
            variance: std_dev * std_dev,
            d1,
            d2,
            alpha,
            beta,
            dalpha_dd1,
            dbeta_dd2,
            cum_d1,
            cum_d2,
            x: strike,
            dx_ds: 0.0,
            dx_dstrike: 1.0,
        })
    }

    /// Builds a calculator from a plain-vanilla payoff.
    pub fn with_payoff(
        payoff: &PlainVanillaPayoff,
        forward: Real,
        std_dev: Real,
        discount: Real,
    ) -> QlResult<BlackCalculator> {
        BlackCalculator::new(
            payoff.option_type(),
            payoff.strike(),
            forward,
            std_dev,
            discount,
        )
    }

    /// Zero-volatility sensitivities by moneyness, negated for puts.
    fn zero_vol_ladder(&self, atm: Real, itm: Real, otm: Real) -> Real {
        let is_call = self.option_type == OptionType::Call;
        let sign = if is_call { 1.0 } else { -1.0 };
        if close(self.forward, self.strike) {
            sign * atm
        } else if (self.forward > self.strike) == is_call {
            sign * itm
        } else {
            sign * otm
        }
    }

    /// Present value of the payoff.
    pub fn value(&self) -> Real {
        self.discount * (self.forward * self.alpha + self.x * self.beta)
    }

    /// Sensitivity to a change in the underlying forward price.
    pub fn delta_forward(&self) -> Real {
        if self.std_dev <= Real::EPSILON {
            return self.zero_vol_ladder(0.5 * self.discount, self.discount, 0.0);
        }

        let temp = self.std_dev * self.forward;
        let dalpha_dforward = self.dalpha_dd1 / temp;
        let dbeta_dforward = self.dbeta_dd2 / temp;
        let temp2 = dalpha_dforward * self.forward + self.alpha + dbeta_dforward * self.x;

        self.discount * temp2
    }

    /// Sensitivity to a change in the underlying spot price.
    pub fn delta(&self, spot: Real) -> QlResult<Real> {
        if spot.is_nan() || spot <= 0.0 {
            fail!("positive spot value required: {spot} not allowed");
        }

        let dforward_ds = self.forward / spot;

        if self.std_dev <= Real::EPSILON {
            return Ok(self.zero_vol_ladder(
                0.5 * self.discount * dforward_ds,
                self.discount * dforward_ds,
                0.0,
            ));
        }

        let temp = self.std_dev * spot;
        let dalpha_ds = self.dalpha_dd1 / temp;
        let dbeta_ds = self.dbeta_dd2 / temp;
        let temp2 = dalpha_ds * self.forward
            + self.alpha * dforward_ds
            + dbeta_ds * self.x
            + self.beta * self.dx_ds;

        Ok(self.discount * temp2)
    }

    /// Percent sensitivity to a percent change in the forward price.
    pub fn elasticity_forward(&self) -> Real {
        let value = self.value();
        let delta = self.delta_forward();
        elasticity_from(value, delta, self.forward)
    }

    /// Percent sensitivity to a percent change in the spot price.
    pub fn elasticity(&self, spot: Real) -> QlResult<Real> {
        let value = self.value();
        let delta = self.delta(spot)?;
        Ok(elasticity_from(value, delta, spot))
    }

    /// Second-order sensitivity to a change in the forward price.
    pub fn gamma_forward(&self) -> Real {
        if self.std_dev <= Real::EPSILON {
            return 0.0;
        }

        let temp = self.std_dev * self.forward;
        let dalpha_dforward = self.dalpha_dd1 / temp;
        let dbeta_dforward = self.dbeta_dd2 / temp;

        let d2alpha_dforward2 = -dalpha_dforward / self.forward * (1.0 + self.d1 / self.std_dev);
        let d2beta_dforward2 = -dbeta_dforward / self.forward * (1.0 + self.d2 / self.std_dev);

        let temp2 =
            d2alpha_dforward2 * self.forward + 2.0 * dalpha_dforward + d2beta_dforward2 * self.x;

        self.discount * temp2
    }

    /// Second-order sensitivity to a change in the spot price.
    pub fn gamma(&self, spot: Real) -> QlResult<Real> {
        if spot.is_nan() || spot <= 0.0 {
            fail!("positive spot value required: {spot} not allowed");
        }

        if self.std_dev <= Real::EPSILON {
            return Ok(0.0);
        }

        let dforward_ds = self.forward / spot;

        let temp = self.std_dev * spot;
        let dalpha_ds = self.dalpha_dd1 / temp;
        let dbeta_ds = self.dbeta_dd2 / temp;

        let d2alpha_ds2 = -dalpha_ds / spot * (1.0 + self.d1 / self.std_dev);
        let d2beta_ds2 = -dbeta_ds / spot * (1.0 + self.d2 / self.std_dev);

        let temp2 = d2alpha_ds2 * self.forward
            + 2.0 * dalpha_ds * dforward_ds
            + d2beta_ds2 * self.x
            + 2.0 * dbeta_ds * self.dx_ds;

        Ok(self.discount * temp2)
    }

    /// Sensitivity to the passage of time.
    pub fn theta(&self, spot: Real, maturity: Time) -> QlResult<Real> {
        if maturity.is_nan() || maturity < 0.0 {
            fail!("maturity ({maturity}) must be non-negative");
        }
        if close(maturity, 0.0) {
            return Ok(0.0);
        }
        Ok(-(self.discount.ln() * self.value()
            + (self.forward / spot).ln() * spot * self.delta(spot)?
            + 0.5 * self.variance * spot * spot * self.gamma(spot)?)
            / maturity)
    }

    /// Sensitivity to the passage of time per day, on a 365-day year.
    pub fn theta_per_day(&self, spot: Real, maturity: Time) -> QlResult<Real> {
        Ok(self.theta(spot, maturity)? / 365.0)
    }

    /// Sensitivity to volatility.
    pub fn vega(&self, maturity: Time) -> QlResult<Real> {
        if maturity.is_nan() || maturity < 0.0 {
            fail!("negative maturity not allowed");
        }

        if self.std_dev <= Real::EPSILON {
            return Ok(0.0);
        }

        let temp = (self.strike / self.forward).ln() / self.variance;
        let dalpha_dsigma = self.dalpha_dd1 * (temp + 0.5);
        let dbeta_dsigma = self.dbeta_dd2 * (temp - 0.5);

        let temp2 = dalpha_dsigma * self.forward + dbeta_dsigma * self.x;

        Ok(self.discount * maturity.sqrt() * temp2)
    }

    /// Sensitivity to the discounting rate.
    pub fn rho(&self, maturity: Time) -> QlResult<Real> {
        if maturity.is_nan() || maturity < 0.0 {
            fail!("negative maturity not allowed");
        }

        if self.std_dev <= Real::EPSILON {
            let delta_forward = self.delta_forward();
            return Ok(maturity * (delta_forward * self.forward - self.value()));
        }

        let dalpha_dr = self.dalpha_dd1 / self.std_dev;
        let dbeta_dr = self.dbeta_dd2 / self.std_dev;
        let temp = dalpha_dr * self.forward + self.alpha * self.forward + dbeta_dr * self.x;

        Ok(maturity * (self.discount * temp - self.value()))
    }

    /// Sensitivity to the dividend or growth rate.
    pub fn dividend_rho(&self, maturity: Time) -> QlResult<Real> {
        if maturity.is_nan() || maturity < 0.0 {
            fail!("negative maturity not allowed");
        }

        if self.std_dev <= Real::EPSILON {
            let delta_forward = self.delta_forward() / self.discount;
            return Ok(-maturity * self.discount * delta_forward * self.forward);
        }

        let dalpha_dq = -self.dalpha_dd1 / self.std_dev;
        let dbeta_dq = -self.dbeta_dd2 / self.std_dev;

        let temp = dalpha_dq * self.forward - self.alpha * self.forward + dbeta_dq * self.x;

        Ok(maturity * self.discount * temp)
    }

    /// Probability of exercise in the bond martingale measure, `N(d2)`.
    pub fn itm_cash_probability(&self) -> Real {
        self.cum_d2
    }

    /// Probability of exercise in the asset martingale measure, `N(d1)`.
    pub fn itm_asset_probability(&self) -> Real {
        self.cum_d1
    }

    /// Sensitivity to the strike.
    pub fn strike_sensitivity(&self) -> Real {
        if self.std_dev <= Real::EPSILON {
            return self.zero_vol_ladder(-0.5 * self.discount, -self.discount, 0.0);
        }

        let temp = self.std_dev * self.strike;
        let dalpha_dstrike = -self.dalpha_dd1 / temp;
        let dbeta_dstrike = -self.dbeta_dd2 / temp;

        let temp2 =
            dalpha_dstrike * self.forward + dbeta_dstrike * self.x + self.beta * self.dx_dstrike;

        self.discount * temp2
    }

    /// `alpha`, the coefficient of the forward in the value formula.
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// `beta`, the coefficient of the strike in the value formula.
    pub fn beta(&self) -> Real {
        self.beta
    }
}

fn elasticity_from(value: Real, delta: Real, underlying: Real) -> Real {
    if value > Real::EPSILON {
        delta / value * underlying
    } else if delta.abs() < Real::EPSILON {
        0.0
    } else if delta > 0.0 {
        Real::MAX
    } else {
        Real::MIN
    }
}
