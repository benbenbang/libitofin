use crate::error::InvalidInput;
use crate::finite_difference::FiniteDifference;

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
    /// Checks that every budget given is positive and that a shared tolerance
    /// is finite and positive.
    pub fn validate(&self) -> Result<(), InvalidInput> {
        for (option, budget) in [("maxiter", self.maxiter), ("maxfev", self.maxfev)] {
            if budget == Some(0) {
                return Err(InvalidInput::NotPositive { option });
            }
        }
        if let Some(tol) = self.tol
            && (!tol.is_finite() || tol <= 0.0)
        {
            return Err(InvalidInput::NotFinitePositive { option: "tol" });
        }
        Ok(())
    }
}

/// Nelder-Mead options.
///
/// An unset tolerance falls back to [`Common::tol`] and then to `1e-4`, so that
/// a caller who sets only the shared tolerance spreads it over both, and a
/// caller who sets one of them keeps the default for the other.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NelderMeadOptions {
    /// Absolute convergence tolerance on the simplex spread in `x`.
    pub xatol: Option<f64>,
    /// Absolute convergence tolerance on the simplex spread in `f`.
    pub fatol: Option<f64>,
    /// Whether to scale the reflection coefficients with the dimension.
    pub adaptive: bool,
    /// An explicit starting simplex of `n + 1` points.
    pub initial_simplex: Option<Vec<Vec<f64>>>,
}

impl NelderMeadOptions {
    fn validate(&self, n: usize) -> Result<(), InvalidInput> {
        for (option, tolerance) in [("xatol", self.xatol), ("fatol", self.fatol)] {
            if let Some(tolerance) = tolerance
                && (!tolerance.is_finite() || tolerance < 0.0)
            {
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

/// The vector norm a gradient tolerance is measured in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Norm {
    /// The largest absolute component.
    #[default]
    Inf,
    /// The Euclidean length.
    Two,
}

/// BFGS options.
///
/// An unset `gtol` falls back to [`Common::tol`] and then to `1e-5`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BfgsOptions {
    /// The run converges once the gradient norm is at most `gtol`.
    pub gtol: Option<f64>,
    /// Absolute finite-difference step; defaults to the scheme's relative step.
    pub eps: Option<f64>,
    /// The norm `gtol` is measured in.
    pub norm: Norm,
    /// The approximation used when the objective supplies no gradient.
    pub finite_difference: FiniteDifference,
}

impl BfgsOptions {
    fn validate(&self) -> Result<(), InvalidInput> {
        if let Some(gtol) = self.gtol
            && (!gtol.is_finite() || gtol <= 0.0)
        {
            return Err(InvalidInput::NotFinitePositive { option: "gtol" });
        }
        if let Some(eps) = self.eps
            && (!eps.is_finite() || eps <= 0.0)
        {
            return Err(InvalidInput::NotFinitePositive { option: "eps" });
        }
        Ok(())
    }
}

/// Limited-memory BFGS with box constraints.
///
/// An unset `ftol` uses `1e-9`; an unset `gtol` uses `1e-5`. The gradient
/// tolerance applies to the infinity norm of the projected gradient.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LbfgsbOptions {
    /// Relative decrease tolerance on the objective.
    pub ftol: Option<f64>,
    /// Infinity-norm tolerance on the projected gradient.
    pub gtol: Option<f64>,
    /// Maximum stored correction pairs, `10` when unset.
    pub maxcor: Option<usize>,
    /// Absolute finite-difference step, if no analytic gradient is supplied.
    pub eps: Option<f64>,
    /// Finite-difference scheme used when no analytic gradient is supplied.
    pub finite_difference: FiniteDifference,
}

impl LbfgsbOptions {
    fn validate(&self) -> Result<(), InvalidInput> {
        for (option, value) in [("ftol", self.ftol), ("gtol", self.gtol)] {
            if let Some(value) = value
                && (!value.is_finite() || value < 0.0)
            {
                return Err(InvalidInput::NotFiniteNonnegative { option });
            }
        }
        if self.maxcor == Some(0) {
            return Err(InvalidInput::NotPositive { option: "maxcor" });
        }
        if let Some(eps) = self.eps
            && (!eps.is_finite() || eps <= 0.0)
        {
            return Err(InvalidInput::NotFinitePositive { option: "eps" });
        }
        Ok(())
    }
}

/// SLSQP options.
///
/// An unset `ftol` falls back to [`Common::tol`] and then to `1e-6`. The
/// iteration budget comes from [`Common::maxiter`], `100` when unset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlsqpOptions {
    /// The convergence tolerance on the change in the objective, which the
    /// maximum constraint violation must also meet.
    pub ftol: Option<f64>,
}

impl SlsqpOptions {
    fn validate(&self) -> Result<(), InvalidInput> {
        if let Some(ftol) = self.ftol
            && (!ftol.is_finite() || ftol <= 0.0)
        {
            return Err(InvalidInput::NotFinitePositive { option: "ftol" });
        }
        Ok(())
    }
}

/// The solver to run. Later tickets add variants, hence `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    /// The simplex method of Nelder and Mead.
    NelderMead(NelderMeadOptions),
    /// The quasi-Newton method of Broyden, Fletcher, Goldfarb and Shanno.
    Bfgs(BfgsOptions),
    /// Limited-memory BFGS with a projected path and box-constrained subspace.
    Lbfgsb(LbfgsbOptions),
    /// Sequential least-squares quadratic programming, which honours box
    /// bounds and general constraints.
    Slsqp(SlsqpOptions),
}

impl Method {
    /// The SciPy name of the method.
    pub fn name(&self) -> &'static str {
        match self {
            Method::NelderMead(_) => "Nelder-Mead",
            Method::Bfgs(_) => "BFGS",
            Method::Lbfgsb(_) => "L-BFGS-B",
            Method::Slsqp(_) => "SLSQP",
        }
    }

    /// Whether the method honours box bounds.
    pub fn supports_bounds(&self) -> bool {
        match self {
            Method::NelderMead(_) | Method::Bfgs(_) => false,
            Method::Lbfgsb(_) | Method::Slsqp(_) => true,
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
            Method::Bfgs(options) => options.validate(),
            Method::Lbfgsb(options) => options.validate(),
            Method::Slsqp(options) => options.validate(),
        }
    }
}
