//! Facades for the calibration machinery: [`PyLevenbergMarquardt`],
//! [`PyEndCriteria`] and [`PyCalibrationErrorType`].
//!
//! These are the optimizer, stopping rule and error measure shared by the
//! Heston and Hull-White calibrations (follow-up tickets H2/W2).

use crate::PyQlError;
use libitofin::math::optimization::endcriteria::EndCriteria;
use libitofin::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use libitofin::models::CalibrationErrorType;
use pyo3::prelude::*;

/// Python `LevenbergMarquardt`: the least-squares optimizer used to fit model
/// parameters (`math::optimization::levenbergmarquardt`).
///
/// Holds the method by value so a later calibration facade can hand out the
/// `&mut dyn OptimizationMethod` that the core `calibrate` free function takes.
#[pyclass(name = "LevenbergMarquardt", unsendable)]
pub struct PyLevenbergMarquardt {
    inner: LevenbergMarquardt,
}

#[pymethods]
impl PyLevenbergMarquardt {
    #[new]
    #[pyo3(signature = (epsfcn = 1e-8, xtol = 1e-8, gtol = 1e-8, use_cost_functions_jacobian = false))]
    fn new(epsfcn: f64, xtol: f64, gtol: f64, use_cost_functions_jacobian: bool) -> Self {
        PyLevenbergMarquardt {
            inner: LevenbergMarquardt::new(epsfcn, xtol, gtol, use_cost_functions_jacobian),
        }
    }
}

impl PyLevenbergMarquardt {
    /// The wrapped core method, mutably, for the `calibrate` free function.
    pub(crate) fn inner_mut(&mut self) -> &mut LevenbergMarquardt {
        &mut self.inner
    }
}

/// Python `EndCriteria`: the optimizer stopping rule
/// (`math::optimization::endcriteria`).
///
/// The core constructor is fallible - it requires
/// `1 < max_stationary_state_iterations < max_iterations` and finite,
/// non-negative epsilons - so the ctor routes its `QlResult` through
/// [`struct@crate::ItofinError`].
#[pyclass(name = "EndCriteria", unsendable)]
pub struct PyEndCriteria {
    inner: EndCriteria,
}

#[pymethods]
impl PyEndCriteria {
    #[new]
    #[pyo3(signature = (
        max_iterations,
        max_stationary_state_iterations,
        root_epsilon,
        function_epsilon,
        gradient_norm_epsilon,
    ))]
    fn new(
        max_iterations: usize,
        max_stationary_state_iterations: Option<usize>,
        root_epsilon: f64,
        function_epsilon: f64,
        gradient_norm_epsilon: Option<f64>,
    ) -> PyResult<Self> {
        let inner = EndCriteria::new(
            max_iterations,
            max_stationary_state_iterations,
            root_epsilon,
            function_epsilon,
            gradient_norm_epsilon,
        )
        .map_err(PyQlError::from)?;
        Ok(PyEndCriteria { inner })
    }
}

impl PyEndCriteria {
    /// The wrapped core criteria; `calibrate` borrows it as `&EndCriteria`.
    pub(crate) fn inner(&self) -> &EndCriteria {
        &self.inner
    }
}

/// Python `CalibrationErrorType`: how market and model prices are compared
/// during calibration (`models::CalibrationErrorType`).
///
/// A fieldless pyo3 enum mirroring the core variants; the comparison formulas
/// live in the core, so the facade only maps the variant across.
#[pyclass(name = "CalibrationErrorType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum PyCalibrationErrorType {
    RelativePriceError,
    PriceError,
    ImpliedVolError,
}

impl PyCalibrationErrorType {
    /// The core [`CalibrationErrorType`] this variant stands for.
    pub(crate) fn inner(self) -> CalibrationErrorType {
        match self {
            PyCalibrationErrorType::RelativePriceError => CalibrationErrorType::RelativePriceError,
            PyCalibrationErrorType::PriceError => CalibrationErrorType::PriceError,
            PyCalibrationErrorType::ImpliedVolError => CalibrationErrorType::ImpliedVolError,
        }
    }
}
