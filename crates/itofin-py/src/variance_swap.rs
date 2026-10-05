//! Spot-start variance swaps and retained discrete option-strip replication.

use crate::PyQlError;
use crate::fra::PyPosition;
use crate::market::PyBlackScholesProcess;
use crate::option::PyOptionType;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::instrument::Instrument;
use libitofin::instruments::VarianceSwap;
use libitofin::option::OptionType;
use libitofin::position::Position;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::ReplicatingVarianceSwapEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// A spot-start contract on annualized variance, not volatility.
///
/// Notional multiplies one whole variance unit. Live pricing requires start,
/// evaluation and all market reference dates to coincide. Realized fixings,
/// forward starts and discrete-monitoring corrections are unsupported.
#[gen_stub_pyclass]
#[pyclass(name = "VarianceSwap", unsendable, module = "itofin.instruments")]
pub struct PyVarianceSwap {
    inner: VarianceSwap,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyVarianceSwap {
    /// Retain settings and immutable positive variance strike and notional.
    #[new]
    #[pyo3(signature = (position, strike, notional, start_date, maturity_date, settings))]
    fn new(
        position: PyPosition,
        strike: f64,
        notional: f64,
        start_date: &PyDate,
        maturity_date: &PyDate,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: VarianceSwap::new(
                position.inner(),
                strike,
                notional,
                start_date.inner(),
                maturity_date.inner(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        })
    }

    /// Attach an engine, retaining its process after Python owners disappear.
    fn set_engine(&mut self, engine: &PyReplicatingVarianceSwapEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Return the discounted signed payoff on one whole variance unit.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }

    /// Return the finite-strip annualized variance, which can be signed.
    fn variance(&mut self) -> PyResult<f64> {
        Ok(self.inner.variance().map_err(PyQlError::from)?)
    }

    /// Return a fresh list of (option type, strike, weight) tuples.
    ///
    /// Calls ascend, then puts descend. Synthetic terminal strikes are not
    /// purchased. Expired contracts have no variance or replication weights.
    fn option_weights(&mut self) -> PyResult<Vec<(PyOptionType, f64, f64)>> {
        Ok(self
            .inner
            .option_weights()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(|entry| {
                let kind = match entry.option_type {
                    OptionType::Call => PyOptionType::Call,
                    OptionType::Put => PyOptionType::Put,
                };
                (kind, entry.strike, entry.weight)
            })
            .collect())
    }

    /// Force a fresh calculation without bypassing live-input validation.
    fn recalculate(&mut self) -> PyResult<()> {
        Ok(self.inner.recalculate().map_err(PyQlError::from)?)
    }

    /// Return whether a successful lazy valuation is cached.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }

    /// Return expiry under the retained explicit settings.
    fn is_expired(&self) -> PyResult<bool> {
        Ok(self.inner.is_expired().map_err(PyQlError::from)?)
    }

    /// Return the immutable long or short position.
    fn position(&self) -> PyPosition {
        match self.inner.position() {
            Position::Long => PyPosition::Long,
            Position::Short => PyPosition::Short,
        }
    }

    /// Return the annualized variance strike, not volatility.
    fn strike(&self) -> f64 {
        self.inner.strike()
    }

    /// Return notional per one whole variance unit.
    fn notional(&self) -> f64 {
        self.inner.notional()
    }

    /// Return the immutable contract start date.
    fn start_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.start_date())
    }

    /// Return the immutable contract maturity date.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.maturity_date())
    }
}

/// Discrete log-payoff replication on a retained live Black-Scholes market.
///
/// Each side requires 2..4096 positive finite raw strikes and at least two
/// distinct strikes. The minimum call equals the maximum put exactly. Tail
/// extension dk must remain positive and representable on both sides.
/// This is a finite strip, not infinite-tail integration or a sigma-squared
/// guarantee. Native nonzero-dividend drift has a boundary-dependent mismatch.
#[gen_stub_pyclass]
#[pyclass(
    name = "ReplicatingVarianceSwapEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyReplicatingVarianceSwapEngine {
    inner: SharedMut<ReplicatingVarianceSwapEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyReplicatingVarianceSwapEngine {
    /// Copy list or tuple strike inputs, canonicalizing each side independently.
    ///
    /// PyO3 extracts Python sequences before core bounds; additional core
    /// allocations are bounded. No iterator-only generator contract is promised.
    #[new]
    #[pyo3(signature = (process, call_strikes, put_strikes, *, dk=5.0))]
    fn new(
        process: &PyBlackScholesProcess,
        call_strikes: Vec<f64>,
        put_strikes: Vec<f64>,
        dk: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                ReplicatingVarianceSwapEngine::new(
                    process.inner(),
                    dk,
                    &call_strikes,
                    &put_strikes,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }
}

impl PyReplicatingVarianceSwapEngine {
    fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
