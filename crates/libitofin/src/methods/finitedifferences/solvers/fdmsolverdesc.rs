//! Everything a finite-difference solver needs to roll a grid back.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdmsolverdesc.hpp:35-43`.

use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::methods::finitedifferences::stepconditions::FdmStepConditionComposite;
use crate::methods::finitedifferences::utilities::{
    FdmBoundaryConditionSet, FdmInnerValueCalculator,
};
use crate::shared::Shared;
use crate::types::{Size, Time};

/// The grid, the conditions on it and the time discretisation, bundled.
///
/// C++ holds all seven fields `const` (`fdmsolverdesc.hpp:36-42`); they are
/// plain public fields here, and a descriptor is built as a struct literal the
/// way the C++ aggregate is.
///
/// Copyable in C++, and the copy is load-bearing rather than incidental:
/// `FdmBlackScholesSolver` keeps a descriptor (`fdmblackscholessolver.hpp:61`)
/// and hands it to a fresh `Fdm1DimSolver` on every recalculation
/// (`fdmblackscholessolver.cpp:55`), which stores a copy of its own
/// (`fdm1dimsolver.hpp:54`). [`Clone`] is what those two consumers need; it
/// costs three reference-count bumps and a copy of the boundary-condition
/// vector.
///
/// The `dyn` fields carry no [`Debug`] or [`PartialEq`], so neither is derived.
#[derive(Clone)]
pub struct FdmSolverDesc {
    /// The grid the rollback runs over.
    pub mesher: Shared<dyn FdmMesher>,
    /// The boundary conditions applied at every step, empty on the plain
    /// European path.
    pub bc_set: FdmBoundaryConditionSet,
    /// The step conditions applied between steps.
    pub condition: Shared<FdmStepConditionComposite>,
    /// The seed values on the grid at maturity.
    pub calculator: Shared<dyn FdmInnerValueCalculator>,
    /// The time the rollback starts from.
    pub maturity: Time,
    /// The number of steps the rollback takes.
    pub time_steps: Size,
    /// The number of implicit-Euler steps taken first to damp the payoff kink.
    pub damping_steps: Size,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::CashOrNothingPayoff;
    use crate::methods::finitedifferences::schemes::testops;
    use crate::methods::finitedifferences::utilities::fdm_log_inner_value;
    use crate::option::OptionType::Put;
    use crate::payoff::Payoff;
    use crate::shared::shared;

    /// Nothing constructs a descriptor until #666, so the field types are only
    /// proved to compose by building one here out of the real ported types.
    #[test]
    fn a_descriptor_is_built_from_the_ported_types_and_clones() {
        let mesher = testops::mesher();
        let payoff = shared(CashOrNothingPayoff::new(Put, 100.0, 10.0)) as Shared<dyn Payoff>;
        let desc = FdmSolverDesc {
            mesher: Shared::clone(&mesher),
            bc_set: Vec::new(),
            condition: shared(FdmStepConditionComposite::new(&[], Vec::new())),
            calculator: shared(fdm_log_inner_value(payoff, mesher, 0)),
            maturity: 0.75,
            time_steps: 25,
            damping_steps: 3,
        };

        let copy = desc.clone();

        assert_eq!(copy.maturity, 0.75);
        assert_eq!(copy.time_steps, 25);
        assert_eq!(copy.damping_steps, 3);
        assert!(copy.bc_set.is_empty());
        assert!(copy.condition.stopping_times().is_empty());
    }
}
