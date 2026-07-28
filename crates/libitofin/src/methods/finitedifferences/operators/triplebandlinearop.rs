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
//! No QuantLib test constructs a `TripleBandLinearOp` directly, so the
//! hand-computed products and algebraic identities below are not an oracle:
//! they pin this port's own behaviour. The oracle is `testTripleBandMapSolve`
//! (`QuantLib/test-suite/fdmlinearop.cpp:756`), which reaches
//! `solve_splitting` through the derivative operators of #640; it is ported
//! alongside them.

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

    /// Overwrites this operator's bands with `a * x + y`, adding `b` to the
    /// diagonal (`triplebandlinearop.cpp:89-158`).
    ///
    /// `a` and `b` each broadcast: empty drops the term entirely, one element
    /// scales every row by it, and a full-length array scales row by row. C++
    /// reaches this through pointer arithmetic with a zeroed stride
    /// (`triplebandlinearop.cpp:114`), which silently reads past the end at any
    /// other length; here the length is checked.
    ///
    /// Only the bands are read, not the index maps, so `x` and `y` are expected
    /// to run along the same direction of the same mesh as this operator. C++
    /// does not check that either.
    ///
    /// Taking `x` and `y` as shared references means this operator cannot also
    /// be `x` or `y`. No caller needs that: `x == y` is the common shape
    /// (`fdmg2op.cpp:69`, `fdmsabrop.cpp:54`) and works, while the target is
    /// always a distinct operator across every call site in `ql/`.
    pub fn axpyb(&mut self, a: &Array, x: &TripleBandLinearOp, y: &TripleBandLinearOp, b: &Array) {
        let size = self.size();
        assert!(broadcasts_over(a, size), "inconsistent size of a");
        assert!(broadcasts_over(b, size), "inconsistent size of b");

        let a_stride = if a.size() > 1 { 1 } else { 0 };
        let b_stride = if b.size() > 1 { 1 } else { 0 };

        for i in 0..size {
            let (lower, mut diag, upper) = if a.is_empty() {
                (y.lower[i], y.diag[i], y.upper[i])
            } else {
                let s = a[i * a_stride];
                (
                    y.lower[i] + s * x.lower[i],
                    y.diag[i] + s * x.diag[i],
                    y.upper[i] + s * x.upper[i],
                )
            };
            if !b.is_empty() {
                diag += b[i * b_stride];
            }

            self.lower[i] = lower;
            self.diag[i] = diag;
            self.upper[i] = upper;
        }
    }

    /// A copy of this operator with row `i` scaled by `u[i]`, that is
    /// `diag(u) * self` (`triplebandlinearop.cpp:175-189`).
    pub fn mult(&self, u: &Array) -> TripleBandLinearOp {
        assert_eq!(u.size(), self.size(), "inconsistent size of u");
        self.map_bands(|i, lower, diag, upper| {
            let s = u[i];
            (lower * s, diag * s, upper * s)
        })
    }

    /// A copy of this operator with each band scaled by `u` sampled at the
    /// entry that band multiplies, that is `self * diag(u)`
    /// (`triplebandlinearop.cpp:191-207`).
    ///
    /// `u` is sampled at the flat positions `i - 1`, `i` and `i + 1`, not at
    /// the neighbours along `direction` that the bands actually reach, and
    /// `1.0` stands in at the two ends of the flat array rather than at the
    /// ends of each line. The two agree only on a one-dimensional mesh. This
    /// port keeps the C++ indexing exactly (`triplebandlinearop.cpp:198-200`),
    /// because it is what the derivative operators are calibrated against.
    pub fn mult_r(&self, u: &Array) -> TripleBandLinearOp {
        let size = self.size();
        assert_eq!(u.size(), size, "inconsistent size of rhs");
        self.map_bands(|i, lower, diag, upper| {
            let previous = if i > 0 { u[i - 1] } else { 1.0 };
            let next = if i + 1 < size { u[i + 1] } else { 1.0 };
            (lower * previous, diag * u[i], upper * next)
        })
    }

    /// The band-wise sum of this operator and `m`
    /// (`triplebandlinearop.cpp:160-172`).
    ///
    /// C++ overloads `add` on the argument type (`triplebandlinearop.hpp:55-56`);
    /// Rust names the two forms separately. Both return a new operator rather
    /// than mutating, as in C++.
    pub fn add_op(&self, m: &TripleBandLinearOp) -> TripleBandLinearOp {
        self.map_bands(|i, lower, diag, upper| {
            (lower + m.lower[i], diag + m.diag[i], upper + m.upper[i])
        })
    }

    /// A copy of this operator with `u` added to its diagonal
    /// (`triplebandlinearop.cpp:209-222`).
    pub fn add_diagonal(&self, u: &Array) -> TripleBandLinearOp {
        assert_eq!(u.size(), self.size(), "inconsistent size of u");
        self.map_bands(|i, lower, diag, upper| (lower, diag + u[i], upper))
    }

    /// C++ builds the result through the public constructor, which recomputes
    /// the index maps from the mesh (`triplebandlinearop.cpp:162`). Cloning
    /// copies them instead, as the C++ copy constructor does
    /// (`triplebandlinearop.cpp:61-78`); they depend only on the mesh and the
    /// direction, which the result shares.
    fn map_bands(
        &self,
        bands: impl Fn(Size, Real, Real, Real) -> (Real, Real, Real),
    ) -> TripleBandLinearOp {
        let mut result = self.clone();
        for i in 0..self.size() {
            let (lower, diag, upper) = bands(i, self.lower[i], self.diag[i], self.upper[i]);
            result.lower[i] = lower;
            result.diag[i] = diag;
            result.upper[i] = upper;
        }
        result
    }

    fn size(&self) -> Size {
        self.diag.len()
    }
}

fn broadcasts_over(values: &Array, size: Size) -> bool {
    values.is_empty() || values.size() == 1 || values.size() == size
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
    use crate::methods::finitedifferences::operators::{first_derivative_op, second_derivative_op};
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

    /// A second operator over the same mesh and direction, so that its bands
    /// address the same neighbours as `operator`'s.
    fn alternate(operator: &TripleBandLinearOp) -> TripleBandLinearOp {
        TripleBandLinearOp::with_bands(
            operator.direction(),
            Shared::clone(operator.mesher()),
            |_, position| {
                let i = position.index() as Real;
                (0.5 - 0.05 * i, 2.0 * i, 1.0 + 0.15 * i)
            },
        )
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
    fn axpyb_broadcasts_a_and_b_over_the_grid() {
        let operator = banded(&DIM, DIRECTION);
        let size = operator.size();
        let r = Array::incremental(size, 1.0, 2.0);
        let applied = operator.apply(&r);

        let scalar = 3.0;
        let vector = Array::incremental(size, 0.5, 0.25);
        let shift = Array::incremental(size, -2.0, 0.125);

        let cases: [(Array, Array); 4] = [
            (Array::new(), Array::new()),
            (Array::new(), Array::filled(1, 0.75)),
            (Array::filled(1, scalar), Array::new()),
            (vector.clone(), shift.clone()),
        ];

        for (a, b) in cases {
            let mut target = TripleBandLinearOp::new(DIRECTION, Shared::clone(operator.mesher()));
            target.axpyb(&a, &operator, &operator, &b);

            let mut expected = Array::with_size(size);
            for i in 0..size {
                let scale = if a.is_empty() {
                    0.0
                } else if a.size() == 1 {
                    a[0]
                } else {
                    a[i]
                };
                let bias = if b.is_empty() {
                    0.0
                } else if b.size() == 1 {
                    b[0]
                } else {
                    b[i]
                };
                expected[i] = applied[i] * (1.0 + scale) + bias * r[i];
            }

            assert_close(&target.apply(&r), &expected);
        }
    }

    #[test]
    fn axpyb_combines_two_distinct_operators() {
        let x = banded(&DIM, DIRECTION);
        let y = alternate(&x);
        let size = x.size();
        let r = Array::incremental(size, 3.0, -1.0);
        let a = Array::incremental(size, 1.0, 0.5);

        let mut target = TripleBandLinearOp::new(DIRECTION, Shared::clone(x.mesher()));
        target.axpyb(&a, &x, &y, &Array::new());

        let (applied_x, applied_y) = (x.apply(&r), y.apply(&r));
        let expected: Array = (0..size)
            .map(|i| applied_y[i] + a[i] * applied_x[i])
            .collect();

        assert_close(&target.apply(&r), &expected);
    }

    #[test]
    #[should_panic(expected = "inconsistent size of a")]
    fn axpyb_rejects_an_unbroadcastable_a() {
        let operator = banded(&DIM, DIRECTION);
        let mut target = TripleBandLinearOp::new(DIRECTION, Shared::clone(operator.mesher()));
        target.axpyb(&Array::with_size(2), &operator, &operator, &Array::new());
    }

    #[test]
    #[should_panic(expected = "inconsistent length of r")]
    fn apply_rejects_a_mismatched_argument() {
        let operator = banded(&DIM, DIRECTION);
        operator.apply(&Array::with_size(operator.size() + 1));
    }

    #[test]
    fn mult_scales_each_row() {
        let operator = banded(&DIM, DIRECTION);
        let size = operator.size();
        let r = Array::incremental(size, 1.0, 1.5);
        let u = Array::incremental(size, 0.5, 0.25);

        let applied = operator.apply(&r);
        let expected: Array = (0..size).map(|i| u[i] * applied[i]).collect();

        assert_close(&operator.mult(&u).apply(&r), &expected);
    }

    #[test]
    fn mult_r_scales_each_column() {
        let operator = banded(&[4], 0);
        let r = Array::from([1.0, 2.0, 3.0, 4.0]);
        let u = Array::from([0.5, -1.5, 2.0, 3.5]);

        let scaled: Array = (0..r.size()).map(|i| u[i] * r[i]).collect();
        assert_close(&operator.mult_r(&u).apply(&r), &operator.apply(&scaled));
    }

    #[test]
    fn mult_r_substitutes_one_at_the_flat_array_ends() {
        let operator = TripleBandLinearOp::with_bands(0, mesher(&[3]), |_, _| (1.0, 1.0, 1.0));
        let scaled = operator.mult_r(&Array::with_size(3));

        assert_eq!(
            scaled.apply(&Array::from([1.0, 2.0, 3.0])),
            Array::from([2.0, 0.0, 2.0])
        );
    }

    #[test]
    fn add_op_sums_the_operators() {
        let operator = banded(&DIM, DIRECTION);
        let other = alternate(&operator);
        let r = Array::incremental(operator.size(), 2.0, -0.75);

        let expected = &operator.apply(&r) + &other.apply(&r);
        assert_close(&operator.add_op(&other).apply(&r), &expected);
    }

    #[test]
    fn add_diagonal_adds_to_the_diagonal() {
        let operator = banded(&DIM, DIRECTION);
        let size = operator.size();
        let r = Array::incremental(size, 1.0, 0.5);
        let u = Array::incremental(size, -1.0, 0.3);

        let applied = operator.apply(&r);
        let expected: Array = (0..size).map(|i| applied[i] + u[i] * r[i]).collect();

        assert_close(&operator.add_diagonal(&u).apply(&r), &expected);
    }

    #[test]
    #[should_panic(expected = "inconsistent size of rhs")]
    fn mult_r_rejects_a_mismatched_argument() {
        let operator = banded(&DIM, DIRECTION);
        operator.mult_r(&Array::with_size(operator.size() - 1));
    }

    /// `testTripleBandMapSolve`, `QuantLib/test-suite/fdmlinearop.cpp:756`.
    ///
    /// Two C++ shapes have no direct Rust equivalent. `dy.axpyb(a, dy, dy, b)`
    /// (`:770`) and `dxx.axpyb(a, dxx, dx, b)` (`:805`) alias the target with a
    /// source, which the borrow checker rejects; the C++ loop reads a slot's
    /// `x` and `y` before writing that slot and never looks ahead
    /// (`triplebandlinearop.cpp:106-156`), so sourcing from a copy taken
    /// beforehand produces the same bands. And `copyOfDxx.add(...)` (`:821`)
    /// returns a new operator, leaving `copyOfDxx` untouched, so that arm only
    /// exercises the assignment on the next line - here a clone.
    #[test]
    fn triple_band_map_solve_matches_quantlib() {
        let layout = shared(FdmLinearOpLayout::new(vec![100, 400]));
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &[(0.0, 1.0); 2]).unwrap());

        let one = Array::filled(1, 1.0);

        let u: Array = (0..layout.size())
            .map(|i| (0.1 * i as Real).sin() + (0.35 * i as Real).cos())
            .collect();
        let assert_recovers = |recovered: &Array| {
            for i in 0..u.size() {
                assert!(
                    (u[i] - recovered[i]).abs() <= 1e-6,
                    "solve and apply are not consistent at {i}: {} != {}",
                    recovered[i],
                    u[i]
                );
            }
        };

        let mut dy = first_derivative_op(1, Shared::clone(&mesher));
        let dy_before = dy.clone();
        dy.axpyb(&Array::filled(1, 2.0), &dy_before, &dy_before, &one);
        let copy_of_dy = dy.clone();

        assert_recovers(&dy.solve_splitting(&copy_of_dy.apply(&u), 1.0, 0.0).unwrap());

        let mut dx = first_derivative_op(0, Shared::clone(&mesher));
        let dx_before = dx.clone();
        dx.axpyb(&Array::new(), &dx_before, &dx_before, &one);
        let copy_of_dx = dx.clone();

        assert_recovers(&dx.solve_splitting(&copy_of_dx.apply(&u), 1.0, 0.0).unwrap());

        let mut dxx = second_derivative_op(0, Shared::clone(&mesher));
        let dxx_before = dxx.clone();
        dxx.axpyb(&Array::filled(1, 0.5), &dxx_before, &dx, &one);
        let copy_of_dxx = dxx.clone();

        assert_recovers(
            &dxx.solve_splitting(&copy_of_dxx.apply(&u), 1.0, 0.0)
                .unwrap(),
        );

        let _ = copy_of_dxx.add_op(&second_derivative_op(1, Shared::clone(&mesher)));
        let copy_of_dxx = dxx.clone();

        assert_recovers(
            &dxx.solve_splitting(&copy_of_dxx.apply(&u), 1.0, 0.0)
                .unwrap(),
        );
    }
}
