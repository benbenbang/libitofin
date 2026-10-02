//! Eight-parameter Bates model with live markets and atomic parameter updates.

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint};
use crate::models::model::{CalibratedModel, CalibratedModelHolder, register_with_term_structure};
use crate::models::parameter::ConstantParameter;
use crate::patterns::observable::{AsObservable, Observer};
use crate::processes::BatesProcess;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::types::Real;

/// Calibrated parameters in `(theta, kappa, sigma, rho, v0, nu, delta, lambda)` order.
pub struct BatesModel {
    model: CalibratedModel,
    process: Shared<BatesProcess>,
    _observer: Option<SharedMut<dyn Observer>>,
}

struct FiniteDomain {
    positive: bool,
}

impl Constraint for FiniteDomain {
    fn test(&self, params: &Array) -> bool {
        params
            .iter()
            .all(|&p| p.is_finite() && (!self.positive || p > 0.0))
    }

    fn lower_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), if self.positive { 0.0 } else { -Real::MAX })
    }
}

impl BatesModel {
    /// Retains the process and observes its live risk-free, dividend and spot handles.
    ///
    /// # Errors
    /// Rejects invalid parameter domains or unrepresentable jump compensation.
    pub fn new(process: Shared<BatesProcess>) -> QlResult<SharedMut<Self>> {
        let params = Array::from([
            process.theta(),
            process.kappa(),
            process.sigma(),
            process.rho(),
            process.v0(),
            process.nu(),
            process.delta(),
            process.lambda(),
        ]);
        BatesProcess::validate_parameters(&params)?;
        let mut model = CalibratedModel::new(8);
        for (index, &value) in params.iter().enumerate() {
            let constraint: Rc<dyn Constraint> = match index {
                3 => Rc::new(BoundaryConstraint::new(-1.0, 1.0)),
                6 | 7 => Rc::new(BoundaryConstraint::new(0.0, Real::MAX)),
                _ => Rc::new(FiniteDomain {
                    positive: index != 5,
                }),
            };
            model.arguments_mut()[index] = ConstantParameter::new(value, constraint)?;
        }
        let risk_free = process.risk_free_rate();
        let dividend = process.dividend_yield();
        let spot = process.s0();
        let mut bates = Self {
            model,
            process,
            _observer: None,
        };
        bates.generate_arguments();
        let model = shared_mut(bates);
        let observer = register_with_term_structure(&model, &risk_free);
        dividend.register_observer(&observer);
        spot.register_observer(&observer);
        model.borrow_mut()._observer = Some(observer);
        Ok(model)
    }

    /// Long-run variance.
    pub fn theta(&self) -> Real {
        self.model.arguments()[0].value(0.0)
    }
    /// Variance mean-reversion speed.
    pub fn kappa(&self) -> Real {
        self.model.arguments()[1].value(0.0)
    }
    /// Volatility of variance.
    pub fn sigma(&self) -> Real {
        self.model.arguments()[2].value(0.0)
    }
    /// Spot/variance correlation.
    pub fn rho(&self) -> Real {
        self.model.arguments()[3].value(0.0)
    }
    /// Initial variance.
    pub fn v0(&self) -> Real {
        self.model.arguments()[4].value(0.0)
    }
    /// Mean logarithmic jump multiplier.
    pub fn nu(&self) -> Real {
        self.model.arguments()[5].value(0.0)
    }
    /// Standard deviation of logarithmic jump multiplier.
    pub fn delta(&self) -> Real {
        self.model.arguments()[6].value(0.0)
    }
    /// Original jump intensity, including the exact no-jump boundary zero.
    pub fn lambda(&self) -> Real {
        self.model.arguments()[7].value(0.0)
    }
    /// Current process rebuilt after every validated parameter update or market notification.
    pub fn process(&self) -> Shared<BatesProcess> {
        self.process.clone()
    }
}

impl CalibratedModelHolder for BatesModel {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.model
    }
    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.model
    }
    fn generate_arguments(&mut self) {
        self.process = shared(self.process.with_parameters(&self.model.params()));
    }
    fn set_params(&mut self, params: &Array) -> QlResult<()> {
        BatesProcess::validate_parameters(params)?;
        self.model.write_params(params)?;
        self.generate_arguments();
        self.model.observable().notify_observers();
        Ok(())
    }
}
