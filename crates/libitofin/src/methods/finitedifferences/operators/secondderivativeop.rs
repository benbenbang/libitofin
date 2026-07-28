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
}
