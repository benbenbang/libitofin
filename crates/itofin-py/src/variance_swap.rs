//! Spot-start variance swaps and retained discrete option-strip replication.

use crate::PyQlError;
use crate::fra::PyPosition;
use crate::market::PyBlackScholesProcess;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::instrument::Instrument;
use libitofin::instruments::VarianceSwap;
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
