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

    use crate::math::array::Array;
    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::{FdmLinearOp, FdmLinearOpLayout};
    use crate::shared::shared;
    use crate::types::Real;

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

    /// `testFirstDerivativesMapApply`,
    /// `QuantLib/test-suite/fdmlinearop.cpp:360`. The grid is two million
    /// points, as in C++, so the walks below use the layout's cursor rather
    /// than [`iter`](FdmLinearOpLayout::iter), which clones a position per
    /// point.
    #[test]
    fn first_derivatives_map_apply_matches_quantlib() {
        let dim = [400, 100, 50];
        let layout = shared(FdmLinearOpLayout::new(dim.to_vec()));
        let boundaries = [(-5.0, 5.0), (0.0, 10.0), (5.0, 15.0)];
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &boundaries).unwrap());

        let map = first_derivative_op(2, Shared::clone(&mesher));

        let mut r = Array::with_size(layout.size());
        let mut position = layout.begin();
        while position.index() < layout.size() {
            r[position.index()] =
                mesher.location(&position, 0).sin() + mesher.location(&position, 2).cos();
            position.advance();
        }

        let t = map.apply(&r);

        let (z_min, z_max) = boundaries[2];
        let dz = (z_max - z_min) / (dim[2] - 1) as Real;

        let mut position = layout.begin();
        while position.index() < layout.size() {
            let z = position.coordinates()[2];
            let z0 = if z > 0 { z - 1 } else { 1 };
            let z2 = if z < dim[2] - 1 { z + 1 } else { dim[2] - 2 };
            let lz0 = z_min + z0 as Real * dz;
            let lz2 = z_min + z2 as Real * dz;

            let expected = if z == 0 {
                ((z_min + dz).cos() - z_min.cos()) / dz
            } else if z == dim[2] - 1 {
                (z_max.cos() - (z_max - dz).cos()) / dz
            } else {
                (lz2.cos() - lz0.cos()) / (2.0 * dz)
            };

            let calculated = t[position.index()];
            assert!(
                (calculated - expected).abs() <= 1e-10,
                "first derivative at {}: {calculated} != {expected}",
                position.index()
            );
            position.advance();
        }
    }
}
