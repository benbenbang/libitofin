//! Tensor product of 1-D meshers.
//!
//! Port of `ql/methods/finitedifferences/meshers/fdmmeshercomposite.hpp:35` and
//! its `.cpp`.

use crate::math::array::Array;
use crate::methods::finitedifferences::operators::{FdmLinearOpIterator, FdmLinearOpLayout};
use crate::shared::{Shared, shared};
use crate::types::{Real, Size};

use super::fdm1dmesher::Fdm1dMesher;
use super::fdmmesher::FdmMesher;

/// An N-D grid built as the tensor product of one [`Fdm1dMesher`] per
/// dimension.
///
/// Every [`FdmMesher`] method delegates to the mesher of the direction asked
/// about, at that direction's coordinate of the position given
/// (`fdmmeshercomposite.cpp:83-105`), so the grid is a full product: dimension
/// `i` carries the same points wherever the other coordinates sit.
///
/// Only the constructor that derives its layout from the meshers' sizes is
/// ported (`cpp:70-73`). The five fixed-arity conveniences (`hpp:41-55`) exist
/// because C++ has no vector literal and collapse into `vec![...]` here; the
/// constructor taking a prepared layout (`cpp:75-82`) has a single caller in
/// QuantLib, `ql/termstructures/volatility/zabr.cpp:277`, and is left for
/// whichever ticket ports it.
#[derive(Debug)]
pub struct FdmMesherComposite {
    layout: Shared<FdmLinearOpLayout>,
    meshers: Vec<Fdm1dMesher>,
}

impl FdmMesherComposite {
    /// A composite over `meshers`, one per dimension, with a layout of their
    /// sizes (`fdmmeshercomposite.cpp:31-37`). Panics on an empty `meshers`,
    /// which leaves [`FdmLinearOpLayout`] without a dimension.
    pub fn new(meshers: Vec<Fdm1dMesher>) -> Self {
        let dim = meshers.iter().map(Fdm1dMesher::size).collect();
        FdmMesherComposite {
            layout: shared(FdmLinearOpLayout::new(dim)),
            meshers,
        }
    }

    /// The 1-D meshers this composite is built from, in dimension order.
    ///
    /// `getFdm1dMeshers` in C++ (`fdmmeshercomposite.hpp:62-63`); the `get_`
    /// prefix is dropped per the Rust API guidelines.
    pub fn fdm_1d_meshers(&self) -> &[Fdm1dMesher] {
        &self.meshers
    }
}

impl FdmMesher for FdmMesherComposite {
    fn layout(&self) -> &Shared<FdmLinearOpLayout> {
        &self.layout
    }

    fn dplus(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.meshers[direction].dplus(iter.coordinates()[direction])
    }

    fn dminus(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.meshers[direction].dminus(iter.coordinates()[direction])
    }

    fn location(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real {
        self.meshers[direction].location(iter.coordinates()[direction])
    }

    fn locations(&self, direction: Size) -> Array {
        let mut result = Array::with_size(self.layout.size());
        let mut position = self.layout.begin();
        while position.index() < self.layout.size() {
            result[position.index()] =
                self.meshers[direction].locations()[position.coordinates()[direction]];
            position.advance();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::meshers::{
        UniformGridMesher, concentrating_1d_mesher, uniform_1d_mesher,
    };

    const DIM: [Size; 3] = [21, 11, 5];

    fn composite() -> FdmMesherComposite {
        FdmMesherComposite::new(vec![
            concentrating_1d_mesher(-1.0, 1.6, DIM[0], Some((0.0, 0.1)), false).unwrap(),
            concentrating_1d_mesher(-3.0, 4.0, DIM[1], Some((1.0, 0.01)), false).unwrap(),
            concentrating_1d_mesher(-2.0, 1.0, DIM[2], Some((0.5, 0.1)), false).unwrap(),
        ])
    }

    #[test]
    fn the_layout_takes_its_dimensions_from_the_meshers() {
        let composite = composite();
        assert_eq!(composite.layout().dim(), DIM);
        assert_eq!(composite.layout().size(), DIM.iter().product::<Size>());
        assert_eq!(composite.fdm_1d_meshers().len(), DIM.len());
        for (direction, mesher) in composite.fdm_1d_meshers().iter().enumerate() {
            assert_eq!(mesher.size(), DIM[direction]);
        }
    }

    #[test]
    fn every_method_delegates_to_the_mesher_of_its_direction() {
        let composite = composite();

        for position in composite.layout().iter() {
            for (direction, mesher) in composite.fdm_1d_meshers().iter().enumerate() {
                let coordinate = position.coordinates()[direction];
                assert_eq!(
                    composite.location(&position, direction),
                    mesher.location(coordinate)
                );
                if coordinate + 1 < DIM[direction] {
                    assert_eq!(
                        composite.dplus(&position, direction),
                        mesher.dplus(coordinate)
                    );
                }
                if coordinate > 0 {
                    assert_eq!(
                        composite.dminus(&position, direction),
                        mesher.dminus(coordinate)
                    );
                }
            }
        }
    }

    #[test]
    fn locations_are_flattened_over_the_whole_grid() {
        let composite = composite();

        for direction in 0..DIM.len() {
            let flattened = composite.locations(direction);
            assert_eq!(flattened.size(), composite.layout().size());
            for position in composite.layout().iter() {
                assert_eq!(
                    flattened[position.index()],
                    composite.location(&position, direction)
                );
            }
        }
    }

    #[test]
    fn a_composite_of_uniform_meshers_matches_the_uniform_grid_mesher() {
        let boundaries = [(-5.0, 10.0), (5.0, 100.0), (10.0, 20.0)];
        let composite = FdmMesherComposite::new(
            boundaries
                .iter()
                .enumerate()
                .map(|(direction, &(lower, upper))| {
                    uniform_1d_mesher(lower, upper, DIM[direction]).unwrap()
                })
                .collect(),
        );
        let uniform =
            UniformGridMesher::new(Shared::clone(composite.layout()), &boundaries).unwrap();

        let tol = 100.0 * Real::EPSILON;
        for position in composite.layout().iter() {
            for (direction, &extent) in DIM.iter().enumerate() {
                let expected = uniform.location(&position, direction);
                assert!((composite.location(&position, direction) - expected).abs() <= tol);

                // The uniform mesher reports its constant spacing everywhere,
                // where the composite carries the null sentinel off the grid.
                let coordinate = position.coordinates()[direction];
                if coordinate + 1 < extent {
                    let expected = uniform.dplus(&position, direction);
                    assert!((composite.dplus(&position, direction) - expected).abs() <= tol);
                }
            }
        }
    }

    #[test]
    fn a_composite_can_be_held_as_a_trait_object() {
        let mesher: Shared<dyn FdmMesher> = shared(composite());
        assert_eq!(mesher.layout().dim(), DIM);
        assert_eq!(mesher.location(&mesher.layout().begin(), 0), -1.0);
    }

    #[test]
    #[should_panic(expected = "at least one dimension")]
    fn a_composite_needs_at_least_one_mesher() {
        FdmMesherComposite::new(Vec::new());
    }
}
