//! GJR-GARCH diffusion approximation with daily parameters and live market inputs.
//!
//! Ports QuantLib's three truncation/reflection schemes. This is the continuous
//! diffusion approximation, not a discrete GJR-GARCH return recursion.

mod parameters;
#[cfg(test)]
mod tests;

pub use parameters::{GjrGarchCoefficients, GjrGarchDiscretization, GjrGarchParameters};
use parameters::{validate_state, validate_step};

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::{array::Array, matrix::Matrix};
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::require;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{date::Date, frequency::Frequency};
use crate::types::{Real, Size, Time};

/// Observable two-factor GJR-GARCH spot and annual-variance process.
///
/// The inherent [`Self::apply`] reports errors. The infallible base trait
/// `apply` instead returns two NaNs for invalid input or overflow; all fallible
/// process methods use the checked inherent implementation.
pub struct GjrGarchProcess {
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    s0: Handle<dyn Quote>,
    parameters: GjrGarchParameters,
    coefficients: GjrGarchCoefficients,
    discretization: GjrGarchDiscretization,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl GjrGarchProcess {
    /// Retains live market handles and validates daily process parameters.
    ///
    /// # Errors
    /// Rejects invalid parameters, empty curves, null reference dates or invalid spot.
    pub fn new(
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        s0: Handle<dyn Quote>,
        parameters: GjrGarchParameters,
        discretization: GjrGarchDiscretization,
    ) -> QlResult<Self> {
        let coefficients = parameters.coefficients()?;
        require!(
            risk_free_rate.current_link()?.reference_date()? != Date::null()
                && dividend_yield.current_link()?.reference_date()? != Date::null(),
            "GJR-GARCH curves require non-null reference dates"
        );
        validate_state([s0.current_link()?.value()?, coefficients.initial_variance()])?;
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::broadcasting(Shared::clone(&observable), || {});
        let observer = listener.clone() as SharedMut<dyn Observer>;
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);
        s0.register_observer(&observer);
        Ok(Self {
            risk_free_rate,
            dividend_yield,
            s0,
            parameters,
            coefficients,
            discretization,
            observable,
            _listener: listener,
        })
    }

    /// Daily parameters before annualization.
    pub fn parameters(&self) -> GjrGarchParameters {
        self.parameters
    }
    /// Selected variance treatment.
    pub fn discretization(&self) -> GjrGarchDiscretization {
        self.discretization
    }
    /// Live spot quote handle.
    pub fn s0(&self) -> Handle<dyn Quote> {
        self.s0.clone()
    }
    /// Live risk-free curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.risk_free_rate.clone()
    }
    /// Live dividend curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.dividend_yield.clone()
    }
    /// Initial daily variance.
    pub fn v0(&self) -> Real {
        self.parameters.v0
    }
    /// Daily variance intercept.
    pub fn omega(&self) -> Real {
        self.parameters.omega
    }
    /// Symmetric innovation coefficient.
    pub fn alpha(&self) -> Real {
        self.parameters.alpha
    }
    /// Lagged variance coefficient.
    pub fn beta(&self) -> Real {
        self.parameters.beta
    }
    /// Negative-innovation coefficient.
    pub fn gamma(&self) -> Real {
        self.parameters.gamma
    }
    /// Innovation displacement.
    pub fn lambda(&self) -> Real {
        self.parameters.lambda
    }
    /// Annualization days.
    pub fn days_per_year(&self) -> Real {
        self.parameters.days_per_year
    }
    /// State dimension.
    pub fn size(&self) -> Size {
        2
    }
    /// Number of independent Gaussian factors.
    pub fn factors(&self) -> Size {
        2
    }

    /// Reads live spot and initial annual variance.
    ///
    /// # Errors
    /// Rejects missing, invalid or nonpositive spot quotes.
    pub fn initial_values(&self) -> QlResult<Array> {
        let state = [
            self.s0.current_link()?.value()?,
            self.coefficients.initial_variance(),
        ];
        validate_state(state)?;
        Ok(Array::from(state))
    }

    /// Instantaneous logarithmic-spot and annual-variance drift.
    ///
    /// # Errors
    /// Rejects invalid dimensions/state/time, invalid curve rates and overflow.
    pub fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        let state = state_array(x)?;
        Ok(Array::from(self.coefficients.drift(
            state[1],
            self.rates(t, t)?,
            self.discretization,
        )?))
    }

    /// Diffusion matrix retaining QuantLib's signed reflection and 1e-8 floor.
    ///
    /// # Errors
    /// Rejects invalid dimensions/state/time and unrepresentable coefficients.
    pub fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
        validate_step(t)?;
        let state = state_array(x)?;
        Ok(Matrix::from(
            self.coefficients.diffusion(state[1], self.discretization)?,
        ))
    }

    /// Applies a logarithmic spot increment and additive raw variance increment.
    ///
    /// # Errors
    /// Rejects invalid dimensions/state/increments or unrepresentable output.
    pub fn apply(&self, x: &Array, dx: &Array) -> QlResult<Array> {
        let state = state_array(x)?;
        let change = pair_array(dx)?;
        let result = [state[0] * change[0].exp(), state[1] + change[1]];
        validate_state(result)?;
        Ok(Array::from(result))
    }

    /// Evolves with interval forwards and two independent normal variates.
    ///
    /// # Errors
    /// Rejects invalid dimensions/state/time/shocks, curve failures and overflow.
    pub fn evolve(&self, t0: Time, x: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        validate_step(dt)?;
        let state = state_array(x)?;
        let shocks = pair_array(dw)?;
        Ok(Array::from(self.coefficients.evolve(
            state,
            dt,
            self.rates(t0, t0 + dt)?,
            shocks,
            self.discretization,
        )?))
    }

    /// Euler expectation, as used by QuantLib's base discretization.
    ///
    /// # Errors
    /// Rejects invalid step or the failures of [`Self::drift`] and [`Self::apply`].
    pub fn expectation(&self, t0: Time, x: &Array, dt: Time) -> QlResult<Array> {
        validate_step(dt)?;
        self.apply(x, &(&self.drift(t0, x)? * dt))
    }

    /// Euler diffusion matrix multiplied by square-root time.
    ///
    /// # Errors
    /// Rejects invalid step, diffusion failures or overflow.
    pub fn std_deviation(&self, t0: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        validate_step(dt)?;
        let result = &self.diffusion(t0, x)? * dt.sqrt();
        validate_matrix(&result)?;
        Ok(result)
    }

    /// Euler covariance matrix.
    ///
    /// # Errors
    /// Rejects invalid step, diffusion failures or overflow.
    pub fn covariance(&self, t0: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        validate_step(dt)?;
        let diffusion = self.diffusion(t0, x)?;
        let result = &(&diffusion * &diffusion.transpose()) * dt;
        validate_matrix(&result)?;
        Ok(result)
    }

    /// Converts a date using the current risk-free curve's clock.
    ///
    /// # Errors
    /// Propagates curve or date conversion failures.
    pub fn time(&self, date: &Date) -> QlResult<Time> {
        self.risk_free_rate
            .current_link()?
            .time_from_reference(*date)
    }

    fn rates(&self, start: Time, end: Time) -> QlResult<Real> {
        validate_step(start)?;
        validate_step(end)?;
        let rate = |curve: &Handle<dyn YieldTermStructure>| -> QlResult<Real> {
            Ok(curve
                .current_link()?
                .forward_rate(
                    start,
                    end,
                    Compounding::Continuous,
                    Frequency::NoFrequency,
                    false,
                )?
                .rate())
        };
        let result = rate(&self.risk_free_rate)? - rate(&self.dividend_yield)?;
        require!(result.is_finite(), "GJR-GARCH forward rates must be finite");
        Ok(result)
    }
}

impl AsObservable for GjrGarchProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess for GjrGarchProcess {
    fn size(&self) -> Size {
        self.size()
    }
    fn factors(&self) -> Size {
        self.factors()
    }
    fn initial_values(&self) -> QlResult<Array> {
        self.initial_values()
    }
    fn drift(&self, t: Time, x: &Array) -> QlResult<Array> {
        self.drift(t, x)
    }
    fn diffusion(&self, t: Time, x: &Array) -> QlResult<Matrix> {
        self.diffusion(t, x)
    }
    fn evolve(&self, t: Time, x: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
        self.evolve(t, x, dt, dw)
    }
    fn expectation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Array> {
        self.expectation(t, x, dt)
    }
    fn std_deviation(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        self.std_deviation(t, x, dt)
    }
    fn covariance(&self, t: Time, x: &Array, dt: Time) -> QlResult<Matrix> {
        self.covariance(t, x, dt)
    }
    fn apply(&self, x: &Array, dx: &Array) -> Array {
        self.apply(x, dx)
            .unwrap_or_else(|_| Array::from([Real::NAN; 2]))
    }
    fn time(&self, date: &Date) -> QlResult<Time> {
        self.time(date)
    }
}

fn pair_array(values: &Array) -> QlResult<[Real; 2]> {
    require!(
        values.size() == 2 && values.iter().all(|x| x.is_finite()),
        "GJR-GARCH requires two finite values"
    );
    Ok([values[0], values[1]])
}

fn state_array(values: &Array) -> QlResult<[Real; 2]> {
    let result = pair_array(values)?;
    validate_state(result)?;
    Ok(result)
}

fn validate_matrix(matrix: &Matrix) -> QlResult<()> {
    require!(
        (0..2).all(|i| (0..2).all(|j| matrix[(i, j)].is_finite())),
        "GJR-GARCH covariance or deviation overflow"
    );
    Ok(())
}
