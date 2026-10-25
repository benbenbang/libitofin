//! Immutable vanilla and cash-digital Black 1976 scalar calculator.
use crate::option::PyOptionType;
use crate::{ItofinError, PyQlError};
use libitofin::instruments::CashOrNothingPayoff;
use libitofin::pricingengines::BlackCalculator;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

fn finite(value: f64) -> PyResult<f64> {
    if !value.is_finite() {
        return Err(ItofinError::new_err(
            "BlackCalculator result must be finite",
        ));
    }
    Ok(value)
}

/// Immutable vanilla or cash-digital standalone calculator, not a pricing engine.
/// Every nonfinite result raises ItofinError, including native zero-strike NaNs.
#[gen_stub_pyclass]
#[pyclass(name = "BlackCalculator", frozen, module = "itofin.pricingengines")]
pub struct PyBlackCalculator {
    inner: BlackCalculator,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBlackCalculator {
    /// Copy finite strike >= 0, forward > 0, std_dev >= 0 and discount > 0.
    /// std_dev is volatility times sqrt(time); no payoff objects are accepted.
    #[new]
    fn new(
        option_type: PyOptionType,
        strike: f64,
        forward: f64,
        std_dev: f64,
        discount: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: BlackCalculator::new(option_type.inner(), strike, forward, std_dev, discount)
                .map_err(PyQlError::from)?,
        })
    }
    /// Copy a cash-or-nothing payoff into a standalone calculator.
    /// Cash payoff may be signed and must be finite. std_dev must exceed
    /// f64::EPSILON, excluding the core's degenerate-volatility Greek shortcut.
    /// Other inputs use the vanilla constructor domains. std_dev is volatility
    /// times sqrt(time). Finite elasticity sentinels remain.
    #[staticmethod]
    fn cash_digital(
        option_type: PyOptionType,
        strike: f64,
        cash_payoff: f64,
        forward: f64,
        std_dev: f64,
        discount: f64,
    ) -> PyResult<Self> {
        if !std_dev.is_finite() || std_dev <= f64::EPSILON {
            return Err(ItofinError::new_err(
                "cash-digital std_dev must exceed f64::EPSILON",
            ));
        }
        if !cash_payoff.is_finite() {
            return Err(ItofinError::new_err("cash payoff must be finite"));
        }
        let payoff = CashOrNothingPayoff::new(option_type.inner(), strike, cash_payoff);
        Ok(Self {
            inner: BlackCalculator::with_striked_payoff(&payoff, forward, std_dev, discount)
                .map_err(PyQlError::from)?,
        })
    }
    /// Present value.
    fn value(&self) -> PyResult<f64> {
        finite(self.inner.value())
    }
    /// Forward delta.
    fn delta_forward(&self) -> PyResult<f64> {
        finite(self.inner.delta_forward())
    }
    /// Spot delta, requiring positive finite spot.
    fn delta(&self, spot: f64) -> PyResult<f64> {
        finite(self.inner.delta(spot).map_err(PyQlError::from)?)
    }
    /// Forward elasticity; native finite MAX/MIN sentinels are retained.
    fn elasticity_forward(&self) -> PyResult<f64> {
        finite(self.inner.elasticity_forward())
    }
    /// Spot elasticity, requiring positive finite spot.
    fn elasticity(&self, spot: f64) -> PyResult<f64> {
        finite(self.inner.elasticity(spot).map_err(PyQlError::from)?)
    }
    /// Forward gamma.
    fn gamma_forward(&self) -> PyResult<f64> {
        finite(self.inner.gamma_forward())
    }
    /// Spot gamma, requiring positive finite spot.
    fn gamma(&self, spot: f64) -> PyResult<f64> {
        finite(self.inner.gamma(spot).map_err(PyQlError::from)?)
    }
    /// Theta per year. Native expiry shortcut is retained.
    fn theta(&self, spot: f64, maturity: f64) -> PyResult<f64> {
        finite(self.inner.theta(spot, maturity).map_err(PyQlError::from)?)
    }
    /// Theta divided by 365. Native expiry shortcut is retained.
    fn theta_per_day(&self, spot: f64, maturity: f64) -> PyResult<f64> {
        finite(
            self.inner
                .theta_per_day(spot, maturity)
                .map_err(PyQlError::from)?,
        )
    }
    /// Vega for finite nonnegative maturity.
    fn vega(&self, maturity: f64) -> PyResult<f64> {
        finite(self.inner.vega(maturity).map_err(PyQlError::from)?)
    }
    /// Discount-rate rho for finite nonnegative maturity.
    fn rho(&self, maturity: f64) -> PyResult<f64> {
        finite(self.inner.rho(maturity).map_err(PyQlError::from)?)
    }
    /// Growth-rate rho for finite nonnegative maturity.
    fn dividend_rho(&self, maturity: f64) -> PyResult<f64> {
        finite(self.inner.dividend_rho(maturity).map_err(PyQlError::from)?)
    }
    /// Native N(d2), independent of option sign.
    fn itm_cash_probability(&self) -> PyResult<f64> {
        finite(self.inner.itm_cash_probability())
    }
    /// Native N(d1), independent of option sign.
    fn itm_asset_probability(&self) -> PyResult<f64> {
        finite(self.inner.itm_asset_probability())
    }
    /// Strike sensitivity.
    fn strike_sensitivity(&self) -> PyResult<f64> {
        finite(self.inner.strike_sensitivity())
    }
    /// Forward coefficient in the native value formula.
    fn alpha(&self) -> PyResult<f64> {
        finite(self.inner.alpha())
    }
    /// Strike coefficient in the native value formula.
    fn beta(&self) -> PyResult<f64> {
        finite(self.inner.beta())
    }
}
