//! Second-derivative operator.
//!
//! Port of `ql/methods/finitedifferences/operators/secondderivativeop.hpp:33`
//! and its `.cpp`.

use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::shared::Shared;
use crate::types::Size;

use super::triplebandlinearop::TripleBandLinearOp;

/// The d2/dx2 operator along `direction` of `mesher`
/// (`secondderivativeop.cpp:28-52`).
///
/// All three bands vanish on the two boundaries along `direction`: the second
/// derivative is simply not evaluated there, and the scheme that uses the
/// operator supplies its own boundary condition. Interior points get the
/// three-point stencil of the non-uniform grid.
///
/// The port is a constructor function returning `TripleBandLinearOp` for the
/// reason given on
/// [`first_derivative_op`](super::first_derivative_op).
pub fn second_derivative_op(direction: Size, mesher: Shared<dyn FdmMesher>) -> TripleBandLinearOp {
    let extent = mesher.layout().dim()[direction];

    TripleBandLinearOp::with_bands(direction, mesher, move |mesher, position| {
        let hm = mesher.dminus(position, direction);
        let hp = mesher.dplus(position, direction);

        let zetam1 = hm * (hm + hp);
        let zeta0 = hm * hp;
        let zetap1 = hp * (hm + hp);

        let coordinate = position.coordinates()[direction];
        if coordinate == 0 || coordinate == extent - 1 {
            (0.0, 0.0, 0.0)
        } else {
            (2.0 / zetam1, -2.0 / zeta0, 2.0 / zetap1)
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

    /// A quadratic is differenced exactly by the interior stencil, and the
    /// boundaries return zero rather than a one-sided approximation.
    #[test]
    fn differences_a_parabola_to_two_inside_and_zero_on_the_boundary() {
        let extent = 5;
        let layout = shared(FdmLinearOpLayout::new(vec![extent]));
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &[(0.0, 1.0)]).unwrap());

        let x = mesher.locations(0);
        let square: Array = (0..x.size()).map(|i| x[i] * x[i]).collect();
        let t = second_derivative_op(0, mesher).apply(&square);

        for i in 0..t.size() {
            let expected: Real = if i == 0 || i == extent - 1 { 0.0 } else { 2.0 };
            assert!((t[i] - expected).abs() <= 1e-12, "at {i}: {}", t[i]);
        }
    }

    /// `testSecondDerivativesMapApply`,
    /// `QuantLib/test-suite/fdmlinearop.cpp:413`. C++ writes the three
    /// directions as three copies of one loop; they differ only in the sign
    /// the exact second derivative of `sin(x) cos(y) exp(z)` carries along
    /// that direction, so they are one loop over that sign here.
    #[test]
    fn second_derivatives_map_apply_matches_quantlib() {
        let dim = [50, 50, 50];
        let layout = shared(FdmLinearOpLayout::new(dim.to_vec()));
        let boundaries = [(0.0, 0.5); 3];
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &boundaries).unwrap());

        let value = |position: &_| {
            mesher.location(position, 0).sin()
                * mesher.location(position, 1).cos()
                * mesher.location(position, 2).exp()
        };

        let mut r = Array::with_size(layout.size());
        let mut position = layout.begin();
        while position.index() < layout.size() {
            r[position.index()] = value(&position);
            position.advance();
        }

        let tol = 5e-2;
        for (direction, sign) in [(0, -1.0), (1, -1.0), (2, 1.0)] {
            let t = second_derivative_op(direction, Shared::clone(&mesher)).apply(&r);

            let mut position = layout.begin();
            while position.index() < layout.size() {
                let coordinate = position.coordinates()[direction];
                let expected: Real = if coordinate == 0 || coordinate == dim[direction] - 1 {
                    0.0
                } else {
                    sign * value(&position)
                };

                let calculated = t[position.index()];
                assert!(
                    (calculated - expected).abs() <= tol,
                    "d2/dx{direction}2 at {}: {calculated} != {expected}",
                    position.index()
                );
                position.advance();
            }
        }
    }
}
