//! Stateless chart indicators shared by the Go and Python bindings.

use crate::errors::{QlError, QlResult};
use crate::require;
use crate::types::Real;

mod bands_rsi;
mod kd_macd;
mod trend_volume;
mod true_range_atr;
mod vwap_obv;

pub use bands_rsi::{BollingerBands, bollinger_bands, rsi};
pub use kd_macd::{Kd, Macd, kd, kd_default, macd, macd_default};
pub use trend_volume::{VolumeBars, ema, sma, volume_bars};
pub use true_range_atr::{atr, atr_default, true_range};
pub use vwap_obv::{obv, vwap};

/// Dense chart values aligned with the input bars. Entries before
/// `first_valid` are zero placeholders and must be treated as missing.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub values: Vec<Real>,
    pub first_valid: usize,
}

impl ChartSeries {
    pub(crate) fn zeroed(len: usize, first_valid: usize) -> QlResult<Self> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(len)
            .map_err(|_| QlError::new("chart output allocation failed", file!(), line!()))?;
        values.resize(len, 0.0);
        Ok(Self {
            values,
            first_valid: first_valid.min(len),
        })
    }

    /// Returns `None` while the indicator is warming up.
    pub fn get(&self, index: usize) -> Option<Real> {
        self.values
            .get(index)
            .copied()
            .filter(|_| index >= self.first_valid)
    }
}

pub(crate) fn check_period(period: usize) -> QlResult<()> {
    require!(period > 0, "chart period must be positive");
    Ok(())
}

pub(crate) fn check_close(close: &[Real]) -> QlResult<()> {
    for (index, value) in close.iter().enumerate() {
        require!(value.is_finite(), "nonfinite close at index {index}");
    }
    Ok(())
}

pub(crate) fn check_hlc(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<()> {
    require!(
        high.len() == low.len() && high.len() == close.len(),
        "chart high/low/close lengths differ"
    );
    for index in 0..close.len() {
        require!(
            high[index].is_finite() && low[index].is_finite() && close[index].is_finite(),
            "nonfinite high/low/close at index {index}"
        );
        require!(
            low[index] <= close[index] && close[index] <= high[index],
            "invalid high/low/close at index {index}"
        );
    }
    Ok(())
}
