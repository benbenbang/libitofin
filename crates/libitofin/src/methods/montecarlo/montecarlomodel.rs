//! General-purpose Monte Carlo model.
//!
//! Port of `ql/methods/montecarlo/montecarlomodel.hpp`: the accumulation engine
//! that ties a [`PathGenerator`] to a path pricer and a statistics accumulator.
//! [`add_samples`](MonteCarloModel::add_samples) draws `n` paths, prices each,
//! and feeds `(price, weight)` into the accumulator (`montecarlomodel.hpp:92`).
//!
//! Divergences from `montecarlomodel.hpp`, all deliberate:
//! - **`result_type` fixed to [`Real`]**: C++ generalizes over
//!   `path_pricer_type::result_type` (`montecarlomodel.hpp:59`); the single-asset
//!   pricers this stack builds all return a scalar, so [`PathPricer`] fixes the
//!   result to `Real`.
//! - **concrete `PathGenerator<GSG>`**: C++ holds a `shared_ptr` to an abstract
//!   `path_generator_type` (`montecarlomodel.hpp:80`). Composing the concrete
//!   generator is enough for the one consumer (the MC vanilla engine) and keeps
//!   the borrow of the mutable generator local.
//! - **`add_samples` is fallible and aborts on the first `Err`**:
//!   [`PathGenerator::next`] and [`Statistics::add_weighted`] both return
//!   `QlResult` on main; a mid-loop `Err` leaves the samples drawn so far in the
//!   accumulator and returns, faithful to a C++ `QL_REQUIRE` throw unwinding the
//!   loop.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **antithetic variate** (`montecarlomodel.hpp:108-120`): [`new`] keeps the
//!   `antithetic_variate` flag but returns `Err` when it is `true`, mirroring
//!   [`PathGenerator`]'s `brownian_bridge` deferral on this stack.
//! - **control variate** (`montecarlomodel.hpp:67-69,98-106`): the four CV
//!   constructor parameters are dropped entirely.
//!
//! [`new`]: MonteCarloModel::new

use crate::errors::QlResult;
use crate::math::randomnumbers::rngtraits::SequenceGenerator;
use crate::math::statistics::{GeneralStatistics, Statistics};
use crate::methods::montecarlo::{Path, PathGenerator};
use crate::require;
use crate::types::{Real, Size};

/// Maps a realized [`Path`] to its payoff (the C++ `path_pricer_type`,
/// `montecarlomodel.hpp:57`, whose `operator()(path)` returns `result_type`).
///
/// Blanket-implemented for any `Fn(&Path) -> Real`, so a plain closure serves as
/// a pricer.
pub trait PathPricer {
    /// The payoff realized along `path`.
    fn price(&self, path: &Path) -> Real;
}

impl<F: Fn(&Path) -> Real> PathPricer for F {
    fn price(&self, path: &Path) -> Real {
        self(path)
    }
}

/// Draws paths, prices them, and accumulates the sample statistics.
///
/// `S` defaults to [`GeneralStatistics`], QuantLib's default `Statistics` tool.
pub struct MonteCarloModel<GSG, P, S = GeneralStatistics> {
    path_generator: PathGenerator<GSG>,
    path_pricer: P,
    sample_accumulator: S,
}

impl<GSG, P, S> MonteCarloModel<GSG, P, S>
where
    GSG: SequenceGenerator,
    P: PathPricer,
    S: Statistics,
{
    /// Builds the model from a path generator, a path pricer, and a (typically
    /// empty) accumulator (`montecarlomodel.hpp:62`).
    ///
    /// # Errors
    ///
    /// Returns `Err("antithetic variate not supported")` when
    /// `antithetic_variate` is `true` (deferred, `montecarlomodel.hpp:108`).
    pub fn new(
        path_generator: PathGenerator<GSG>,
        path_pricer: P,
        sample_accumulator: S,
        antithetic_variate: bool,
    ) -> QlResult<Self> {
        require!(!antithetic_variate, "antithetic variate not supported");
        Ok(MonteCarloModel {
            path_generator,
            path_pricer,
            sample_accumulator,
        })
    }

    /// Draws, prices, and accumulates `samples` paths (`montecarlomodel.hpp:92`).
    ///
    /// # Errors
    ///
    /// Propagates a [`PathGenerator::next`] or [`Statistics::add_weighted`]
    /// failure, aborting with the samples drawn so far already accumulated.
    pub fn add_samples(&mut self, samples: Size) -> QlResult<()> {
        for _ in 0..samples {
            let sample = self.path_generator.next()?;
            let price = self.path_pricer.price(&sample.value);
            self.sample_accumulator.add_weighted(price, sample.weight)?;
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
    use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
    use crate::math::statistics::MeanStdDev;
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
    use crate::stochasticprocess::StochasticProcess1D;
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Volatility};

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

    fn model<P: PathPricer>(
        pricer: P,
        steps: Size,
        seed: u32,
    ) -> MonteCarloModel<<PseudoRandom as McRngTraits>::RsgType, P> {
        let generator = PseudoRandom::make_sequence_generator(steps, seed).unwrap();
        let pg = PathGenerator::new(gbs_process(), 1.0, steps, generator, false).unwrap();
        MonteCarloModel::new(pg, pricer, GeneralStatistics::new(), false).unwrap()
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

    #[test]
    fn antithetic_variate_is_rejected_as_deferred() {
        let generator = PseudoRandom::make_sequence_generator(4, 42).unwrap();
        let pg = PathGenerator::new(gbs_process(), 1.0, 4, generator, false).unwrap();
        match MonteCarloModel::new(pg, |_: &Path| 1.0, GeneralStatistics::new(), true) {
            Err(e) => assert!(e.message().contains("antithetic")),
            Ok(_) => panic!("antithetic_variate = true must be rejected as deferred"),
        }
    }

    #[test]
    fn a_non_finite_price_aborts_accumulation() {
        let mut m = model(|_: &Path| Real::NAN, 4, 42);
        assert!(m.add_samples(1).is_err());
    }
}
