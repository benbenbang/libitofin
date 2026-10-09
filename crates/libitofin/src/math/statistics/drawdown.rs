//! Maximum running-peak loss for ordered, strictly positive NAV values.

use crate::errors::QlResult;
use crate::fail;
use crate::types::Real;

/// Maximum nonnegative fractional loss and its zero-based ordered NAV indices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawdownResult {
    /// Fractional loss `(peak - trough) / peak`, not a signed return.
    pub drawdown: Real,
    /// Earliest occurrence of the running peak preceding the winning trough.
    pub peak_index: usize,
    /// First trough attaining the greatest computed fractional loss.
    pub trough_index: usize,
}

/// Evaluate maximum drawdown without sorting or treating observations as returns.
///
/// Every NAV must be finite and strictly positive. Equal peaks retain their
/// earliest index; equal computed losses retain the first winning trough.
/// A single observation or nondecreasing sequence returns zero with both
/// indices zero. Results use floating-point loss comparisons; extreme positive
/// peak/trough ratios can round the fractional loss to one.
///
/// # Errors
/// Returns an error for empty input or any nonfinite/nonpositive NAV.
///
/// # Examples
/// ```
/// use libitofin::math::statistics::maximum_drawdown;
/// let result = maximum_drawdown(&[100.0, 120.0, 90.0, 130.0])?;
/// assert_eq!(result.drawdown, 0.25);
/// assert_eq!((result.peak_index, result.trough_index), (1, 2));
/// # Ok::<(), libitofin::errors::QlError>(())
/// ```
pub fn maximum_drawdown(values: &[Real]) -> QlResult<DrawdownResult> {
    let Some(&first) = values.first() else {
        fail!("maximum drawdown requires at least one NAV value");
    };
    let mut peak = first;
    let mut peak_index = 0;
    let mut result = DrawdownResult {
        drawdown: 0.0,
        peak_index: 0,
        trough_index: 0,
    };
    for (index, &value) in values.iter().enumerate() {
        if !value.is_finite() || value <= 0.0 {
            fail!("NAV value at index {index} must be finite and strictly positive");
        }
        if value > peak {
            peak = value;
            peak_index = index;
        } else {
            let loss = (peak - value) / peak;
            if loss > result.drawdown {
                result = DrawdownResult {
                    drawdown: loss,
                    peak_index,
                    trough_index: index,
                };
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawdown_hand_calculated_order_and_ties() {
        for (values, loss, peak, trough) in [
            (vec![100.0], 0.0, 0, 0),
            (vec![100.0, 110.0, 120.0], 0.0, 0, 0),
            (vec![100.0, 80.0, 50.0], 0.5, 0, 2),
            (vec![100.0, 120.0, 90.0, 130.0], 0.25, 1, 2),
            (vec![100.0, 80.0, 100.0, 70.0], 0.3, 0, 3),
            (vec![100.0, 50.0, 200.0, 100.0], 0.5, 0, 1),
            (vec![100.0, 120.0, 120.0, 60.0, 60.0], 0.5, 1, 3),
            (vec![100.0, 80.0, 120.0, 60.0, 140.0, 100.0], 0.5, 2, 3),
        ] {
            assert_eq!(
                maximum_drawdown(&values).unwrap(),
                DrawdownResult {
                    drawdown: loss,
                    peak_index: peak,
                    trough_index: trough,
                }
            );
        }
    }

    #[test]
    fn drawdown_validates_entire_sequence() {
        assert!(maximum_drawdown(&[]).is_err());
        for value in [
            0.0,
            -0.0,
            -1.0,
            Real::NAN,
            Real::INFINITY,
            Real::NEG_INFINITY,
        ] {
            assert!(maximum_drawdown(&[value]).is_err());
            assert!(maximum_drawdown(&[100.0, 1.0, value]).is_err());
        }
    }

    #[test]
    fn drawdown_finite_extremes_scale_and_tiny_losses() {
        let maximum = Real::MAX;
        assert_eq!(
            maximum_drawdown(&[maximum, maximum / 2.0])
                .unwrap()
                .drawdown,
            0.5
        );
        assert_eq!(
            maximum_drawdown(&[maximum, Real::from_bits(1)])
                .unwrap()
                .drawdown,
            1.0
        );
        assert_eq!(
            maximum_drawdown(&[Real::from_bits(2), Real::from_bits(1)])
                .unwrap()
                .drawdown,
            0.5
        );
        assert_eq!(
            maximum_drawdown(&[1.0, Real::from_bits(1.0_f64.to_bits() - 1)])
                .unwrap()
                .drawdown,
            Real::EPSILON / 2.0
        );
        let original = maximum_drawdown(&[100.0, 120.0, 90.0]).unwrap();
        assert_eq!(
            original,
            maximum_drawdown(&[1000.0, 1200.0, 900.0]).unwrap()
        );
        assert_ne!(
            original.drawdown,
            maximum_drawdown(&[90.0, 100.0, 120.0]).unwrap().drawdown
        );
    }
}
