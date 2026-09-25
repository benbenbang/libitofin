//! The `itofin.optimize` submodule: SciPy-style `minimize` over the
//! finance-independent `itofin-optimize` crate.
//!
//! Distinct from `itofin.optimization`, the QuantLib calibration port. An
//! objective runs outside any bootstrap callback, so it may mutate a
//! `SimpleQuote` and reprice.

use crate::ItofinError;
use itofin_optimize::{
    Common, Converged, Flow, IterationState, Method, Minimize, MinimizeError, NelderMeadOptions,
    Objective, Problem, Termination, minimize as run,
};
use numpy::PyArray1;
use pyo3::exceptions::PyStopIteration;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// Why a minimize run stopped.
///
/// The integer values are fixed and append-only, matching the C
/// ItofinOptimizeStatus and the Go OptimizeStatus: 7 and 8 are reserved for
/// the line-search and constrained solvers.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "Status",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.optimize"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyStatus {
    ConvergedXTol = 0,
    ConvergedFTol = 1,
    ConvergedGTol = 2,
    MaxIterations = 3,
    MaxEvaluations = 4,
    Cancelled = 5,
    Nonfinite = 6,
}

impl PyStatus {
    fn from_termination(termination: Termination) -> PyResult<Self> {
        Ok(match termination {
            Termination::Converged(Converged::XTol) => Self::ConvergedXTol,
            Termination::Converged(Converged::FTol) => Self::ConvergedFTol,
            Termination::Converged(Converged::GTol) => Self::ConvergedGTol,
            Termination::MaxIterations => Self::MaxIterations,
            Termination::MaxEvaluations => Self::MaxEvaluations,
            Termination::Cancelled => Self::Cancelled,
            Termination::Nonfinite => Self::Nonfinite,
            other => {
                return Err(ItofinError::new_err(format!(
                    "unmapped termination: {other}"
                )));
            }
        })
    }
}

/// The outcome of a minimize run: the best point found and why it stopped.
#[gen_stub_pyclass]
#[pyclass(name = "OptimizeResult", frozen, module = "itofin.optimize")]
pub struct PyOptimizeResult {
    x_values: Vec<f64>,
    #[pyo3(get)]
    fun: f64,
    #[pyo3(get)]
    nit: usize,
    #[pyo3(get)]
    nfev: usize,
    #[pyo3(get)]
    njev: usize,
    #[pyo3(get)]
    status: PyStatus,
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    message: String,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyOptimizeResult {
    /// The best point found, as a new float64 array.
    #[getter]
    fn x<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_slice(py, &self.x_values)
    }
}

struct PyObjective<'py> {
    fun: Bound<'py, PyAny>,
    callback: Option<Bound<'py, PyAny>>,
}

impl Objective for PyObjective<'_> {
    type Error = PyErr;

    fn value(&mut self, x: &[f64]) -> PyResult<f64> {
        let point = PyArray1::from_slice(self.fun.py(), x);
        self.fun.call1((point,))?.extract::<f64>()
    }

    fn callback(&mut self, state: &IterationState<'_>) -> PyResult<Flow> {
        let Some(callback) = &self.callback else {
            return Ok(Flow::Continue);
        };
        let py = callback.py();
        match callback.call1((PyArray1::from_slice(py, state.x),)) {
            Ok(_) => Ok(Flow::Continue),
            Err(error) if error.is_instance_of::<PyStopIteration>(py) => Ok(Flow::Stop),
            Err(error) => Err(error),
        }
    }
}

fn nelder_mead(options: Option<&Bound<'_, PyDict>>) -> PyResult<(NelderMeadOptions, Common)> {
    let mut method = NelderMeadOptions::default();
    let mut common = Common::default();
    for (key, value) in options.into_iter().flat_map(|options| options.iter()) {
        match key.extract::<String>()?.as_str() {
            "maxiter" => common.maxiter = value.extract()?,
            "maxfev" => common.maxfev = value.extract()?,
            "xatol" => method.xatol = value.extract()?,
            "fatol" => method.fatol = value.extract()?,
            "adaptive" => method.adaptive = value.extract()?,
            other => {
                return Err(ItofinError::new_err(format!(
                    "method Nelder-Mead does not support option {other}"
                )));
            }
        }
    }
    Ok((method, common))
}

/// Minimize a scalar function of one or more variables.
///
/// Args:
///     fun (Callable): Called as fun(x) with a float64 array; returns a float.
///     x0 (Sequence[float]): The starting point.
///     method (str): The solver; only "Nelder-Mead" (any case) for now.
///     options (dict | None): maxiter, maxfev, xatol, fatol and adaptive;
///         any other key is rejected.
///     callback (Callable | None): Called as callback(xk) after every
///         iteration. Raising StopIteration stops the run with
///         Status.Cancelled.
///
/// Returns:
///     OptimizeResult: The best point found and why the run stopped.
///
/// Raises:
///     ItofinError: On an unknown method or option, or a rejected input.
///     Exception: Whatever fun or callback raised, re-raised unchanged.
#[gen_stub_pyfunction(module = "itofin.optimize")]
#[pyfunction]
#[pyo3(signature = (fun, x0, method = "Nelder-Mead", options = None, callback = None))]
pub(crate) fn minimize(
    #[gen_stub(override_type(type_repr = "typing.Callable[[numpy.typing.NDArray[numpy.float64]], float]", imports = ("typing", "numpy", "numpy.typing")))]
    fun: Bound<'_, PyAny>,
    x0: Vec<f64>,
    method: &str,
    options: Option<Bound<'_, PyDict>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[numpy.typing.NDArray[numpy.float64]], object]]", imports = ("typing", "numpy", "numpy.typing")))]
    callback: Option<Bound<'_, PyAny>>,
) -> PyResult<PyOptimizeResult> {
    if !method.eq_ignore_ascii_case("nelder-mead") {
        return Err(ItofinError::new_err(format!("unknown method {method}")));
    }
    let (options, common) = nelder_mead(options.as_ref())?;
    let problem = Problem { x0, bounds: None };
    let mut objective = PyObjective { fun, callback };
    let method = Method::NelderMead(options);
    let result: Minimize = match run(&mut objective, &problem, &method, &common) {
        Ok(result) => result,
        Err(MinimizeError::InvalidInput(error)) => {
            return Err(ItofinError::new_err(error.to_string()));
        }
        Err(MinimizeError::Objective(error)) => return Err(error),
    };
    Ok(PyOptimizeResult {
        status: PyStatus::from_termination(result.status)?,
        x_values: result.x,
        fun: result.fun,
        nit: result.nit,
        nfev: result.nfev,
        njev: result.njev,
        success: result.success,
        message: result.message,
    })
}
