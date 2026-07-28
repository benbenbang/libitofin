//! First-derivative operator.
//!
//! Port of `ql/methods/finitedifferences/operators/firstderivativeop.hpp:33`
//! and its `.cpp`.
//!
//! `testDerivativeWeightsOnNonUniformGrids`
//! (`QuantLib/test-suite/fdmlinearop.cpp:491`) is deferred: it reads the bands
//! back out through `toMatrix` (`:508`, `:510`) over a composite mesh, both of
//! which land with the sparse-matrix work of #636. The apply-based oracles
//! here and in [`second_derivative_op`](super::second_derivative_op) pin the
//! same weights on a uniform grid.

use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::shared::Shared;
use crate::types::Size;

use super::triplebandlinearop::TripleBandLinearOp;

/// The d/dx operator along `direction` of `mesher`
/// (`firstderivativeop.cpp:28-59`).
///
/// The stencil is centred in the interior, and one-sided on the two boundaries
/// along `direction`: upwinding at the first coordinate, downwinding at the
/// last. That leaves `lower` zero at the first coordinate and `upper` zero at
/// the last, which is the precondition
/// [`solve_splitting`](TripleBandLinearOp::solve_splitting) relies on.
///
/// C++ makes this a `TripleBandLinearOp` subclass that adds no members and no
/// behaviour, so the port is a constructor function returning the base - the
/// same treatment
/// [`uniform_1d_mesher`](crate::methods::finitedifferences::meshers::uniform_1d_mesher)
/// gives `Uniform1dMesher`. Every C++ use of the type is a by-value member or
/// a temporary consumed through a base method, so nothing downstream needs the
/// distinct type.
///
/// `hm` and `hp` are read before the boundary branch, as in C++, so on a
/// composite mesh one of them is the `Null` sentinel at a boundary; the branch
/// taken there discards whatever was formed from it.
pub fn first_derivative_op(direction: Size, mesher: Shared<dyn FdmMesher>) -> TripleBandLinearOp {
    let extent = mesher.layout().dim()[direction];

    TripleBandLinearOp::with_bands(direction, mesher, move |mesher, position| {
        let hm = mesher.dminus(position, direction);
        let hp = mesher.dplus(position, direction);

        let zetam1 = hm * (hm + hp);
        let zeta0 = hm * hp;
        let zetap1 = hp * (hm + hp);

        let coordinate = position.coordinates()[direction];
        if coordinate == 0 {
            let upper = 1.0 / hp;
            (0.0, -upper, upper)
        } else if coordinate == extent - 1 {
            let diag = 1.0 / hm;
            (-diag, diag, 0.0)
        } else {
            (-hp / zetam1, (hp - hm) / zeta0, hm / zetap1)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::{FdmLinearOp, FdmLinearOpLayout};
    use crate::shared::shared;

    /// A linear function is differenced exactly by all three stencils, so this
    /// pins the interior weights and both one-sided boundary weights at once.
    #[test]
    fn differences_a_ramp_to_one_everywhere() {
        let layout = shared(FdmLinearOpLayout::new(vec![5]));
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &[(0.0, 1.0)]).unwrap());

        let x = mesher.locations(0);
        let t = first_derivative_op(0, mesher).apply(&x);

        for i in 0..t.size() {
            assert!((t[i] - 1.0).abs() <= 1e-12, "at {i}: {}", t[i]);
        }
    }
}
