//! Probability distributions ported from `ql/math/distributions/`.
//!
//! QuantLib models each distribution as its own class with bespoke methods
//! (`operator()` for the density, a separate cumulative class for the CDF, and
//! an inverse-cumulative class for the quantile). We instead factor those
//! responsibilities into small capability traits - [`Density`], [`Cdf`],
//! [`Quantile`] and [`Support`] - so a type implements exactly the operations
//! it can support, and the probability argument to [`Quantile`] is carried by
//! the validated [`Probability`] newtype rather than a raw [`Real`].

use crate::errors::{QlError, QlResult};
use crate::fail;
use crate::types::Real;

pub mod gamma;
pub mod normal;
pub mod studentt;

/// A probability value, validated to lie in the closed interval `[0, 1]`.
///
/// Constructed through [`TryFrom<Real>`], which validates once; every consumer
/// (e.g. [`Quantile::quantile`]) then takes a `Probability` and is relieved of
/// re-checking the range.
///
/// # Examples
///
/// ```
/// use itofin::math::distributions::Probability;
/// assert_eq!(Probability::try_from(0.25)?.value(), 0.25);
/// assert!(Probability::try_from(1.5).is_err());
/// assert!(Probability::try_from(f64::NAN).is_err());
/// # Ok::<(), itofin::errors::QlError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Probability(Real);

impl Probability {
    /// The underlying value, guaranteed to lie in `[0, 1]`.
    pub fn value(self) -> Real {
        self.0
    }
}

impl TryFrom<Real> for Probability {
    type Error = QlError;

    fn try_from(value: Real) -> QlResult<Self> {
        // `contains` is false for NaN, so this also rejects NaN (see beta.rs).
        if !(0.0..=1.0).contains(&value) {
            fail!("invalid probability: {value}");
        }
        Ok(Self(value))
    }
}

/// A distribution with a probability density (or mass) function.
pub trait Density {
    /// The density at `x`.
    fn pdf(&self, x: Real) -> Real;

    /// The natural log of the density at `x`.
    ///
    /// Provided so log-likelihood consumers can avoid underflow. The default
    /// is `pdf(x).ln()`; implementations whose density is naturally computed
    /// in log space should override this and define [`pdf`](Density::pdf) as
    /// `ln_pdf(x).exp()` instead.
    fn ln_pdf(&self, x: Real) -> Real {
        self.pdf(x).ln()
    }
}

/// A distribution with a cumulative distribution function.
pub trait Cdf {
    /// `P(X <= x)`.
    fn cdf(&self, x: Real) -> Real;

    /// The survival (complementary) function `P(X > x) = 1 - cdf(x)`.
    ///
    /// The default subtracts from one; implementations with a more accurate
    /// tail (e.g. a symmetric distribution, where `survival(x) = cdf(-x)`)
    /// should override it.
    fn survival(&self, x: Real) -> Real {
        1.0 - self.cdf(x)
    }
}

/// A distribution whose CDF can be inverted to a quantile.
pub trait Quantile {
    /// The smallest `x` with `cdf(x) >= p`, i.e. the generalized inverse CDF.
    ///
    /// # Errors
    ///
    /// Returns an error if the inversion cannot be carried out (for example a
    /// solver that fails to converge for the requested probability).
    fn quantile(&self, p: Probability) -> QlResult<Real>;
}

/// A distribution with a known support interval.
pub trait Support {
    /// The greatest lower bound of the support (may be [`Real::NEG_INFINITY`]).
    fn lower_bound(&self) -> Real;

    /// The least upper bound of the support (may be [`Real::INFINITY`]).
    fn upper_bound(&self) -> Real;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_accepts_unit_interval() {
        for p in [0.0, 0.25, 0.5, 1.0] {
            assert_eq!(Probability::try_from(p).unwrap().value(), p);
        }
    }

    #[test]
    fn probability_rejects_out_of_range_and_nonfinite() {
        for p in [-0.1, 1.1, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(Probability::try_from(p).is_err(), "should reject {p}");
        }
    }

    // Minimal implementor exercising only the default trait methods.
    struct Unit;

    impl Density for Unit {
        fn pdf(&self, _x: Real) -> Real {
            0.5
        }
    }

    impl Cdf for Unit {
        fn cdf(&self, x: Real) -> Real {
            x
        }
    }

    #[test]
    fn ln_pdf_default_is_ln_of_pdf() {
        assert_eq!(Unit.ln_pdf(0.0), 0.5_f64.ln());
    }

    #[test]
    fn survival_default_is_complement() {
        assert_eq!(Unit.survival(0.3), 0.7);
    }
}
