//! The scheme choice a backward solver switches on.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdmbackwardsolver.hpp:35-59`.
//! `FdmBackwardSolver`, the rest of that header (`:61`), lands with #658.

use crate::types::Real;

/// The scheme families a descriptor can name
/// (`fdmbackwardsolver.hpp:36-40`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FdmSchemeType {
    /// Hundsdorfer-Verwer splitting.
    Hundsdorfer,
    /// Douglas splitting, Crank-Nicolson in one dimension.
    Douglas,
    /// Craig-Sneyd splitting.
    CraigSneyd,
    /// Modified Craig-Sneyd splitting.
    ModifiedCraigSneyd,
    /// Fully implicit Euler.
    ImplicitEuler,
    /// Fully explicit Euler.
    ExplicitEuler,
    /// Method of lines.
    MethodOfLines,
    /// Trapezoidal rule with a second-order backward difference.
    TrBDF2,
    /// Crank-Nicolson.
    CrankNicolson,
}

/// A scheme type and the two parameters it is built from.
///
/// C++ holds all three fields `const` (`fdmbackwardsolver.hpp:44`); they are
/// plain public fields here.
///
/// The header declares ten factories over these nine types
/// (`fdmbackwardsolver.cpp:46-78`) - `ModifiedHundsdorfer` (`cpp:62`) is a
/// second `HundsdorferType` with a different `theta`. Two of the ten are
/// ported, [`douglas`](Self::douglas) and
/// [`implicit_euler`](Self::implicit_euler), which are the schemes #657
/// implements. The other eight - `CrankNicolson`, `CraigSneyd`,
/// `ModifiedCraigSneyd`, `Hundsdorfer`, `ModifiedHundsdorfer`,
/// `ExplicitEuler`, `MethodOfLines` and `TrBDF2` - are omitted rather than
/// accepted and left wrong: every type stays constructible through
/// [`new`](Self::new), and the rollback of #658 answers a type it has no
/// scheme for with an error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FdmSchemeDesc {
    /// The scheme family to step with.
    pub scheme_type: FdmSchemeType,
    /// The implicitness weight of the splitting.
    pub theta: Real,
    /// The second parameter, whose meaning depends on the family.
    pub mu: Real,
}

impl FdmSchemeDesc {
    /// A descriptor with the given type and parameters
    /// (`fdmbackwardsolver.cpp:43-44`).
    pub fn new(scheme_type: FdmSchemeType, theta: Real, mu: Real) -> Self {
        FdmSchemeDesc {
            scheme_type,
            theta,
            mu,
        }
    }

    /// Douglas splitting, the same as Crank-Nicolson in one dimension
    /// (`fdmbackwardsolver.cpp:46`).
    pub fn douglas() -> Self {
        FdmSchemeDesc::new(FdmSchemeType::Douglas, 0.5, 0.0)
    }

    /// Fully implicit Euler (`fdmbackwardsolver.cpp:70-72`).
    pub fn implicit_euler() -> Self {
        FdmSchemeDesc::new(FdmSchemeType::ImplicitEuler, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn douglas_carries_the_cpp_parameters() {
        let desc = FdmSchemeDesc::douglas();

        assert_eq!(desc.scheme_type, FdmSchemeType::Douglas);
        assert_eq!(desc.theta, 0.5);
        assert_eq!(desc.mu, 0.0);
    }

    #[test]
    fn implicit_euler_carries_the_cpp_parameters() {
        let desc = FdmSchemeDesc::implicit_euler();

        assert_eq!(desc.scheme_type, FdmSchemeType::ImplicitEuler);
        assert_eq!(desc.theta, 0.0);
        assert_eq!(desc.mu, 0.0);
    }

    /// The eight unported factories are omitted, not their types: the rollback
    /// of #658 must be able to name every one of them to reject it.
    #[test]
    fn every_scheme_type_is_constructible() {
        let types = [
            FdmSchemeType::Hundsdorfer,
            FdmSchemeType::Douglas,
            FdmSchemeType::CraigSneyd,
            FdmSchemeType::ModifiedCraigSneyd,
            FdmSchemeType::ImplicitEuler,
            FdmSchemeType::ExplicitEuler,
            FdmSchemeType::MethodOfLines,
            FdmSchemeType::TrBDF2,
            FdmSchemeType::CrankNicolson,
        ];

        for scheme_type in types {
            let desc = FdmSchemeDesc::new(scheme_type, 0.25, 0.75);
            assert_eq!(desc.scheme_type, scheme_type);
            assert_eq!(desc.theta, 0.25);
            assert_eq!(desc.mu, 0.75);
        }
    }

    #[test]
    fn the_two_ported_factories_differ() {
        assert_ne!(FdmSchemeDesc::douglas(), FdmSchemeDesc::implicit_euler());
    }
}
