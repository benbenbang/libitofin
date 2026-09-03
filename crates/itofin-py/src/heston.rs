//! Facades for the Heston stack: [`PyHestonProcess`], [`PyHestonModel`] and
//! [`PyHestonModelHelper`].

use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::settings::PySettings;
use crate::time::{PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{HestonModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// The square-root stochastic-variance process.
///
/// The two flat yield curves and the spot quote are assembled behind their
/// handles internally, so no handle crosses the binding boundary.
#[pyclass(name = "HestonProcess", unsendable)]
pub struct PyHestonProcess {
    inner: Shared<HestonProcess>,
}

#[pymethods]
impl PyHestonProcess {
    /// Build the process from scalar market inputs and the five parameters.
    ///
    /// Args:
    ///     risk_free_rate (float): The flat risk-free rate, made into a curve
    ///         compounded continuously on an annual frequency.
    ///     dividend_yield (float): The flat dividend yield, made into a curve on the
    ///         same convention as the risk-free rate.
    ///     spot (float): The spot level, held as a quote.
    ///     v0 (float): The initial variance.
    ///     kappa (float): The mean-reversion speed.
    ///     theta (float): The long-run variance.
    ///     sigma (float): The volatility of variance.
    ///     rho (float): The spot/variance correlation.
    ///     reference_date (Date): The date the two flat curves are anchored on.
    ///     day_counter (DayCounter): The day count the curves accrue on.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        risk_free_rate: f64,
        dividend_yield: f64,
        spot: f64,
        v0: f64,
        kappa: f64,
        theta: f64,
        sigma: f64,
        rho: f64,
        reference_date: &PyDate,
        day_counter: &PyDayCounter,
    ) -> Self {
        let ref_date = reference_date.inner();
        let dc = day_counter.inner();

        let risk_free_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            risk_free_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let dividend_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            dividend_yield,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let s0 = Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>);

        PyHestonProcess {
            inner: shared(HestonProcess::new(
                risk_free_curve,
                dividend_curve,
                s0,
                v0,
                kappa,
                theta,
                sigma,
                rho,
            )),
        }
    }

    /// Return the initial variance.
    ///
    /// Returns:
    ///     float: The initial variance v0.
    fn v0(&self) -> f64 {
        self.inner.v0()
    }

    /// Return the mean-reversion speed.
    ///
    /// Returns:
    ///     float: The mean-reversion speed kappa.
    fn kappa(&self) -> f64 {
        self.inner.kappa()
    }

    /// Return the long-run variance.
    ///
    /// Returns:
    ///     float: The long-run variance theta.
    fn theta(&self) -> f64 {
        self.inner.theta()
    }

    /// Return the volatility of variance.
    ///
    /// Returns:
    ///     float: The volatility of variance sigma.
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }

    /// Return the spot/variance correlation.
    ///
    /// Returns:
    ///     float: The spot/variance correlation rho.
    fn rho(&self) -> f64 {
        self.inner.rho()
    }
}

impl PyHestonProcess {
    /// A clone of the inner process for the model ctor.
    pub(crate) fn inner(&self) -> Shared<HestonProcess> {
        Shared::clone(&self.inner)
    }
}

/// The five-parameter calibrated Heston model.
///
/// The parameters are seeded from the process it is built on and overwritten in
/// place by a calibration, so the getters read the fitted values afterwards.
#[pyclass(name = "HestonModel", unsendable)]
pub struct PyHestonModel {
    inner: SharedMut<HestonModel>,
}

#[pymethods]
impl PyHestonModel {
    /// Seed the model from a process.
    ///
    /// Args:
    ///     process (HestonProcess): The process whose five parameters seed the model.
    ///
    /// Raises:
    ///     ItofinError: If a seeded parameter violates its constraint: theta,
    ///         kappa, sigma and v0 must be strictly positive and rho must lie
    ///         in [-1, 1].
    #[new]
    fn new(process: &PyHestonProcess) -> PyResult<Self> {
        let inner = HestonModel::new(process.inner()).map_err(PyQlError::from)?;
        Ok(PyHestonModel { inner })
    }

    /// Return the long-run variance.
    ///
    /// Returns:
    ///     float: The current value of theta.
    fn theta(&self) -> f64 {
        self.inner.borrow().theta()
    }

    /// Return the mean-reversion speed.
    ///
    /// Returns:
    ///     float: The current value of kappa.
    fn kappa(&self) -> f64 {
        self.inner.borrow().kappa()
    }

    /// Return the volatility of variance.
    ///
    /// Returns:
    ///     float: The current value of sigma.
    fn sigma(&self) -> f64 {
        self.inner.borrow().sigma()
    }

    /// Return the spot/variance correlation.
    ///
    /// Returns:
    ///     float: The current value of rho.
    fn rho(&self) -> f64 {
        self.inner.borrow().rho()
    }

    /// Return the initial variance.
    ///
    /// Returns:
    ///     float: The current value of v0.
    fn v0(&self) -> f64 {
        self.inner.borrow().v0()
    }

    /// Fit the five parameters to the helpers and write them back.
    ///
    /// One analytic Heston engine of the given integration order is built on
    /// this model and installed on every helper, so all helpers price through
    /// the same engine the optimizer drives. The fitted parameters are readable
    /// through the getters afterwards.
    ///
    /// Args:
    ///     helpers (list[HestonModelHelper]): The calibration instruments to fit; must not be empty.
    ///     method (LevenbergMarquardt): The optimizer driving the fit.
    ///     end_criteria (EndCriteria): The stopping rule handed to the optimizer.
    ///     integration_order (int): The order of the Gauss-Laguerre integration the
    ///         engine uses; at most 192.
    ///
    /// Raises:
    ///     ItofinError: If integration_order exceeds 192, if helpers is empty,
    ///         or if the optimization itself fails.
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PyHestonModelHelper>>,
        method: &mut PyLevenbergMarquardt,
        end_criteria: &PyEndCriteria,
        integration_order: usize,
    ) -> PyResult<()> {
        let engine = shared_mut(
            AnalyticHestonEngine::new(SharedMut::clone(&self.inner), integration_order)
                .map_err(PyQlError::from)?,
        ) as SharedMut<dyn PricingEngine>;
        for helper in &helpers {
            helper
                .inner
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(SharedMut::clone(&engine));
        }
        let dyn_helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
            .iter()
            .map(|helper| SharedMut::clone(&helper.inner) as SharedMut<dyn CalibrationHelper>)
            .collect();
        calibrate(
            &self.inner,
            &dyn_helpers,
            method.inner_mut(),
            end_criteria.inner(),
            None,
            Vec::new(),
            Vec::new(),
        )
        .map_err(PyQlError::from)?;
        Ok(())
    }
}

impl PyHestonModel {
    /// A clone of the inner model handle for the engine facade (H2 also calibrates).
    pub(crate) fn inner(&self) -> SharedMut<HestonModel> {
        SharedMut::clone(&self.inner)
    }
}

/// A Black-vol calibration helper over a flat-vol surface.
///
/// Assembles its own volatility quote and two flat curves from the scalar
/// market inputs, so no handle crosses the binding boundary.
#[pyclass(name = "HestonModelHelper", unsendable)]
pub struct PyHestonModelHelper {
    inner: SharedMut<HestonModelHelper>,
}

#[pymethods]
impl PyHestonModelHelper {
    /// Build the helper from scalar market inputs.
    ///
    /// Args:
    ///     maturity (Period): The option tenor.
    ///     calendar (Calendar): The calendar the maturity rolls on.
    ///     s0 (float): The spot level.
    ///     strike (float): The option strike.
    ///     volatility (float): The market Black volatility, held as a quote.
    ///     risk_free_rate (float): The flat risk-free rate, made into a curve compounded
    ///         continuously on an annual frequency.
    ///     dividend_yield (float): The flat dividend yield, made into a curve on the
    ///         same convention as the risk-free rate.
    ///     error_type (CalibrationErrorType): How the market and model prices are compared.
    ///     reference_date (Date): The date the two flat curves are anchored on; it is
    ///         used only to assemble them, not forwarded to the core.
    ///     day_counter (DayCounter): The day count the curves accrue on, used the same way.
    ///     settings (Settings): The evaluation-date store the helper reads.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        maturity: &PyPeriod,
        calendar: &PyCalendar,
        s0: f64,
        strike: f64,
        volatility: f64,
        risk_free_rate: f64,
        dividend_yield: f64,
        error_type: &PyCalibrationErrorType,
        reference_date: &PyDate,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> Self {
        let ref_date = reference_date.inner();
        let dc = day_counter.inner();

        let vol = Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
        let risk_free_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            risk_free_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let dividend_curve = Handle::new(shared(FlatForward::with_rate(
            ref_date,
            dividend_yield,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        PyHestonModelHelper {
            inner: shared_mut(HestonModelHelper::new(
                maturity.inner(),
                calendar.inner(),
                s0,
                strike,
                vol,
                risk_free_curve,
                dividend_curve,
                error_type.inner(),
                settings.inner(),
            )),
        }
    }

    /// Return the error between the market and model values.
    ///
    /// Meaningful once a calibration has installed a pricing engine on the
    /// helper; the comparison follows the helper's error type.
    ///
    /// Returns:
    ///     float: The calibration error under the configured error type.
    ///
    /// Raises:
    ///     ItofinError: If the market or model valuation fails, or the implied
    ///         volatility solve does.
    fn calibration_error(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .calibration_error()
            .map_err(PyQlError::from)?)
    }
}
