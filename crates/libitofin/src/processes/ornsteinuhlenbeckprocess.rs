//! Ornstein-Uhlenbeck process.
//!
//! Port of `ql/processes/ornsteinuhlenbeckprocess.{hpp,cpp}`:
//! [`OrnsteinUhlenbeckProcess`] describes the mean-reverting process
//! `dx = speed (level - x) dt + vol dW_t`. The transition mean and variance
//! are known in closed form, so the process overrides the trait's Euler
//! defaults for [`expectation`](StochasticProcess1D::expectation),
//! [`variance`](StochasticProcess1D::variance) and
//! [`std_deviation`](StochasticProcess1D::std_deviation) with the exact
//! analytic forms, exactly as the C++ virtuals do.
//!
//! All inputs are plain scalars, so - unlike the Black-Scholes process - there
//! is nothing to register with: the embedded
//! [`Observable`](crate::patterns::observable::Observable) exists only to
//! satisfy [`AsObservable`] and never fires.

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Time};

/// Ornstein-Uhlenbeck mean-reverting process.
pub struct OrnsteinUhlenbeckProcess {
    x0: Real,
    speed: Real,
    level: Real,
    volatility: Real,
    observable: Shared<Observable>,
}

impl OrnsteinUhlenbeckProcess {
    /// Builds the process `dx = speed (level - x) dt + vol dW_t` starting from
    /// `x0`. Fails on a negative volatility (C++ `QL_REQUIRE`, cpp:37).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(speed: Real, volatility: Real, x0: Real, level: Real) -> QlResult<Self> {
        require!(volatility >= 0.0, "negative volatility given");
        Ok(OrnsteinUhlenbeckProcess {
            x0,
            speed,
            level,
            volatility,
            observable: shared(Observable::new()),
        })
    }

    /// Returns the mean-reversion speed.
    pub fn speed(&self) -> Real {
        self.speed
    }

    /// Returns the volatility.
    pub fn volatility(&self) -> Real {
        self.volatility
    }

    /// Returns the mean-reversion level.
    pub fn level(&self) -> Real {
        self.level
    }
}

impl AsObservable for OrnsteinUhlenbeckProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess1D for OrnsteinUhlenbeckProcess {
    fn x0(&self) -> QlResult<Real> {
        Ok(self.x0)
    }

    fn drift(&self, _t: Time, x: Real) -> QlResult<Real> {
        Ok(self.speed * (self.level - x))
    }

    fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
        Ok(self.volatility)
    }

    fn expectation(&self, _t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        Ok(self.level + (x0 - self.level) * (-self.speed * dt).exp())
    }

    fn variance(&self, _t0: Time, _x0: Real, dt: Time) -> QlResult<Real> {
        if self.speed.abs() < f64::EPSILON.sqrt() {
            Ok(self.volatility * self.volatility * dt)
        } else {
            Ok(0.5 * self.volatility * self.volatility / self.speed
                * (1.0 - (-2.0 * self.speed * dt).exp()))
        }
    }

    fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        Ok(self.variance(t0, x0, dt)?.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED: Real = 0.5;
    const VOL: Real = 0.2;
    const X0: Real = 1.0;
    const LEVEL: Real = 3.0;
    const DT: Time = 0.7;

    fn process() -> OrnsteinUhlenbeckProcess {
        OrnsteinUhlenbeckProcess::new(SPEED, VOL, X0, LEVEL).unwrap()
    }

    #[test]
    fn accessors_and_x0() {
        let p = process();
        assert_eq!(p.speed(), SPEED);
        assert_eq!(p.volatility(), VOL);
        assert_eq!(p.level(), LEVEL);
        assert_eq!(p.x0().unwrap(), X0);
    }

    #[test]
    fn drift_and_diffusion_match_closed_form() {
        let p = process();
        let x = 1.5;
        assert!((p.drift(0.0, x).unwrap() - SPEED * (LEVEL - x)).abs() < 1e-15);
        assert_eq!(p.diffusion(0.0, x).unwrap(), VOL);
    }

    #[test]
    fn expectation_matches_analytic_transition_mean() {
        let p = process();
        let expected = LEVEL + (X0 - LEVEL) * (-SPEED * DT).exp();
        assert!((p.expectation(0.0, X0, DT).unwrap() - expected).abs() < 1e-15);
    }

    #[test]
    fn variance_and_std_deviation_match_analytic_forms() {
        let p = process();
        let expected = 0.5 * VOL * VOL / SPEED * (1.0 - (-2.0 * SPEED * DT).exp());
        let variance = p.variance(0.0, X0, DT).unwrap();
        assert!((variance - expected).abs() < 1e-15);
        assert!((p.std_deviation(0.0, X0, DT).unwrap() - expected.sqrt()).abs() < 1e-15);
    }

    #[test]
    fn small_speed_branch_is_the_algebraic_limit() {
        let below = f64::EPSILON.sqrt() * (1.0 - 1e-3);
        let p = OrnsteinUhlenbeckProcess::new(below, VOL, X0, LEVEL).unwrap();
        assert_eq!(p.variance(0.0, X0, DT).unwrap(), VOL * VOL * DT);
    }

    #[test]
    fn variance_is_continuous_across_the_small_speed_threshold() {
        let threshold = f64::EPSILON.sqrt();
        let below =
            OrnsteinUhlenbeckProcess::new(threshold * (1.0 - 1e-3), VOL, X0, LEVEL).unwrap();
        let above =
            OrnsteinUhlenbeckProcess::new(threshold * (1.0 + 1e-3), VOL, X0, LEVEL).unwrap();
        let v_below = below.variance(0.0, X0, DT).unwrap();
        let v_above = above.variance(0.0, X0, DT).unwrap();
        let relative = (v_below - v_above).abs() / v_below;
        assert!(
            relative < 1e-6,
            "variance discontinuous at threshold: {relative}"
        );
    }

    #[test]
    fn negative_volatility_is_rejected() {
        let err = OrnsteinUhlenbeckProcess::new(SPEED, -1e-9, X0, LEVEL)
            .err()
            .unwrap();
        assert_eq!(err.message(), "negative volatility given");
    }

    #[test]
    fn expectation_override_is_live_not_euler_default() {
        let p = OrnsteinUhlenbeckProcess::new(1.0, VOL, X0, LEVEL).unwrap();
        let dt: Time = 1.0;
        let analytic = p.expectation(0.0, X0, dt).unwrap();
        let euler = X0 + p.drift(0.0, X0).unwrap() * dt;
        assert!(
            (analytic - euler).abs() > 0.5,
            "expectation silently inheriting the Euler default: analytic {analytic}, euler {euler}"
        );
    }

    #[test]
    fn variance_override_is_live_not_euler_default() {
        let p = OrnsteinUhlenbeckProcess::new(1.0, VOL, X0, LEVEL).unwrap();
        let dt: Time = 1.0;
        let analytic = p.variance(0.0, X0, dt).unwrap();
        let euler = {
            let sigma = p.diffusion(0.0, X0).unwrap();
            sigma * sigma * dt
        };
        assert!(
            (analytic - euler).abs() > 0.02,
            "variance silently inheriting the Euler default: analytic {analytic}, euler {euler}"
        );
    }

    #[test]
    fn evolve_routes_through_the_overridden_pieces() {
        let p = process();
        let dw = 0.75;
        let expected =
            p.expectation(0.0, X0, DT).unwrap() + p.std_deviation(0.0, X0, DT).unwrap() * dw;
        assert!((p.evolve(0.0, X0, DT, dw).unwrap() - expected).abs() < 1e-15);
    }
}
