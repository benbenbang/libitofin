//! Tridiagonal operator along one direction of a mesh.
//!
//! Port of `ql/methods/finitedifferences/operators/triplebandlinearop.hpp:37`
//! and its `.cpp`. The operator stores three bands - `lower`, `diag`, `upper` -
//! one entry per grid point, and applies them against each point's neighbours
//! one step down and one step up along `direction`. Every derivative operator
//! in the family is one of these with its bands filled differently.
//!
//! `toMatrix` (`triplebandlinearop.hpp:64`) is omitted with the rest of the
//! sparse-matrix work in #636.
//!
//! No QuantLib test constructs a `TripleBandLinearOp` directly, so the tests
//! below are not an oracle: they are hand-computed products and algebraic
//! identities that pin this port's own behaviour. The oracle is
//! `testTripleBandMapSolve` (`QuantLib/test-suite/fdmlinearop.cpp:755`), which
//! reaches `solve_splitting` through the derivative operators of #640.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::shared::Shared;
use crate::types::{Real, Size};
use crate::{ensure, require};

use super::fdmlinearop::FdmLinearOp;
use super::fdmlinearopiterator::FdmLinearOpIterator;
use super::fdmlinearoplayout::FdmLinearOpLayout;

/// A tridiagonal operator over one direction of an [`FdmMesher`].
#[derive(Clone)]
pub struct TripleBandLinearOp {
    direction: Size,
    i0: Vec<Size>,
    i2: Vec<Size>,
    reverse_index: Vec<Size>,
    lower: Vec<Real>,
    diag: Vec<Real>,
    upper: Vec<Real>,
    mesher: Shared<dyn FdmMesher>,
}

impl TripleBandLinearOp {
    /// The operator over `direction` of `mesher`, with all three bands zero
    /// (`triplebandlinearop.cpp:30-59`).
    ///
    /// C++ leaves the bands uninitialized here; zeroing them keeps the value
    /// well defined. The bandless operator is the target form:
    /// `FdmG2Op` builds its `mapX_` this way (`fdmg2op.cpp:53`) and fills it
    /// from other operators through [`axpyb`](Self::axpyb) on every step.
    /// To build an operator that carries its own bands, use
    /// [`with_bands`](Self::with_bands).
    pub fn new(direction: Size, mesher: Shared<dyn FdmMesher>) -> Self {
        let layout = Shared::clone(mesher.layout());
        let size = layout.size();

        let mut new_dim = layout.dim().to_vec();
        new_dim.swap(0, direction);
        let mut new_spacing = FdmLinearOpLayout::new(new_dim).spacing().to_vec();
        new_spacing.swap(0, direction);

        let mut i0 = vec![0; size];
        let mut i2 = vec![0; size];
        let mut reverse_index = vec![0; size];

        let mut position = layout.begin();
        while position.index() < size {
            let i = position.index();
            i0[i] = layout.neighbourhood(&position, direction, -1);
            i2[i] = layout.neighbourhood(&position, direction, 1);

            let transposed: Size = position
                .coordinates()
                .iter()
                .zip(&new_spacing)
                .map(|(coordinate, stride)| coordinate * stride)
                .sum();
            reverse_index[transposed] = i;

            position.advance();
        }

        TripleBandLinearOp {
            direction,
            i0,
            i2,
            reverse_index,
            lower: vec![0.0; size],
            diag: vec![0.0; size],
            upper: vec![0.0; size],
            mesher,
        }
    }

    /// The operator over `direction` of `mesher`, with `bands` supplying
    /// `(lower, diag, upper)` for each grid point.
    ///
    /// This is the Rust form of C++'s protected default constructor
    /// (`triplebandlinearop.hpp:67`): there, `FirstDerivativeOp` and
    /// `SecondDerivativeOp` derive from this class and fill the protected bands
    /// in their own constructors (`firstderivativeop.cpp:33-58`). Rust has no
    /// implementation inheritance, so the fill loop is inverted into a callback
    /// and those operators compose a `TripleBandLinearOp` instead of extending
    /// it. `bands` is handed the mesher so it can read `dplus`/`dminus` without
    /// having to capture a second handle to it.
    pub fn with_bands(
        direction: Size,
        mesher: Shared<dyn FdmMesher>,
        mut bands: impl FnMut(&dyn FdmMesher, &FdmLinearOpIterator) -> (Real, Real, Real),
    ) -> Self {
        let mut operator = Self::new(direction, Shared::clone(&mesher));
        let layout = Shared::clone(mesher.layout());

        let mut position = layout.begin();
        while position.index() < layout.size() {
            let i = position.index();
            let (lower, diag, upper) = bands(&*mesher, &position);
            operator.lower[i] = lower;
            operator.diag[i] = diag;
            operator.upper[i] = upper;
            position.advance();
        }

        operator
    }

    /// The direction of the mesh this operator differences along.
    pub fn direction(&self) -> Size {
        self.direction
    }

    /// The mesh this operator is defined over.
    pub fn mesher(&self) -> &Shared<dyn FdmMesher> {
        &self.mesher
    }

    /// Solves `(a * self + b * I) x = r` for `x` by the Thomas algorithm
    /// (`triplebandlinearop.cpp:256-300`).
    ///
    /// Note the roles: `a` scales the operator and `b` the identity, so the
    /// implicit step of a splitting scheme passes the timestep as `a` and `1.0`
    /// as `b` (`fdmblackscholesop.cpp:122`). C++ defaults `b` to `1.0`
    /// (`triplebandlinearop.hpp:49`); every call site passes it explicitly, so
    /// the default is dropped rather than emulated.
    ///
    /// The sweep runs over [`reverse_index`](Self::new)'s ordering, which walks
    /// `direction` fastest, so the whole grid is one chain of tridiagonal
    /// systems laid end to end. That is only equivalent to solving each line
    /// separately when the bands vanish where they would reach outside a line -
    /// `lower` at coordinate `0` and `upper` at the last coordinate along
    /// `direction`. Both derivative operators of #640 satisfy this; C++ checks
    /// it under `QL_EXTRA_SAFETY_CHECKS` only (`triplebandlinearop.cpp:259`),
    /// and this port likewise leaves it as an unchecked precondition.
    ///
    /// No pivoting: the algorithm assumes a diagonally dominant system, and
    /// reports a zero pivot as an error rather than working around it.
    #[allow(clippy::needless_range_loop)]
    pub fn solve_splitting(&self, r: &Array, a: Real, b: Real) -> QlResult<Array> {
        let size = self.size();
        require!(r.size() == size, "inconsistent size of rhs");

        let mut result = Array::with_size(size);
        let mut tmp = vec![0.0; size];

        let mut previous = self.reverse_index[0];
        let mut bet = 1.0 / (a * self.diag[previous] + b);
        require!(bet != 0.0, "division by zero");
        result[previous] = r[previous] * bet;

        for j in 1..size {
            let current = self.reverse_index[j];
            tmp[j] = a * self.upper[previous] * bet;

            bet = b + a * (self.diag[current] - tmp[j] * self.lower[current]);
            ensure!(bet != 0.0, "division by zero");
            bet = 1.0 / bet;

            result[current] = (r[current] - a * self.lower[current] * result[previous]) * bet;
            previous = current;
        }

        for j in (1..size.saturating_sub(1)).rev() {
            result[self.reverse_index[j]] -= tmp[j + 1] * result[self.reverse_index[j + 1]];
        }
        if size > 1 {
            result[self.reverse_index[0]] -= tmp[1] * result[self.reverse_index[1]];
        }

        Ok(result)
    }

    fn size(&self) -> Size {
        self.diag.len()
    }
}

impl FdmLinearOp for TripleBandLinearOp {
    /// `triplebandlinearop.cpp:224-240`.
    fn apply(&self, r: &Array) -> Array {
        assert_eq!(r.size(), self.size(), "inconsistent length of r");

        let mut result = Array::with_size(r.size());
        for i in 0..self.size() {
            result[i] =
                r[self.i0[i]] * self.lower[i] + r[i] * self.diag[i] + r[self.i2[i]] * self.upper[i];
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::shared::shared;

    const DIM: [Size; 2] = [3, 4];
    const DIRECTION: Size = 1;
    const TOL: Real = 1e-12;

    fn mesher(dim: &[Size]) -> Shared<dyn FdmMesher> {
        let layout = shared(FdmLinearOpLayout::new(dim.to_vec()));
        let boundaries = vec![(0.0, 1.0); dim.len()];
        shared(UniformGridMesher::new(layout, &boundaries).unwrap())
    }

    /// Bands shaped like the derivative operators of #640: `lower` vanishes at
    /// the first coordinate along `direction` and `upper` at the last, which is
    /// what makes `solve_splitting` the inverse of `apply`. The values are
    /// arbitrary but distinct per point, and diagonally dominant.
    fn band_values(index: Size, coordinate: Size, extent: Size) -> (Real, Real, Real) {
        let i = index as Real;
        let lower = if coordinate == 0 { 0.0 } else { -1.0 - 0.1 * i };
        let upper = if coordinate == extent - 1 {
            0.0
        } else {
            1.0 + 0.2 * i
        };
        (lower, 8.0 + 0.3 * i, upper)
    }

    fn banded(dim: &[Size], direction: Size) -> TripleBandLinearOp {
        let mesher = mesher(dim);
        let extent = dim[direction];
        TripleBandLinearOp::with_bands(direction, mesher, move |_, position| {
            band_values(position.index(), position.coordinates()[direction], extent)
        })
    }

    fn assert_close(actual: &Array, expected: &Array) {
        assert_eq!(actual.size(), expected.size());
        for i in 0..actual.size() {
            assert!(
                (actual[i] - expected[i]).abs() <= TOL,
                "element {i}: {} != {}",
                actual[i],
                expected[i]
            );
        }
    }

    #[test]
    fn reverse_index_walks_the_direction_fastest() {
        let operator = banded(&DIM, DIRECTION);
        let size = DIM.iter().product::<Size>();
        let extent = DIM[DIRECTION];

        let mut seen = vec![false; size];
        for &index in &operator.reverse_index {
            assert!(!seen[index], "reverse_index repeats {index}");
            seen[index] = true;
        }

        for j in 0..size {
            if j % extent != extent - 1 {
                assert_eq!(
                    operator.i2[operator.reverse_index[j]],
                    operator.reverse_index[j + 1],
                    "chain broken between {j} and {}",
                    j + 1
                );
            }
        }
    }

    #[test]
    fn reverse_index_is_the_identity_along_the_first_direction() {
        let operator = banded(&DIM, 0);
        let expected: Vec<Size> = (0..DIM.iter().product::<Size>()).collect();
        assert_eq!(operator.reverse_index, expected);
    }

    #[test]
    fn apply_matches_a_hand_computed_product() {
        let mesher = mesher(&[4]);
        let operator =
            TripleBandLinearOp::with_bands(0, mesher, |_, position| match position.index() {
                0 => (0.0, 10.0, 2.0),
                1 => (1.0, 20.0, 2.0),
                2 => (1.0, 30.0, 2.0),
                _ => (1.0, 40.0, 0.0),
            });

        let r = Array::from([1.0, 2.0, 3.0, 4.0]);
        assert_eq!(operator.apply(&r), Array::from([14.0, 47.0, 100.0, 163.0]));
    }

    #[test]
    fn apply_reaches_the_neighbours_the_layout_names() {
        let operator = banded(&DIM, DIRECTION);
        let layout = Shared::clone(operator.mesher().layout());
        let r = Array::incremental(layout.size(), 1.0, 3.0);

        let mut expected = Array::with_size(layout.size());
        for position in layout.iter() {
            let i = position.index();
            let (lower, diag, upper) =
                band_values(i, position.coordinates()[DIRECTION], DIM[DIRECTION]);
            expected[i] = r[layout.neighbourhood(&position, DIRECTION, -1)] * lower
                + r[i] * diag
                + r[layout.neighbourhood(&position, DIRECTION, 1)] * upper;
        }

        assert_close(&operator.apply(&r), &expected);
    }

    #[test]
    fn solve_splitting_inverts_apply() {
        for direction in 0..DIM.len() {
            let operator = banded(&DIM, direction);
            let x = Array::incremental(operator.size(), 2.0, -0.5);

            let recovered = operator
                .solve_splitting(&operator.apply(&x), 1.0, 0.0)
                .unwrap();
            assert_close(&recovered, &x);
        }
    }

    #[test]
    fn solve_splitting_solves_the_shifted_system() {
        let operator = banded(&DIM, DIRECTION);
        let x = Array::incremental(operator.size(), -1.0, 0.7);
        let (a, b) = (0.25, 1.5);

        let r = &(&operator.apply(&x) * a) + &(&x * b);
        let recovered = operator.solve_splitting(&r, a, b).unwrap();

        assert_close(&recovered, &x);
    }

    #[test]
    fn solve_splitting_rejects_a_mismatched_rhs() {
        let operator = banded(&DIM, DIRECTION);
        let err = operator
            .solve_splitting(&Array::with_size(operator.size() - 1), 1.0, 0.0)
            .unwrap_err();
        assert_eq!(err.message(), "inconsistent size of rhs");
    }

    #[test]
    #[should_panic(expected = "inconsistent length of r")]
    fn apply_rejects_a_mismatched_argument() {
        let operator = banded(&DIM, DIRECTION);
        operator.apply(&Array::with_size(operator.size() + 1));
    }
}
