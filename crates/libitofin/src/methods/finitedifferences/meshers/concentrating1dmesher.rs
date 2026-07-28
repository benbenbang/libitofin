//! 1-D mesher concentrating its points around a critical point.
//!
//! Port of `ql/methods/finitedifferences/meshers/concentrating1dmesher.hpp:37`
//! and the pair constructor at its `.cpp:41-107`.

use crate::errors::QlResult;
use crate::math::comparison::close;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::linear::LinearInterpolation;
use crate::require;
use crate::types::{Real, Size};
use crate::utilities::null::Null;

use super::fdm1dmesher::Fdm1dMesher;

/// A grid of `size` points on `[start, end]` whose density peaks at a critical
/// point.
///
/// `c_point` is `Some((point, density))`, where `density` is a *fraction of the
/// range*: the constructor scales it by `end - start` before use
/// (`concentrating1dmesher.cpp:48-49`), so the oracle's `(0.0, 0.1)` over
/// `[-1.0, 1.6]` concentrates with a scale of `0.26`. Interior points are laid
/// out uniformly in `asinh` space around `point` and mapped back through `sinh`
/// (`cpp:65-88`), which packs them towards `point` and stretches them towards
/// the boundaries. `None` falls back to an equidistant grid (`cpp:90-94`),
/// matching [`uniform_1d_mesher`](super::uniform_1d_mesher).
///
/// With `require_c_point` the grid is warped so that `point` is hit exactly by
/// one of its nodes: a piecewise-linear transform pins the `asinh`-space
/// coordinate of `point` to the nearest interior node index (`cpp:69-83`).
///
/// Divergence from C++: the critical point and its density are one
/// [`Option`] rather than a `pair` of [`Null`] sentinels. C++ has to spell the
/// "no concentration" case as `(Null, Null)` and then check that a given point
/// comes with a given density (`cpp:56-57`); the `Option` makes that pairing
/// structural. Nothing is lost - no QuantLib caller passes a null point with a
/// non-null density, and `fdmblackscholesmesher.cpp:112-124` already branches on
/// the null point itself to build a [`Uniform1dMesher`](super::uniform_1d_mesher)
/// instead.
///
/// The multi-point constructor (`cpp:141`, a Brent search over an adaptive
/// Runge-Kutta integration) is deferred to #636 and not ported here.
///
/// # Errors
///
/// Returns an error unless `end > start`, the critical point lies within
/// `[start, end]` with a strictly positive density, and a point is given when
/// `require_c_point` demands one.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn concentrating_1d_mesher(
    start: Real,
    end: Real,
    size: Size,
    c_point: Option<(Real, Real)>,
    require_c_point: bool,
) -> QlResult<Fdm1dMesher> {
    require!(end > start, "end must be larger than start");
    require!(
        !require_c_point || c_point.is_some(),
        "cPoint is required in grid but not given"
    );

    let dx = 1.0 / (size - 1) as Real;

    let mut locations = vec![0.0; size];
    let mut dplus = vec![0.0; size];
    let mut dminus = vec![0.0; size];

    if let Some((c_point, density)) = c_point {
        let density = density * (end - start);
        require!(
            c_point >= start && c_point <= end,
            "cPoint must be between start and end"
        );
        require!(density > 0.0, "density > 0 required");

        let c1 = ((start - c_point) / density).asinh();
        let c2 = ((end - c_point) / density).asinh();

        let transform = if require_c_point {
            let mut u = vec![0.0];
            let mut z = vec![0.0];
            if !close(c_point, start) && !close(c_point, end) {
                let z0 = -c1 / (c2 - c1);
                let steps = (size - 1) as i64;
                let u0 = std::cmp::max(
                    std::cmp::min((z0 * steps as Real).round() as i64, size as i64 - 2),
                    1,
                ) as Real
                    / (size - 1) as Real;
                u.push(u0);
                z.push(z0);
            }
            u.push(1.0);
            z.push(1.0);
            Some(LinearInterpolation::new(u, z)?)
        } else {
            None
        };

        for (i, location) in locations.iter_mut().enumerate().take(size - 1).skip(1) {
            let li = match &transform {
                Some(transform) => transform.value(i as Real * dx)?,
                None => i as Real * dx,
            };
            *location = c_point + density * (c1 * (1.0 - li) + c2 * li).sinh();
        }
    } else {
        for (i, location) in locations.iter_mut().enumerate().take(size - 1).skip(1) {
            *location = start + i as Real * dx * (end - start);
        }
    }

    locations[0] = start;
    locations[size - 1] = end;

    for i in 0..size - 1 {
        let gap = locations[i + 1] - locations[i];
        dplus[i] = gap;
        dminus[i + 1] = gap;
    }
    dplus[size - 1] = Real::null();
    dminus[0] = Real::null();

    Ok(Fdm1dMesher::new(locations, dplus, dminus))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::methods::finitedifferences::meshers::uniform_1d_mesher;

    fn is_increasing(mesher: &Fdm1dMesher) -> bool {
        mesher.locations().windows(2).all(|w| w[1] > w[0])
    }

    #[test]
    fn grid_spans_the_range_and_packs_points_around_the_critical_point() {
        let (start, end, size) = (-1.0, 1.6, 21);
        let c_point = 0.0;
        let mesher =
            concentrating_1d_mesher(start, end, size, Some((c_point, 0.1)), false).unwrap();

        assert_eq!(mesher.size(), size);
        assert_eq!(mesher.location(0), start);
        assert_eq!(mesher.location(size - 1), end);
        assert!(is_increasing(&mesher));

        // The asinh/sinh map is monotone in |x - cPoint|, so the tightest gap is
        // the one straddling the critical point and gaps grow away from it.
        let nearest = (0..size)
            .min_by(|&i, &j| {
                (mesher.location(i) - c_point)
                    .abs()
                    .total_cmp(&(mesher.location(j) - c_point).abs())
            })
            .unwrap();
        let gaps: Vec<Real> = (0..size - 1).map(|i| mesher.dplus(i)).collect();
        let tightest = (0..size - 1)
            .min_by(|&i, &j| gaps[i].total_cmp(&gaps[j]))
            .unwrap();
        assert!(tightest.abs_diff(nearest) <= 1);
        assert!(gaps[0] > gaps[tightest]);
        assert!(gaps[size - 2] > gaps[tightest]);
    }

    #[test]
    fn gaps_mirror_the_locations_with_null_at_the_boundaries() {
        let size = 11;
        let mesher = concentrating_1d_mesher(-3.0, 4.0, size, Some((1.0, 0.01)), false).unwrap();

        for i in 0..size - 1 {
            let gap = mesher.location(i + 1) - mesher.location(i);
            assert_eq!(mesher.dplus(i), gap);
            assert_eq!(mesher.dminus(i + 1), gap);
        }
        assert!(mesher.dplus(size - 1).is_null());
        assert!(mesher.dminus(0).is_null());
    }

    #[test]
    fn without_a_critical_point_the_grid_is_equidistant() {
        let (start, end, size) = (-2.0, 1.0, 9);
        let mesher = concentrating_1d_mesher(start, end, size, None, false).unwrap();
        let uniform = uniform_1d_mesher(start, end, size).unwrap();

        assert_eq!(mesher.locations(), uniform.locations());
    }

    #[test]
    fn a_required_critical_point_lands_on_a_grid_node() {
        let (start, end, size) = (-1.0, 1.6, 21);
        for c_point in [0.0, -0.4, 1.2] {
            let mesher =
                concentrating_1d_mesher(start, end, size, Some((c_point, 0.1)), true).unwrap();

            assert!(is_increasing(&mesher));
            assert!(
                mesher.locations().iter().any(|&x| close(x, c_point)),
                "cPoint {c_point} is not on the grid"
            );
        }
    }

    #[test]
    fn a_critical_point_on_a_boundary_needs_no_interior_node() {
        let (start, end, size) = (-1.0, 1.6, 21);
        for c_point in [start, end] {
            let mesher =
                concentrating_1d_mesher(start, end, size, Some((c_point, 0.1)), true).unwrap();

            assert!(is_increasing(&mesher));
            assert_eq!(mesher.location(0), start);
            assert_eq!(mesher.location(size - 1), end);
        }
    }

    #[test]
    fn a_required_critical_point_must_be_given() {
        let err = concentrating_1d_mesher(-1.0, 1.6, 21, None, true).unwrap_err();
        assert_eq!(err.message(), "cPoint is required in grid but not given");
    }

    #[test]
    fn the_range_the_critical_point_and_the_density_are_checked() {
        let err = concentrating_1d_mesher(1.0, 1.0, 21, None, false).unwrap_err();
        assert_eq!(err.message(), "end must be larger than start");

        let err = concentrating_1d_mesher(-1.0, 1.6, 21, Some((2.0, 0.1)), false).unwrap_err();
        assert_eq!(err.message(), "cPoint must be between start and end");

        let err = concentrating_1d_mesher(-1.0, 1.6, 21, Some((0.0, 0.0)), false).unwrap_err();
        assert_eq!(err.message(), "density > 0 required");
    }
}
