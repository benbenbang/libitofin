//! Immutable scalar payoff evaluation without engine or exercise composition.
use crate::ItofinError;
use crate::option::PyOptionType;
use libitofin::instruments::{
    AssetOrNothingPayoff, CashOrNothingPayoff, GapPayoff, PercentageStrikePayoff,
    PlainVanillaPayoff, StrikedTypePayoff,
};
use libitofin::option::OptionType;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use std::sync::Arc;

/// Safe scalar payoff family, not an engine compatibility promise.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "PayoffKind",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPayoffKind {
    PlainVanilla = 0,
    CashOrNothing = 1,
    AssetOrNothing = 2,
    Gap = 3,
    PercentageStrike = 4,
}

/// Immutable payoff terms and evaluation at a finite signed underlying price.
/// Signed strikes and amounts are allowed. Factories copy all input values.
/// This object cannot be passed to existing option constructors or engines.
#[gen_stub_pyclass]
#[pyclass(name = "ScalarPayoff", frozen, module = "itofin.instruments")]
pub struct PyScalarPayoff {
    inner: Arc<dyn StrikedTypePayoff + Send + Sync>,
    kind: PyPayoffKind,
}

impl PyScalarPayoff {
    fn create(kind: PyPayoffKind, option: PyOptionType, strike: f64, extra: f64) -> PyResult<Self> {
        if !strike.is_finite() || !extra.is_finite() {
            return Err(ItofinError::new_err("payoff parameters must be finite"));
        }
        let option = option.inner();
        let inner: Arc<dyn StrikedTypePayoff + Send + Sync> = match kind {
            PyPayoffKind::PlainVanilla => Arc::new(PlainVanillaPayoff::new(option, strike)),
            PyPayoffKind::CashOrNothing => {
                Arc::new(CashOrNothingPayoff::new(option, strike, extra))
            }
            PyPayoffKind::AssetOrNothing => Arc::new(AssetOrNothingPayoff::new(option, strike)),
            PyPayoffKind::Gap => Arc::new(GapPayoff::new(option, strike, extra)),
            PyPayoffKind::PercentageStrike => Arc::new(PercentageStrikePayoff::new(option, strike)),
        };
        Ok(Self { inner, kind })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyScalarPayoff {
    /// Vanilla intrinsic value, floored at zero.
    #[staticmethod]
    fn plain_vanilla(option_type: PyOptionType, strike: f64) -> PyResult<Self> {
        Self::create(PyPayoffKind::PlainVanilla, option_type, strike, 0.0)
    }
    /// Fixed cash amount strictly beyond the strike; zero at equality.
    #[staticmethod]
    fn cash_or_nothing(option_type: PyOptionType, strike: f64, cash_payoff: f64) -> PyResult<Self> {
        Self::create(
            PyPayoffKind::CashOrNothing,
            option_type,
            strike,
            cash_payoff,
        )
    }
    /// Underlying price strictly beyond the strike; zero at equality.
    #[staticmethod]
    fn asset_or_nothing(option_type: PyOptionType, strike: f64) -> PyResult<Self> {
        Self::create(PyPayoffKind::AssetOrNothing, option_type, strike, 0.0)
    }
    /// Inclusive trigger strike and separate payment strike; no payout floor.
    #[staticmethod]
    fn gap(option_type: PyOptionType, strike: f64, second_strike: f64) -> PyResult<Self> {
        Self::create(PyPayoffKind::Gap, option_type, strike, second_strike)
    }
    /// Dimensionless ratio, not percent points or an absolute monetary strike.
    #[staticmethod]
    fn percentage_strike(option_type: PyOptionType, moneyness: f64) -> PyResult<Self> {
        Self::create(PyPayoffKind::PercentageStrike, option_type, moneyness, 0.0)
    }
    /// Evaluate at a finite signed price. Nonfinite computed results raise.
    fn value(&self, price: f64) -> PyResult<f64> {
        if !price.is_finite() {
            return Err(ItofinError::new_err("payoff price must be finite"));
        }
        let value = self.inner.value(price);
        if !value.is_finite() {
            return Err(ItofinError::new_err("payoff value must be finite"));
        }
        Ok(value)
    }
    /// Stored payoff family.
    fn kind(&self) -> PyPayoffKind {
        self.kind
    }
    /// Stored call or put flag.
    fn option_type(&self) -> PyOptionType {
        match self.inner.option_type() {
            OptionType::Call => PyOptionType::Call,
            OptionType::Put => PyOptionType::Put,
        }
    }
    /// Stored trigger strike, or dimensionless ratio for percentage-strike.
    fn strike(&self) -> f64 {
        self.inner.strike()
    }
    /// Stored cash amount for cash-or-nothing, otherwise None.
    fn cash_payoff(&self) -> Option<f64> {
        (&*self.inner as &dyn std::any::Any)
            .downcast_ref::<CashOrNothingPayoff>()
            .map(CashOrNothingPayoff::cash_payoff)
    }
    /// Stored payment strike for gap, otherwise None.
    fn second_strike(&self) -> Option<f64> {
        (&*self.inner as &dyn std::any::Any)
            .downcast_ref::<GapPayoff>()
            .map(GapPayoff::second_strike)
    }
}
