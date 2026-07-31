//! The Black-Scholes wrapper around the one-dimensional solver.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdmblackscholessolver.hpp:40`
//! and its `.cpp`.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::methods::finitedifferences::operators::{FdmBlackScholesOp, FdmLinearOpComposite};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::types::{Real, Size};

use super::{Fdm1DimSolver, FdmSchemeDesc, FdmSolverDesc};

/// The direction of the mesh the generator is built over (`cpp:49`).
const DIRECTION: Size = 0;

/// Builds the Black-Scholes generator over a mesh, rolls it back through
/// [`Fdm1DimSolver`] and answers value and greeks at a spot.
///
/// The grid is laid out in `ln(S)`, so every accessor takes a spot and passes
/// its logarithm down (`cpp:57-75`); delta and gamma carry the chain rule for
/// that change of variable.
///
/// C++ is a `LazyObject` registered with the process (`hpp:40`, `cpp:41-42`)
/// and this is not. The engine of #668 builds a solver fresh inside its own
/// calculation (`fdblackscholesvanillaengine.cpp:202-207`) and drops it again,
/// so no notification could ever reach one; what the laziness buys is the
/// compute-once behaviour within a single pricing, and that is a plain
/// internal cache.
///
/// Deferred to #636, and omitted rather than accepted and ignored:
///
/// - the local-volatility branch, the `localVol` flag and its
///   `illegalLocalVolOverwrite` fallback (`cpp:49`);
/// - the quanto branch and the `FdmQuantoHelper` it adjusts the drift through
///   (`cpp:50-52`); that helper is not ported.
///
/// [`FdmBlackScholesOp`] takes neither, so both are absent from this
/// constructor rather than accepted and dropped on the floor.
pub struct FdmBlackScholesSolver {
    process: Shared<GeneralizedBlackScholesProcess>,
    strike: Real,
    solver_desc: FdmSolverDesc,
    scheme_desc: FdmSchemeDesc,
    solver: RefCell<Option<Fdm1DimSolver>>,
}

impl FdmBlackScholesSolver {
    /// The solver pricing off `process` at `strike`, over the grid
    /// `solver_desc` describes and under the scheme `scheme_desc` names
    /// (`cpp:30-43`).
    ///
    /// C++ defaults the scheme to `FdmSchemeDesc::Douglas()` (`hpp:45`); the
    /// scheme is named explicitly here, as its one caller already does.
    ///
    /// The process is held by [`Shared`] rather than C++'s
    /// `Handle<GeneralizedBlackScholesProcess>` (`hpp:59`). The handle buys
    /// relinking, which reaches C++ through the observer registration this port
    /// does not carry.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        strike: Real,
        solver_desc: FdmSolverDesc,
        scheme_desc: FdmSchemeDesc,
    ) -> Self {
        FdmBlackScholesSolver {
            process,
            strike,
            solver_desc,
            scheme_desc,
            solver: RefCell::new(None),
        }
    }

    /// The option value at spot `s` (`cpp:57-60`).
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn value_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| solver.interpolate_at(s.ln()))
    }

    /// The delta at spot `s` (`cpp:62-65`), the grid's derivative in `ln(S)`
    /// over `s`.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn delta_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| Ok(solver.derivative_x(s.ln())? / s))
    }

    /// The gamma at spot `s` (`cpp:67-71`), the second derivative in `ln(S)`
    /// less the first, over `s` squared.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn gamma_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| {
            let x = s.ln();

            Ok((solver.derivative_xx(x)? - solver.derivative_x(x)?) / (s * s))
        })
    }

    /// The theta at spot `s` (`cpp:73-75`).
    ///
    /// [`None`] is C++'s `Null<Real>`, which
    /// [`Fdm1DimSolver::theta_at`] returns when the capture would sit on today.
    ///
    /// A deliberate divergence from `cpp:74`: C++ dereferences `solver_`
    /// without calling `calculate()` first, unlike the other three accessors,
    /// and so relies on the engine's fixed value, delta, gamma, theta call
    /// order (`fdblackscholesvanillaengine.cpp:211-214`) having built the
    /// solver already. Ported literally that would leave a caller who asks for
    /// the theta first reading an absent solver. This builds it like the other
    /// three; the numbers are unchanged, only the order they may be asked in.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn theta_at(&self, s: Real) -> QlResult<Option<Real>> {
        self.with_solver(|solver| solver.theta_at(s.ln()))
    }

    /// Builds the generator and the solver over it, once (`cpp:45-55`).
    fn calculate(&self) -> QlResult<()> {
        if self.solver.borrow().is_some() {
            return Ok(());
        }

        let op = shared_mut(FdmBlackScholesOp::new(
            Shared::clone(&self.solver_desc.mesher),
            &self.process,
            self.strike,
            DIRECTION,
        )?) as SharedMut<dyn FdmLinearOpComposite>;
        let solver = Fdm1DimSolver::new(self.solver_desc.clone(), self.scheme_desc, op);

        *self.solver.borrow_mut() = Some(solver);

        Ok(())
    }

    fn with_solver<T>(&self, read: impl FnOnce(&Fdm1DimSolver) -> QlResult<T>) -> QlResult<T> {
        self.calculate()?;

        let cached = self.solver.borrow();
        let solver = cached.as_ref().expect("calculate leaves the solver built");

        read(solver)
    }
}
