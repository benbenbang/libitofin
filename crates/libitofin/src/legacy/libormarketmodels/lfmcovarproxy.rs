//! Instantaneous covariance and diffusion for an immutable volatility/correlation pair.
//!
//! Matches the standalone linear-exponential/exponential case of QuantLib's
//! `ql/legacy/libormarketmodels/lfmcovarproxy.{hpp,cpp}`. No integrated covariance,
//! calibration, process connection or arbitrary model polymorphism is provided.

use super::{LmExponentialCorrelationModel, LmLinearExponentialVolatilityModel};
use crate::errors::QlResult;
use crate::math::matrix::Matrix;
use crate::require;
use crate::shared::Shared;
use crate::types::{Size, Time};

/// Checked instantaneous covariance proxy owning shared immutable components.
#[derive(Clone, Debug)]
pub struct LfmCovarianceProxy {
    vola: Shared<LmLinearExponentialVolatilityModel>,
    corr: Shared<LmExponentialCorrelationModel>,
}

impl LfmCovarianceProxy {
    /// Combines two components without changing their forward-row order.
    ///
    /// # Errors
    ///
    /// Rejects mismatched row dimensions.
    pub fn new(
        vola: Shared<LmLinearExponentialVolatilityModel>,
        corr: Shared<LmExponentialCorrelationModel>,
    ) -> QlResult<Self> {
        require!(
            vola.size() == corr.size(),
            "volatility and correlation sizes differ"
        );
        Ok(Self { vola, corr })
    }

    /// Number of forward rows.
    pub fn size(&self) -> Size {
        self.corr.size()
    }

    /// Number of Brownian factors, with no rank reduction.
    pub fn factors(&self) -> Size {
        self.corr.factors()
    }

    /// Borrows the immutable shared volatility model.
    pub fn volatility_model(&self) -> &Shared<LmLinearExponentialVolatilityModel> {
        &self.vola
    }

    /// Borrows the immutable shared correlation model.
    pub fn correlation_model(&self) -> &Shared<LmExponentialCorrelationModel> {
        &self.corr
    }

    /// Returns `diag(volatility) * correlation_factor`.
    ///
    /// Factor orientation is implementation-dependent; compare factor products.
    ///
    /// # Errors
    ///
    /// Rejects non-finite time or unrepresentable volatility/diffusion entries.
    pub fn diffusion(&self, t: Time) -> QlResult<Matrix> {
        let mut factor = self.corr.pseudo_sqrt(t)?.clone();
        let vol = self.vola.volatility(t)?;
        for i in 0..self.size() {
            for entry in factor.row_mut(i) {
                *entry *= vol[i];
                require!(entry.is_finite(), "diffusion entry is unrepresentable");
            }
        }
        Ok(factor)
    }

    /// Returns `diag(volatility) * correlation * diag(volatility)`.
    ///
    /// # Errors
    ///
    /// Rejects non-finite time or unrepresentable volatility/covariance entries.
    pub fn covariance(&self, t: Time) -> QlResult<Matrix> {
        let vol = self.vola.volatility(t)?;
        let corr = self.corr.correlation(t)?;
        let mut result = Matrix::with_size(self.size(), self.size());
        for i in 0..self.size() {
            for j in i..self.size() {
                let value = (vol[i] * vol[j]) * corr[(i, j)];
                require!(value.is_finite(), "covariance entry is unrepresentable");
                result[(i, j)] = value;
                result[(j, i)] = value;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::types::Real;

    fn proxy() -> LfmCovarianceProxy {
        LfmCovarianceProxy::new(
            shared(
                LmLinearExponentialVolatilityModel::new(vec![3.0, 1.0, 2.0], 0.2, 0.4, 0.1, 0.3)
                    .unwrap(),
            ),
            shared(LmExponentialCorrelationModel::new(3, 0.2).unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn covariance_matches_formula_and_diffusion_product() {
        let model = proxy();
        assert_eq!(model.size(), 3);
        assert_eq!(model.factors(), 3);
        assert_eq!(model.volatility_model().fixing_times(), &[3.0, 1.0, 2.0]);
        assert_eq!(model.correlation_model().size(), 3);
        for t in [-1.0, 0.0, 1.0, 2.0, 3.0, 1000.0] {
            let covariance = model.covariance(t).unwrap();
            let diffusion = model.diffusion(t).unwrap();
            let product = &diffusion * &diffusion.transpose();
            let vols = model.volatility_model().volatility(t).unwrap();
            for i in 0..3 {
                for j in 0..3 {
                    let expected = vols[i] * (-0.2 * i.abs_diff(j) as Real).exp() * vols[j];
                    assert!((covariance[(i, j)] - expected).abs() < 1e-15);
                    assert!((product[(i, j)] - expected).abs() < 1e-14);
                    assert_eq!(covariance[(i, j)], covariance[(j, i)]);
                    if model.volatility_model().fixing_times()[i] <= t {
                        assert_eq!(covariance[(i, j)], 0.0);
                        assert_eq!(diffusion[(i, j)], 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let vol =
            shared(LmLinearExponentialVolatilityModel::new(vec![1.0], 1.0, 1.0, 1.0, 1.0).unwrap());
        let corr = shared(LmExponentialCorrelationModel::new(2, 0.2).unwrap());
        assert!(LfmCovarianceProxy::new(vol, corr).is_err());
    }

    #[test]
    fn rejects_nonfinite_times_and_covariance_overflow() {
        for t in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(proxy().covariance(t).is_err());
            assert!(proxy().diffusion(t).is_err());
        }
        let model = LfmCovarianceProxy::new(
            shared(
                LmLinearExponentialVolatilityModel::new(vec![1.0], 1.0, 1.0, 1e200, 1.0).unwrap(),
            ),
            shared(LmExponentialCorrelationModel::new(1, 0.2).unwrap()),
        )
        .unwrap();
        assert!(model.covariance(0.0).is_err());
        assert!(model.diffusion(0.0).unwrap()[(0, 0)].is_finite());
        assert_eq!(model.covariance(1.0).unwrap()[(0, 0)], 0.0);
    }

    #[test]
    fn covariance_avoids_intermediate_underflow_for_disparate_rows() {
        let model = LfmCovarianceProxy::new(
            shared(
                LmLinearExponentialVolatilityModel::new(vec![1000.0, 1.0], 1.0, 1.0, 1e-250, 1e150)
                    .unwrap(),
            ),
            shared(LmExponentialCorrelationModel::new(2, 230.0).unwrap()),
        )
        .unwrap();
        let vol = model.volatility_model().volatility(0.0).unwrap();
        let covariance = model.covariance(0.0).unwrap();
        let expected = (vol[0] * vol[1]) * (-230.0_f64).exp();
        assert!(expected > 1e-202);
        assert_eq!(covariance[(0, 1)], expected);
        assert_eq!(covariance[(1, 0)], expected);
    }

    #[test]
    fn shares_immutable_components_without_cloning_their_storage() {
        let model = proxy();
        let copy = model.clone();
        assert!(Shared::ptr_eq(
            model.volatility_model(),
            copy.volatility_model()
        ));
        assert!(Shared::ptr_eq(
            model.correlation_model(),
            copy.correlation_model()
        ));
    }
}
