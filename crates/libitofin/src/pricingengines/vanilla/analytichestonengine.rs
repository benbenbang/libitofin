//! The Heston characteristic function `chF`/`lnChF`.
//!
//! Port of the characteristic-function core of
//! `ql/pricingengines/vanilla/analytichestonengine.cpp:578-657`: the
//! Andersen-Lake cancellation-safe form of the Heston model's normalized
//! characteristic function. In C++ `chF`/`lnChF` are `AnalyticHestonEngine`
//! methods that the `AP_Helper` integrand calls back into
//! (`analytichestonengine.cpp:578`, `.hpp:143-144`). Porting them as a
//! parameter-carrying [`HestonChf`] rather than engine methods avoids an
//! `AP_Helper` <-> engine circular dependency: the future integrand (#416) and
//! engine (#417) both build a [`HestonChf`] from the five params and call it.
//!
//! Deferrals (see the type docs):
//! - the small-`sigma` series expansion branch of `chF`
//!   (`analytichestonengine.cpp:584-617`), off the calibrated oracle path;
//! - the `addOnTerm` virtual hook (`analytichestonengine.hpp:179-181`, base
//!   returns 0), the Bates jump-diffusion extension point.

use crate::errors::QlResult;
use crate::fail;
use crate::math::expm1::{expm1, log1p};
use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
use crate::types::{Complex, Real, Size, Time};

/// The Heston characteristic function, carrying the five model parameters.
///
/// Mirrors the parameters `AnalyticHestonEngine` reads from its `HestonModel`
/// inside `chF`/`lnChF` (`analytichestonengine.cpp:585-589, 626-630`): `kappa`
/// (mean-reversion speed), `theta` (long-run variance), `sigma` (vol of vol),
/// `rho` (spot/variance correlation) and `v0` (initial variance). The
/// characteristic function reads *only* these five; it takes no term structures
/// (the C++ code reads no discount curve here either - see the module-level
/// divergence note in issue #415).
#[derive(Clone, Copy, Debug)]
pub struct HestonChf {
    kappa: Real,
    theta: Real,
    sigma: Real,
    rho: Real,
    v0: Real,
}

impl HestonChf {
    /// Builds the characteristic function from the five Heston parameters.
    pub fn new(kappa: Real, theta: Real, sigma: Real, rho: Real, v0: Real) -> Self {
        HestonChf {
            kappa,
            theta,
            sigma,
            rho,
            v0,
        }
    }

    /// `lnChF(z, t)` (`analytichestonengine.cpp:621-657`): the log
    /// characteristic function in Andersen-Lake form `A + v0*B`.
    ///
    /// The `r` branch (`cpp:638-641`) and the [`expm1`]/[`log1p`] calls
    /// (`cpp:646, 652`) are the cancellation-error reductions of Andersen &
    /// Lake; the naive `exp(x)-1` / `ln(1+x)` would lose precision for small
    /// `D*t` or `r*y`.
    pub fn ln_chf(&self, z: Complex, t: Time) -> Complex {
        let kappa = self.kappa;
        let sigma = self.sigma;
        let theta = self.theta;
        let rho = self.rho;
        let v0 = self.v0;

        let sigma2 = sigma * sigma;

        let g = Complex::new(kappa, 0.0) + rho * sigma * Complex::new(z.im, -z.re);

        let d = (g * g + (z * z + Complex::new(-z.im, z.re)) * sigma2).sqrt();

        let mut r = g - d;
        if g.re * d.re + g.im * d.im > 0.0 {
            r = -sigma2 * z * Complex::new(z.re, z.im + 1.0) / (g + d);
        }

        let y = if d.re != 0.0 || d.im != 0.0 {
            expm1(-d * t) / (2.0 * d)
        } else {
            Complex::new(-0.5 * t, 0.0)
        };

        let a = kappa * theta / sigma2 * (r * t - 2.0 * log1p(-r * y));
        let b = z * Complex::new(z.re, z.im + 1.0) * y / (Complex::new(1.0, 0.0) - r * y);

        a + v0 * b
    }

    /// `chF(z, t)` (`analytichestonengine.cpp:578-619`): the characteristic
    /// function itself.
    ///
    /// The calibrated path (`sigma > 1e-6 || kappa < 1e-8`, `cpp:580`) returns
    /// `exp(lnChF(z, t))`.
    ///
    /// # Errors
    ///
    /// The small-`sigma` series expansion (`cpp:584-617`), reached only when
    /// `sigma <= 1e-6 && kappa >= 1e-8`, is deferred (issue #415): it is off the
    /// calibrated oracle path, so this returns `Err` rather than silently
    /// falling through to `exp(lnChF)`.
    pub fn chf(&self, z: Complex, t: Time) -> QlResult<Complex> {
        if self.sigma > 1e-6 || self.kappa < 1e-8 {
            Ok(self.ln_chf(z, t).exp())
        } else {
            fail!(
                "Heston chF small-sigma series expansion (analytichestonengine.cpp:584-617) is \
                 deferred: only the exp(lnChF) branch (sigma > 1e-6 || kappa < 1e-8) is ported \
                 (issue #415); got sigma={} kappa={}",
                self.sigma,
                self.kappa
            )
        }
    }
}

/// The Gauss-Laguerre `Integration` wrapper
/// (`analytichestonengine.cpp:878-884, 924-930, 974-1035`), reduced to the
/// Gauss-Laguerre algorithm.
///
/// In C++ `Integration` dispatches over 11 quadrature/adaptive algorithms
/// (`cpp:886-972`) selected by an `Algorithm` enum. Only Gauss-Laguerre is on
/// the calibrated oracle path (issue #416); the other ten factories
/// (`gaussLegendre`/`gaussChebyshev`/`gaussChebyshev2nd`/`gaussLobatto`/
/// `gaussKronrod`/`simpson`/`trapezoid`/`discreteSimpson`/`discreteTrapezoid`/
/// `expSinh`) and the `c_inf` `integrand1/2/3` domain transforms
/// (`cpp:54-91`) are deferred to issue #418.
///
/// Divergences from C++:
/// - QuantLib names the Gauss-Laguerre rule `GaussLaguerreIntegration`; the
///   Rust port exposes it as [`GaussianQuadrature::laguerre`] with generalized
///   exponent `s`, so this wraps `laguerre(n, 0.0)` (the plain rule).
/// - [`Integration::calculate`] keeps `c_inf` for interface fidelity with the
///   deferred non-Laguerre paths and issue #417's call site, but the
///   Gauss-Laguerre branch ignores it (`cpp:1001-1003`): the integrand is
///   passed straight to the quadrature. The `maxBound`/`scaling` parameters
///   and the second `calculate` overload (`cpp:1037-1044`), used only by the
///   adaptive/exp-sinh branches, are omitted.
pub struct Integration {
    quadrature: GaussianQuadrature,
}

impl Integration {
    /// `gaussLaguerre` factory (`analytichestonengine.cpp:924-930`): wraps an
    /// order-`int_order` Gauss-Laguerre rule.
    ///
    /// # Errors
    ///
    /// Errors if `int_order > 192` (QuantLib's `QL_REQUIRE`, `cpp:926`) or if
    /// the underlying quadrature construction fails.
    pub fn gauss_laguerre(int_order: Size) -> QlResult<Self> {
        if int_order > 192 {
            fail!("maximum integration order (192) exceeded: got {int_order}");
        }
        Ok(Integration {
            quadrature: GaussianQuadrature::laguerre(int_order, 0.0)?,
        })
    }

    /// `numberOfEvaluations` (`analytichestonengine.cpp:974-982`): the
    /// quadrature order for the Gauss-Laguerre case.
    pub fn number_of_evaluations(&self) -> Size {
        self.quadrature.order()
    }

    /// `isAdaptiveIntegration` (`analytichestonengine.cpp:984-990`): always
    /// `false` for Gauss-Laguerre (only Lobatto/Kronrod/Simpson/Trapezoid/
    /// ExpSinh are adaptive).
    pub fn is_adaptive_integration(&self) -> bool {
        false
    }

    /// `calculate` reduced to the Gauss-Laguerre case
    /// (`analytichestonengine.cpp:992-1035`): `(*gaussianQuadrature_)(f)`.
    ///
    /// `c_inf` is unused for Gauss-Laguerre (it drives the `integrand1/2/3`
    /// domain transforms of the deferred non-Laguerre branches only); it is
    /// retained for interface fidelity. The Rust [`GaussianQuadrature::laguerre`]
    /// rule folds the `e^{-x}` weight into its nodes so `f` is the raw
    /// integrand over `[0, inf)` (i.e. it computes `int_0^inf f(x) dx`).
    pub fn calculate<F: FnMut(Real) -> Real>(&self, _c_inf: Real, f: F) -> Real {
        self.quadrature.integrate(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The #417 calibration oracle parameter set (Heston 1993 / QuantLib
    /// `AnalyticHestonEngine` test fixtures): a moderately-correlated,
    /// finite-vol-of-vol regime well inside the Feller region.
    const KAPPA: Real = 3.16;
    const THETA: Real = 0.09;
    const SIGMA: Real = 0.4;
    const RHO: Real = -0.2;
    const V0: Real = 0.1;

    fn fixture() -> HestonChf {
        HestonChf::new(KAPPA, THETA, SIGMA, RHO, V0)
    }

    /// Independent "Little Heston Trap" (Albrecher et al. 2007) closed form of
    /// the same normalized characteristic function, derived from the standard
    /// `g`/`D`/`C`/`D`-function Heston literature - NOT from the Andersen-Lake
    /// code under test.
    ///
    /// With `g = kappa - i*rho*sigma*z` and `d = sqrt(g^2 + sigma^2 z(z+i))`
    /// (`analytichestonengine.cpp:632-636`, since `Complex(z.im,-z.re) = -i z`
    /// and `Complex(-z.im,z.re) = i z`), the trap form is `C + v0*D` with
    /// `g2 = (g-d)/(g+d)`,
    /// `C = kappa*theta/sigma^2 [(g-d)t - 2 ln((1 - g2 e^{-dt})/(1 - g2))]` and
    /// `D = (g-d)/sigma^2 * (1 - e^{-dt})/(1 - g2 e^{-dt})`. This is
    /// algebraically identical to the ported `A + v0*B` on the non-swapped
    /// branch (`A == C`, `B == D`), cross-checking the transcription.
    fn gatheral_ln_chf(chf: &HestonChf, z: Complex, t: Time) -> Complex {
        let kappa = chf.kappa;
        let theta = chf.theta;
        let sigma = chf.sigma;
        let rho = chf.rho;
        let v0 = chf.v0;
        let sigma2 = sigma * sigma;
        let i = Complex::new(0.0, 1.0);

        let g = Complex::new(kappa, 0.0) - i * rho * sigma * z;
        let d = (g * g + sigma2 * z * (z + i)).sqrt();
        let g2 = (g - d) / (g + d);
        let emdt = (-d * t).exp();

        let c = kappa * theta / sigma2
            * ((g - d) * t
                - 2.0
                    * ((Complex::new(1.0, 0.0) - g2 * emdt) / (Complex::new(1.0, 0.0) - g2)).ln());
        let d_func = (g - d) / sigma2 * (Complex::new(1.0, 0.0) - emdt)
            / (Complex::new(1.0, 0.0) - g2 * emdt);

        c + v0 * d_func
    }

    /// KEY PIN: the ported Andersen-Lake `lnChF` equals the independent Little
    /// Heston Trap form at a benign complex point (moderate `u`, `t = 0.5`, away
    /// from branch cuts and cancellation). Two algebraically-equivalent forms
    /// agreeing to ~1e-12 on a nontrivial complex value catches transcription
    /// errors that invariant pins miss.
    #[test]
    fn ln_chf_matches_gatheral_little_trap() {
        let chf = fixture();
        let t = 0.5;
        for &z in &[
            Complex::new(1.5, -0.5),
            Complex::new(3.0, 0.0),
            Complex::new(2.0, 1.0),
        ] {
            let ported = chf.ln_chf(z, t);
            let reference = gatheral_ln_chf(&chf, z, t);
            let err = (ported - reference).norm();
            assert!(
                err < 1e-12,
                "lnChF mismatch at z={z:?}: ported={ported:?} reference={reference:?} err={err:e}"
            );
        }
    }

    /// Invariant pin: `chF(0, t) == 1` (both components). At `z = 0` the
    /// characteristic function of any distribution is 1; here `r = 0`, so
    /// `lnChF = 0` and `chF = exp(0) = 1`.
    #[test]
    fn chf_at_zero_is_one() {
        let chf = fixture();
        for &t in &[0.1, 0.5, 2.0] {
            let value = chf.chf(Complex::new(0.0, 0.0), t).unwrap();
            assert!(
                (value.re - 1.0).abs() < 1e-14,
                "Re chF(0,{t}) = {}",
                value.re
            );
            assert!(value.im.abs() < 1e-14, "Im chF(0,{t}) = {}", value.im);
        }
    }

    /// The deferred small-`sigma` expansion branch returns `Err` (visible
    /// omission), not a silent fall-through to `exp(lnChF)`. Reached only when
    /// `sigma <= 1e-6 && kappa >= 1e-8` (`analytichestonengine.cpp:580,584`).
    #[test]
    fn chf_small_sigma_branch_is_deferred_error() {
        let chf = HestonChf::new(1.0, 0.04, 1e-8, -0.5, 0.04);
        let err = chf.chf(Complex::new(1.0, 0.0), 0.5).unwrap_err();
        assert!(err.to_string().contains("deferred"));
    }

    /// CONFIRM-BY-STUBBING: the [`expm1`]/[`log1p`] helpers are load-bearing in
    /// `lnChF`. A naive re-implementation swapping them for `exp(x)-1` /
    /// `ln(1+x)` loses precision *relative* to the ported (helper-based) `lnChF`
    /// at a short-maturity, cancellation-prone point (`D*t -> 0`, so
    /// `expm1(-D*t)` and `log1p(-r*y)` both subtract two near-equal quantities),
    /// while the two agree to eps at a benign point. The comparison is relative
    /// because `lnChF` itself is `O(t)` and vanishes with `t`, so a large
    /// relative error at tiny `t` is a small absolute norm.
    ///
    /// The accurate helper form is the reference: the primitive accuracy pins in
    /// [`crate::math::expm1`] independently establish that the helper values -
    /// not the naive ones - are the correct ones.
    #[test]
    fn expm1_log1p_helpers_are_load_bearing_in_ln_chf() {
        let chf = fixture();
        let z = Complex::new(3.0, -0.5);

        let rel_gap = |t: Time| -> Real {
            let ported = chf.ln_chf(z, t);
            let naive = naive_ln_chf(&chf, z, t);
            (ported - naive).norm() / ported.norm()
        };

        let short_rel = rel_gap(1e-10);
        let benign_rel = rel_gap(0.5);

        assert!(
            benign_rel < 1e-13,
            "helper and naive lnChF should agree at t=0.5: relative gap={benign_rel:e}"
        );
        assert!(
            short_rel > 1e4 * benign_rel.max(f64::EPSILON),
            "helper and naive lnChF should diverge at t=1e-10: short_rel={short_rel:e} benign_rel={benign_rel:e}"
        );
    }

    /// `lnChF` with the Andersen-Lake helpers replaced by the naive
    /// `exp(x) - 1` / `ln(1 + x)` forms - the stub for the load-bearing test.
    fn naive_ln_chf(chf: &HestonChf, z: Complex, t: Time) -> Complex {
        let kappa = chf.kappa;
        let sigma = chf.sigma;
        let theta = chf.theta;
        let rho = chf.rho;
        let v0 = chf.v0;
        let sigma2 = sigma * sigma;

        let g = Complex::new(kappa, 0.0) + rho * sigma * Complex::new(z.im, -z.re);
        let d = (g * g + (z * z + Complex::new(-z.im, z.re)) * sigma2).sqrt();

        let mut r = g - d;
        if g.re * d.re + g.im * d.im > 0.0 {
            r = -sigma2 * z * Complex::new(z.re, z.im + 1.0) / (g + d);
        }

        let y = if d.re != 0.0 || d.im != 0.0 {
            ((-d * t).exp() - Complex::new(1.0, 0.0)) / (2.0 * d)
        } else {
            Complex::new(-0.5 * t, 0.0)
        };

        let a = kappa * theta / sigma2 * (r * t - 2.0 * (Complex::new(1.0, 0.0) - r * y).ln());
        let b = z * Complex::new(z.re, z.im + 1.0) * y / (Complex::new(1.0, 0.0) - r * y);

        a + v0 * b
    }

    /// PIN (Integration): Gauss-Laguerre `calculate` on `f(x) = x*e^{-x}`
    /// reproduces `int_0^inf x e^{-x} dx = 1`. The Rust `laguerre` rule folds
    /// the `e^{-x}` weight into its nodes (`f` is the raw integrand over
    /// `[0, inf)`), so the integrand that yields `1` is `x*e^{-x}`, a degree-1
    /// polynomial against the weight - integrated exactly (this diverges for the
    /// ticket's literal `f(x)=x`; see the report). `c_inf` is ignored.
    #[test]
    fn integration_gauss_laguerre_integrates_x_exp() {
        let integration = Integration::gauss_laguerre(64).unwrap();
        let got = integration.calculate(1.0, |x| x * (-x).exp());
        assert!(
            (got - 1.0).abs() < 1e-13,
            "int x e^-x dx = {got}, expected 1"
        );
    }

    /// `numberOfEvaluations` equals the quadrature order for Gauss-Laguerre
    /// (`analytichestonengine.cpp:976-978`); `isAdaptiveIntegration` is `false`.
    #[test]
    fn integration_evaluations_and_adaptivity() {
        let integration = Integration::gauss_laguerre(48).unwrap();
        assert_eq!(integration.number_of_evaluations(), 48);
        assert!(!integration.is_adaptive_integration());
    }

    /// `gaussLaguerre` rejects `int_order > 192`
    /// (`analytichestonengine.cpp:926`).
    #[test]
    fn integration_gauss_laguerre_rejects_high_order() {
        assert!(Integration::gauss_laguerre(193).is_err());
    }
}
