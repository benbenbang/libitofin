//! Bivariate copulas ported from `ql/math/copulas/`.
//!
//! QuantLib re-validates the `[0, 1]` arguments inside every `operator()`;
//! here [`Copula::value`] takes the validated
//! [`Probability`](crate::math::distributions::Probability) newtype instead,
//! so evaluation is infallible and parameter checks happen once at
//! construction.

use crate::errors::QlResult;
use crate::math::distributions::Probability;
use crate::math::distributions::bivariatenormal::BivariateCumulativeNormalDistributionWe04DP;
use crate::math::distributions::normal::{CumulativeNormalDistribution, InverseCumulativeNormal};
use crate::require;
use crate::types::Real;

/// A bivariate copula `C(x, y)` on the unit square.
pub trait Copula {
    /// The copula value at `(x, y)`.
    fn value(&self, x: Probability, y: Probability) -> Real;
}

/// Ali-Mikhail-Haq copula. Port of `QuantLib::AliMikhailHaqCopula`.
#[derive(Clone, Copy, Debug)]
pub struct AliMikhailHaqCopula {
    theta: Real,
}

impl AliMikhailHaqCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta` is in `[-1, 1]`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (-1.0..=1.0).contains(&theta),
            "theta ({theta}) must be in [-1,1]"
        );
        Ok(AliMikhailHaqCopula { theta })
    }
}

impl Copula for AliMikhailHaqCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        (x * y) / (1.0 - self.theta * (1.0 - x) * (1.0 - y))
    }
}

/// Clayton copula. Port of `QuantLib::ClaytonCopula`.
///
/// Deliberate divergence from QuantLib: for `theta` in `(-1, 0)` the term
/// `x^-theta + y^-theta - 1` can go negative, where C++ `std::max` propagates
/// the `NaN` produced by `pow`; this port clamps the value to `0.0`, the
/// standard literature definition of the Clayton copula in that region.
#[derive(Clone, Copy, Debug)]
pub struct ClaytonCopula {
    theta: Real,
}

impl ClaytonCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta >= -1` and `theta != 0`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (-1.0..).contains(&theta),
            "theta ({theta}) must be greater or equal to -1"
        );
        require!(theta != 0.0, "theta ({theta}) must be different from 0");
        Ok(ClaytonCopula { theta })
    }
}

impl Copula for ClaytonCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        (x.powf(-self.theta) + y.powf(-self.theta) - 1.0)
            .powf(-1.0 / self.theta)
            .max(0.0)
    }
}

/// Farlie-Gumbel-Morgenstern copula. Port of `QuantLib::FarlieGumbelMorgensternCopula`.
#[derive(Clone, Copy, Debug)]
pub struct FarlieGumbelMorgensternCopula {
    theta: Real,
}

impl FarlieGumbelMorgensternCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta` is in `[-1, 1]`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (-1.0..=1.0).contains(&theta),
            "theta ({theta}) must be in [-1,1]"
        );
        Ok(FarlieGumbelMorgensternCopula { theta })
    }
}

impl Copula for FarlieGumbelMorgensternCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        x * y + self.theta * x * y * (1.0 - x) * (1.0 - y)
    }
}

/// Frank copula. Port of `QuantLib::FrankCopula`.
#[derive(Clone, Copy, Debug)]
pub struct FrankCopula {
    theta: Real,
}

impl FrankCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta != 0`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(theta != 0.0, "theta ({theta}) must be different from 0");
        Ok(FrankCopula { theta })
    }
}

impl Copula for FrankCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        let theta = self.theta;
        -1.0 / theta
            * (1.0
                + ((-theta * x).exp() - 1.0) * ((-theta * y).exp() - 1.0) / ((-theta).exp() - 1.0))
                .ln()
    }
}

/// Galambos copula. Port of `QuantLib::GalambosCopula`.
#[derive(Clone, Copy, Debug)]
pub struct GalambosCopula {
    theta: Real,
}

impl GalambosCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta >= 0`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (0.0..).contains(&theta),
            "theta ({theta}) must be greater or equal to 0"
        );
        Ok(GalambosCopula { theta })
    }
}

impl Copula for GalambosCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        x * y
            * ((-x.ln()).powf(-self.theta) + (-y.ln()).powf(-self.theta))
                .powf(-1.0 / self.theta)
                .exp()
    }
}

/// Gaussian copula. Port of `QuantLib::GaussianCopula`.
#[derive(Clone, Copy, Debug)]
pub struct GaussianCopula {
    bivariate_normal_cdf: BivariateCumulativeNormalDistributionWe04DP,
}

impl GaussianCopula {
    /// # Errors
    ///
    /// Returns an error unless `rho` is in `[-1, 1]`.
    pub fn new(rho: Real) -> QlResult<Self> {
        require!((-1.0..=1.0).contains(&rho), "rho ({rho}) must be in [-1,1]");
        Ok(GaussianCopula {
            bivariate_normal_cdf: BivariateCumulativeNormalDistributionWe04DP::new(rho)?,
        })
    }
}

impl Copula for GaussianCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let inverse = |p: Probability| {
            InverseCumulativeNormal::standard_value(p.value())
                .expect("the standard inverse cumulative normal is defined on [0, 1]")
        };
        self.bivariate_normal_cdf.value(inverse(x), inverse(y))
    }
}

/// Gumbel copula. Port of `QuantLib::GumbelCopula`.
#[derive(Clone, Copy, Debug)]
pub struct GumbelCopula {
    theta: Real,
}

impl GumbelCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta >= 1`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (1.0..).contains(&theta),
            "theta ({theta}) must be greater or equal to 1"
        );
        Ok(GumbelCopula { theta })
    }
}

impl Copula for GumbelCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        (-((-x.ln()).powf(self.theta) + (-y.ln()).powf(self.theta)).powf(1.0 / self.theta)).exp()
    }
}

/// Husler-Reiss copula. Port of `QuantLib::HuslerReissCopula`.
///
/// As in QuantLib, the value at `x == 1` or `y == 1` is `NaN` (the formula
/// takes the log of a negative infinity there).
#[derive(Clone, Copy, Debug)]
pub struct HuslerReissCopula {
    theta: Real,
    cum_normal: CumulativeNormalDistribution,
}

impl HuslerReissCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta >= 0`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (0.0..).contains(&theta),
            "theta ({theta}) must be greater or equal to 0"
        );
        Ok(HuslerReissCopula {
            theta,
            cum_normal: CumulativeNormalDistribution::standard(),
        })
    }
}

impl Copula for HuslerReissCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        let theta = self.theta;
        x.powf(
            self.cum_normal
                .value(1.0 / theta + 0.5 * theta * (x.ln() / y.ln()).ln()),
        ) * y.powf(
            self.cum_normal
                .value(1.0 / theta + 0.5 * theta * (y.ln() / x.ln()).ln()),
        )
    }
}

/// Independent copula `C(x, y) = x y`. Port of `QuantLib::IndependentCopula`.
#[derive(Clone, Copy, Debug, Default)]
pub struct IndependentCopula;

impl Copula for IndependentCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        x.value() * y.value()
    }
}

/// Marshall-Olkin copula. Port of `QuantLib::MarshallOlkinCopula`.
#[derive(Clone, Copy, Debug)]
pub struct MarshallOlkinCopula {
    exponent1: Real,
    exponent2: Real,
}

impl MarshallOlkinCopula {
    /// # Errors
    ///
    /// Returns an error unless both parameters are non-negative.
    pub fn new(a1: Real, a2: Real) -> QlResult<Self> {
        require!(
            (0.0..).contains(&a1),
            "1st parameter ({a1}) must be non-negative"
        );
        require!(
            (0.0..).contains(&a2),
            "2nd parameter ({a2}) must be non-negative"
        );
        Ok(MarshallOlkinCopula {
            exponent1: 1.0 - a1,
            exponent2: 1.0 - a2,
        })
    }
}

impl Copula for MarshallOlkinCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        (y * x.powf(self.exponent1)).min(x * y.powf(self.exponent2))
    }
}

/// Upper Frechet bound `C(x, y) = min(x, y)`. Port of `QuantLib::MaxCopula`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaxCopula;

impl Copula for MaxCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        x.value().min(y.value())
    }
}

/// Lower Frechet bound `C(x, y) = max(x + y - 1, 0)`. Port of `QuantLib::MinCopula`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MinCopula;

impl Copula for MinCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        (x.value() + y.value() - 1.0).max(0.0)
    }
}

/// Plackett copula. Port of `QuantLib::PlackettCopula`.
#[derive(Clone, Copy, Debug)]
pub struct PlackettCopula {
    theta: Real,
}

impl PlackettCopula {
    /// # Errors
    ///
    /// Returns an error unless `theta >= 0` and `theta != 1`.
    pub fn new(theta: Real) -> QlResult<Self> {
        require!(
            (0.0..).contains(&theta),
            "theta ({theta}) must be greater or equal to 0"
        );
        require!(theta != 1.0, "theta ({theta}) must be different from 1");
        Ok(PlackettCopula { theta })
    }
}

impl Copula for PlackettCopula {
    fn value(&self, x: Probability, y: Probability) -> Real {
        let (x, y) = (x.value(), y.value());
        let theta = self.theta;
        let s = 1.0 + (theta - 1.0) * (x + y);
        (s - (s.powi(2) - 4.0 * x * y * theta * (theta - 1.0)).sqrt()) / (2.0 * (theta - 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: Real) -> Probability {
        Probability::try_from(v).unwrap()
    }

    const X: Real = 0.3;
    const Y: Real = 0.7;

    #[test]
    fn matches_reference_values() {
        let cases: Vec<(&str, Box<dyn Copula>, Real)> = vec![
            (
                "ali-mikhail-haq(0.5)",
                Box::new(AliMikhailHaqCopula::new(0.5).unwrap()),
                0.23463687150837986,
            ),
            (
                "clayton(2)",
                Box::new(ClaytonCopula::new(2.0).unwrap()),
                0.2868649025057026,
            ),
            (
                "farlie-gumbel-morgenstern(0.5)",
                Box::new(FarlieGumbelMorgensternCopula::new(0.5).unwrap()),
                0.23205,
            ),
            (
                "frank(2)",
                Box::new(FrankCopula::new(2.0).unwrap()),
                0.24972133337304844,
            ),
            (
                "galambos(1.5)",
                Box::new(GalambosCopula::new(1.5).unwrap()),
                0.2900199444768978,
            ),
            (
                "gumbel(2)",
                Box::new(GumbelCopula::new(2.0).unwrap()),
                0.2848780620209499,
            ),
            (
                "husler-reiss(1.5)",
                Box::new(HuslerReissCopula::new(1.5).unwrap()),
                0.2783507123856862,
            ),
            ("independent", Box::new(IndependentCopula), 0.21),
            (
                "marshall-olkin(0.25, 0.75)",
                Box::new(MarshallOlkinCopula::new(0.25, 0.75).unwrap()),
                0.2744073657686083,
            ),
            ("max", Box::new(MaxCopula), 0.3),
            ("min", Box::new(MinCopula), 0.0),
            (
                "plackett(2)",
                Box::new(PlackettCopula::new(2.0).unwrap()),
                0.2384226894136091,
            ),
        ];
        for (name, copula, expected) in cases {
            let got = copula.value(p(X), p(Y));
            assert!(
                (got - expected).abs() <= 1e-15,
                "{name}: got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn gaussian_copula_composes_inverse_and_bivariate_cdf() {
        let copula = GaussianCopula::new(0.5).unwrap();
        let bvn = BivariateCumulativeNormalDistributionWe04DP::new(0.5).unwrap();
        let a = InverseCumulativeNormal::standard_value(X).unwrap();
        let b = InverseCumulativeNormal::standard_value(Y).unwrap();
        assert_eq!(copula.value(p(X), p(Y)), bvn.value(a, b));

        let frechet_upper = GaussianCopula::new(1.0).unwrap();
        assert!((frechet_upper.value(p(X), p(Y)) - X.min(Y)).abs() <= 1e-7);
    }

    #[test]
    fn independence_special_cases_reduce_to_product() {
        let grid = [0.1, 0.3, 0.5, 0.9];
        for x in grid {
            for y in grid {
                let product = x * y;
                for (copula, tol) in [
                    (
                        Box::new(AliMikhailHaqCopula::new(0.0).unwrap()) as Box<dyn Copula>,
                        1e-15,
                    ),
                    (
                        Box::new(FarlieGumbelMorgensternCopula::new(0.0).unwrap()),
                        1e-15,
                    ),
                    (Box::new(GumbelCopula::new(1.0).unwrap()), 1e-15),
                    (Box::new(GaussianCopula::new(0.0).unwrap()), 1e-8),
                ] {
                    let got = copula.value(p(x), p(y));
                    assert!(
                        (got - product).abs() <= tol,
                        "C({x}, {y}) = {got}, want {product}"
                    );
                }
            }
        }
    }

    #[test]
    fn boundary_values_match_copula_limits() {
        let copulas: Vec<Box<dyn Copula>> = vec![
            Box::new(AliMikhailHaqCopula::new(0.5).unwrap()),
            Box::new(ClaytonCopula::new(2.0).unwrap()),
            Box::new(FarlieGumbelMorgensternCopula::new(0.5).unwrap()),
            Box::new(FrankCopula::new(2.0).unwrap()),
            Box::new(GalambosCopula::new(1.5).unwrap()),
            Box::new(GumbelCopula::new(2.0).unwrap()),
            Box::new(IndependentCopula),
            Box::new(MarshallOlkinCopula::new(0.25, 0.75).unwrap()),
            Box::new(MaxCopula),
            Box::new(MinCopula),
            Box::new(PlackettCopula::new(2.0).unwrap()),
        ];
        for (i, copula) in copulas.iter().enumerate() {
            let lower = copula.value(p(0.0), p(Y));
            assert!(lower.abs() <= 1e-12, "copula #{i}: C(0, y) = {lower}");
            let upper = copula.value(p(1.0), p(Y));
            assert!(
                (upper - Y).abs() <= 1e-12,
                "copula #{i}: C(1, y) = {upper}, want {Y}"
            );
        }

        let husler_reiss = HuslerReissCopula::new(1.5).unwrap();
        assert!(husler_reiss.value(p(0.0), p(Y)).abs() <= 1e-12);
        assert!(husler_reiss.value(p(1.0), p(Y)).is_nan());
    }

    #[test]
    fn clayton_clamps_negative_generator_region_to_zero() {
        let copula = ClaytonCopula::new(-0.4).unwrap();
        assert_eq!(copula.value(p(0.1), p(0.1)), 0.0);
    }

    #[test]
    fn gaussian_boundary_values_match_copula_limits() {
        let copula = GaussianCopula::new(0.5).unwrap();
        let lower = copula.value(p(0.0), p(Y));
        assert!(lower.abs() <= 1e-8, "C(0, y) = {lower}");
        let upper = copula.value(p(1.0), p(Y));
        assert!((upper - Y).abs() <= 1e-8, "C(1, y) = {upper}, want {Y}");
    }

    #[test]
    fn constructors_reject_invalid_parameters() {
        assert!(AliMikhailHaqCopula::new(1.5).is_err());
        assert!(ClaytonCopula::new(-1.5).is_err());
        assert!(ClaytonCopula::new(0.0).is_err());
        assert!(FarlieGumbelMorgensternCopula::new(-1.1).is_err());
        assert!(FrankCopula::new(0.0).is_err());
        assert!(GalambosCopula::new(-0.1).is_err());
        assert!(GaussianCopula::new(1.1).is_err());
        assert!(GumbelCopula::new(0.9).is_err());
        assert!(HuslerReissCopula::new(-0.1).is_err());
        assert!(MarshallOlkinCopula::new(-0.1, 0.5).is_err());
        assert!(MarshallOlkinCopula::new(0.5, -0.1).is_err());
        assert!(PlackettCopula::new(-0.1).is_err());
        assert!(PlackettCopula::new(1.0).is_err());
    }
}
