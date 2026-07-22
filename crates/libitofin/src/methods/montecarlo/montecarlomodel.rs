//! General-purpose Monte Carlo model.
//!
//! Port of `ql/methods/montecarlo/montecarlomodel.hpp`: the accumulation engine
//! that ties a path generator to a path pricer and a statistics accumulator.
//! [`add_samples`](MonteCarloModel::add_samples) draws `n` paths, prices each,
//! and feeds `(price, weight)` into the accumulator (`montecarlomodel.hpp:92`).
//!
//! Divergences from `montecarlomodel.hpp`, all deliberate:
//! - **`result_type` fixed to [`Real`]**: C++ generalizes over
//!   `path_pricer_type::result_type` (`montecarlomodel.hpp:59`); the pricers this
//!   stack builds all return a scalar, so [`PathPricer`] fixes the result to
//!   `Real`.
//! - **generic over [`PathGen`], not a concrete generator**: C++ holds a
//!   `shared_ptr` to an abstract `path_generator_type` (`montecarlomodel.hpp:80`)
//!   selected by the `MC` traits policy (`mctraits.hpp:44,55`). Here the model is
//!   generic over `PG: PathGen`, so the same spine drives both the single-factor
//!   [`PathGenerator`](super::PathGenerator) and the multi-factor
//!   [`MultiPathGenerator`](super::MultiPathGenerator).
//! - **`add_samples` is fallible and aborts on the first `Err`**:
//!   [`PathGen::next`] and [`Statistics::add_weighted`] both return `QlResult` on
//!   main; a mid-loop `Err` leaves the samples drawn so far in the accumulator
//!   and returns, faithful to a C++ `QL_REQUIRE` throw unwinding the loop.
//!
//! Antithetic averaging (`montecarlomodel.hpp:108-123`) is ported: when
//! `antithetic_variate` is set, each iteration draws the forward path, then its
//! antithetic partner, prices both, and accumulates `(price + price2) / 2` under
//! the FORWARD sample's weight (`montecarlomodel.hpp:120,127`). Because the
//! single-factor [`PathGen::antithetic`] is a fail-loud `Err`, averaging is
//! exercised only over a multi-factor generator.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **control variate** (`montecarlomodel.hpp:67-69,98-106`): the four CV
//!   constructor parameters are dropped entirely.
//!
//! [`new`]: MonteCarloModel::new

use crate::errors::QlResult;
use crate::math::statistics::{GeneralStatistics, Statistics};
use crate::methods::montecarlo::PathGen;
use crate::types::{Real, Size};

/// Maps a realized path of type `P` to its payoff (the C++ `path_pricer_type`,
/// `montecarlomodel.hpp:57`, whose `operator()(path)` returns `result_type`).
///
/// `P` is the generator's [`PathGen::PathType`] (`Path` or `MultiPath`).
/// Blanket-implemented for any `Fn(&P) -> Real`, so a plain closure serves as a
/// pricer.
pub trait PathPricer<P> {
    /// The payoff realized along `path`.
    fn price(&self, path: &P) -> Real;
}

impl<P, F: Fn(&P) -> Real> PathPricer<P> for F {
    fn price(&self, path: &P) -> Real {
        self(path)
    }
}

/// Draws paths, prices them, and accumulates the sample statistics.
///
/// Generic over the path generator `PG` (single- or multi-factor,
/// `mctraits.hpp:44,55`). `S` defaults to [`GeneralStatistics`], QuantLib's
/// default `Statistics` tool.
pub struct MonteCarloModel<PG, P, S = GeneralStatistics> {
    path_generator: PG,
    path_pricer: P,
    sample_accumulator: S,
    antithetic_variate: bool,
}

impl<PG, P, S> MonteCarloModel<PG, P, S>
where
    PG: PathGen,
    P: PathPricer<PG::PathType>,
    S: Statistics,
{
    /// Builds the model from a path generator, a path pricer, and a (typically
    /// empty) accumulator (`montecarlomodel.hpp:62`). When `antithetic_variate`
    /// is set, [`add_samples`](MonteCarloModel::add_samples) averages each
    /// forward path with its antithetic partner (`montecarlomodel.hpp:108-120`).
    ///
    /// # Errors
    ///
    /// Infallible today; the `QlResult` return preserves the C++ constructor's
    /// fallibility for the deferred control-variate branch.
    pub fn new(
        path_generator: PG,
        path_pricer: P,
        sample_accumulator: S,
        antithetic_variate: bool,
    ) -> QlResult<Self> {
        Ok(MonteCarloModel {
            path_generator,
            path_pricer,
            sample_accumulator,
            antithetic_variate,
        })
    }

    /// Draws, prices, and accumulates `samples` paths (`montecarlomodel.hpp:92`).
    ///
    /// With antithetic variate on, each iteration draws the forward path, then
    /// its antithetic partner (the order is not commutative: `antithetic` reads
    /// the last sequence, `montecarlomodel.hpp:95,109`), prices both, and
    /// accumulates `(price + price2) / 2` under the FORWARD weight
    /// (`montecarlomodel.hpp:120,127`).
    ///
    /// # Errors
    ///
    /// Propagates a [`PathGen::next`], [`PathGen::antithetic`], or
    /// [`Statistics::add_weighted`] failure, aborting with the samples drawn so
    /// far already accumulated.
    pub fn add_samples(&mut self, samples: Size) -> QlResult<()> {
        for _ in 0..samples {
            let sample = self.path_generator.next()?;
            let price = self.path_pricer.price(&sample.value);
            if self.antithetic_variate {
                let antithetic = self.path_generator.antithetic()?;
                let price2 = self.path_pricer.price(&antithetic.value);
                self.sample_accumulator
                    .add_weighted((price + price2) / 2.0, sample.weight)?;
            } else {
                self.sample_accumulator.add_weighted(price, sample.weight)?;
            }
        }
        Ok(())
    }

    /// The sample accumulator, for the mean, error estimate, and richer
    /// statistics (`montecarlomodel.hpp:78`).
    pub fn sample_accumulator(&self) -> &S {
        &self.sample_accumulator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::interestrate::Compounding;
    use crate::math::array::Array;
    use crate::math::matrix::Matrix;
    use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
    use crate::math::statistics::MeanStdDev;
    use crate::math::timegrid::TimeGrid;
    use crate::methods::montecarlo::{MultiPath, MultiPathGenerator, Path, PathGenerator, Sample};
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::processes::{BlackScholesMertonProcess, StochasticProcessArray};
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
    use crate::stochasticprocess::{StochasticProcess, StochasticProcess1D};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Time, Volatility};

    const SPOT: Real = 100.0;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const VOL: Volatility = 0.20;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn gbs_process() -> Shared<dyn StochasticProcess1D> {
        let spot = make_quote_handle(SPOT);
        let vol = RelinkableHandle::new(shared(BlackConstantVol::new(
            reference(),
            Some(Target::new()),
            VOL,
            Actual360::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(BlackScholesMertonProcess::new(
            spot.handle(),
            flat_yield(Q),
            flat_yield(R),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>
    }

    fn gbs_1d(spot: Real, r: Rate, q: Rate, sigma: Volatility) -> Shared<dyn StochasticProcess1D> {
        let quote = make_quote_handle(spot);
        let vol = RelinkableHandle::new(shared(BlackConstantVol::new(
            reference(),
            Some(Target::new()),
            sigma,
            Actual360::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(BlackScholesMertonProcess::new(
            quote.handle(),
            flat_yield(q),
            flat_yield(r),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>
    }

    fn model<P: PathPricer<Path>>(
        pricer: P,
        steps: Size,
        seed: u32,
    ) -> MonteCarloModel<PathGenerator<<PseudoRandom as McRngTraits>::RsgType>, P> {
        let generator = PseudoRandom::make_sequence_generator(steps, seed).unwrap();
        let pg = PathGenerator::new(gbs_process(), 1.0, steps, generator, false).unwrap();
        MonteCarloModel::new(pg, pricer, GeneralStatistics::new(), false).unwrap()
    }

    /// A hand-built [`PathGen`] returning DISTINCT value AND weight on `next()`
    /// versus `antithetic()`. A real generator reuses the forward draw's weight
    /// on the antithetic partner, so only a stub with `w1 != w2` can pin the
    /// "accumulate the FORWARD weight" trap (`montecarlomodel.hpp:120,127`).
    struct WeightStub {
        forward: Sample<Real>,
        anti: Sample<Real>,
    }

    impl PathGen for WeightStub {
        type PathType = Real;

        fn next(&mut self) -> QlResult<Sample<Real>> {
            Ok(self.forward)
        }

        fn antithetic(&mut self) -> QlResult<Sample<Real>> {
            Ok(self.anti)
        }

        fn dimension(&self) -> Size {
            1
        }
    }

    #[test]
    fn constant_pricer_reproduces_its_value_and_count() {
        const K: Real = 3.5;
        const N: Size = 128;
        let mut m = model(|_: &Path| K, 12, 42);
        m.add_samples(N).unwrap();
        assert_eq!(m.sample_accumulator().mean().unwrap(), K);
        assert_eq!(m.sample_accumulator().samples(), N);
    }

    /// The error estimate must fall as `1/sqrt(N)`: quadrupling the sample count
    /// roughly halves the standard error. `#452`'s convergence pin reads
    /// `3 * error_estimate()`, so this scaling is load-bearing. Confirm-by-
    /// stubbing: hardcoding `GeneralStatistics::error_estimate` to a constant
    /// drives the ratio to ~1.0 and fails the `[0.4, 0.6]` band.
    #[test]
    fn error_estimate_shrinks_as_inverse_sqrt_n() {
        const N: Size = 4_000;
        const STEPS: Size = 4;
        let terminal = |path: &Path| path.back();

        let mut m_n = model(terminal, STEPS, 42);
        m_n.add_samples(N).unwrap();
        let se_n = m_n.sample_accumulator().error_estimate().unwrap();

        let mut m_4n = model(terminal, STEPS, 42);
        m_4n.add_samples(4 * N).unwrap();
        let se_4n = m_4n.sample_accumulator().error_estimate().unwrap();

        let ratio = se_4n / se_n;
        assert!(
            (0.4..=0.6).contains(&ratio),
            "se(4N)/se(N) = {ratio}, expected ~0.5 for 1/sqrt(N) scaling"
        );
    }

    /// Antithetic-averaging pin (oracle 3a) plus trap (b), the forward weight.
    /// With antithetic on, every accumulated sample is `((p1+p2)/2, w1)`, so:
    /// - `samples() == n`: `n` forward/antithetic PAIRS, not `2n` draws;
    /// - `mean() == (p1+p2)/2`: averaging is wired, not stubbed to `price` only;
    /// - `weight_sum() == n*w1`: the FORWARD weight (`montecarlomodel.hpp:127`),
    ///   never `n*w2`.
    ///
    /// Confirm-by-stubbing (verified manually, not committed): accumulating
    /// `price` alone drops the mean to `p1`; accumulating `antithetic.weight`
    /// drives `weight_sum` to `n*w2`. Both break this pin.
    #[test]
    fn antithetic_averages_pair_under_the_forward_weight() {
        const N: Size = 16;
        const P1: Real = 4.0;
        const P2: Real = 10.0;
        const W1: Real = 2.0;
        const W2: Real = 3.0;

        let stub = WeightStub {
            forward: Sample::new(P1, W1),
            anti: Sample::new(P2, W2),
        };
        let mut m =
            MonteCarloModel::new(stub, |x: &Real| *x, GeneralStatistics::new(), true).unwrap();
        m.add_samples(N).unwrap();

        let acc = m.sample_accumulator();
        assert_eq!(acc.samples(), N, "n pairs must count as n samples, not 2n");
        assert_eq!(
            acc.mean().unwrap(),
            (P1 + P2) / 2.0,
            "must average the pair"
        );
        assert_eq!(
            acc.weight_sum(),
            N as Real * W1,
            "must accumulate the forward weight, not the antithetic weight"
        );
    }

    /// Oracle 2: the generalized model driven end-to-end over [`MultiPath`]. A
    /// [`MonteCarloModel`] over a two-factor [`MultiPathGenerator`] with a
    /// `PathPricer<MultiPath>` reading asset 0's terminal reproduces the GBM
    /// forward `E[S_0(T)] = S_0 exp((r - q) T)` to a `k/sqrt(N)` band. This is
    /// the integration pin that the same spine drives a multi-factor generator,
    /// not only the hand-built `Path`/`Real` stubs.
    #[test]
    fn multipath_model_reproduces_the_forward_mean() {
        const N: Size = 40_000;
        const STEPS: Size = 4;
        const T: Time = 1.0;
        const S0: Real = 100.0;
        const RR: Rate = 0.05;
        const QQ: Rate = 0.02;
        const SIG: Volatility = 0.20;

        let array = StochasticProcessArray::new(
            vec![gbs_1d(S0, RR, QQ, SIG), gbs_1d(S0, RR, QQ, SIG)],
            &Matrix::from([[1.0, 0.0], [0.0, 1.0]]),
        )
        .unwrap();
        let process = shared(array) as Shared<dyn StochasticProcess>;
        let grid = TimeGrid::new(T, STEPS).unwrap();
        let generator = PseudoRandom::make_sequence_generator(2 * STEPS, 42).unwrap();
        let mpg = MultiPathGenerator::new(process, grid, generator, false).unwrap();

        let mut m = MonteCarloModel::new(
            mpg,
            |mp: &MultiPath| mp[0].back(),
            GeneralStatistics::new(),
            false,
        )
        .unwrap();
        m.add_samples(N).unwrap();

        let mean = m.sample_accumulator().mean().unwrap();
        let target = S0 * ((RR - QQ) * T).exp();
        let var = S0 * S0 * (2.0 * (RR - QQ) * T).exp() * ((SIG * SIG * T).exp() - 1.0);
        let se = (var / N as Real).sqrt();
        assert!(
            (mean - target).abs() < 5.0 * se,
            "E[S0(T)] {mean} vs {target}: {:.2} se",
            (mean - target).abs() / se
        );
    }

    /// A process affine in the Brownian increment: `evolve = x + a*dt + b*dw`,
    /// elementwise. From `x0 = 0` the terminal asset `j` is `a*T + b*sum_i(dw)`,
    /// so a pricer linear in the terminal is linear in the draws: the property
    /// the antithetic variance-collapse pin needs. Raw draws reach `evolve`
    /// directly (this process, not a `StochasticProcessArray`, sits at the
    /// generator boundary), so the antithetic partner negates `dw` block for
    /// block.
    struct ArithmeticProcess {
        a: Real,
        b: Real,
        observable: Shared<Observable>,
    }

    impl ArithmeticProcess {
        fn new(a: Real, b: Real) -> Self {
            ArithmeticProcess {
                a,
                b,
                observable: shared(Observable::new()),
            }
        }
    }

    impl AsObservable for ArithmeticProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess for ArithmeticProcess {
        fn size(&self) -> Size {
            2
        }

        fn factors(&self) -> Size {
            2
        }

        fn initial_values(&self) -> QlResult<Array> {
            Ok(Array::with_size(2))
        }

        fn drift(&self, _t: Time, _x: &Array) -> QlResult<Array> {
            Ok(Array::from([self.a, self.a]))
        }

        fn diffusion(&self, _t: Time, _x: &Array) -> QlResult<Matrix> {
            Ok(Matrix::from([[self.b, 0.0], [0.0, self.b]]))
        }

        fn evolve(&self, _t0: Time, x0: &Array, dt: Time, dw: &Array) -> QlResult<Array> {
            let mut out = x0.clone();
            for i in 0..2 {
                out[i] = x0[i] + self.a * dt + self.b * dw[i];
            }
            Ok(out)
        }
    }

    /// Oracle 3b: antithetic variance collapse. For a process affine in `dw` and
    /// a pricer linear in the terminal, the forward and antithetic terminals are
    /// `a*T +/- b*sum(z)`, so `(price + price2)/2 = a*T` exactly for every pair:
    /// the antithetic estimator's sample variance collapses to machine zero. The
    /// non-antithetic run over the same seed keeps the full draw variance. This
    /// proves the averaging negates real increments (`montecarlomodel.hpp:109`),
    /// not a stub returning the forward value.
    #[test]
    fn antithetic_collapses_a_linear_pricer_variance() {
        const N: Size = 2_000;
        const STEPS: Size = 4;
        const T: Time = 1.0;

        let build = || {
            let process = shared(ArithmeticProcess::new(0.3, 1.0)) as Shared<dyn StochasticProcess>;
            let grid = TimeGrid::new(T, STEPS).unwrap();
            let generator = PseudoRandom::make_sequence_generator(2 * STEPS, 7).unwrap();
            MultiPathGenerator::new(process, grid, generator, false).unwrap()
        };

        let mut anti = MonteCarloModel::new(
            build(),
            |mp: &MultiPath| mp[0].back(),
            GeneralStatistics::new(),
            true,
        )
        .unwrap();
        anti.add_samples(N).unwrap();
        let se_anti = anti.sample_accumulator().error_estimate().unwrap();

        let mut plain = MonteCarloModel::new(
            build(),
            |mp: &MultiPath| mp[0].back(),
            GeneralStatistics::new(),
            false,
        )
        .unwrap();
        plain.add_samples(N).unwrap();
        let se_plain = plain.sample_accumulator().error_estimate().unwrap();

        assert!(
            se_anti < 1e-9,
            "antithetic linear estimator variance must collapse: se={se_anti}"
        );
        assert!(
            se_plain > 0.01,
            "non-antithetic run must retain material variance: se={se_plain}"
        );
    }

    #[test]
    fn a_non_finite_price_aborts_accumulation() {
        let mut m = model(|_: &Path| Real::NAN, 4, 42);
        assert!(m.add_samples(1).is_err());
    }
}
