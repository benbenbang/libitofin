//! Noncentral chi-square distribution.
//!
//! Port of `NonCentralCumulativeChiSquareDistribution`,
//! `NonCentralCumulativeChiSquareSankaranApprox` and
//! `InverseNonCentralCumulativeChiSquareDistribution` from
//! `ql/math/distributions/chisquaredistribution.{hpp,cpp}`: the noncentral
//! chi-square CDF with `df` degrees of freedom and noncentrality `ncp`, via
//! Ding's (1992) Poisson-weighted series, Sankaran's normal-based approximation,
//! and the inverse CDF (a Brent search over the forward CDF).

use std::f64::consts::PI;

use super::normal::CumulativeNormalDistribution;
use super::{Probability, Quantile};
use crate::errors::QlResult;
use crate::math::gammafunction::log_gamma;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::require;
use crate::types::Real;

/// The cumulative noncentral chi-square distribution with `df` degrees of
/// freedom and noncentrality parameter `ncp`.
#[derive(Clone, Copy, Debug)]
pub struct NonCentralCumulativeChiSquareDistribution {
    df: Real,
    ncp: Real,
}

impl NonCentralCumulativeChiSquareDistribution {
    /// A noncentral chi-square CDF with `df` degrees of freedom and
    /// noncentrality `ncp`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `df` is finite and `> 0` and `ncp` is finite and
    /// `>= 0`.
    pub fn new(df: Real, ncp: Real) -> QlResult<Self> {
        require!(
            df.is_finite() && df > 0.0,
            "degrees of freedom must be a finite positive number, got {df}"
        );
        require!(
            ncp.is_finite() && ncp >= 0.0,
            "noncentrality must be a finite non-negative number, got {ncp}"
        );
        Ok(NonCentralCumulativeChiSquareDistribution { df, ncp })
    }

    /// The Sankaran approximation with the same parameters, used as the fallback
    /// when the direct series cannot be computed in `f64`.
    fn sankaran(&self) -> NonCentralCumulativeChiSquareSankaranApprox {
        NonCentralCumulativeChiSquareSankaranApprox {
            df: self.df,
            ncp: self.ncp,
        }
    }

    /// `P(X <= x)` via Ding's Poisson-weighted series.
    ///
    /// A `NaN` argument yields `NaN`; the support limits are `0` at `x <= 0`
    /// (including `-inf`) and `1` at `+inf`. When the series seed terms lose
    /// precision (large `df + ncp`, or deep in a tail) the result comes from the
    /// [`NonCentralCumulativeChiSquareSankaranApprox`], or `0` in the deep left
    /// tail, since the exact series is then not representable in `f64`; see the
    /// body for details.
    pub fn value(&self, x: Real) -> Real {
        if x.is_nan() {
            return Real::NAN;
        }
        // 0 below the support (covers -inf); the CDF saturates to 1 at +inf.
        if x <= 0.0 {
            return 0.0;
        }
        if x.is_infinite() {
            return 1.0;
        }
        let errmax = 1e-12;
        let itrmax: u64 = 10000;
        let lam = 0.5 * self.ncp;

        let mut u = (-lam).exp();
        let mut v = u;
        let x2 = 0.5 * x;
        let f2 = 0.5 * self.df;

        let mut t = if f2 * Real::EPSILON > 0.125 && (x2 - f2).abs() < Real::EPSILON.sqrt() * f2 {
            // QuantLib's large-f2 form, with its `t` still 0:
            // exp((1-t)(2-t/(f2+1)))/sqrt(2 pi (f2+1)) reduces to exp(2)/sqrt(...).
            2.0_f64.exp() / (2.0 * PI * (f2 + 1.0)).sqrt()
        } else {
            (f2 * x2.ln() - x2 - log_gamma(f2 + 1.0).expect("f2 + 1 > 1 is a valid log_gamma arg"))
                .exp()
        };

        // The direct series is reliable only while its two seed terms keep full
        // f64 precision. The recurrence multiplies each seed forward, so a
        // degraded seed - subnormal (as few as 1 significant bit) or a hard 0 -
        // scales the whole sum into garbage. The threshold is therefore the
        // smallest NORMAL f64, not 0; which seed fails dictates the replacement:
        //
        // * v = exp(-ncp/2) goes subnormal for ncp above ~1417. The Poisson
        //   weighting is then lost across the whole distribution, but df+ncp is
        //   large, so the Sankaran normal approximation (error well under 1e-3
        //   in that regime, and shrinking as df+ncp grows; exact in the tail
        //   limits) is the best f64 can do. (Checked first: with large ncp, t
        //   may still be normal, yet the series is already unusable.)
        // * t (the leading central chi^2(df) term) goes subnormal deep in a tail
        //   of that term, i.e. far from its centre x = df. On the RIGHT (x > df)
        //   the CDF is ~1 and Sankaran is accurate. On the LEFT (x < df) the CDF
        //   underflows toward 0; Sankaran would instead return a spurious df-
        //   dependent floor (e.g. 3e-5 for df=4, 0.0038 for df=2 as x -> 0),
        //   breaking monotonicity and central chi^2 parity, so we return 0.
        if v < Real::MIN_POSITIVE {
            return self.sankaran().value(x);
        }
        if t < Real::MIN_POSITIVE {
            return if x < self.df {
                0.0
            } else {
                self.sankaran().value(x)
            };
        }

        let mut ans = v * t;
        let mut n: u64 = 1;
        let mut f_2n = self.df + 2.0;
        let mut f_x_2n = self.df - x + 2.0;

        // Phase 1: accumulate until the tail factor f_x_2n turns positive (the
        // C++ flag/goto preamble), before testing convergence.
        while f_x_2n <= 0.0 && n <= itrmax {
            u *= lam / n as Real;
            v += u;
            t *= x / f_2n;
            ans += v * t;
            n += 1;
            f_2n += 2.0;
            f_x_2n += 2.0;
        }

        // Phase 2: keep accumulating with the error-bound test.
        let mut bound = t * x / f_x_2n;
        while bound > errmax && n <= itrmax {
            u *= lam / n as Real;
            v += u;
            t *= x / f_2n;
            ans += v * t;
            n += 1;
            f_2n += 2.0;
            f_x_2n += 2.0;
            bound = t * x / f_x_2n;
        }

        // Non-convergence within itrmax is a defect for valid parameters in any
        // practical range (the Sankaran approximation covers very large df+ncp).
        assert!(
            bound <= errmax,
            "noncentral chi-square series did not converge (df={}, ncp={}, x={x})",
            self.df,
            self.ncp
        );
        // Deep in a finite right tail the leading term is subnormal-but-nonzero,
        // so the series runs (no fallback) but accumulates rounding error and can
        // drift just past 1 (e.g. 1.000419 at df=4, x=1500). A CDF must stay in
        // [0, 1]; clamp the rounding overshoot away.
        ans.clamp(0.0, 1.0)
    }
}

/// Sankaran's normal-based approximation of the noncentral chi-square CDF.
#[derive(Clone, Copy, Debug)]
pub struct NonCentralCumulativeChiSquareSankaranApprox {
    df: Real,
    ncp: Real,
}

impl NonCentralCumulativeChiSquareSankaranApprox {
    /// A Sankaran approximation with `df` degrees of freedom and noncentrality
    /// `ncp`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `df` is finite and `> 0` and `ncp` is finite and
    /// `>= 0`.
    pub fn new(df: Real, ncp: Real) -> QlResult<Self> {
        require!(
            df.is_finite() && df > 0.0,
            "degrees of freedom must be a finite positive number, got {df}"
        );
        require!(
            ncp.is_finite() && ncp >= 0.0,
            "noncentrality must be a finite non-negative number, got {ncp}"
        );
        Ok(NonCentralCumulativeChiSquareSankaranApprox { df, ncp })
    }

    /// The approximate `P(X <= x)`.
    pub fn value(&self, x: Real) -> Real {
        if x <= 0.0 {
            return 0.0;
        }
        let df = self.df;
        let ncp = self.ncp;
        let h = 1.0 - 2.0 * (df + ncp) * (df + 3.0 * ncp) / (3.0 * (df + 2.0 * ncp).powi(2));
        let p = (df + 2.0 * ncp) / (df + ncp).powi(2);
        let m = (h - 1.0) * (1.0 - 3.0 * h);
        let u = ((x / (df + ncp)).powf(h) - (1.0 + h * p * (h - 1.0 - 0.5 * (2.0 - h) * m * p)))
            / (h * (2.0 * p).sqrt() * (1.0 + 0.5 * m * p));
        CumulativeNormalDistribution::standard().value(u)
    }
}

/// The inverse of the noncentral chi-square CDF: the quantile function.
///
/// Port of `InverseNonCentralCumulativeChiSquareDistribution`. There is no closed
/// form, so it brackets the root by doubling out from the mean `df + ncp` and
/// then refines it with a [`Brent`] search over the forward CDF.
#[derive(Clone, Copy, Debug)]
pub struct InverseNonCentralCumulativeChiSquareDistribution {
    dist: NonCentralCumulativeChiSquareDistribution,
    guess: Real,
    max_evaluations: usize,
    accuracy: Real,
}

impl InverseNonCentralCumulativeChiSquareDistribution {
    /// An inverse noncentral chi-square CDF with `df` degrees of freedom and
    /// noncentrality `ncp`, with `1e-8` accuracy and a budget of `100`.
    ///
    /// The single budget is shared (as in QuantLib) between the bracketing
    /// doublings and the Brent search. We deviate from QuantLib's default of `10`,
    /// which is too small to converge to `1e-8` for ordinary tail probabilities
    /// once a doubling or two has consumed part of it; `100` leaves the solver
    /// ample room while still stopping the doublings as soon as the CDF reaches
    /// `p`. Override either via [`with_max_evaluations`](Self::with_max_evaluations)
    /// / [`with_accuracy`](Self::with_accuracy).
    ///
    /// # Errors
    ///
    /// Returns an error unless `df` is finite and `> 0` and `ncp` is finite and
    /// `>= 0`.
    pub fn new(df: Real, ncp: Real) -> QlResult<Self> {
        Ok(InverseNonCentralCumulativeChiSquareDistribution {
            dist: NonCentralCumulativeChiSquareDistribution::new(df, ncp)?,
            guess: df + ncp,
            max_evaluations: 100,
            accuracy: 1e-8,
        })
    }

    /// Set the number of bracketing doublings and the Brent evaluation budget.
    pub fn with_max_evaluations(mut self, max_evaluations: usize) -> Self {
        self.max_evaluations = max_evaluations;
        self
    }

    /// Set the root-finding accuracy.
    pub fn with_accuracy(mut self, accuracy: Real) -> Self {
        self.accuracy = accuracy;
        self
    }
}

impl Quantile for InverseNonCentralCumulativeChiSquareDistribution {
    /// The smallest `x` with `cdf(x) >= p`, found by bracketing then Brent.
    ///
    /// # Errors
    ///
    /// Returns an error if the interior Brent search fails to bracket or converge.
    fn quantile(&self, p: Probability) -> QlResult<Real> {
        let x = p.value();
        // The generalized inverse maps the closed endpoints to the support
        // [0, +inf), as in the Poisson and Student-t inverses. Without this, p = 1
        // drives the bracketing to cdf(y) - 1 < 0 everywhere ("root not bracketed").
        if x == 0.0 {
            return Ok(0.0);
        }
        if x == 1.0 {
            return Ok(Real::INFINITY);
        }

        // Find the right end of the bracket: double out from the mean until the
        // CDF reaches x (or the doubling budget is spent).
        let mut upper = self.guess;
        let mut evaluations = self.max_evaluations;
        while self.dist.value(upper) < x && evaluations > 0 {
            upper *= 2.0;
            evaluations -= 1;
        }
        // The left end is 0 if no doubling was needed, else the prior upper.
        let lower = if evaluations == self.max_evaluations {
            0.0
        } else {
            0.5 * upper
        };

        let mut solver = Brent::new().with_max_evaluations(evaluations);
        solver.solve_bracketed(
            |y| self.dist.value(y) - x,
            self.accuracy,
            0.75 * upper,
            lower,
            upper,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cdf;
    use super::super::chisquare::CumulativeChiSquareDistribution;
    use super::*;

    type NonCentral = NonCentralCumulativeChiSquareDistribution;
    type Sankaran = NonCentralCumulativeChiSquareSankaranApprox;

    // Regression: for ncp in roughly [1417, 1490] the Poisson seed exp(-ncp/2) is
    // a nonzero SUBNORMAL f64 (as few as ~1 significant bit), not a hard 0. The
    // old `v <= 0.0` guard let it through, so the series ran on a catastrophically
    // rounded seed: e.g. df=30, ncp=1488 returned 1.000000 where the true CDF is
    // ~0.79, then dropped back below 1 (a 0.066 non-monotone step) once the seed
    // finally underflowed to 0. The fix routes any subnormal seed to Sankaran.
    #[test]
    fn subnormal_poisson_seed_stays_in_range_and_monotone() {
        for ncp in [1417.0, 1450.0, 1480.0, 1488.0, 1490.0] {
            let d = NonCentral::new(30.0, ncp).unwrap();
            let mut prev = 0.0;
            let mut x = 1.0;
            while x <= 2200.0 {
                let p = d.value(x);
                assert!(
                    (0.0..=1.0).contains(&p),
                    "ncp={ncp} x={x}: {p} out of [0,1]"
                );
                assert!(
                    p >= prev - 1e-12,
                    "ncp={ncp} not increasing at x={x}: {prev} -> {p}"
                );
                prev = p;
                x += 4.0;
            }
        }
        // Reference values (true CDF ~0.412955 / 0.789385 from an mpmath oracle).
        let d = NonCentral::new(30.0, 1488.0).unwrap();
        assert!(
            (d.value(1500.0) - 0.412956).abs() < 1e-4,
            "{}",
            d.value(1500.0)
        );
        assert!(
            (d.value(1580.0) - 0.789386).abs() < 1e-4,
            "{}",
            d.value(1580.0)
        );
    }

    // Regression: in the deep left tail the leading central chi^2(df) term t goes
    // subnormal (its log term f2*ln(x2) -> -inf), where the true CDF underflows
    // toward 0. The old `t < MIN_POSITIVE -> Sankaran` fallback instead returned a
    // df-dependent floor (3.08e-5 for df=4, 3.83e-3 for df=2), non-monotone and
    // breaking central chi^2 parity. Left-tail t-underflow now returns 0.
    #[test]
    fn left_tail_underflow_returns_zero_not_sankaran_floor() {
        let xs = [
            f64::MIN_POSITIVE,
            1e-300,
            1e-250,
            1e-200,
            1e-150,
            1e-100,
            1e-50,
            1e-20,
            1e-10,
        ];
        for df in [2.0, 4.0, 10.0] {
            let d = NonCentral::new(df, 0.0).unwrap();
            let central = CumulativeChiSquareDistribution::new(df).unwrap();
            let mut prev = 0.0;
            for x in xs {
                let p = d.value(x);
                assert!(
                    (0.0..=1.0).contains(&p),
                    "df={df} x={x:e}: {p} out of [0,1]"
                );
                assert!(
                    p >= prev,
                    "df={df} not increasing at x={x:e}: {prev} -> {p}"
                );
                // ncp = 0 IS the central chi-square, including in the deep tail.
                assert!(
                    (p - central.cdf(x)).abs() < 1e-12,
                    "df={df} x={x:e}: {p} vs central {}",
                    central.cdf(x)
                );
                prev = p;
            }
        }
    }

    // Port of testSankaranApproximation: the Sankaran approximation must track
    // the exact (Ding-series) CDF within 0.01 across df, ncp and x.
    #[test]
    fn cdf_matches_sankaran_approximation() {
        for df in [2.0, 4.0] {
            for ncp in [1.0, 2.0, 3.0] {
                let d = NonCentral::new(df, ncp).unwrap();
                let s = Sankaran::new(df, ncp).unwrap();
                let mut x = 0.25;
                while x < 10.0 {
                    let (exact, approx) = (d.value(x), s.value(x));
                    assert!(
                        (exact - approx).abs() < 0.01,
                        "df={df} ncp={ncp} x={x}: exact {exact} vs sankaran {approx}"
                    );
                    x += 0.1;
                }
            }
        }
    }

    // Absolute check: with ncp = 0 the noncentral chi-square IS the central
    // chi-square, computed here by an independent algorithm (incomplete gamma).
    #[test]
    fn ncp_zero_reduces_to_central_chi_square() {
        for df in [1.0, 2.0, 5.0, 10.0] {
            let nc = NonCentral::new(df, 0.0).unwrap();
            let central = CumulativeChiSquareDistribution::new(df).unwrap();
            for x in [0.5, 1.0, 3.0, 7.0, 15.0] {
                assert!(
                    (nc.value(x) - central.cdf(x)).abs() < 1e-9,
                    "df={df} x={x}: {} vs {}",
                    nc.value(x),
                    central.cdf(x)
                );
            }
        }
    }

    #[test]
    fn cdf_is_zero_below_support_and_in_unit_interval() {
        let d = NonCentral::new(4.0, 2.0).unwrap();
        assert_eq!(d.value(0.0), 0.0);
        assert_eq!(d.value(-1.0), 0.0);
        let mut prev = 0.0;
        let mut x = 0.1;
        while x < 60.0 {
            let p = d.value(x);
            assert!((0.0..=1.0).contains(&p), "cdf({x}) = {p}");
            assert!(p >= prev - 1e-12, "not increasing at x={x}: {prev} -> {p}");
            prev = p;
            x += 0.1;
        }
        assert!(d.value(60.0) > 0.999);
    }

    // Regression: the leading series term underflows for large x, which used to
    // collapse the CDF to 0 (e.g. value(20000) = 0) and break monotonicity;
    // +inf/NaN used to hit the convergence assert and panic.
    #[test]
    fn upper_tail_saturates_and_non_finite_is_handled() {
        let d = NonCentral::new(4.0, 2.0).unwrap();
        assert!(d.value(1000.0) > 0.999);
        // far right tail saturates to ~1 (Sankaran fallback, within an ulp)
        assert!((d.value(20000.0) - 1.0).abs() < 1e-12);
        assert!((d.value(1.0e9) - 1.0).abs() < 1e-12);
        assert!(d.value(1000.0) <= d.value(20000.0));
        // non-finite arguments
        assert_eq!(d.value(Real::INFINITY), 1.0);
        assert_eq!(d.value(Real::NEG_INFINITY), 0.0);
        assert!(d.value(Real::NAN).is_nan());

        // For large df the seed also underflows in the LEFT tail, where the CDF
        // is ~0 - it must NOT saturate to 1 there (Sankaran fallback gives ~0).
        // The right tail of the same distribution still -> ~1.
        let large_df = NonCentral::new(1000.0, 0.0).unwrap();
        assert!(
            large_df.value(50.0) < 1e-10,
            "left tail: {}",
            large_df.value(50.0)
        );
        assert!((large_df.value(5000.0) - 1.0).abs() < 1e-12);
    }

    // Regression: for large noncentrality the leading Poisson weight exp(-ncp/2)
    // underflows, collapsing the direct series. The result must track the body of
    // the distribution (via the Sankaran fallback) and must NOT report a saturated
    // 1.0 below the mean. Earlier guards returned 0.0, then 1.0, in this regime.
    #[test]
    fn large_noncentrality_tracks_sankaran_not_saturation() {
        let d = NonCentral::new(30.0, 2000.0).unwrap();
        // mean = df + ncp = 2030; the CDF must be well under 1 below it. This is
        // the robust invariant - returning ~1.0 here is definitely wrong.
        assert!(d.value(1900.0) < 0.5, "must not saturate below the mean");
        // These pin the Sankaran-fallback output (a plausible body value for this
        // mean, not an independently proven exact CDF) so the regime stays fixed.
        assert!(
            (d.value(1900.0) - 0.072034).abs() < 1e-4,
            "{}",
            d.value(1900.0)
        );
        assert!(
            (d.value(2030.0) - 0.504434).abs() < 1e-4,
            "{}",
            d.value(2030.0)
        );
        // Monotone through the mean and out to the saturated upper tail.
        assert!(d.value(1500.0) <= d.value(1900.0));
        assert!(d.value(1900.0) <= d.value(2100.0));
        assert!((d.value(5000.0) - 1.0).abs() < 1e-12);
    }

    // Regression: in a finite right tail the leading term is subnormal-but-
    // nonzero, so the direct series runs and rounding drifts the sum past 1
    // (df=4 gave 1.000419 at x=1500), breaking both the [0,1] range and
    // monotonicity against the neighbouring fallback point. The output is clamped.
    #[test]
    fn finite_right_tail_does_not_overshoot_one() {
        for ncp in [2.0, 100.0, 1000.0] {
            let d = NonCentral::new(4.0, ncp).unwrap();
            let mut prev = 0.0;
            let mut x = 1000.0;
            while x <= 3000.0 {
                let p = d.value(x);
                assert!(
                    (0.0..=1.0).contains(&p),
                    "ncp={ncp} x={x}: {p} out of [0,1]"
                );
                assert!(
                    p >= prev - 1e-12,
                    "ncp={ncp} not increasing at x={x}: {prev} -> {p}"
                );
                prev = p;
                x += 50.0;
            }
        }
    }

    #[test]
    fn constructors_reject_invalid_parameters() {
        assert!(NonCentral::new(0.0, 1.0).is_err());
        assert!(NonCentral::new(-1.0, 1.0).is_err());
        assert!(NonCentral::new(2.0, -0.5).is_err());
        assert!(NonCentral::new(Real::NAN, 1.0).is_err());
        assert!(Sankaran::new(2.0, Real::INFINITY).is_err());
    }

    type InverseNonCentral = InverseNonCentralCumulativeChiSquareDistribution;

    fn prob(p: Real) -> Probability {
        Probability::try_from(p).unwrap()
    }

    // The inverse must undo the forward CDF: cdf(quantile(p)) == p. This is the
    // natural oracle for an inverse with no closed form, and exercises the default
    // evaluation budget (which must be large enough to converge for ordinary tails).
    #[test]
    fn inverse_round_trips_the_cdf() {
        for df in [2.0, 4.0, 10.0] {
            for ncp in [0.0, 1.0, 5.0] {
                let fwd = NonCentral::new(df, ncp).unwrap();
                let inv = InverseNonCentral::new(df, ncp).unwrap();
                for p in [0.05, 0.25, 0.5, 0.75, 0.95] {
                    let q = inv.quantile(prob(p)).unwrap();
                    let back = fwd.value(q);
                    assert!(
                        (back - p).abs() < 1e-6,
                        "df={df} ncp={ncp} p={p}: q={q} cdf(q)={back}"
                    );
                }
            }
        }
    }

    #[test]
    fn inverse_is_monotone() {
        let inv = InverseNonCentral::new(4.0, 2.0).unwrap();
        let mut prev = 0.0;
        for p in [0.1, 0.3, 0.5, 0.7, 0.9, 0.99] {
            let q = inv.quantile(prob(p)).unwrap();
            assert!(q >= prev, "not increasing at p={p}: {prev} -> {q}");
            prev = q;
        }
    }

    // The closed endpoints map to the support [0, +inf), matching Poisson/Student-t.
    #[test]
    fn inverse_endpoints_are_support_bounds() {
        let inv = InverseNonCentral::new(4.0, 2.0).unwrap();
        assert_eq!(inv.quantile(prob(0.0)).unwrap(), 0.0);
        assert_eq!(inv.quantile(prob(1.0)).unwrap(), Real::INFINITY);
    }

    #[test]
    fn inverse_rejects_invalid_parameters() {
        assert!(InverseNonCentral::new(0.0, 1.0).is_err());
        assert!(InverseNonCentral::new(2.0, -0.5).is_err());
        assert!(InverseNonCentral::new(Real::INFINITY, 1.0).is_err());
    }
}
