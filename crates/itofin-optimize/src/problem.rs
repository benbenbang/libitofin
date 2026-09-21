use crate::error::InvalidInput;

/// Box bounds, one entry per coordinate.
///
/// A `-inf` lower bound or a `+inf` upper bound leaves that side open, and two
/// equal finite endpoints fix the coordinate. `NaN` is rejected, because `NaN`
/// is reserved for nonfinite evaluations, and so is a pair that admits no
/// finite coordinate: a `+inf` lower bound or a `-inf` upper bound.
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
    /// Checks `x0` and, when present, the shape, the ordering and the
    /// feasibility of the bounds.
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
            if *lower == f64::INFINITY || *upper == f64::NEG_INFINITY {
                return Err(InvalidInput::InfeasibleBound { index });
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

impl NelderMeadOptions {
    fn validate(&self, n: usize) -> Result<(), InvalidInput> {
        for (option, tolerance) in [("xatol", self.xatol), ("fatol", self.fatol)] {
            if !tolerance.is_finite() || tolerance < 0.0 {
                return Err(InvalidInput::NotFiniteNonnegative { option });
            }
        }
        let Some(simplex) = &self.initial_simplex else {
            return Ok(());
        };
        let expected = n + 1;
        if simplex.len() != expected {
            return Err(InvalidInput::SimplexPointCount {
                expected,
                found: simplex.len(),
            });
        }
        for (point, coordinates) in simplex.iter().enumerate() {
            if coordinates.len() != n {
                return Err(InvalidInput::SimplexPointLength {
                    point,
                    expected: n,
                    found: coordinates.len(),
                });
            }
        }
        for (point, coordinates) in simplex.iter().enumerate() {
            if let Some(index) = coordinates.iter().position(|value| !value.is_finite()) {
                return Err(InvalidInput::NonfiniteSimplex { point, index });
            }
        }
        Ok(())
    }
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

    /// Rejects an option this method cannot honour and an option it cannot
    /// read.
    ///
    /// The checks run in a fixed order: the unsupported options first, then the
    /// tolerances, then the shape of an initial simplex, then its coordinates.
    pub fn validate(&self, problem: &Problem) -> Result<(), InvalidInput> {
        if problem.bounds.is_some() && !self.supports_bounds() {
            return Err(InvalidInput::Unsupported {
                method: self.name(),
                option: "bounds",
            });
        }
        match self {
            Method::NelderMead(options) => options.validate(problem.x0.len()),
        }
    }
}
