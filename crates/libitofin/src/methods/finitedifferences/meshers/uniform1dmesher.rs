//! Equidistant 1-D mesher.
//!
//! Port of `ql/methods/finitedifferences/meshers/uniform1dmesher.hpp:35`.

use crate::errors::QlResult;
use crate::require;
use crate::types::{Real, Size};
use crate::utilities::null::Null;

use super::fdm1dmesher::Fdm1dMesher;

/// A grid of `size` equally spaced points on `[start, end]`, both endpoints
/// included.
///
/// The two outermost gaps that fall outside the grid - `dplus` at the last
/// point and `dminus` at the first - are the [`Null`] sentinel, as in C++
/// (`uniform1dmesher.hpp:49`). They are *not* an `Option`, unlike the
/// `Null<Real>` result sentinels elsewhere in the core: those are API answers a
/// caller inspects, whereas these are in-band elements of a numeric container
/// that the derivative operators read unconditionally, before they branch on
/// whether the point is on a boundary (`firstderivativeop.cpp:35-41`,
/// `secondderivativeop.cpp:36-42`, both of which then discard the boundary
/// value). An `Option` would force those reads to panic where C++ carries a
/// large finite value the boundary branch discards. The precedent for a
/// sentinel inside a
/// container is the `DiscountFactor::null()` pushed into the leg results of
/// `pricingengines/swap/discountingswapengine.rs:161`.
///
/// Returns an error unless `end > start`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn uniform_1d_mesher(start: Real, end: Real, size: Size) -> QlResult<Fdm1dMesher> {
    require!(end > start, "end must be larger than start");

    let dx = (end - start) / (size - 1) as Real;

    let mut locations = vec![0.0; size];
    let mut dplus = vec![0.0; size];
    let mut dminus = vec![0.0; size];
    for i in 0..size - 1 {
        locations[i] = start + i as Real * dx;
        dplus[i] = dx;
        dminus[i + 1] = dx;
    }

    locations[size - 1] = end;
    dplus[size - 1] = Real::null();
    dminus[0] = Real::null();

    Ok(Fdm1dMesher::new(locations, dplus, dminus))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_equidistant_with_null_gaps_at_the_boundaries() {
        let size = 5;
        let mesher = uniform_1d_mesher(0.0, 1.0, size).unwrap();
        let dx = 0.25;

        assert_eq!(mesher.size(), size);
        assert_eq!(mesher.locations(), [0.0, 0.25, 0.5, 0.75, 1.0]);
        for i in 0..size {
            assert_eq!(mesher.location(i), i as Real * dx);
        }

        for i in 0..size - 1 {
            assert_eq!(mesher.dplus(i), dx);
            assert_eq!(mesher.dminus(i + 1), dx);
        }

        assert!(mesher.dplus(size - 1).is_null());
        assert!(mesher.dminus(0).is_null());
    }

    #[test]
    fn end_must_be_larger_than_start() {
        let err = uniform_1d_mesher(1.0, 1.0, 5).unwrap_err();
        assert_eq!(err.message(), "end must be larger than start");
    }
}
