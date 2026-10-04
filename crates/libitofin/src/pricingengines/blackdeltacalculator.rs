//! Black FX delta and ATM strike conventions from QuantLib.
//!
//! Premium-adjusted calls invert on the high-strike branch, to the right of
//! the maximum delta, as in `ql/pricingengines/blackdeltacalculator.cpp`.
//! Inversion requires positive standard deviation and an interior delta.

use crate::errors::QlResult;
use crate::math::distributions::normal::{CumulativeNormalDistribution, InverseCumulativeNormal};
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::option::OptionType;
use crate::quotes::{AtmType, DeltaType};
use crate::types::{DiscountFactor, Real};
use crate::{fail, require};

/// Black delta calculator with spot, forward and premium-adjusted conventions.
#[derive(Clone, Copy, Debug)]
pub struct BlackDeltaCalculator {
    delta_type: DeltaType,
    f_discount: DiscountFactor,
    std_dev: Real,
    spot: Real,
    forward: Real,
    phi: Real,
    f_exp_pos: Real,
    f_exp_neg: Real,
}

impl BlackDeltaCalculator {
    /// Builds a calculator. `std_dev` is volatility times square root of time.
    ///
    /// # Errors
    /// Rejects non-finite inputs, non-positive spot/discounts, negative deviation,
    /// and derived forwards or ATM strikes outside the finite positive range.
    pub fn new(
        option_type: OptionType,
        delta_type: DeltaType,
        spot: Real,
        d_discount: DiscountFactor,
        f_discount: DiscountFactor,
        std_dev: Real,
    ) -> QlResult<Self> {
        require!(
            spot.is_finite() && spot > 0.0,
            "finite positive spot required"
        );
        require!(
            d_discount.is_finite() && d_discount > 0.0,
            "finite positive domestic discount required"
        );
        require!(
            f_discount.is_finite() && f_discount > 0.0,
            "finite positive foreign discount required"
        );
        require!(
            std_dev.is_finite() && std_dev >= 0.0,
            "finite non-negative standard deviation required"
        );
        let forward = spot * f_discount / d_discount;
        let f_exp_pos = forward * (0.5 * std_dev * std_dev).exp();
        let f_exp_neg = forward * (-0.5 * std_dev * std_dev).exp();
        require!(
            forward.is_finite() && forward > 0.0 && f_exp_pos.is_finite() && f_exp_neg > 0.0,
            "forward or ATM strike outside finite positive range"
        );
        Ok(Self {
            delta_type,
            f_discount,
            std_dev,
            spot,
            forward,
            phi: option_type as i32 as Real,
            f_exp_pos,
            f_exp_neg,
        })
    }

    /// Changes the delta convention.
    pub fn set_delta_type(&mut self, delta_type: DeltaType) {
        self.delta_type = delta_type;
    }

    /// Changes the call/put flag.
    pub fn set_option_type(&mut self, option_type: OptionType) {
        self.phi = option_type as i32 as Real;
    }

    fn scale(&self) -> Real {
        match self.delta_type {
            DeltaType::Spot | DeltaType::PaSpot => self.f_discount,
            DeltaType::Fwd | DeltaType::PaFwd => 1.0,
        }
    }

    fn cumulative(&self, strike_ratio: Real, shift: Real) -> Real {
        if self.std_dev >= Real::EPSILON && strike_ratio > 0.0 {
            CumulativeNormalDistribution::standard()
                .value(self.phi * (-strike_ratio.ln() / self.std_dev + shift * self.std_dev))
        } else if strike_ratio == 1.0 {
            CumulativeNormalDistribution::standard().value(self.phi * shift * self.std_dev)
        } else if (self.phi > 0.0 && strike_ratio < 1.0) || (self.phi < 0.0 && strike_ratio > 1.0) {
            1.0
        } else {
            0.0
        }
    }

    fn normalized_delta(&self, strike_ratio: Real) -> Real {
        match self.delta_type {
            DeltaType::Spot | DeltaType::Fwd => self.phi * self.cumulative(strike_ratio, 0.5),
            DeltaType::PaSpot | DeltaType::PaFwd => {
                self.phi * self.cumulative(strike_ratio, -0.5) * strike_ratio
            }
        }
    }

    /// Calculates signed delta from a non-negative strike, including zero vol.
    ///
    /// # Errors
    /// Rejects non-finite/negative strikes and unrepresentable strike ratios/deltas.
    pub fn delta_from_strike(&self, strike: Real) -> QlResult<Real> {
        require!(
            strike.is_finite() && strike >= 0.0,
            "finite non-negative strike required"
        );
        let ratio = strike / self.forward;
        require!(ratio.is_finite(), "strike ratio outside finite range");
        let delta = self.scale() * self.normalized_delta(ratio);
        require!(delta.is_finite(), "delta outside finite range");
        Ok(delta)
    }

    /// Inverts signed delta. Premium-adjusted calls use the high-strike root.
    ///
    /// # Errors
    /// Rejects incoherent signs, boundary/out-of-range deltas, zero deviation,
    /// infeasible premium-adjusted call deltas, unrepresentable strikes or
    /// a numerically unresolved inverse (relative delta residual above `1e-8`).
    pub fn strike_from_delta(&self, delta: Real) -> QlResult<Real> {
        let probability = self.phi * delta / self.scale();
        require!(
            delta.is_finite() && probability > 0.0 && probability < 1.0,
            "delta must have the option sign and be strictly inside its range"
        );
        if self.std_dev < Real::EPSILON {
            fail!("delta inversion requires positive standard deviation");
        }
        let exponent =
            -self.phi * InverseCumulativeNormal::standard_value(probability)? * self.std_dev
                + 0.5 * self.std_dev * self.std_dev;
        let right = exponent.exp();
        require!(
            right.is_finite() && right > 0.0,
            "strike ratio outside finite positive range"
        );
        let ratio = match self.delta_type {
            DeltaType::Spot | DeltaType::Fwd => right,
            DeltaType::PaSpot | DeltaType::PaFwd => {
                let mut solver = Brent::new().with_max_evaluations(1000);
                let target = delta / self.scale();
                let mut f = |x| self.normalized_delta(x) - target;
                if self.phi < 0.0 {
                    let upper = (right * 2.0).max(100.0);
                    require!(
                        upper.is_finite(),
                        "premium-adjusted inversion bracket overflow"
                    );
                    solver.solve_bracketed(&mut f, 1e-12, right, 0.0, upper)?
                } else {
                    let mut g = |x: Real| {
                        if x == 0.0 {
                            return self.std_dev;
                        }
                        let d2 = -x.ln() / self.std_dev - 0.5 * self.std_dev;
                        let normal = CumulativeNormalDistribution::standard();
                        normal.value(d2) * self.std_dev - normal.derivative(d2)
                    };
                    let left = solver.solve_bracketed(&mut g, 1e-12, right * 0.5, 0.0, right)?;
                    solver.solve_bracketed(
                        &mut f,
                        1e-12,
                        left + (right - left) * 0.5,
                        left,
                        right,
                    )?
                }
            }
        };
        let recovered = self.normalized_delta(ratio);
        let target = delta / self.scale();
        require!(
            recovered.is_finite() && (recovered - target).abs() <= 1e-8 * target.abs(),
            "delta inversion did not resolve the requested delta"
        );
        let strike = self.forward * ratio;
        require!(
            strike.is_finite() && strike > 0.0,
            "strike outside finite positive range"
        );
        Ok(strike)
    }

    /// Calculates ATM strike under the selected convention.
    ///
    /// # Errors
    /// Rejects `Null`, and `PutCall50` unless using unadjusted forward delta.
    pub fn atm_strike(&self, atm_type: AtmType) -> QlResult<Real> {
        match atm_type {
            AtmType::Spot => Ok(self.spot),
            AtmType::Fwd => Ok(self.forward),
            AtmType::DeltaNeutral => Ok(
                if matches!(self.delta_type, DeltaType::Spot | DeltaType::Fwd) {
                    self.f_exp_pos
                } else {
                    self.f_exp_neg
                },
            ),
            AtmType::GammaMax | AtmType::VegaMax => Ok(self.f_exp_pos),
            AtmType::PutCall50 => {
                require!(
                    self.delta_type == DeltaType::Fwd,
                    "put/call 50 ATM requires forward delta"
                );
                Ok(self.f_exp_pos)
            }
            AtmType::Null => fail!("invalid ATM type"),
        }
    }
}

#[cfg(test)]
mod tests;
