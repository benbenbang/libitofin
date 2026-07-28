//! Equidistant N-D mesher.
//!
//! Port of `ql/methods/finitedifferences/meshers/uniformgridmesher.hpp:35` and
//! its `.cpp`.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::operators::{FdmLinearOpIterator, FdmLinearOpLayout};
use crate::require;
use crate::shared::Shared;
use crate::types::{Real, Size};

use super::fdmmesher::FdmMesher;

/// A grid whose points are equally spaced along every dimension.
///
/// Because the spacing is constant, `dplus` and `dminus` ignore the position
/// they are asked about and return `dx[direction]` everywhere, boundaries
/// included (`uniformgridmesher.hpp:41-46`).
#[derive(Debug)]
pub struct UniformGridMesher {
    layout: Shared<FdmLinearOpLayout>,
    dx: Vec<Real>,
    locations: Vec<Vec<Real>>,
}

impl UniformGridMesher {
    /// A mesher spanning `boundaries[i] = (lower, upper)` along dimension `i`
    /// with `layout.dim()[i]` points, the outermost of which sit exactly on the
    /// boundaries.
    pub fn new(layout: Shared<FdmLinearOpLayout>, boundaries: &[(Real, Real)]) -> QlResult<Self> {
        require!(
            boundaries.len() == layout.dim().len(),
            "inconsistent boundaries given"
        );

        let mut dx = Vec::with_capacity(boundaries.len());
        let mut locations = Vec::with_capacity(boundaries.len());
        for (direction, &(lower, upper)) in boundaries.iter().enumerate() {
            let extent = layout.dim()[direction];
            let step = (upper - lower) / (extent - 1) as Real;
            locations.push((0..extent).map(|j| lower + j as Real * step).collect());
            dx.push(step);
        }

        Ok(UniformGridMesher {
            layout,
            dx,
            locations,
        })
    }
}

impl FdmMesher for UniformGridMesher {
    fn layout(&self) -> &Shared<FdmLinearOpLayout> {
        &self.layout
    }

    fn dplus(&self, _iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.dx[direction]
    }

    fn dminus(&self, _iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.dx[direction]
    }

    fn location(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.locations[direction][iter.coordinates()[direction]]
    }

    fn locations(&self, direction: Size) -> Array {
        let mut result = Array::with_size(self.layout.size());
        let mut position = self.layout.begin();
        while position.index() < self.layout.size() {
            result[position.index()] = self.locations[direction][position.coordinates()[direction]];
            position.advance();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::shared::shared;

    const DIM: [Size; 3] = [5, 7, 8];
    const BOUNDARIES: [(Real, Real); 3] = [(-5.0, 10.0), (5.0, 100.0), (10.0, 20.0)];

    fn mesher() -> UniformGridMesher {
        let layout = shared(FdmLinearOpLayout::new(DIM.to_vec()));
        UniformGridMesher::new(layout, &BOUNDARIES).unwrap()
    }

    /// `testUniformGridMesher`, `QuantLib/test-suite/fdmlinearop.cpp:334`.
    #[test]
    fn uniform_grid_mesher_spacings_match_quantlib() {
        let mesher = mesher();
        let begin = mesher.layout().begin();

        let dx1 = 15.0 / (DIM[0] - 1) as Real;
        let dx2 = 95.0 / (DIM[1] - 1) as Real;
        let dx3 = 10.0 / (DIM[2] - 1) as Real;

        let tol = 100.0 * Real::EPSILON;
        for (direction, expected) in [dx1, dx2, dx3].into_iter().enumerate() {
            assert!((expected - mesher.dminus(&begin, direction)).abs() <= tol);
            assert!((expected - mesher.dplus(&begin, direction)).abs() <= tol);
        }
    }

    #[test]
    fn spacings_are_constant_across_the_grid() {
        let mesher = mesher();
        let reference: Vec<Real> = (0..DIM.len())
            .map(|d| mesher.dplus(&mesher.layout().begin(), d))
            .collect();

        for position in mesher.layout().iter() {
            for (direction, expected) in reference.iter().enumerate() {
                assert_eq!(mesher.dplus(&position, direction), *expected);
                assert_eq!(mesher.dminus(&position, direction), *expected);
            }
        }
    }

    #[test]
    fn locations_are_equidistant_between_the_boundaries() {
        let mesher = mesher();

        for position in mesher.layout().iter() {
            for (direction, &(lower, _)) in BOUNDARIES.iter().enumerate() {
                let step = position.coordinates()[direction] as Real;
                let expected = lower + step * mesher.dplus(&position, direction);
                assert_eq!(mesher.location(&position, direction), expected);
            }
        }

        let begin = mesher.layout().begin();
        for (direction, &(lower, upper)) in BOUNDARIES.iter().enumerate() {
            assert_eq!(mesher.location(&begin, direction), lower);

            let last = mesher.layout().iter_neighbourhood(
                &begin,
                direction,
                (DIM[direction] - 1) as crate::types::Integer,
            );
            assert!((mesher.location(&last, direction) - upper).abs() <= 100.0 * Real::EPSILON);
        }
    }

    #[test]
    fn locations_are_flattened_over_the_whole_grid() {
        let mesher = mesher();

        for direction in 0..DIM.len() {
            let flattened = mesher.locations(direction);
            assert_eq!(flattened.size(), mesher.layout().size());
            for position in mesher.layout().iter() {
                assert_eq!(
                    flattened[position.index()],
                    mesher.location(&position, direction)
                );
            }
        }
    }

    #[test]
    fn a_mesher_can_be_held_as_a_trait_object() {
        let layout = shared(FdmLinearOpLayout::new(DIM.to_vec()));
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(Shared::clone(&layout), &BOUNDARIES).unwrap());

        assert_eq!(mesher.dplus(&layout.begin(), 0), 15.0 / 4.0);
        assert_eq!(mesher.layout().dim(), DIM);
    }

    #[test]
    fn boundaries_must_match_the_layout_dimensions() {
        let layout = shared(FdmLinearOpLayout::new(DIM.to_vec()));
        let err = UniformGridMesher::new(layout, &BOUNDARIES[..2]).unwrap_err();
        assert_eq!(err.message(), "inconsistent boundaries given");
    }
}
