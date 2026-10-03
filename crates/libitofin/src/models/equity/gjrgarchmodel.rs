//! Six-parameter GJR-GARCH model with live markets and atomic parameter updates.
//!
//! QuantLib's model bounds are intersected with the supported process domain.
//! In particular, `alpha + gamma >= 0` is an additional economic guard not
//! imposed by QuantLib's calibrated model. No stationarity restriction is added.

#[cfg(test)]
mod calibration_oracle;
#[cfg(test)]
mod calibration_tests;
mod constraint;
#[cfg(test)]
mod rollback_market_tests;
#[cfg(test)]
mod tests;

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::Constraint;
use crate::models::model::{
    CalibratedModel, CalibratedModelHolder, CalibrationRollback, register_with_term_structure,
};
use crate::models::parameter::ConstantParameter;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess};
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::types::Real;

use constraint::{GjrGarchConstraint, ScalarDomain};

/// Daily calibrated parameters in `[omega, alpha, beta, gamma, lambda, v0]` order.
///
/// Rebuilt processes use full truncation, matching QuantLib's model constructor
/// and `generateArguments`, regardless of the supplied process's scheme.
/// Unlike QuantLib's unchecked `setParams`, holder updates validate the model
/// constraints and supported process domain before changing any state.
pub struct GjrGarchModel {
    model: CalibratedModel,
    process: Shared<GjrGarchProcess>,
    _observer: Option<SharedMut<dyn Observer>>,
}

impl GjrGarchModel {
    /// Retains live spot/curve handles and the process's days-per-year convention.
    ///
    /// # Errors
    /// Rejects nonpositive omega/v0, alpha/beta outside `[0, 1]`, gamma outside
    /// `[-1, 1]`, negative beta + gamma, unsupported process coefficients, and
    /// invalid live market handles.
    pub fn new(process: Shared<GjrGarchProcess>) -> QlResult<SharedMut<Self>> {
        let parameters = process.parameters();
        let params = Array::from([
            parameters.omega,
            parameters.alpha,
            parameters.beta,
            parameters.gamma,
            parameters.lambda,
            parameters.v0,
        ]);
        let domain = GjrGarchConstraint::new(parameters.days_per_year);
        require!(domain.test(&params), "invalid GJR-GARCH model parameters");
        let mut model = CalibratedModel::new(6);
        for (index, &value) in params.iter().enumerate() {
            model.arguments_mut()[index] =
                ConstantParameter::new(value, Rc::new(ScalarDomain::new(index)))?;
        }
        let risk_free = process.risk_free_rate();
        let dividend = process.dividend_yield();
        let spot = process.s0();
        let mut gjr = Self {
            model,
            process,
            _observer: None,
        };
        gjr.process = gjr.rebuilt_process(&params)?;
        let model = shared_mut(gjr);
        let observer = register_with_term_structure(&model, &risk_free);
        dividend.register_observer(&observer);
        spot.register_observer(&observer);
        model.borrow_mut()._observer = Some(observer);
        Ok(model)
    }

    /// Daily variance intercept.
    pub fn omega(&self) -> Real {
        self.model.arguments()[0].value(0.0)
    }
    /// Symmetric innovation coefficient.
    pub fn alpha(&self) -> Real {
        self.model.arguments()[1].value(0.0)
    }
    /// Lagged variance coefficient.
    pub fn beta(&self) -> Real {
        self.model.arguments()[2].value(0.0)
    }
    /// Additional negative-innovation coefficient.
    pub fn gamma(&self) -> Real {
        self.model.arguments()[3].value(0.0)
    }
    /// Innovation displacement.
    pub fn lambda(&self) -> Real {
        self.model.arguments()[4].value(0.0)
    }
    /// Initial daily variance.
    pub fn v0(&self) -> Real {
        self.model.arguments()[5].value(0.0)
    }
    /// Current process sharing the model's live market handles.
    pub fn process(&self) -> Shared<GjrGarchProcess> {
        Shared::clone(&self.process)
    }

    fn rebuilt_process(&self, params: &Array) -> QlResult<Shared<GjrGarchProcess>> {
        let domain = GjrGarchConstraint::new(self.process.days_per_year());
        require!(domain.test(params), "invalid GJR-GARCH model parameters");
        Ok(shared(GjrGarchProcess::new(
            self.process.risk_free_rate(),
            self.process.dividend_yield(),
            self.process.s0(),
            GjrGarchParameters {
                omega: params[0],
                alpha: params[1],
                beta: params[2],
                gamma: params[3],
                lambda: params[4],
                v0: params[5],
                days_per_year: self.process.days_per_year(),
            },
            GjrGarchDiscretization::FullTruncation,
        )?))
    }
}

impl AsObservable for GjrGarchModel {
    fn observable(&self) -> &Observable {
        self.model.observable()
    }
}

impl CalibratedModelHolder for GjrGarchModel {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.model
    }
    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.model
    }
    fn constraint(&self) -> Box<dyn Constraint> {
        Box::new(GjrGarchConstraint::new(self.process.days_per_year()))
    }
    fn calibration_rollback(&self) -> CalibrationRollback<Self> {
        let params = self.model.params();
        let process = Shared::clone(&self.process);
        Box::new(move |model| {
            model.model.write_params(&params)?;
            model.process = process;
            model.model.observable().notify_observers();
            Ok(())
        })
    }
    /// Keeps live handles on an invalid market notification so subsequent
    /// pricing reports that market error without a notification-time panic.
    fn generate_arguments(&mut self) {
        if let Ok(process) = self.rebuilt_process(&self.model.params()) {
            self.process = process;
        }
    }
    fn set_params(&mut self, params: &Array) -> QlResult<()> {
        let process = self.rebuilt_process(params)?;
        self.model.write_params(params)?;
        self.process = process;
        self.model.observable().notify_observers();
        Ok(())
    }
}
