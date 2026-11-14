//! Initial estimate of the Student-t quantile.
//!
//! Port of `inverse_students_t` and its helpers from Boost.Math
//! `boost/math/special_functions/detail/t_distribution_inv.hpp` (Boost 1.92,
//! Copyright John Maddock 2007, Paul A. Bristow 2007, distributed under the
//! Boost Software License 1.0, see `THIRD_PARTY_NOTICES.md`). QuantLib v1.44's
//! `InverseCumulativeStudent` delegates to this code through
//! `boost::math::quantile`, which refines the estimate with one Halley step.
//!
//! The methods are the closed forms of Shaw (2006) for `ν` in {1, 2, 4}, the
//! polynomial iteration of Shaw for `ν = 6`, Hill (1970) Algorithm 396, and
//! the tail and body series of Shaw for small non-integer `ν`. The
//! `f`-suffixed Boost literals are single-precision constants; they are
//! written here as their decimal values in double precision, so results agree
//! with Boost to rounding, not bit for bit. Shaw's series coefficients are
//! transcribed verbatim, beyond `f64` precision.

#![allow(clippy::excessive_precision)]

use std::f64::consts::PI;

use crate::errors::QlResult;
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::types::Real;

/// Above this many degrees of freedom the Student-t is replaced by the normal.
const NORMAL_LIMIT_DOF: Real = 268_435_456.0;

/// The estimate is exact (no refinement needed) from this many degrees of freedom.
const NORMAL_EXACT_DOF: Real = 1e20;

/// The lower-tail Student-t quantile estimate for `u` in `(0, 0.5]`.
///
/// `ln_normalization` is the log of the density normalization
/// `Γ((ν+1)/2) / (Γ(ν/2) √(νπ))`, so its negated exponential is the
/// `tgamma_delta_ratio(ν/2, 1/2) √(νπ)` factor of Shaw's series. Returns the
/// estimate and whether Boost flags it exact (no refinement step).
///
/// Boost picks Hill or the tail series for `ν >= 3` by `u_exp < ν / 0.654`,
/// where `u_exp` is the binary exponent of `u`. That is always true for
/// `u <= 0.5` (`u_exp <= 0`), so Hill is always taken there and the Shaw
/// series are reached only for non-integer `ν` in `[2, 3)`.
///
/// # Errors
///
/// Propagates an error from the normal quantile used by Hill's method.
pub(super) fn inverse_students_t(
    df: Real,
    ln_normalization: Real,
    u: Real,
) -> QlResult<(Real, bool)> {
    let v = 1.0 - u;
    if df.floor() == df && df < 20.0 {
        match df as u32 {
            1 => {
                let x = if u == 0.5 {
                    0.0
                } else {
                    -(PI * u).cos() / (PI * u).sin()
                };
                return Ok((x, true));
            }
            2 => return Ok(((2.0 * u - 1.0) / (2.0 * u * v).sqrt(), true)),
            4 => {
                let alpha = 4.0 * u * v;
                let root_alpha = alpha.sqrt();
                let r = 4.0 * (root_alpha.acos() / 3.0).cos() / root_alpha;
                let x = (r - 4.0).sqrt();
                return Ok((if u - 0.5 < 0.0 { -x } else { x }, true));
            }
            6 => return Ok((students_t_6(df, u)?, false)),
            _ => {}
        }
    }
    if df > NORMAL_LIMIT_DOF {
        Ok((normal_quantile(u)?, df >= NORMAL_EXACT_DOF))
    } else if df < 3.0 {
        let crossover = 0.2742 - df * 0.0242143;
        let x = if u > crossover {
            body_series(df, ln_normalization, u)
        } else {
            tail_series(df, ln_normalization, u)
        };
        Ok((x, false))
    } else {
        Ok((hill(df, u)?, false))
    }
}

/// `ν = 6`: one Newton step on Shaw's polynomial (Eq 41) from Shaw's seed.
///
/// Boost iterates while the relative step exceeds `ldexp(1, 2 * 53 / 3)`,
/// i.e. `2^35`, so its loop body runs exactly once; this ports that single
/// step. Deep tails (`u < 1e-150`) overflow the polynomial and use Hill.
fn students_t_6(df: Real, u: Real) -> QlResult<Real> {
    if u < 1e-150 {
        return hill(df, u);
    }
    let a = 4.0 * (u - u * u);
    let b = a.cbrt();
    let c = 0.85498797333834849467655443627193;
    let p = 6.0 * (1.0 + c * (1.0 / b - 1.0));
    let p2 = p * p;
    let p4 = p2 * p2;
    let p5 = p * p4;
    let p = 2.0 * (8.0 * a * p5 - 270.0 * p2 + 2187.0) / (5.0 * (4.0 * a * p4 - 216.0 * p - 243.0));
    let x = (p - df).sqrt();
    Ok(if u - 0.5 < 0.0 { -x } else { x })
}

/// The standard normal quantile, in place of Boost's `-erfc_inv(2u) √2`.
///
/// This is Acklam's approximation, so beyond `NORMAL_LIMIT_DOF` the result
/// carries its accuracy rather than Boost's full-precision `erfc_inv`.
fn normal_quantile(u: Real) -> QlResult<Real> {
    InverseCumulativeNormal::standard_value(u)
}

/// G. W. Hill, Algorithm 396, Student's t-Quantiles, CACM 13(10), 1970.
fn hill(ndf: Real, u: Real) -> QlResult<Real> {
    if ndf > NORMAL_EXACT_DOF {
        return normal_quantile(u);
    }
    let a = 1.0 / (ndf - 0.5);
    let b = 48.0 / (a * a);
    let mut c = ((20700.0 * a / b - 98.0) * a - 16.0) * a + 96.36;
    let d = ((94.5 / (b + c) - 3.0) / b + 1.0) * (a * PI / 2.0).sqrt() * ndf;
    let mut y = (d * 2.0 * u).powf(2.0 / ndf);
    if y > 0.05 + a {
        let x = normal_quantile(u)?;
        y = x * x;
        if ndf < 5.0 {
            c += 0.3 * (ndf - 4.5) * (x + 0.6);
        }
        c += (((0.05 * d * x - 5.0) * x - 7.0) * x - 2.0) * x + b;
        y = (((((0.4 * y + 6.3) * y + 36.0) * y + 94.5) / c - y - 3.0) / b + 1.0) * x;
        y = (a * y * y).exp_m1();
    } else {
        y = ((1.0 / (((ndf + 6.0) / (ndf * y) - 0.089 * d - 0.822) * (ndf + 2.0) * 3.0)
            + 0.5 / (ndf + 4.0))
            * y
            - 1.0)
            * (ndf + 1.0)
            / (ndf + 2.0)
            + 1.0 / y;
    }
    Ok(-(ndf * y).sqrt())
}

/// Shaw's tail series (section 6, Eqs 60 and 62).
fn tail_series(df: Real, ln_normalization: Real, v: Real) -> Real {
    let w = (-ln_normalization).exp() * v;
    let np2 = df + 2.0;
    let np4 = df + 4.0;
    let np6 = df + 6.0;
    let d = [
        1.0,
        -(df + 1.0) / (2.0 * np2),
        -df * (df + 1.0) * (df + 3.0) / (8.0 * np2.powi(2) * np4),
        -df * (df + 1.0) * (df + 5.0) * ((3.0 * df + 7.0) * df - 2.0)
            / (48.0 * np2.powi(3) * np4 * np6),
        -df * (df + 1.0)
            * (df + 7.0)
            * (((((15.0 * df + 154.0) * df + 465.0) * df + 286.0) * df - 336.0) * df + 64.0)
            / (384.0 * np2.powi(4) * np4.powi(2) * np6 * (df + 8.0)),
        -df * (df + 1.0)
            * (df + 3.0)
            * (df + 9.0)
            * ((((((35.0 * df + 452.0) * df + 1573.0) * df + 600.0) * df - 2020.0) * df + 928.0)
                * df
                - 128.0)
            / (1280.0 * np2.powi(5) * np4.powi(2) * np6 * (df + 8.0) * (df + 10.0)),
        -df * (df + 1.0)
            * (df + 11.0)
            * (((((((((((945.0 * df + 31506.0) * df + 425858.0) * df + 2980236.0) * df
                + 11266745.0)
                * df
                + 20675018.0)
                * df
                + 7747124.0)
                * df
                - 22574632.0)
                * df
                - 8565600.0)
                * df
                + 18108416.0)
                * df
                - 7099392.0)
                * df
                + 884736.0)
            / (46080.0
                * np2.powi(6)
                * np4.powi(3)
                * np6.powi(2)
                * (df + 8.0)
                * (df + 10.0)
                * (df + 12.0)),
    ];
    let rn = df.sqrt();
    let div = (rn * w).powf(1.0 / df);
    let power = div * div;
    let series = d.iter().rev().fold(0.0, |acc, &dk| acc * power + dk);
    -(series * rn / div)
}

/// Shaw's body series coefficients `c[1..=10]` of Eq 57, each a polynomial in
/// `1/ν` listed from the highest power down.
#[rustfmt::skip]
const BODY_SERIES: [&[Real]; 10] = [
    &[1.0],
    &[0.16666666666666666667, 0.16666666666666666667],
    &[0.0083333333333333333333, 0.066666666666666666667, 0.058333333333333333333],
    &[0.00019841269841269841270, 0.0017857142857142857143, 0.026785714285714285714, 0.025198412698412698413],
    &[2.7557319223985890653e-6, 0.00037477954144620811287, -0.0011078042328042328042, 0.010559964726631393298, 0.012039792768959435626],
    &[2.5052108385441718775e-8, -0.000062705427288760622094, 0.00059458674042007375341, -0.0016095979637646304313, 0.0061039211560044893378, 0.0038370059724226390893],
    &[1.6059043836821614599e-10, 0.000015401265401265401265, -0.00016376804137220803887, 0.00069084207973096861986, -0.0012579159844784844785, 0.0010898206731540064873, 0.0032177478835464946576],
    &[7.6471637318198164759e-13, -3.9851014346715404916e-6, 0.000049255746366361445727, -0.00024947258047043099953, 0.00064513046951456342991, -0.00076245135440323932387, 0.000033530976880017885309, 0.0017438262298340009980],
    &[2.8114572543455207632e-15, 1.0914179173496789432e-6, -0.000015303004486655377567, 0.000090867107935219902229, -0.00029133414466938067350, 0.00051406605788341121363, -0.00036307660358786885787, -0.00031101086326318780412, 0.00096472747321388644237],
    &[8.2206352466243297170e-18, -3.1239569599829868045e-7, 4.8903045291975346210e-6, -0.000033202652391372058698, 0.00012645437628698076975, -0.00028690924218514613987, 0.00035764655430568632777, -0.00010230378073700412687, -0.00036942667800009661203, 0.00054229262813129686486],
];

/// Shaw's body series for small `ν` (Eqs 56 and 57).
fn body_series(df: Real, ln_normalization: Real, u: Real) -> Real {
    let v = (-ln_normalization).exp() * (u - 0.5);
    let i = 1.0 / df;
    let v2 = v * v;
    v * BODY_SERIES.iter().rev().fold(0.0, |acc, poly| {
        acc * v2 + poly.iter().fold(0.0, |ck, &coeff| ck * i + coeff)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::studentt::StudentT;
    use crate::math::distributions::{Probability, Quantile};

    fn oracle() -> impl Iterator<Item = (Real, Real, Real)> {
        include_str!("../../../../tests/fixtures/studentt_quantile/quantiles.csv")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                let f: Vec<Real> = line.split(',').map(|x| x.parse().unwrap()).collect();
                (f[0], f[1], f[2])
            })
    }

    /// Rows where Boost's exact `ν = 4` closed form cancels (`r - 4` as
    /// `p -> 0.5`, about `4e-4` relative at `p = 0.5 + 1e-7`). They are held to
    /// Boost parity by `four_dof_matches_boost_near_the_centre` instead.
    fn four_dof_centre(nu: Real, p: Real) -> bool {
        nu == 4.0 && (0.499..0.6).contains(&p)
    }

    fn estimate(df: Real, u: Real) -> (Real, bool) {
        let dist = StudentT::new(df).unwrap();
        inverse_students_t(df, dist.ln_normalization, u).unwrap()
    }

    #[test]
    fn closed_forms_are_exact_for_one_two_and_four_dof() {
        for u in [1e-12, 1e-6, 0.01, 0.25, 0.49] {
            let (cauchy, exact) = estimate(1.0, u);
            assert!(exact);
            let expected = -1.0 / (PI * u).tan();
            assert!((cauchy - expected).abs() <= 1e-12 * expected.abs().max(1.0));
            let (two, exact) = estimate(2.0, u);
            assert!(exact);
            let expected = (2.0 * u - 1.0) / (2.0 * u * (1.0 - u)).sqrt();
            assert!((two - expected).abs() <= 1e-12 * expected.abs().max(1.0));
        }
        assert_eq!(estimate(1.0, 0.5), (0.0, true));
        for (nu, p, expected) in
            oracle().filter(|&(nu, p, _)| nu == 4.0 && p < 0.5 && !four_dof_centre(nu, p))
        {
            let (x, exact) = estimate(nu, p);
            assert!(exact);
            assert!(
                (x - expected).abs() <= 1e-12 * expected.abs().max(1.0),
                "p={p}: {x}"
            );
        }
    }

    #[test]
    fn estimates_are_within_hill_and_shaw_accuracy() {
        for (nu, p, expected) in oracle().filter(|&(nu, p, _)| nu != 4.0 && p < 0.5) {
            let (x, exact) = estimate(nu, p);
            assert!(!exact);
            assert!(
                ((x - expected) / expected).abs() <= 1e-4,
                "nu={nu}, p={p}: {x} vs {expected}"
            );
        }
    }

    #[test]
    fn deep_six_dof_tail_and_huge_dof_use_hill_and_normal() {
        let (x, exact) = estimate(6.0, 1e-200);
        assert!(!exact && x < -1e33 && x.is_finite());
        let (x, exact) = estimate(1e21, 0.025);
        assert!(exact);
        assert_eq!(x, InverseCumulativeNormal::standard_value(0.025).unwrap());
        let (x, exact) = estimate(1e9, 0.025);
        assert!(!exact);
        assert!((x + 1.959963984540054).abs() < 1e-8);
    }

    #[test]
    fn quantile_matches_mpmath_table_to_1e_10_relative() {
        for (nu, p, expected) in oracle().filter(|&(nu, p, _)| !four_dof_centre(nu, p)) {
            let dist = StudentT::new(nu).unwrap();
            let x = dist.quantile(Probability::try_from(p).unwrap()).unwrap();
            assert!(
                ((x - expected) / expected).abs() <= 1e-10,
                "nu={nu}, p={p}: {x} vs {expected}"
            );
        }
    }

    /// Values from compiled Boost 1.92 `quantile(students_t(4), p)`
    /// (`tests/fixtures/studentt_quantile/boost_nu4.cpp`), which v1.44 returns.
    #[test]
    fn four_dof_matches_boost_near_the_centre() {
        let dist = StudentT::new(4.0).unwrap();
        for (p, boost) in [
            (0.499, -0.002_666_670_617_245_677),
            (0.4999, -0.000_266_666_671_037_377_94),
            (0.49999, -2.666_666_776_987_159e-5),
            (0.5 + 1.0e-7, 2.665_600_749_850_022_6e-7),
        ] {
            let x = dist.quantile(Probability::try_from(p).unwrap()).unwrap();
            assert!(
                ((x - boost) / boost).abs() <= 1e-12,
                "p={p}: {x} vs {boost}"
            );
        }
    }
}
