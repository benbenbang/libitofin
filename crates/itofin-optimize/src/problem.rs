use crate::error::InvalidInput;

/// Box bounds. An infinite entry leaves that side open; `NaN` is rejected,
/// because `NaN` is reserved for nonfinite evaluations.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    /// One lower bound per coordinate.
    pub lower: Vec<f64>,
    /// One upper bound per coordinate.
    pub upper: Vec<f64>,
}

/// What to minimize and where to start.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// The starting point. Its length is the dimension of the problem.
    pub x0: Vec<f64>,
    /// Optional box bounds, honoured only by the methods that support them.
    pub bounds: Option<Bounds>,
}

impl Problem {
    /// Checks `x0` and, when present, the shape and ordering of the bounds.
    pub fn validate(&self) -> Result<(), InvalidInput> {
        if self.x0.is_empty() {
            return Err(InvalidInput::EmptyX0);
        }
        if let Some(index) = self.x0.iter().position(|value| !value.is_finite()) {
            return Err(InvalidInput::NonfiniteX0 { index });
        }
        let Some(bounds) = &self.bounds else {
            return Ok(());
        };
        let expected = self.x0.len();
        for side in [&bounds.lower, &bounds.upper] {
            if side.len() != expected {
                return Err(InvalidInput::BoundsLength {
                    expected,
                    found: side.len(),
                });
            }
            if let Some(index) = side.iter().position(|value| value.is_nan()) {
                return Err(InvalidInput::NanBound { index });
            }
        }
        for (index, (lower, upper)) in bounds.lower.iter().zip(&bounds.upper).enumerate() {
            if lower > upper {
                return Err(InvalidInput::BoundsOrder { index });
            }
        }
        Ok(())
    }
}

/// The budgets and the tolerance every method understands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Common {
    /// Maximum iterations, unlimited when absent.
    pub maxiter: Option<usize>,
    /// Maximum objective evaluations, unlimited when absent.
    pub maxfev: Option<usize>,
    /// A single tolerance the method spreads over its own tolerances.
    pub tol: Option<f64>,
}

impl Common {
    /// Checks that every budget and tolerance given is positive.
    pub fn validate(&self) -> Result<(), InvalidInput> {
        for (option, budget) in [("maxiter", self.maxiter), ("maxfev", self.maxfev)] {
            if budget == Some(0) {
                return Err(InvalidInput::NotPositive { option });
            }
        }
        if let Some(tol) = self.tol
            && (tol.is_nan() || tol <= 0.0)
        {
            return Err(InvalidInput::NotPositive { option: "tol" });
        }
        Ok(())
    }
}

/// Nelder-Mead options, with the SciPy defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct NelderMeadOptions {
    /// Absolute convergence tolerance on the simplex spread in `x`.
    pub xatol: f64,
    /// Absolute convergence tolerance on the simplex spread in `f`.
    pub fatol: f64,
    /// Whether to scale the reflection coefficients with the dimension.
    pub adaptive: bool,
    /// An explicit starting simplex of `n + 1` points.
    pub initial_simplex: Option<Vec<Vec<f64>>>,
}

impl Default for NelderMeadOptions {
    fn default() -> Self {
        Self {
            xatol: 1e-4,
            fatol: 1e-4,
            adaptive: false,
            initial_simplex: None,
        }
    }
}

/// The solver to run. Later tickets add variants, hence `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    /// The simplex method of Nelder and Mead.
    NelderMead(NelderMeadOptions),
}

impl Method {
    /// The SciPy name of the method.
    pub fn name(&self) -> &'static str {
        match self {
            Method::NelderMead(_) => "Nelder-Mead",
        }
    }

    /// Whether the method honours box bounds.
    pub fn supports_bounds(&self) -> bool {
        match self {
            Method::NelderMead(_) => false,
        }
    }

    /// Rejects a problem carrying an option this method cannot honour.
    pub fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        if problem.bounds.is_some() && !self.supports_bounds() {
            return Err(InvalidInput::Unsupported {
                method: self.name(),
                option: "bounds",
            });
        }
        Ok(())
    }
}
