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
//! - the `addOnTerm` virtual hook (`analytichestonengine.hpp:179-181`, base
//!   returns 0), the Bates jump-diffusion extension point.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{
    OneAssetOptionEngine, OneAssetOptionResults, OptionArguments, PlainVanillaPayoff,
    StrikedTypePayoff, TypePayoff,
};
use crate::math::expm1::{expm1, log1p};
use crate::math::integrals::exponential_integrals::{ci_complex, si_complex};
use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
use crate::models::HestonModel;
use crate::models::model::CalibratedModelHolder;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::BlackCalculator;
use crate::shared::SharedMut;
use crate::stochasticprocess::StochasticProcess;
use crate::types::{Complex, Real, Size, Time};
use crate::{fail, require};

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

    /// Mean-reversion speed `kappa`.
    pub fn kappa(&self) -> Real {
        self.kappa
    }

    /// Long-run variance `theta`.
    pub fn theta(&self) -> Real {
        self.theta
    }

    /// Vol-of-vol `sigma`.
    pub fn sigma(&self) -> Real {
        self.sigma
    }

    /// Spot/variance correlation `rho`.
    pub fn rho(&self) -> Real {
        self.rho
    }

    /// Initial variance `v0`.
    pub fn v0(&self) -> Real {
        self.v0
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
    /// function itself, total over both branches.
    ///
    /// The calibrated path (`sigma > 1e-6 || kappa < 1e-8`, `cpp:580`) returns
    /// `exp(lnChF(z, t))`. The complementary small-`sigma` path
    /// (`sigma <= 1e-6 && kappa >= 1e-8`) returns the [`chf_small_sigma`] series
    /// expansion (`cpp:584-617`), where `exp(lnChF)` loses precision as
    /// `sigma -> 0`; a flat-vol calibration drives `sigma` into this branch
    /// (issue #425).
    ///
    /// [`chf_small_sigma`]: HestonChf::chf_small_sigma
    pub fn chf(&self, z: Complex, t: Time) -> Complex {
        if self.sigma > 1e-6 || self.kappa < 1e-8 {
            self.ln_chf(z, t).exp()
        } else {
            self.chf_small_sigma(z, t)
        }
    }

    /// The small-`sigma` series expansion of `chF`
    /// (`analytichestonengine.cpp:584-617`): the sigma-Taylor expansion of
    /// `exp(lnChF)` to order `sigma^2`, taken when `sigma <= 1e-6 &&
    /// kappa >= 1e-8` where the closed form suffers catastrophic cancellation.
    ///
    /// Transcribed term-by-term as `sigma^0 + sigma^1*rho-term + sigma^2-term`.
    /// `a1`/`a2` name the two real subexpressions C++ repeats inline (identical
    /// value, computed once), and `squared(squared(kappa)) = kappa^4`.
    fn chf_small_sigma(&self, z: Complex, t: Time) -> Complex {
        let kappa = self.kappa;
        let sigma = self.sigma;
        let theta = self.theta;
        let rho = self.rho;
        let v0 = self.v0;

        let sigma2 = sigma * sigma;
        let kappa2 = kappa * kappa;
        let kt = kappa * t;
        let ekt = kt.exp();
        let e2kt = (2.0 * kt).exp();
        let rho2 = rho * rho;
        let zpi = z + Complex::new(0.0, 1.0);
        let one_minus_iz = Complex::new(1.0, 0.0) - Complex::new(-z.im, z.re);

        let a1 = theta - v0 + ekt * ((-1.0 + kt) * theta + v0);
        let a2 = 2.0 * theta + kt * theta - v0 - kt * v0 + ekt * ((-2.0 + kt) * theta + v0);

        let term0 = (-(a1 * z * zpi / ekt) / (2.0 * kappa)).exp();

        let term1 =
            (-kt - a1 * z * zpi / (2.0 * ekt * kappa)).exp() * rho * a2 * one_minus_iz * z * z
                / (2.0 * kappa2)
                * sigma;

        let s1 = -2.0 * rho2 * (a2 * a2) * z * z * zpi;
        let s2 = 2.0
            * kappa
            * v0
            * (-zpi + e2kt * (zpi + 4.0 * rho2 * z)
                - 2.0 * ekt * (2.0 * rho2 * z + kt * (zpi + rho2 * (2.0 + kt) * z)));
        let s3 = kappa
            * theta
            * (zpi
                + e2kt * (-5.0 * zpi - 24.0 * rho2 * z + 2.0 * kt * (zpi + 4.0 * rho2 * z))
                + 4.0 * ekt * (zpi + 6.0 * rho2 * z + kt * (zpi + rho2 * (4.0 + kt) * z)));
        let term2 =
            (-2.0 * kt - a1 * z * zpi / (2.0 * ekt * kappa)).exp() * z * z * zpi * (s1 + s2 + s3)
                / (16.0 * kappa2 * kappa2)
                * sigma2;

        term0 + term1 + term2
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

/// The control-variate formula selecting `AP_Helper`'s integrand
/// (`analytichestonengine.hpp:100-118`, the `AP_Helper`-relevant subset).
///
/// [`AngledContour`](ComplexLogFormula::AngledContour) (issue #416) and
/// [`AsymptoticChF`](ComplexLogFormula::AsymptoticChF) (issue #426) are ported:
/// `optimalControlVariate` selects between exactly these two, so both are on the
/// calibrated oracle path. The remaining three variants are deferred to issue
/// #418 and fail loud in [`ApHelper::new`] (they are NOT stubbed to a ported
/// branch: a silent stub would mis-price; #262-class visible omission).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplexLogFormula {
    /// Gatheral form with Andersen-Piterbarg control variate (deferred).
    AndersenPiterbarg,
    /// A slightly better Andersen-Piterbarg control variate (deferred).
    AndersenPiterbargOptCV,
    /// Asymptotic expansion of the characteristic function as control variate
    /// (ported, issue #426).
    AsymptoticChF,
    /// Angled-contour integration with control variate (ported).
    AngledContour,
    /// Angled-contour integration without control variate (deferred).
    AngledContourNoCV,
}

/// The Andersen-Piterbarg `AP_Helper` control-variate integrand
/// (`analytichestonengine.cpp:448-576`), AngledContour and AsymptoticChF
/// branches.
///
/// In C++ `AP_Helper` holds an `AnalyticHestonEngine*` and reads the five
/// Heston parameters plus the characteristic function `chF` off it. There is no
/// engine yet (issue #417), so this carries a [`HestonChf`] (from issue #415)
/// directly and reads the parameters through its accessors.
///
/// `phi`/`psi` (`cpp:479-488`) are the asymptotic control-variate coefficients;
/// they are computed only for [`AsymptoticChF`](ComplexLogFormula::AsymptoticChF)
/// and left zero on the AngledContour branch, which never reads them.
///
/// The `addOnTerm` zero-guard at the top of `operator()` (`cpp:508-512`) is the
/// Bates jump-diffusion hook (base returns 0); it is deferred with `chF` per
/// issue #415 and not ported.
#[derive(Clone, Copy, Debug)]
pub struct ApHelper {
    term: Time,
    fwd: Real,
    strike: Real,
    freq: Real,
    alpha: Real,
    s_alpha: Real,
    v_avg: Real,
    tan_phi: Real,
    phi: Complex,
    psi: Complex,
    cpx_log: ComplexLogFormula,
    chf: HestonChf,
}

impl ApHelper {
    /// `AP_Helper` constructor (`analytichestonengine.cpp:448-505`),
    /// AngledContour and AsymptoticChF branches.
    ///
    /// For AsymptoticChF the C++ `switch` computes `phi_`/`psi_` (`cpp:479-488`),
    /// then `[[fallthrough]]` through the AngledContour `vAvg` (`cpp:490-492`)
    /// and the AngledContourNoCV `tanPhi` block (`cpp:496-500`), so the helper
    /// carries `phi`, `psi`, `vAvg` AND `tanPhi`; AngledContour computes only
    /// `vAvg` and `tanPhi`. The whole `psi_` complex is divided by `sigma^2`
    /// (`cpp:488`), and the imaginary part carries an overall leading minus sign
    /// (`cpp:485`), both transcribed exactly.
    ///
    /// The `boost::math::sign(freq)` in `tanPhi` (`cpp:498`) is [`Real::signum`]
    /// here; they differ only at `freq == 0`, which the `r*freq < 0.0` guard
    /// excludes (the product is `0`, not `< 0`, so the branch is not taken).
    ///
    /// # Errors
    ///
    /// Errors for `AndersenPiterbarg`/`AndersenPiterbargOptCV`/`AngledContourNoCV`:
    /// those branches are deferred to issue #418.
    pub fn new(
        term: Time,
        fwd: Real,
        strike: Real,
        cpx_log: ComplexLogFormula,
        chf: HestonChf,
        alpha: Real,
    ) -> QlResult<Self> {
        let kappa = chf.kappa();
        let theta = chf.theta();
        let sigma = chf.sigma();
        let rho = chf.rho();
        let v0 = chf.v0();

        let freq = (fwd / strike).ln();
        let s_alpha = (alpha * freq).exp();

        let (phi, psi) = match cpx_log {
            ComplexLogFormula::AngledContour => (Complex::new(0.0, 0.0), Complex::new(0.0, 0.0)),
            ComplexLogFormula::AsymptoticChF => {
                let sqrt_1mrho2 = (1.0 - rho * rho).sqrt();
                let phi = -(v0 + term * kappa * theta) / sigma * Complex::new(sqrt_1mrho2, rho);
                let psi = Complex::new(
                    (kappa - 0.5 * rho * sigma) * (v0 + term * kappa * theta)
                        + kappa * theta * (4.0 * (1.0 - rho * rho)).ln(),
                    -((0.5 * rho * rho * sigma - kappa * rho) / sqrt_1mrho2
                        * (v0 + kappa * theta * term)
                        - 2.0 * kappa * theta * (rho / sqrt_1mrho2).atan()),
                ) / (sigma * sigma);
                (phi, psi)
            }
            other => fail!(
                "AP_Helper control variate {other:?} is deferred (issue #418): only \
                 AngledContour (issue #416) and AsymptoticChF (issue #426) are ported; the \
                 deferred branches are not stubbed to a ported one because optimalControlVariate \
                 selects between exactly those two"
            ),
        };

        let v_avg = (1.0 - (-kappa * term).exp()) * (v0 - theta) / (kappa * term) + theta;

        let r = rho - sigma * freq / (v0 + kappa * theta * term);
        let contour_angle = if r * freq < 0.0 {
            std::f64::consts::PI / 12.0 * freq.signum()
        } else {
            0.0
        };
        let tan_phi = contour_angle.tan();

        Ok(ApHelper {
            term,
            fwd,
            strike,
            freq,
            alpha,
            s_alpha,
            v_avg,
            tan_phi,
            phi,
            psi,
            cpx_log,
            chf,
        })
    }

    /// `operator()(u)` (`analytichestonengine.cpp:507-533`), the shared
    /// AngledContour/AsymptoticChF contour: the real-valued integrand fed to
    /// Gauss-Laguerre.
    ///
    /// The contour (`h_u`/`hPrime`, the `exp(-u*tanPhi*freq)` outer factor, the
    /// `/(h_u*hPrime)` and `.real()*s_alpha`) is identical for both branches;
    /// only `phiBS` differs (`cpp:521-526`): AngledContour uses the `vAvg` Black
    /// form, AsymptoticChF uses `exp(u*(1, tanPhi)*phi + psi)`. The complex
    /// `phiBS - chF` is reduced to a `Real` by `.real()` (`cpp:528-532`) inside
    /// the integrand, so the quadrature integrates a real function.
    /// [`HestonChf::chf`] is total (both branches ported, issue #425), mirroring
    /// C++ `chF` which is infallible.
    pub fn evaluate(&self, u: Real) -> Real {
        let i = Complex::new(0.0, 1.0);
        let h_u = Complex::new(u, u * self.tan_phi - self.alpha);
        let h_prime = h_u - i;

        let phi_bs = if self.cpx_log == ComplexLogFormula::AsymptoticChF {
            (u * Complex::new(1.0, self.tan_phi) * self.phi + self.psi).exp()
        } else {
            (-0.5
                * self.v_avg
                * self.term
                * (h_prime * h_prime + Complex::new(-h_prime.im, h_prime.re)))
            .exp()
        };

        let chf_val = self.chf.chf(h_prime, self.term);

        (-u * self.tan_phi * self.freq).exp()
            * (Complex::new(0.0, u * self.freq).exp()
                * Complex::new(1.0, self.tan_phi)
                * (phi_bs - chf_val)
                / (h_u * h_prime))
                .re
            * self.s_alpha
    }

    /// `controlVariateValue` (`analytichestonengine.cpp:550-567`), AngledContour
    /// and AsymptoticChF branches.
    ///
    /// AngledContour (`cpp:551-556`): `BlackCalculator(Call, strike, fwd,
    /// sqrt(vAvg*term)).value()`. The discount is `1.0` (C++ uses the 4-arg
    /// `BlackCalculator` overload with default `discount = 1.0`); the price
    /// assembly in issue #417 multiplies by the risk-free discount, so
    /// discounting here would double-discount.
    ///
    /// AsymptoticChF (`cpp:557-567`): the closed-form asymptotic control-variate
    /// value `fwd - sqrt(strike*fwd)/PI * (exp(psi) * (-2*Ci(-0.5*phiFreq)*
    /// sin(0.5*phiFreq) + cos(0.5*phiFreq)*(PI + 2*Si(0.5*phiFreq)))).real()`,
    /// with `phiFreq = (phi.re, phi.im + freq)` and complex `Ci`/`Si`. The
    /// `alpha == -0.5` requirement (`cpp:558`) holds on the engine path (the
    /// constructor fixes `alpha_ = -0.5`).
    ///
    /// # Errors
    ///
    /// Errors if [`BlackCalculator::new`] rejects its arguments (AngledContour),
    /// if `alpha != -0.5` (AsymptoticChF, `cpp:558`), or if the complex `Ci`/`Si`
    /// series fail to converge.
    pub fn control_variate_value(&self) -> QlResult<Real> {
        if self.cpx_log == ComplexLogFormula::AsymptoticChF {
            require!(self.alpha == -0.5, "alpha must be equal to -0.5");

            let phi_freq = Complex::new(self.phi.re, self.phi.im + self.freq);
            let ci = ci_complex(-0.5 * phi_freq)?;
            let si = si_complex(0.5 * phi_freq)?;

            let bracket = -2.0 * ci * (0.5 * phi_freq).sin()
                + (0.5 * phi_freq).cos() * (Complex::new(std::f64::consts::PI, 0.0) + 2.0 * si);

            return Ok(self.fwd
                - (self.strike * self.fwd).sqrt() / std::f64::consts::PI
                    * (self.psi.exp() * bracket).re);
        }

        Ok(BlackCalculator::new(
            OptionType::Call,
            self.strike,
            self.fwd,
            (self.v_avg * self.term).sqrt(),
            1.0,
        )?
        .value())
    }
}

/// The analytic Heston pricing engine (`analytichestonengine.{hpp,cpp}`),
/// `integrationOrder` constructor / `OptimalCV` path.
///
/// Ports `AnalyticHestonEngine(model, integrationOrder)`
/// (`analytichestonengine.cpp:659-671`): a Gauss-Laguerre [`Integration`] of the
/// requested order, `cpxLog_ = OptimalCV`, and `alpha_ = -0.5`. It prices a
/// European [`VanillaOption`](crate::instruments::VanillaOption) carrying a
/// [`PlainVanillaPayoff`] through the Andersen-Piterbarg control-variate integral
/// (`priceVanillaPayoff`, `cpp:748-859`, the `OptimalCV` case only).
///
/// Follows the [`GenericModelEngine`] precedent (`jamshidianswaptionengine.rs`):
/// the engine holds the model by [`SharedMut`] and registers as an observer of
/// its [`CalibratedModel`](crate::models::model::CalibratedModel) observable, so
/// a parameter or curve change invalidates a priced option.
///
/// ## Ported vs deferred (visible omissions)
///
/// - Only the `integrationOrder` constructor is ported. The Gauss-Lobatto
///   `(model, relTolerance, maxEvaluations)` constructor (`cpp:673-685`) and the
///   explicit `(model, cpxLog, integration, epsilon, alpha)` constructor
///   (`cpp:687-705`) are deferred with their integration algorithms (issue #418).
/// - `cpxLog_` is fixed to `OptimalCV`, so [`calculate`](Self::calculate) always
///   routes through
///   [`optimal_control_variate`](AnalyticHestonEngine::optimal_control_variate).
///   The `Gatheral` / `BranchCorrection` `Fj_Helper` case (`cpp:772-802`) is
///   deferred; only the Andersen-Piterbarg case (`cpp:808-852`) is ported.
/// - `andersenPiterbargEpsilon_` and the `uM` / `epsilon` integration-limit
///   machinery (`cpp:812-818`, `1046-1065`) are dead for Gauss-Laguerre (its
///   `calculate` never calls `maxBound`), so they are omitted (issue #418).
/// - `evaluations_` / `numberOfEvaluations` (`cpp:721-723`) counts quadrature
///   evaluations; it feeds no result the oracle reads, so it is deferred (the
///   count is available on [`Integration::number_of_evaluations`]).
pub struct AnalyticHestonEngine {
    base: OneAssetOptionEngine,
    model: SharedMut<HestonModel>,
    integration: Integration,
    alpha: Real,
}

impl AnalyticHestonEngine {
    /// `AnalyticHestonEngine(model, Size integrationOrder)`
    /// (`analytichestonengine.cpp:659-671`): a Gauss-Laguerre integration of the
    /// given order, `cpxLog = OptimalCV`, `alpha = -0.5`.
    ///
    /// # Errors
    ///
    /// Propagates [`Integration::gauss_laguerre`] failure (`integration_order >
    /// 192`).
    pub fn new(
        model: SharedMut<HestonModel>,
        integration_order: Size,
    ) -> QlResult<AnalyticHestonEngine> {
        let integration = Integration::gauss_laguerre(integration_order)?;
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(AnalyticHestonEngine {
            base,
            model,
            integration,
            alpha: -0.5,
        })
    }

    /// `AnalyticHestonEngine(model)` with the default integration order 144
    /// (`analytichestonengine.hpp:131`).
    ///
    /// # Errors
    ///
    /// Propagates [`Integration::gauss_laguerre`] failure (does not occur for
    /// order 144).
    pub fn with_default_order(model: SharedMut<HestonModel>) -> QlResult<AnalyticHestonEngine> {
        AnalyticHestonEngine::new(model, 144)
    }

    /// `optimalControlVariate` (`analytichestonengine.cpp:707-719`): selects
    /// [`AsymptoticChF`](ComplexLogFormula::AsymptoticChF) when all three
    /// asymptotic-regime conditions hold, else
    /// [`AngledContour`](ComplexLogFormula::AngledContour).
    ///
    /// Ported exactly, including the C++ operator precedence
    /// `((v0 + t*kappa*theta) / sigma) * sqrt(1 - rho^2)` (division by `sigma`
    /// first, then the product) and the natural logarithm in the third
    /// condition. It is deliberately not stubbed to `AngledContour`: on a
    /// parameter set that flips to `AsymptoticChF`, [`ApHelper::new`] fails loud
    /// (issue #418), the intended #262-safe behaviour rather than a silent
    /// mis-price.
    pub fn optimal_control_variate(
        t: Time,
        v0: Real,
        kappa: Real,
        theta: Real,
        sigma: Real,
        rho: Real,
    ) -> ComplexLogFormula {
        if t > 0.15
            && (v0 + t * kappa * theta) / sigma * (1.0 - rho * rho).sqrt() < 0.15
            && ((kappa - 0.5 * rho * sigma) * (v0 + t * kappa * theta)
                + kappa * theta * (4.0 * (1.0 - rho * rho)).ln())
                / (sigma * sigma)
                < 0.1
        {
            ComplexLogFormula::AsymptoticChF
        } else {
            ComplexLogFormula::AngledContour
        }
    }
}

impl AsObservable for AnalyticHestonEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticHestonEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `calculate` (`analytichestonengine.cpp:861-875`): guards a European
    /// [`PlainVanillaPayoff`], then `priceVanillaPayoff(payoff, exerciseDate)`
    /// (`cpp:725-735`, `748-859`) - the forward at the exercise date, the
    /// maturity time and the risk-free discount, then the OptimalCV assembly.
    fn calculate(&mut self) -> QlResult<()> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        require!(
            exercise.exercise_type() == ExerciseType::European,
            "not an European option"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff: &dyn StrikedTypePayoff = &**payoff;
        let Some(payoff) = (payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("non plain vanilla payoff given");
        };
        let payoff = *payoff;
        let maturity_date = exercise.last_date();

        let model = self.model.borrow();
        let process = model.process();
        let kappa = model.kappa();
        let sigma = model.sigma();
        let theta = model.theta();
        let rho = model.rho();
        let v0 = model.v0();
        drop(model);

        let spot = process.s0().current_link()?.value()?;
        if spot.is_nan() || spot <= 0.0 {
            fail!("negative or null underlying given");
        }

        // fwd at the exercise DATE (`cpp:730-732`): s0 * qDiscount / rDiscount.
        let dividend_discount = process
            .dividend_yield()
            .current_link()?
            .discount_date(maturity_date, false)?;
        let risk_free_discount_date = process
            .risk_free_rate()
            .current_link()?
            .discount_date(maturity_date, false)?;
        let fwd = spot * dividend_discount / risk_free_discount_date;

        // maturity as a TIME and the discount over it (`cpp:734, 755`).
        let time = process.time(&maturity_date)?;
        let dr = process
            .risk_free_rate()
            .current_link()?
            .discount(time, false)?;

        let strike = payoff.strike();
        let c_inf = (1.0 - rho * rho).sqrt() * (v0 + kappa * theta * time) / sigma;

        let final_log =
            AnalyticHestonEngine::optimal_control_variate(time, v0, kappa, theta, sigma, rho);
        let chf = HestonChf::new(kappa, theta, sigma, rho, v0);
        let cv_helper = ApHelper::new(time, fwd, strike, final_log, chf, self.alpha)?;

        let cv_value = cv_helper.control_variate_value()?;
        let h_cv = fwd / std::f64::consts::PI
            * self.integration.calculate(c_inf, |u| cv_helper.evaluate(u));

        let value = match payoff.option_type() {
            OptionType::Call => (cv_value + h_cv) * dr,
            OptionType::Put => (cv_value + h_cv - (fwd - strike)) * dr,
        };

        self.base.results_mut().instrument.value = Some(value);
        Ok(())
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
            let value = chf.chf(Complex::new(0.0, 0.0), t);
            assert!(
                (value.re - 1.0).abs() < 1e-14,
                "Re chF(0,{t}) = {}",
                value.re
            );
            assert!(value.im.abs() < 1e-14, "Im chF(0,{t}) = {}", value.im);
        }
    }

    /// The small-`sigma` fixture from the branch-continuity pins: `kappa = 1`
    /// keeps `kappa >= 1e-8` so the series branch is genuinely reachable.
    fn small_sigma_fixture(sigma: Real) -> HestonChf {
        HestonChf::new(1.0, 0.02, sigma, -0.75, 0.01)
    }

    /// BRANCH-CONTINUITY PIN (transcription, independent of the calibration):
    /// at `sigma = 1e-6` the series branch (`cpp:584-617`) is taken, and the
    /// `sigma^0 + sigma^1 + sigma^2` truncation of `exp(lnChF)` agrees with the
    /// closed form `exp(lnChF)` to `O(sigma^3) ~ 1e-18` relative. This pins
    /// branch routing plus term0/term1 (a term0/term1 error shows as `~1e-6`).
    #[test]
    fn chf_small_sigma_series_matches_exp_lnchf() {
        let chf = small_sigma_fixture(1e-6);
        let t = 0.5;
        for &z in &[Complex::new(1.5, -0.5), Complex::new(3.0, 0.0)] {
            let series = chf.chf(z, t);
            let closed = chf.ln_chf(z, t).exp();
            let err = (series - closed).norm();
            assert!(
                err < 1e-12,
                "series vs exp(lnChF) at z={z:?}: series={series:?} closed={closed:?} err={err:e}"
            );
        }
    }

    /// BRANCH-CONTINUITY PIN (term2 discriminator): at `sigma = 1e-3` the
    /// series lands `~O(sigma^3) ~ 1e-9` from `exp(lnChF)` while a wrong
    /// `sigma^2` term shifts it by `~O(sigma^2) ~ 1e-6`, so the `~1e-8` tolerance
    /// discriminates a term2 transcription error that the `sigma=1e-6` pin (and
    /// the calibration, which kills sigma to `~1e-7`) cannot see. Calls
    /// [`HestonChf::chf_small_sigma`] directly because at `sigma = 1e-3` the
    /// public `chf` would route to the `exp(lnChF)` branch.
    #[test]
    fn chf_small_sigma_series_term2_matches_exp_lnchf() {
        let chf = small_sigma_fixture(1e-3);
        let t = 0.5;
        for &z in &[Complex::new(1.5, -0.5), Complex::new(3.0, 0.0)] {
            let series = chf.chf_small_sigma(z, t);
            let closed = chf.ln_chf(z, t).exp();
            let err = (series - closed).norm();
            assert!(
                err < 1e-8,
                "term2 pin at z={z:?}: series={series:?} closed={closed:?} err={err:e}"
            );
        }
    }

    /// INVARIANT PIN through the series branch: `chF(0, t) == 1`. At `z = 0`
    /// term0's exponent is 0 (`exp(0) = 1`) and terms 1/2 carry a `z*z` factor
    /// (`= 0`), so the series is exactly 1 - the discriminating check for a
    /// stray additive constant in term0.
    #[test]
    fn chf_small_sigma_at_zero_is_one() {
        let chf = small_sigma_fixture(1e-6);
        for &t in &[0.1, 0.5, 2.0] {
            let value = chf.chf(Complex::new(0.0, 0.0), t);
            assert!(
                (value.re - 1.0).abs() < 1e-14,
                "Re chF(0,{t}) = {}",
                value.re
            );
            assert!(value.im.abs() < 1e-14, "Im chF(0,{t}) = {}", value.im);
        }
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

    // ---- Integration (Gauss-Laguerre) + AP_Helper (AngledContour) ----

    const TERM: Time = 0.5;
    const FWD: Real = 1.0;
    const STRIKE: Real = 1.05;
    const ALPHA: Real = -0.5;

    /// The closed-form `vAvg` (`analytichestonengine.cpp:490-492`) written out
    /// independently of `ApHelper`, for the oracle-fixture parameters.
    fn v_avg() -> Real {
        (1.0 - (-KAPPA * TERM).exp()) * (V0 - THETA) / (KAPPA * TERM) + THETA
    }

    fn helper() -> ApHelper {
        ApHelper::new(
            TERM,
            FWD,
            STRIKE,
            ComplexLogFormula::AngledContour,
            fixture(),
            ALPHA,
        )
        .unwrap()
    }

    /// PIN (Y2, load-bearing): `controlVariateValue` equals a directly
    /// constructed `BlackCalculator(Call, strike, fwd, sqrt(vAvg*term))` with
    /// discount `1.0` (`analytichestonengine.cpp:553-555`), to machine
    /// precision.
    #[test]
    fn control_variate_value_matches_black_calculator() {
        let reference =
            BlackCalculator::new(OptionType::Call, STRIKE, FWD, (v_avg() * TERM).sqrt(), 1.0)
                .unwrap()
                .value();
        let got = helper().control_variate_value().unwrap();
        assert!(
            (got - reference).abs() < 1e-12,
            "controlVariateValue={got} reference={reference}"
        );
    }

    /// CONFIRM-BY-STUBBING (gate amendment 1): the `discount = 1.0` choice is
    /// load-bearing. The same `BlackCalculator` discounted at `exp(-r*term)`
    /// does NOT equal `controlVariateValue`, so passing the risk-free discount
    /// here (double-discounting) would be caught.
    #[test]
    fn control_variate_value_discount_is_one_not_risk_free() {
        let discounted = BlackCalculator::new(
            OptionType::Call,
            STRIKE,
            FWD,
            (v_avg() * TERM).sqrt(),
            (-0.0225 * TERM).exp(),
        )
        .unwrap()
        .value();
        let got = helper().control_variate_value().unwrap();
        assert!(
            (got - discounted).abs() > 1e-6,
            "discount=1.0 must differ from risk-free-discounted: got={got} discounted={discounted}"
        );
    }

    /// CONFIRM-BY-STUBBING: `controlVariateValue` moves with `vAvg`. A helper
    /// built at a different `term` has a different `vAvg` and thus a different
    /// control-variate value.
    #[test]
    fn control_variate_value_moves_with_v_avg() {
        let other = ApHelper::new(
            2.0,
            FWD,
            STRIKE,
            ComplexLogFormula::AngledContour,
            fixture(),
            ALPHA,
        )
        .unwrap();
        let a = helper().control_variate_value().unwrap();
        let b = other.control_variate_value().unwrap();
        assert!(
            (a - b).abs() > 1e-6,
            "controlVariateValue should move with vAvg: {a} vs {b}"
        );
    }

    /// PIN (Y2, smoke - near-tautological per gate amendment 3): `operator()(u)`
    /// equals a hand-composition of the full AngledContour expression
    /// (`analytichestonengine.cpp:519-532`) built from [`HestonChf::chf`].
    #[test]
    fn evaluate_matches_hand_composition() {
        let h = helper();
        let u = 1.0;
        let tan_phi = h.tan_phi;
        let freq = h.freq;
        let i = Complex::new(0.0, 1.0);

        let h_u = Complex::new(u, u * tan_phi - ALPHA);
        let h_prime = h_u - i;
        let phi_bs =
            (-0.5 * v_avg() * TERM * (h_prime * h_prime + Complex::new(-h_prime.im, h_prime.re)))
                .exp();
        let chf_val = fixture().chf(h_prime, TERM);
        let expected = (-u * tan_phi * freq).exp()
            * (Complex::new(0.0, u * freq).exp() * Complex::new(1.0, tan_phi) * (phi_bs - chf_val)
                / (h_u * h_prime))
                .re
            * h.s_alpha;

        let got = h.evaluate(u);
        assert!(
            (got - expected).abs() < 1e-14,
            "operator()(1.0)={got} expected={expected}"
        );
    }

    /// CONFIRM-BY-STUBBING: `operator()(u)` moves materially with `u`.
    #[test]
    fn evaluate_moves_with_u() {
        let h = helper();
        let a = h.evaluate(0.5);
        let b = h.evaluate(2.5);
        assert!(
            (a - b).abs() > 1e-6,
            "operator() should move with u: {a} vs {b}"
        );
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

    /// Deferred `AP_Helper` control variates fail loud (issue #418), naming the
    /// deferral rather than stubbing to a ported branch. `AsymptoticChF` is now
    /// ported (issue #426) and dropped from the deferred list.
    #[test]
    fn ap_helper_deferred_variants_error() {
        for cpx in [
            ComplexLogFormula::AndersenPiterbarg,
            ComplexLogFormula::AndersenPiterbargOptCV,
            ComplexLogFormula::AngledContourNoCV,
        ] {
            let err = ApHelper::new(TERM, FWD, STRIKE, cpx, fixture(), ALPHA).unwrap_err();
            assert!(err.to_string().contains("deferred"), "{cpx:?}: {err}");
        }
    }

    /// The AsymptoticChF small-vol fixture from the #422/#426 calibration oracle:
    /// `(v0=0.01, kappa=0.2, theta=0.02, sigma=0.3, rho=-0.75)`. At `term > 0.15`
    /// `optimalControlVariate` selects AsymptoticChF for this set.
    fn asymptotic_fixture() -> HestonChf {
        HestonChf::new(0.2, 0.02, 0.3, -0.75, 0.01)
    }

    /// DIRECT PIN (the load-bearing transcription check): the ctor `phi_`/`psi_`
    /// (`analytichestonengine.cpp:479-488`) at `term = 0.50555555555555554`
    /// against C++-computed values (built QuantLib 1.43, the same
    /// `std::complex` expression, `setprecision(17)`). This pins the ctor
    /// coefficients WITHOUT the control-variate cancellation: an AsymptoticChF
    /// price is invariant to a consistent `phi`/`psi` error (the CV is exact for
    /// any valid control variate), so the NPV pin below cannot see a `psi`
    /// imaginary-sign flip or a dropped inner factor - this pin can.
    #[test]
    fn asymptotic_phi_psi_match_cpp() {
        let term: Time = 0.505_555_555_555_555_5;
        let helper = ApHelper::new(
            term,
            1.0,
            0.9,
            ComplexLogFormula::AsymptoticChF,
            asymptotic_fixture(),
            ALPHA,
        )
        .unwrap();

        let expected_phi = Complex::new(-0.026_506_508_505_295_25, 0.030055555555555558);
        let expected_psi = Complex::new(0.066_615_639_957_623_73, -0.12271634681177794);
        assert!(
            (helper.phi - expected_phi).norm() < 1e-12,
            "phi {:?} vs C++ {expected_phi:?}",
            helper.phi
        );
        assert!(
            (helper.psi - expected_psi).norm() < 1e-12,
            "psi {:?} vs C++ {expected_psi:?}",
            helper.psi
        );
    }

    /// CONFIRM-BY-STUBBING (`alpha == -0.5` guard, `cpp:558`): the AsymptoticChF
    /// `controlVariateValue` requires `alpha == -0.5` and errors otherwise. The
    /// engine path fixes `alpha_ = -0.5`, so this only fires on misuse.
    #[test]
    fn asymptotic_control_variate_requires_alpha_minus_half() {
        let helper = ApHelper::new(
            0.5,
            1.0,
            0.9,
            ComplexLogFormula::AsymptoticChF,
            asymptotic_fixture(),
            -0.4,
        )
        .unwrap();
        assert!(helper.control_variate_value().is_err());
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    use crate::exercise::{EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::VanillaOption;
    use crate::interestrate::Compounding;
    use crate::processes::HestonProcess;
    use crate::quotes::make_quote_handle;
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    /// `optimalControlVariate` (`analytichestonengine.cpp:711-718`) selects
    /// `AngledContour` for BOTH testAnalyticVsCached parameter sets - the branch
    /// the ported [`ApHelper`] integrand implements. Arm 1 fails condition 2
    /// (`~0.419 >= 0.15`); Arm 2 passes condition 2 (`~0.078`) but fails
    /// condition 3 (`~0.112 >= 0.1`) - so it is condition 3 that keeps Arm 2 on
    /// the ported branch, transcribed exactly (natural log, exact coefficients).
    #[test]
    fn optimal_control_variate_selects_angled_contour_for_both_oracle_arms() {
        assert_eq!(
            AnalyticHestonEngine::optimal_control_variate(0.2492776, 0.1, 3.16, 0.09, 0.4, -0.2),
            ComplexLogFormula::AngledContour
        );
        assert_eq!(
            AnalyticHestonEngine::optimal_control_variate(0.6986002, 0.09, 1.2, 0.08, 1.8, -0.45),
            ComplexLogFormula::AngledContour
        );
    }

    /// CONFIRM-BY-STUBBING (selector is live): a parameter set satisfying all
    /// three conditions (`t = 1`, `v0 = 0.01`, `kappa = 0.5`, `theta = 0.01`,
    /// `sigma = 2`, `rho = 0`: condition 2 `~0.0075`, condition 3 `~0.0036`)
    /// flips the selector to `AsymptoticChF`, proving it is not hard-wired to
    /// `AngledContour`.
    #[test]
    fn optimal_control_variate_flips_to_asymptotic_chf_in_the_asymptotic_regime() {
        assert_eq!(
            AnalyticHestonEngine::optimal_control_variate(1.0, 0.01, 0.5, 0.01, 2.0, 0.0),
            ComplexLogFormula::AsymptoticChF
        );
    }

    fn flat360(rate: Real, reference: Date) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// CACHED-NPV PIN (independent of the LM loop): an end-to-end
    /// `AnalyticHestonEngine` price in the AsymptoticChF regime against a
    /// C++-generated value (built QuantLib 1.43, `AnalyticHestonEngine(model,
    /// 96)`, `setprecision(17)`).
    ///
    /// Fixture: evaluation date 27 Dec 2004, Actual360, flat `r = 0.04`,
    /// `q = 0.50`, `s0 = 1.0`, `HestonProcess(v0=0.01, kappa=0.2, theta=0.02,
    /// sigma=0.3, rho=-0.75)`, one European `Call(0.9)` expiring 27 Jun 2005
    /// (`tau = 0.50555555555555554`). At that `tau`+params
    /// `optimalControlVariate` returns `AsymptoticChF` (C++ `cpxLog == 4`; all
    /// three conditions hold: `~0.0265 < 0.15`, `~0.0665 < 0.1`), so this
    /// exercises the ported `phi`/`psi` phiBS arm and the `Ci`/`Si` control
    /// variate. C++ prints `NPV = 0.00010317795851725543`; pinned at `1e-10`.
    /// This catches a gross ctor error and any inconsistency between the phiBS
    /// and `Ci`/`Si` arms; the direct `phi`/`psi` pin
    /// (`asymptotic_phi_psi_match_cpp`) catches a consistent transcription error
    /// the CV cancellation hides here.
    #[test]
    fn asymptotic_chf_cached_npv_order_96() {
        let reference = Date::new(27, Month::December, 2004);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference);

        let process = shared(HestonProcess::new(
            flat360(0.04, reference),
            flat360(0.50, reference),
            make_quote_handle(1.0).handle(),
            0.01,
            0.2,
            0.02,
            0.3,
            -0.75,
        ));
        let model = HestonModel::new(process).unwrap();

        let payoff =
            shared(PlainVanillaPayoff::new(OptionType::Call, 0.9)) as Shared<dyn StrikedTypePayoff>;
        let exercise =
            shared(EuropeanExercise::new(Date::new(27, Month::June, 2005))) as Shared<dyn Exercise>;
        let mut option = VanillaOption::new(payoff, exercise, Shared::clone(&settings));
        let engine = shared_mut(AnalyticHestonEngine::new(model, 96).unwrap())
            as SharedMut<dyn PricingEngine>;
        option.base_mut().set_pricing_engine(engine);

        let npv = option.npv().unwrap();
        let expected = 0.00010317795851725543;
        assert!(
            (npv - expected).abs() < 1e-10,
            "AsymptoticChF npv {npv} vs C++ cached {expected} (error {})",
            (npv - expected).abs()
        );
    }
}

/// The `testAnalyticVsCached` oracle (`test-suite/hestonmodel.cpp:439-534`): the
/// `AnalyticHestonEngine` reproduces QuantLib's cached analytic Heston prices.
#[cfg(test)]
mod engine_oracle {
    use super::*;

    use crate::exercise::{EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::VanillaOption;
    use crate::interestrate::Compounding;
    use crate::processes::HestonProcess;
    use crate::quotes::make_quote_handle;
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Day, Month};
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::frequency::Frequency;

    fn settlement() -> Date {
        Date::new(27, Month::December, 2004)
    }

    fn isda() -> DayCounter {
        ActualActual::with_convention(Convention::ISDA)
    }

    /// `flatRate(rate, dc)` (`test-suite/utilities.hpp`): a continuous, annual
    /// [`FlatForward`] on ISDA Actual/Actual anchored at the settlement date.
    /// The evaluation date never moves, so the fixed-reference curve is exactly
    /// equivalent to QuantLib's `FlatForward(0, NullCalendar, ...)`.
    fn flat(rate: Real) -> Shared<FlatForward> {
        shared(FlatForward::with_rate(
            settlement(),
            rate,
            isda(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }

    fn handle(curve: &Shared<FlatForward>) -> Handle<dyn YieldTermStructure> {
        Handle::new(Shared::clone(curve) as Shared<dyn YieldTermStructure>)
    }

    /// Arm 1 (`hestonmodel.cpp:441-481`): a single order-64 price against the
    /// cached `0.0404774515` at `1e-8`. Settlement 27 Dec 2004, exercise 28 Mar
    /// 2005 (ISDA year fraction `~0.2492776`), `Call(1.05)`, flat `r = 0.0225` /
    /// `q = 0.02`, `s0 = 1`, `v0 = 0.1`, `kappa = 3.16`, `theta = 0.09`,
    /// `sigma = 0.4`, `rho = -0.2`.
    #[test]
    fn arm1_cached_analytic_price_order_64() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(settlement());

        let rf = flat(0.0225);
        let div = flat(0.02);
        let process = shared(HestonProcess::new(
            handle(&rf),
            handle(&div),
            make_quote_handle(1.0).handle(),
            0.1,
            3.16,
            0.09,
            0.4,
            -0.2,
        ));
        let model = HestonModel::new(process).unwrap();

        let payoff = shared(PlainVanillaPayoff::new(OptionType::Call, 1.05))
            as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(Date::new(28, Month::March, 2005)))
            as Shared<dyn Exercise>;
        let mut option = VanillaOption::new(payoff, exercise, Shared::clone(&settings));
        let engine = shared_mut(AnalyticHestonEngine::new(model, 64).unwrap())
            as SharedMut<dyn PricingEngine>;
        option.base_mut().set_pricing_engine(engine);

        let npv = option.npv().unwrap();
        let expected = 0.0404774515;
        assert!(
            (npv - expected).abs() < 1e-8,
            "Arm 1: npv {npv} vs cached {expected} (error {})",
            (npv - expected).abs()
        );
    }

    /// Arm 2 (`hestonmodel.cpp:484-533`): the wilmott.com reference set at the
    /// DEFAULT order 144. Six NPVs across exercise dates 8-Sep and 9-Sep 2005
    /// (`8 + i/3`, integer division), strikes `{0.9, 1.0, 1.1}[i % 3]`, flat
    /// `r = 0.05` / `q = 0.02`, per-iteration `s0 = r.discount(0.7) /
    /// q.discount(0.7)` (NOT 1.0), `v0 = 0.09`, `kappa = 1.2`, `theta = 0.08`,
    /// `sigma = 1.8`, `rho = -0.45`. The three T=0.7 values are LINEARLY
    /// INTERPOLATED between the 8-Sep and 9-Sep maturities; the cached numbers
    /// carry that interpolation error, so the tolerance is `100 * 1e-8 = 1e-6`
    /// and a single-point 0.7 NPV would miss.
    #[test]
    fn arm2_wilmott_interpolated_prices_default_order() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(settlement());

        let strikes = [0.9, 1.0, 1.1];
        let mut calculated = [0.0; 6];
        for (i, calc) in calculated.iter_mut().enumerate() {
            let exercise_date = Date::new((8 + i / 3) as Day, Month::September, 2005);

            let rf = flat(0.05);
            let div = flat(0.02);
            let s = rf.discount(0.7, false).unwrap() / div.discount(0.7, false).unwrap();
            let process = shared(HestonProcess::new(
                handle(&rf),
                handle(&div),
                make_quote_handle(s).handle(),
                0.09,
                1.2,
                0.08,
                1.8,
                -0.45,
            ));
            let model = HestonModel::new(process).unwrap();

            let payoff = shared(PlainVanillaPayoff::new(OptionType::Call, strikes[i % 3]))
                as Shared<dyn StrikedTypePayoff>;
            let exercise = shared(EuropeanExercise::new(exercise_date)) as Shared<dyn Exercise>;
            let mut option = VanillaOption::new(payoff, exercise, Shared::clone(&settings));
            let engine = shared_mut(AnalyticHestonEngine::with_default_order(model).unwrap())
                as SharedMut<dyn PricingEngine>;
            option.base_mut().set_pricing_engine(engine);

            *calc = option.npv().unwrap();
        }

        let t1 = isda().year_fraction(settlement(), Date::new(8, Month::September, 2005));
        let t2 = isda().year_fraction(settlement(), Date::new(9, Month::September, 2005));
        let expected = [0.1330371, 0.0641016, 0.0270645];
        for i in 0..3 {
            let interpolated =
                calculated[i] + (calculated[i + 3] - calculated[i]) / (t2 - t1) * (0.7 - t1);
            assert!(
                (interpolated - expected[i]).abs() < 100.0 * 1e-8,
                "Arm 2 strike {}: interpolated {interpolated} vs cached {} (error {})",
                strikes[i],
                expected[i],
                (interpolated - expected[i]).abs()
            );
        }
    }
}
