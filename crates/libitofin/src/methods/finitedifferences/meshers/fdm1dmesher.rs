//! The 1-D grid a composite mesher is built from.
//!
//! Port of `ql/methods/finitedifferences/meshers/fdm1dmesher.hpp:34`.

use crate::types::{Real, Size};

/// The points of a one-dimensional grid and the gaps between them.
///
/// C++ makes this a base class with a protected size-only constructor and
/// protected vectors, so a subclass allocates through the base and then fills
/// `locations_`/`dplus_`/`dminus_` in its own constructor
/// (`uniform1dmesher.hpp:38-50` is the whole pattern). The subclasses add no
/// members and no behaviour, so the port keeps one concrete struct built from
/// the finished vectors, and each C++ subclass becomes a constructor function
/// returning it - see
/// [`uniform_1d_mesher`](super::uniform_1d_mesher).
#[derive(Clone, Debug)]
pub struct Fdm1dMesher {
    locations: Vec<Real>,
    dplus: Vec<Real>,
    dminus: Vec<Real>,
}

impl Fdm1dMesher {
    /// A grid at `locations` with the given forward and backward gaps. Panics
    /// unless the three agree in length, which the C++ base constructor
    /// guarantees by allocating all three itself.
    pub fn new(locations: Vec<Real>, dplus: Vec<Real>, dminus: Vec<Real>) -> Self {
        assert!(
            locations.len() == dplus.len() && locations.len() == dminus.len(),
            "mesher vectors must agree in length"
        );
        Fdm1dMesher {
            locations,
            dplus,
            dminus,
        }
    }

    /// The number of grid points.
    pub fn size(&self) -> Size {
        self.locations.len()
    }

    /// The coordinate of the point at `index`.
    pub fn location(&self, index: Size) -> Real {
        self.locations[index]
    }

    /// The coordinates of every point.
    pub fn locations(&self) -> &[Real] {
        &self.locations
    }

    /// The distance from `index` to the point above it.
    pub fn dplus(&self, index: Size) -> Real {
        self.dplus[index]
    }

    /// The distance from `index` to the point below it.
    pub fn dminus(&self, index: Size) -> Real {
        self.dminus[index]
    }
}
