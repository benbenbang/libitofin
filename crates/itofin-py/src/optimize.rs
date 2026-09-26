//! The `itofin.optimize` submodule: SciPy-style `minimize` over the
//! finance-independent `itofin-optimize` crate.
//!
//! Distinct from `itofin.optimization`, the QuantLib calibration port. An
//! objective runs outside any bootstrap callback, so it may mutate a
//! `SimpleQuote` and reprice.

use crate::ItofinError;
use itofin_optimize::{
    BfgsOptions, Bounds, Common, ConstraintKind, Converged, Flow, IterationState, LbfgsbOptions,
    Method, Minimize, MinimizeError, NelderMeadOptions, Objective, Problem, SlsqpOptions,
    Termination, minimize as run,
};
use numpy::PyArray1;
use pyo3::exceptions::{PyStopIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// Why a minimize run stopped.
///
/// The integer values are fixed and append-only, matching the C
/// ItofinOptimizeStatus and the Go OptimizeStatus.
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
    LineSearchFailed = 7,
    Infeasible = 8,
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
            Termination::LineSearchFailed => Self::LineSearchFailed,
            Termination::Infeasible => Self::Infeasible,
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
    jac: Option<Bound<'py, PyAny>>,
    callback: Option<Bound<'py, PyAny>>,
    constraints: PyConstraints<'py>,
}

struct PyConstraints<'py> {
    descriptors: Vec<PyConstraint<'py>>,
    components: Vec<(usize, usize)>,
}

struct PyConstraint<'py> {
    kind: ConstraintKind,
    fun: Bound<'py, PyAny>,
    jac: Option<Bound<'py, PyAny>>,
    dimension: usize,
}

fn constraint_values(value: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    if let Ok(scalar) = value.extract::<f64>() {
        return Ok(vec![scalar]);
    }
    let values: Vec<f64> = value.extract()?;
    if values.is_empty() {
        return Err(PyValueError::new_err(
            "constraint fun returned an empty vector",
        ));
    }
    Ok(values)
}

fn parse_constraints<'py>(
    constraints: Option<Bound<'py, PyAny>>,
    x0: &[f64],
) -> PyResult<PyConstraints<'py>> {
    let Some(constraints) = constraints else {
        return Ok(PyConstraints {
            descriptors: Vec::new(),
            components: Vec::new(),
        });
    };
    let descriptors: Vec<Bound<'py, PyDict>> = constraints.extract()?;
    let mut parsed = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let kind = match descriptor
            .get_item("type")?
            .as_ref()
            .and_then(|value| value.extract::<String>().ok())
            .as_deref()
        {
            Some("eq") => ConstraintKind::Eq,
            Some("ineq") => ConstraintKind::Ineq,
            _ => {
                return Err(PyValueError::new_err(
                    "constraint type must be 'eq' or 'ineq'",
                ));
            }
        };
        let fun = descriptor
            .get_item("fun")?
            .ok_or_else(|| PyValueError::new_err("constraint fun is required"))?;
        if !fun.is_callable() {
            return Err(PyValueError::new_err("constraint fun must be callable"));
        }
        let jac = descriptor.get_item("jac")?.filter(|value| !value.is_none());
        if jac.as_ref().is_some_and(|value| !value.is_callable()) {
            return Err(PyValueError::new_err("constraint jac must be callable"));
        }
        parsed.push(PyConstraint {
            kind,
            fun,
            jac,
            dimension: 0,
        });
    }
    let mut components = Vec::new();
    for (index, descriptor) in parsed.iter_mut().enumerate() {
        let point = PyArray1::from_slice(descriptor.fun.py(), x0);
        descriptor.dimension = constraint_values(&descriptor.fun.call1((point,))?)?.len();
        components.extend((0..descriptor.dimension).map(|row| (index, row)));
    }
    Ok(PyConstraints {
        descriptors: parsed,
        components,
    })
}

impl Objective for PyObjective<'_> {
    type Error = PyErr;

    fn value(&mut self, x: &[f64]) -> PyResult<f64> {
        let point = PyArray1::from_slice(self.fun.py(), x);
        self.fun.call1((point,))?.extract::<f64>()
    }

    fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> PyResult<bool> {
        let Some(jac) = &self.jac else {
            return Ok(false);
        };
        let point = PyArray1::from_slice(jac.py(), x);
        let values: Vec<f64> = jac.call1((point,))?.extract()?;
        if values.len() != out.len() {
            return Err(PyValueError::new_err(
                "jac returned the wrong gradient length",
            ));
        }
        out.copy_from_slice(&values);
        Ok(true)
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

    fn constraint_count(&self) -> usize {
        self.constraints.components.len()
    }

    fn constraint_kind(&self, i: usize) -> ConstraintKind {
        self.constraints.descriptors[self.constraints.components[i].0].kind
    }

    fn constraint(&mut self, i: usize, x: &[f64]) -> PyResult<f64> {
        let (index, row) = self.constraints.components[i];
        let descriptor = &self.constraints.descriptors[index];
        let point = PyArray1::from_slice(descriptor.fun.py(), x);
        let values = constraint_values(&descriptor.fun.call1((point,))?)?;
        if values.len() != descriptor.dimension {
            return Err(PyValueError::new_err(
                "constraint fun changed output dimension",
            ));
        }
        Ok(values[row])
    }

    fn constraint_jacobian(&mut self, i: usize, x: &[f64], out: &mut [f64]) -> PyResult<bool> {
        let (index, row) = self.constraints.components[i];
        let descriptor = &self.constraints.descriptors[index];
        let Some(jac) = &descriptor.jac else {
            return Ok(false);
        };
        let point = PyArray1::from_slice(jac.py(), x);
        let value = jac.call1((point,))?;
        let values = if let Ok(values) = value.extract::<Vec<f64>>() {
            if descriptor.dimension != 1 {
                return Err(PyValueError::new_err(
                    "constraint jac must return one row per constraint component",
                ));
            }
            values
        } else {
            let rows: Vec<Vec<f64>> = value.extract()?;
            if rows.len() != descriptor.dimension {
                return Err(PyValueError::new_err(
                    "constraint jac returned the wrong row count",
                ));
            }
            rows[row].clone()
        };
        if values.len() != out.len() {
            return Err(PyValueError::new_err(
                "constraint jac returned the wrong gradient length",
            ));
        }
        out.copy_from_slice(&values);
        Ok(true)
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

fn bfgs(options: Option<&Bound<'_, PyDict>>) -> PyResult<(BfgsOptions, Common)> {
    let mut method = BfgsOptions::default();
    let mut common = Common::default();
    for (key, value) in options.into_iter().flat_map(|options| options.iter()) {
        match key.extract::<String>()?.as_str() {
            "maxiter" => common.maxiter = value.extract()?,
            "gtol" => method.gtol = value.extract()?,
            "eps" => method.eps = value.extract()?,
            other => {
                return Err(PyValueError::new_err(format!(
                    "method BFGS does not support option {other}"
                )));
            }
        }
    }
    Ok((method, common))
}

fn lbfgsb(options: Option<&Bound<'_, PyDict>>) -> PyResult<(LbfgsbOptions, Common)> {
    let mut method = LbfgsbOptions::default();
    let mut common = Common::default();
    for (key, value) in options.into_iter().flat_map(|options| options.iter()) {
        match key.extract::<String>()?.as_str() {
            "maxiter" => common.maxiter = value.extract()?,
            "maxfev" => common.maxfev = value.extract()?,
            "maxcor" => method.maxcor = value.extract()?,
            "ftol" => method.ftol = value.extract()?,
            "gtol" => method.gtol = value.extract()?,
            "eps" => method.eps = value.extract()?,
            other => {
                return Err(PyValueError::new_err(format!(
                    "method L-BFGS-B does not support option {other}"
                )));
            }
        }
    }
    Ok((method, common))
}

fn slsqp(options: Option<&Bound<'_, PyDict>>) -> PyResult<(SlsqpOptions, Common)> {
    let mut method = SlsqpOptions::default();
    let mut common = Common::default();
    for (key, value) in options.into_iter().flat_map(|options| options.iter()) {
        match key.extract::<String>()?.as_str() {
            "maxiter" => common.maxiter = value.extract()?,
            "maxfev" => common.maxfev = value.extract()?,
            "ftol" => method.ftol = value.extract()?,
            other => {
                return Err(PyValueError::new_err(format!(
                    "method SLSQP does not support option {other}"
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
///     method (str): "Nelder-Mead", "BFGS", "L-BFGS-B", or "SLSQP" (any case).
///     options (dict | None): Nelder-Mead accepts maxiter, maxfev, xatol,
///         fatol and adaptive. BFGS accepts maxiter, gtol and eps. L-BFGS-B
///         accepts maxiter, maxfev, maxcor, ftol, gtol and eps. SLSQP accepts
///         maxiter, maxfev and ftol.
///     callback (Callable | None): Called as callback(xk) after every
///         iteration. Raising StopIteration stops the run with
///         Status.Cancelled.
///     jac (Callable | None): Analytic objective gradient for BFGS, L-BFGS-B
///         or SLSQP, called as jac(x).
///     bounds: L-BFGS-B or SLSQP pairs of (lower, upper), with None for an
///         open side. Other methods reject bounds.
///     constraints (Sequence[dict] | None): SLSQP constraints with type "eq"
///         (fun(x) == 0) or "ineq" (fun(x) >= 0), fun(x) returning a scalar
///         or vector, and optional jac(x) returning a gradient or 2-D rows.
///
/// Returns:
///     OptimizeResult: The best point found and why the run stopped.
///
/// Raises:
///     ItofinError: On an unknown method or option, or a rejected input.
///     Exception: Whatever fun, jac, a constraint or callback raised, re-raised unchanged.
#[gen_stub_pyfunction(module = "itofin.optimize")]
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "SciPy-style Python signature")]
#[pyo3(signature = (fun, x0, method = "Nelder-Mead", options = None, callback = None, jac = None, bounds = None, constraints = None))]
pub(crate) fn minimize(
    #[gen_stub(override_type(type_repr = "typing.Callable[[numpy.typing.NDArray[numpy.float64]], float]", imports = ("typing", "numpy", "numpy.typing")))]
    fun: Bound<'_, PyAny>,
    x0: Vec<f64>,
    method: &str,
    options: Option<Bound<'_, PyDict>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[numpy.typing.NDArray[numpy.float64]], object]]", imports = ("typing", "numpy", "numpy.typing")))]
    callback: Option<Bound<'_, PyAny>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[numpy.typing.NDArray[numpy.float64]], typing.Sequence[float]]]", imports = ("typing", "numpy", "numpy.typing")))]
    jac: Option<Bound<'_, PyAny>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Sequence[tuple[typing.Optional[float], typing.Optional[float]]]]", imports = ("typing")))]
    bounds: Option<Bound<'_, PyAny>>,
    #[gen_stub(override_type(type_repr = "typing.Optional[typing.Sequence[dict[str, object]]]", imports = ("typing")))]
    constraints: Option<Bound<'_, PyAny>>,
) -> PyResult<PyOptimizeResult> {
    let is_lbfgsb = method.eq_ignore_ascii_case("l-bfgs-b");
    let is_slsqp = method.eq_ignore_ascii_case("slsqp");
    if bounds.is_some() && !is_lbfgsb && !is_slsqp {
        return Err(PyValueError::new_err(format!(
            "method {method} does not support bounds"
        )));
    }
    if constraints.is_some() && !is_slsqp {
        return Err(PyValueError::new_err(format!(
            "method {method} does not support constraints"
        )));
    }
    let (method, common) = if method.eq_ignore_ascii_case("nelder-mead") {
        if jac.is_some() {
            return Err(PyValueError::new_err(
                "method Nelder-Mead does not support jac",
            ));
        }
        let (options, common) = nelder_mead(options.as_ref())?;
        (Method::NelderMead(options), common)
    } else if method.eq_ignore_ascii_case("bfgs") {
        let (options, common) = bfgs(options.as_ref())?;
        (Method::Bfgs(options), common)
    } else if is_lbfgsb {
        let (options, common) = lbfgsb(options.as_ref())?;
        (Method::Lbfgsb(options), common)
    } else if is_slsqp {
        let (options, common) = slsqp(options.as_ref())?;
        (Method::Slsqp(options), common)
    } else {
        return Err(ItofinError::new_err(format!("unknown method {method}")));
    };
    let bounds = if let Some(bounds) = bounds {
        let pairs: Vec<(Option<f64>, Option<f64>)> = bounds.extract()?;
        if pairs.len() != x0.len() {
            return Err(PyValueError::new_err("bounds length must match x0"));
        }
        let (lower, upper) = pairs
            .into_iter()
            .map(|(lo, hi)| (lo.unwrap_or(f64::NEG_INFINITY), hi.unwrap_or(f64::INFINITY)))
            .unzip();
        Some(Bounds { lower, upper })
    } else {
        None
    };
    let problem = Problem { x0, bounds };
    if is_slsqp {
        problem
            .validate()
            .and_then(|_| common.validate())
            .and_then(|_| method.validate(&problem))
            .map_err(|error| ItofinError::new_err(error.to_string()))?;
    }
    let probe = if is_slsqp {
        match &problem.bounds {
            Some(bounds) => problem
                .x0
                .iter()
                .enumerate()
                .map(|(i, value)| value.clamp(bounds.lower[i], bounds.upper[i]))
                .collect(),
            None => problem.x0.clone(),
        }
    } else {
        Vec::new()
    };
    let constraints = parse_constraints(constraints, &probe)?;
    let mut objective = PyObjective {
        fun,
        jac,
        callback,
        constraints,
    };
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
