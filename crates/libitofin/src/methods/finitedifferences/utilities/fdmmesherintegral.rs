//! Discrete integration of a function sampled on a composite mesher.
//!
//! Port of `ql/methods/finitedifferences/utilities/fdmmesherintegral.hpp:31`
//! and its `.cpp`.

use crate::errors::QlResult;
use crate::methods::finitedifferences::meshers::{Fdm1dMesher, FdmMesherComposite};
use crate::require;
use crate::types::{Real, Size};

/// The integral over a [`FdmMesherComposite`] of a function sampled at its grid
/// points, taken one dimension at a time with `integrator_1d`.
///
/// The last dimension is integrated last: its grid is the outer loop, and each
/// of its points contributes the integral of the sub-grid spanned by the other
/// dimensions (`fdmmesherintegral.cpp:33-59`). That works on a flat sample
/// vector because the layout is row-major with dimension 0 fastest
/// (`fdmlinearoplayout.rs`, `spacing[0] == 1`), so the samples of one point of
/// the last dimension are the contiguous block `f[i * sub_size ..]`.
///
/// C++ stores the integrator as a `std::function` reference bound at
/// construction (`hpp:42`); the Rust port is generic over it and threads the
/// [`QlResult`] of the discrete integrators
/// ([`DiscreteSimpsonIntegral`](crate::math::integrals::discrete::DiscreteSimpsonIntegral)
/// and friends are fallible where the C++ ones return a bare `Real`) out
/// through the recursion.
///
/// Divergence: C++ recurses by building a sub-`FdmMesherComposite` from all but
/// the last 1-D mesher (`cpp:41-44`) purely to read its layout size. The port
/// recurses on the mesher slice and multiplies out the sub-grid size directly,
/// which is the same number without the intermediate composite and its layout.
pub struct FdmMesherIntegral<'a, F> {
    meshers: &'a [Fdm1dMesher],
    integrator_1d: F,
}

impl<'a, F> FdmMesherIntegral<'a, F>
where
    F: Fn(&[Real], &[Real]) -> QlResult<Real>,
{
    /// An integral over `mesher`, evaluating each one-dimensional integral with
    /// `integrator_1d` over `(abscissae, samples)`.
    pub fn new(mesher: &'a FdmMesherComposite, integrator_1d: F) -> Self {
        FdmMesherIntegral {
            meshers: mesher.fdm_1d_meshers(),
            integrator_1d,
        }
    }

    /// Integrates the samples `f`, one per grid point in layout order.
    ///
    /// # Errors
    ///
    /// Returns an error if `f` does not hold one sample per grid point, or if
    /// the one-dimensional integrator rejects any of the slices it is given.
    /// The length check has no counterpart in C++, which would read past the end
    /// of a short `Array` (`fdmmesherintegral.cpp:52-53`).
    pub fn integrate(&self, f: &[Real]) -> QlResult<Real> {
        let size: Size = self.meshers.iter().map(Fdm1dMesher::size).product();
        require!(
            f.len() == size,
            "inconsistent size: the grid has {size} points, f has {}",
            f.len()
        );
        self.integrate_over(self.meshers, f)
    }

    fn integrate_over(&self, meshers: &[Fdm1dMesher], f: &[Real]) -> QlResult<Real> {
        let (last, rest) = meshers
            .split_last()
            .expect("a composite mesher holds at least one 1-D mesher");
        let x = last.locations();

        if rest.is_empty() {
            return (self.integrator_1d)(x, f);
        }

        let sub_size: Size = rest.iter().map(Fdm1dMesher::size).product();
        let mut g = vec![0.0; x.len()];
        for (i, value) in g.iter_mut().enumerate() {
            *value = self.integrate_over(rest, &f[i * sub_size..(i + 1) * sub_size])?;
        }

        (self.integrator_1d)(x, &g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::math::array::Array;
    use crate::math::integrals::discrete::{DiscreteSimpsonIntegral, DiscreteTrapezoidIntegral};
    use crate::methods::finitedifferences::meshers::{FdmMesher, concentrating_1d_mesher};

    fn oracle_mesher() -> FdmMesherComposite {
        FdmMesherComposite::new(vec![
            concentrating_1d_mesher(-1.0, 1.6, 21, Some((0.0, 0.1)), false).unwrap(),
            concentrating_1d_mesher(-3.0, 4.0, 11, Some((1.0, 0.01)), false).unwrap(),
            concentrating_1d_mesher(-2.0, 1.0, 5, Some((0.5, 0.1)), false).unwrap(),
        ])
    }

    fn oracle_samples(mesher: &FdmMesherComposite) -> Array {
        let mut f = Array::with_size(mesher.layout().size());
        for position in mesher.layout().iter() {
            let x = mesher.location(&position, 0);
            let y = mesher.location(&position, 1);
            let z = mesher.location(&position, 2);

            f[position.index()] =
                x * x + 3.0 * y * y - 3.0 * z * z + 2.0 * x * y - x * z - 3.0 * y * z + 4.0 * x
                    - y
                    - 3.0 * z
                    + 2.0;
        }
        f
    }

    /// `testFdmMesherIntegral`, `QuantLib/test-suite/fdmlinearop.cpp:1443`.
    ///
    /// Simpson is exact on this polynomial, so its expected value is the
    /// analytic integral over the box (`fdmlinearop.cpp:1468-1471`); the
    /// trapezoid literal is the discretisation the grid actually produces.
    #[test]
    fn mesher_integrals_match_quantlib() {
        let mesher = oracle_mesher();
        let f = oracle_samples(&mesher);
        let tol = 1e-12;

        let expected_simpson = 876.512;
        let simpson =
            FdmMesherIntegral::new(&mesher, |x, f| DiscreteSimpsonIntegral.integrate(x, f))
                .integrate(&f)
                .unwrap();
        assert!(
            (simpson - expected_simpson).abs() <= tol * expected_simpson,
            "simpson {simpson} vs {expected_simpson}"
        );

        let expected_trapezoid = 917.0148209153263;
        let trapezoid =
            FdmMesherIntegral::new(&mesher, |x, f| DiscreteTrapezoidIntegral.integrate(x, f))
                .integrate(&f)
                .unwrap();
        assert!(
            (trapezoid - expected_trapezoid).abs() <= tol * expected_trapezoid,
            "trapezoid {trapezoid} vs {expected_trapezoid}"
        );
    }

    #[test]
    fn a_one_dimensional_integral_is_the_integrator_itself() {
        let mesher = concentrating_1d_mesher(-1.0, 1.6, 21, Some((0.0, 0.1)), false).unwrap();
        let f: Vec<Real> = mesher
            .locations()
            .iter()
            .map(|&x| x * x + 2.0 * x)
            .collect();
        let expected = DiscreteSimpsonIntegral
            .integrate(mesher.locations(), &f)
            .unwrap();

        let composite = FdmMesherComposite::new(vec![mesher]);
        let integral =
            FdmMesherIntegral::new(&composite, |x, f| DiscreteSimpsonIntegral.integrate(x, f));

        assert_eq!(integral.integrate(&f).unwrap(), expected);
    }

    #[test]
    fn the_samples_must_cover_the_grid() {
        let mesher = oracle_mesher();
        let integral =
            FdmMesherIntegral::new(&mesher, |x, f| DiscreteSimpsonIntegral.integrate(x, f));

        let err = integral.integrate(&[1.0, 2.0]).unwrap_err();
        assert_eq!(
            err.message(),
            "inconsistent size: the grid has 1155 points, f has 2"
        );
    }

    #[test]
    fn an_integrator_error_propagates_out_of_the_recursion() {
        let mesher = oracle_mesher();
        let mut f = oracle_samples(&mesher);
        f[0] = Real::NAN;

        let integral =
            FdmMesherIntegral::new(&mesher, |x, f| DiscreteSimpsonIntegral.integrate(x, f));
        assert!(integral.integrate(&f).is_err());
    }
}
