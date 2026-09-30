//! Fixed-parameter GARCH(1,1) filtering for a series of returns.

use crate::errors::QlResult;
use crate::math::chart::ChartSeries;
use crate::require;
use crate::types::Real;

/// A stationary GARCH(1,1) model parameterized by long-run variance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Garch11 {
    alpha: Real,
    beta: Real,
    omega: Real,
}

/// Conditional volatility at each return and the variance forecast after the last return.
#[derive(Clone, Debug, PartialEq)]
pub struct Garch11Filter {
    pub conditional_volatility: ChartSeries,
    pub next_variance: Real,
}

impl Garch11 {
    /// Builds a stationary model with `omega = (1 - alpha - beta) * long_run_variance`.
    ///
    /// # Errors
    /// Returns an error for nonfinite or negative parameters, or `alpha + beta >= 1`.
    pub fn new(alpha: Real, beta: Real, long_run_variance: Real) -> QlResult<Self> {
        require!(
            alpha.is_finite() && alpha >= 0.0,
            "GARCH alpha must be finite and nonnegative"
        );
        require!(
            beta.is_finite() && beta >= 0.0,
            "GARCH beta must be finite and nonnegative"
        );
        require!(
            long_run_variance.is_finite() && long_run_variance >= 0.0,
            "GARCH long-run variance must be finite and nonnegative"
        );
        require!(
            (alpha + beta).total_cmp(&1.0).is_lt(),
            "GARCH alpha plus beta must be below one"
        );
        let omega = (1.0 - alpha - beta) * long_run_variance;
        require!(omega.is_finite(), "nonfinite GARCH intercept");
        Ok(Self { alpha, beta, omega })
    }

    /// Forecasts the next conditional variance from the latest return and variance.
    ///
    /// # Errors
    /// Returns an error for invalid inputs or a nonfinite update.
    pub fn forecast(&self, return_value: Real, current_variance: Real) -> QlResult<Real> {
        require!(return_value.is_finite(), "nonfinite GARCH return");
        require!(
            current_variance.is_finite() && current_variance >= 0.0,
            "GARCH current variance must be finite and nonnegative"
        );
        let squared_return = return_value * return_value;
        require!(squared_return.is_finite(), "GARCH return square overflow");
        let next_variance = self.omega + self.alpha * squared_return + self.beta * current_variance;
        require!(next_variance.is_finite(), "GARCH variance update overflow");
        Ok(next_variance)
    }

    /// Filters returns using the first squared return as the initial variance.
    /// `conditional_volatility[0]` is a zero placeholder; the first valid value
    /// uses return zero and appears at index one. The final return contributes to
    /// `next_variance`, which is in variance units rather than volatility units.
    ///
    /// # Errors
    /// Returns an error for empty or nonfinite returns, squared-return overflow,
    /// or a nonfinite variance update.
    pub fn filter(&self, returns: &[Real]) -> QlResult<Garch11Filter> {
        require!(!returns.is_empty(), "GARCH returns must not be empty");
        for (index, value) in returns.iter().enumerate() {
            require!(value.is_finite(), "nonfinite GARCH return at index {index}");
            require!(
                (value * value).is_finite(),
                "GARCH return square overflow at index {index}"
            );
        }
        let mut conditional_volatility = ChartSeries::zeroed(returns.len(), 1)?;
        let mut variance = returns[0] * returns[0];
        for index in 1..returns.len() {
            variance = self.forecast(returns[index - 1], variance)?;
            conditional_volatility.values[index] = variance.sqrt();
        }
        let next_variance = self.forecast(returns[returns.len() - 1], variance)?;
        Ok(Garch11Filter {
            conditional_volatility,
            next_variance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Garch11 {
        Garch11::new(0.2, 0.3, 0.4).unwrap()
    }

    #[test]
    fn quantlib_fixed_parameter_fixture_and_forecast() {
        let result = model().filter(&[0.1, 0.1, 0.1]).unwrap();
        assert_eq!(result.conditional_volatility.first_valid, 1);
        assert_eq!(result.conditional_volatility.values[0], 0.0);
        assert_eq!(result.conditional_volatility.get(0), None);
        assert!((result.conditional_volatility.values[1] - 0.452_769_256_906_870_87).abs() < 1e-14);
        assert!((result.conditional_volatility.values[2] - 0.513_322_510_708_423_9).abs() < 1e-14);
        assert!((result.next_variance - 0.281_05).abs() < 1e-14);
        assert_eq!(result.next_variance, model().forecast(0.1, 0.2635).unwrap());
    }

    #[test]
    fn one_return_produces_only_a_forecast() {
        let result = model().filter(&[0.1]).unwrap();
        assert_eq!(result.conditional_volatility.values, [0.0]);
        assert_eq!(result.conditional_volatility.first_valid, 1);
        assert!((result.next_variance - 0.205).abs() < 1e-14);
    }

    #[test]
    fn quantlib_longer_fixture() {
        let result = model().filter(&[0.1; 10]).unwrap();
        assert!((result.conditional_volatility.values[9] - 0.537_183_344_352_745_2).abs() < 1e-14);
        assert!((result.next_variance - 0.288_569_783_635).abs() < 1e-14);
    }

    #[test]
    fn differing_returns_use_the_preceding_observation() {
        let result = model().filter(&[0.1, 0.2, 0.3]).unwrap();
        assert!((result.conditional_volatility.values[1] - 0.205_f64.sqrt()).abs() < 1e-14);
        assert!((result.conditional_volatility.values[2] - 0.2695_f64.sqrt()).abs() < 1e-14);
        assert!((result.next_variance - 0.29885).abs() < 1e-14);
    }

    #[test]
    fn rejects_invalid_parameters() {
        for (alpha, beta, long_run_variance) in [
            (-0.1, 0.2, 0.4),
            (f64::NAN, 0.2, 0.4),
            (0.2, f64::INFINITY, 0.4),
            (0.2, -0.1, 0.4),
            (0.2, 0.3, -0.4),
            (0.2, 0.3, f64::INFINITY),
            (0.2, 0.8, 0.4),
            (1.0, 0.0, 0.4),
        ] {
            assert!(Garch11::new(alpha, beta, long_run_variance).is_err());
        }
        assert!(Garch11::new(0.0, 0.0, 0.0).is_ok());
    }

    #[test]
    fn rejects_invalid_returns_and_variances() {
        let model = model();
        assert!(model.filter(&[]).is_err());
        for returns in [
            vec![f64::NAN],
            vec![0.1, f64::INFINITY],
            vec![f64::MAX],
            vec![0.1, f64::MAX],
        ] {
            assert!(model.filter(&returns).is_err());
        }
        for variance in [f64::NAN, f64::INFINITY, -0.1] {
            assert!(model.forecast(0.1, variance).is_err());
        }
        assert!(model.forecast(f64::NAN, 0.1).is_err());
        assert!(model.forecast(f64::MAX, 0.1).is_err());
    }
}
