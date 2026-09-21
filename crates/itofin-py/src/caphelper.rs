//! Cap calibration and Hull-White lattice engine facades.
use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::{PyHullWhite, PyIborIndex};
use crate::market::PySimpleQuote;
use crate::swaptionvol::PyVolatilityType;
use crate::time::{PyDayCounter, PyFrequency, PyPeriod};
use libitofin::math::timegrid::TimeGrid;
use libitofin::models::calibrate;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::shortrate::calibrationhelpers::CapHelper;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::capfloor::TreeCapFloorEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

#[gen_stub_pyclass]
#[pyclass(
    name = "TreeCapFloorEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyTreeCapFloorEngine {
    inner: SharedMut<TreeCapFloorEngine>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyTreeCapFloorEngine {
    #[new]
    fn new(model: &PyHullWhite, time_steps: usize) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                TreeCapFloorEngine::new(model.inner(), time_steps).map_err(PyQlError::from)?,
            ),
        })
    }
    #[staticmethod]
    fn with_time_grid(model: &PyHullWhite, times: Vec<f64>) -> PyResult<Self> {
        if times.is_empty() || times.iter().any(|t| !t.is_finite() || *t < 0.0) {
            return Err(crate::ItofinError::new_err(
                "grid times must be finite, non-negative and nonempty",
            ));
        }
        let grid = TimeGrid::from_mandatory_times(&times).map_err(PyQlError::from)?;
        Ok(Self {
            inner: shared_mut(
                TreeCapFloorEngine::with_time_grid(model.inner(), grid).map_err(PyQlError::from)?,
            ),
        })
    }
}
impl PyTreeCapFloorEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        self.inner.clone() as SharedMut<dyn PricingEngine>
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "CapHelper", unsendable, module = "itofin.models")]
pub struct PyCapHelper {
    inner: SharedMut<CapHelper>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCapHelper {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (length, volatility, index, fixed_leg_frequency, fixed_leg_day_counter, include_first_swaplet, curve, error_type, volatility_type, shift = 0.0))]
    fn new(
        length: &PyPeriod,
        volatility: &PySimpleQuote,
        index: &PyIborIndex,
        fixed_leg_frequency: PyFrequency,
        fixed_leg_day_counter: &PyDayCounter,
        include_first_swaplet: bool,
        curve: &PyYieldTermStructure,
        error_type: &PyCalibrationErrorType,
        volatility_type: PyVolatilityType,
        shift: f64,
    ) -> PyResult<Self> {
        let helper = CapHelper::try_new(
            length.inner(),
            volatility.handle(),
            index.inner(),
            fixed_leg_frequency.inner(),
            fixed_leg_day_counter.inner(),
            include_first_swaplet,
            curve.handle(),
            error_type.inner(),
            volatility_type.inner(),
            shift,
        )
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner: shared_mut(helper),
        })
    }
    fn market_value(&mut self) -> PyResult<f64> {
        self.inner
            .borrow_mut()
            .market_value()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
    fn black_price(&self, volatility: f64) -> PyResult<f64> {
        self.inner
            .borrow()
            .black_price(volatility)
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
    fn model_value(&self) -> PyResult<f64> {
        self.inner
            .borrow()
            .model_value()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
    fn calibration_error(&mut self) -> PyResult<f64> {
        self.inner
            .borrow_mut()
            .calibration_error()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
    fn mandatory_times(&self) -> PyResult<Vec<f64>> {
        self.inner
            .borrow()
            .mandatory_times()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
    fn set_tree_engine(&mut self, engine: &PyTreeCapFloorEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }
}

impl PyHullWhite {
    pub(crate) fn calibrate_caps_impl(
        &self,
        helpers: Vec<PyRef<PyCapHelper>>,
        method: &mut PyLevenbergMarquardt,
        end_criteria: &PyEndCriteria,
        fix_reversion: bool,
        time_steps: usize,
    ) -> PyResult<()> {
        let engine =
            shared_mut(TreeCapFloorEngine::new(self.inner(), time_steps).map_err(PyQlError::from)?)
                as SharedMut<dyn PricingEngine>;
        for helper in &helpers {
            helper
                .inner
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(engine.clone());
        }
        let helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
            .iter()
            .map(|h| h.inner.clone() as SharedMut<dyn CalibrationHelper>)
            .collect();
        calibrate(
            &self.inner(),
            &helpers,
            method.inner_mut(),
            end_criteria.inner(),
            None,
            Vec::new(),
            if fix_reversion {
                vec![true, false]
            } else {
                Vec::new()
            },
        )
        .map_err(PyQlError::from)?;
        Ok(())
    }
}
