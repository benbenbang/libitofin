//! Facades for calibration methods, end criteria and error measures.
//!
//! Heston and Hull-White share these methods, stopping rules and error measures.

use crate::PyQlError;
use libitofin::math::optimization::conjugategradient::ConjugateGradient;
use libitofin::math::optimization::endcriteria::EndCriteria;
use libitofin::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use libitofin::math::optimization::method::OptimizationMethod;
use libitofin::math::optimization::simplex::Simplex;
use libitofin::math::optimization::steepestdescent::SteepestDescent;
use libitofin::models::CalibrationErrorType;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyAny;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// The least-squares optimizer used to fit model parameters.
///
/// Wraps the MINPACK lmdif routine. The Jacobian comes from a built-in
/// forward-difference scheme by default; the cost function's own jacobian
/// method is used instead when use_cost_functions_jacobian is set.
#[gen_stub_pyclass]
#[pyclass(
    name = "LevenbergMarquardt",
    unsendable,
    module = "itofin.optimization"
)]
pub struct PyLevenbergMarquardt {
    inner: LevenbergMarquardt,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyLevenbergMarquardt {
    /// Initialize the optimizer; the defaults are QuantLib's.
    ///
    /// Args:
    ///     epsfcn (float): The finite-difference step seed used when the Jacobian is
    ///         computed by differences.
    ///     xtol (float): The tolerance on the independent variable.
    ///     gtol (float): The tolerance on the gradient.
    ///     use_cost_functions_jacobian (bool): Use the cost function's own jacobian
    ///         method (a central difference, order 2 but costlier) instead of
    ///         the built-in forward-difference scheme.
    #[new]
    #[pyo3(signature = (epsfcn = 1e-8, xtol = 1e-8, gtol = 1e-8, use_cost_functions_jacobian = false))]
    fn new(epsfcn: f64, xtol: f64, gtol: f64, use_cost_functions_jacobian: bool) -> Self {
        PyLevenbergMarquardt {
            inner: LevenbergMarquardt::new(epsfcn, xtol, gtol, use_cost_functions_jacobian),
        }
    }
}

/// The downhill simplex method for derivative-free calibration.
#[gen_stub_pyclass]
#[pyclass(name = "Simplex", unsendable, module = "itofin.optimization")]
pub struct PySimplex {
    inner: Simplex,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySimplex {
    /// Build a simplex with a finite, positive characteristic length.
    ///
    /// Raises:
    ///     ValueError: If lambda_ is zero, negative or non-finite.
    #[new]
    #[pyo3(signature = (lambda_))]
    fn new(lambda_: f64) -> PyResult<Self> {
        if !lambda_.is_finite() || lambda_ <= 0.0 {
            return Err(PyValueError::new_err("lambda_ must be finite and positive"));
        }
        Ok(Self {
            inner: Simplex::new(lambda_),
        })
    }
}

/// The conjugate-gradient method for calibration.
#[gen_stub_pyclass]
#[pyclass(name = "ConjugateGradient", unsendable, module = "itofin.optimization")]
pub struct PyConjugateGradient {
    inner: ConjugateGradient,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyConjugateGradient {
    /// Build a conjugate-gradient method with the core Armijo line search.
    #[new]
    fn new() -> Self {
        Self {
            inner: ConjugateGradient::new(),
        }
    }
}

/// The steepest-descent method for calibration.
#[gen_stub_pyclass]
#[pyclass(name = "SteepestDescent", unsendable, module = "itofin.optimization")]
pub struct PySteepestDescent {
    inner: SteepestDescent,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySteepestDescent {
    /// Build a steepest-descent method with the core Armijo line search.
    #[new]
    fn new() -> Self {
        Self {
            inner: SteepestDescent::new(),
        }
    }
}

pub(crate) fn with_method<R>(
    method: &Bound<'_, PyAny>,
    run: impl FnOnce(&mut dyn OptimizationMethod) -> PyResult<R>,
) -> PyResult<R> {
    if method.is_instance_of::<PyLevenbergMarquardt>() {
        let mut method = method.extract::<PyRefMut<'_, PyLevenbergMarquardt>>()?;
        return run(method.inner_mut());
    }
    if method.is_instance_of::<PySimplex>() {
        let mut method = method.extract::<PyRefMut<'_, PySimplex>>()?;
        return run(&mut method.inner);
    }
    if method.is_instance_of::<PyConjugateGradient>() {
        let mut method = method.extract::<PyRefMut<'_, PyConjugateGradient>>()?;
        return run(&mut method.inner);
    }
    if method.is_instance_of::<PySteepestDescent>() {
        let mut method = method.extract::<PyRefMut<'_, PySteepestDescent>>()?;
        return run(&mut method.inner);
    }
    Err(PyTypeError::new_err(
        "method must be LevenbergMarquardt, Simplex, ConjugateGradient or SteepestDescent",
    ))
}

impl PyLevenbergMarquardt {
    /// The wrapped core method, mutably, for the `calibrate` free function.
    pub(crate) fn inner_mut(&mut self) -> &mut LevenbergMarquardt {
        &mut self.inner
    }
}

/// The optimizer stopping rule.
///
/// Carries the iteration cap and the stationarity thresholds an optimization
/// run is tested against.
#[gen_stub_pyclass]
#[pyclass(name = "EndCriteria", unsendable, module = "itofin.optimization")]
pub struct PyEndCriteria {
    inner: EndCriteria,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEndCriteria {
    /// Initialize the criteria.
    ///
    /// Args:
    ///     max_iterations (int): The iteration count at which the run stops.
    ///     max_stationary_state_iterations (int | None): How many consecutive stationary
    ///         iterations are tolerated before the run is called converged;
    ///         None defaults to min(max_iterations / 2, 100).
    ///     root_epsilon (float): The variation of the independent variable below which
    ///         an iteration counts as stationary.
    ///     function_epsilon (float): The variation of the function value below which an
    ///         iteration counts as stationary, and, for a cost function known
    ///         to be positive, the value below which the run has converged.
    ///     gradient_norm_epsilon (float | None): The gradient norm below which the run has
    ///         converged; None defaults to function_epsilon.
    ///
    /// Raises:
    ///     ItofinError: Unless 1 < max_stationary_state_iterations <
    ///         max_iterations, or if any epsilon is negative or non-finite.
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

/// How market and model prices are compared during calibration.
///
/// RelativePriceError is |market - model| / market, PriceError is
/// market - model, and ImpliedVolError compares the two implied volatilities.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "CalibrationErrorType",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.models"
)]
#[derive(Clone, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum PyCalibrationErrorType {
    RelativePriceError,
    PriceError,
    ImpliedVolError,
}

impl PyCalibrationErrorType {
    /// The core CalibrationErrorType this variant stands for.
    pub(crate) fn inner(self) -> CalibrationErrorType {
        match self {
            PyCalibrationErrorType::RelativePriceError => CalibrationErrorType::RelativePriceError,
            PyCalibrationErrorType::PriceError => CalibrationErrorType::PriceError,
            PyCalibrationErrorType::ImpliedVolError => CalibrationErrorType::ImpliedVolError,
        }
    }
}
