//! Validated OHLC prices and dated price observations.

use crate::errors::QlResult;
use crate::require;
use crate::time::date::Date;
use crate::time::timeseries::TimeSeries;
use crate::types::Real;

/// A finite open-high-low-close observation with an enclosing high-low range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntervalPrice {
    open: Real,
    high: Real,
    low: Real,
    close: Real,
}

impl IntervalPrice {
    /// Builds a price in standard OHLC order.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite prices or a range that excludes open or close.
    pub fn new(open: Real, high: Real, low: Real, close: Real) -> QlResult<Self> {
        require!(
            [open, high, low, close]
                .iter()
                .all(|price| price.is_finite()),
            "OHLC prices must be finite"
        );
        require!(
            low <= open && open <= high && low <= close && close <= high,
            "OHLC open and close must lie within low and high"
        );
        Ok(Self {
            open,
            high,
            low,
            close,
        })
    }

    /// Returns the opening price.
    pub fn open(self) -> Real {
        self.open
    }

    /// Returns the highest price.
    pub fn high(self) -> Real {
        self.high
    }

    /// Returns the lowest price.
    pub fn low(self) -> Real {
        self.low
    }

    /// Returns the closing price.
    pub fn close(self) -> Real {
        self.close
    }
}

/// OHLC observations keyed by non-null dates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriceSeries {
    values: TimeSeries<IntervalPrice>,
}

impl PriceSeries {
    /// Builds a series from equal-length date and OHLC slices.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched lengths, null dates or invalid prices.
    pub fn from_slices(
        dates: &[Date],
        opens: &[Real],
        highs: &[Real],
        lows: &[Real],
        closes: &[Real],
    ) -> QlResult<Self> {
        require!(
            [opens.len(), highs.len(), lows.len(), closes.len()]
                .iter()
                .all(|len| *len == dates.len()),
            "OHLC slices must have the same length as dates"
        );
        let mut values = TimeSeries::new();
        for ((((&date, &open), &high), &low), &close) in
            dates.iter().zip(opens).zip(highs).zip(lows).zip(closes)
        {
            values.insert(date, IntervalPrice::new(open, high, low, close)?)?;
        }
        Ok(Self { values })
    }

    /// Returns the observation at a date, if any.
    pub fn get(&self, date: Date) -> Option<&IntervalPrice> {
        self.values.get(date)
    }

    /// Returns the number of distinct dates.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the series has no observations.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over observations in ascending date order.
    pub fn iter(&self) -> impl Iterator<Item = (Date, &IntervalPrice)> {
        self.values.iter()
    }

    /// Extracts opening prices with their dates.
    pub fn opens(&self) -> TimeSeries<Real> {
        self.values.map_values(|price| price.open())
    }

    /// Extracts highest prices with their dates.
    pub fn highs(&self) -> TimeSeries<Real> {
        self.values.map_values(|price| price.high())
    }

    /// Extracts lowest prices with their dates.
    pub fn lows(&self) -> TimeSeries<Real> {
        self.values.map_values(|price| price.low())
    }

    /// Extracts closing prices with their dates.
    pub fn closes(&self) -> TimeSeries<Real> {
        self.values.map_values(|price| price.close())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_price_validates_finiteness_and_bounds() {
        let price = IntervalPrice::new(-10.0, -8.0, -12.0, -9.0).unwrap();
        assert_eq!(
            (price.open(), price.high(), price.low(), price.close()),
            (-10.0, -8.0, -12.0, -9.0)
        );
        for bad in [
            (-13.0, -8.0, -12.0, -9.0),
            (-10.0, -11.0, -12.0, -9.0),
            (-10.0, -8.0, -9.0, -12.0),
            (Real::NAN, -8.0, -12.0, -9.0),
            (-10.0, Real::INFINITY, -12.0, -9.0),
            (-10.0, -8.0, Real::NEG_INFINITY, -9.0),
        ] {
            assert!(IntervalPrice::new(bad.0, bad.1, bad.2, bad.3).is_err());
        }
    }

    #[test]
    fn price_series_sorts_replaces_and_extracts_each_field() {
        let early = Date::from_serial(45_000);
        let late = Date::from_serial(45_001);
        let dates = [late, early, late];
        let opens = [10.0, 8.0, 12.0];
        let highs = [13.0, 11.0, 15.0];
        let lows = [9.0, 7.0, 11.0];
        let closes = [11.0, 9.0, 14.0];
        let series = PriceSeries::from_slices(&dates, &opens, &highs, &lows, &closes).unwrap();
        assert_eq!(dates, [late, early, late]);
        assert_eq!(opens, [10.0, 8.0, 12.0]);
        assert_eq!(series.len(), 2);
        assert_eq!(
            series.iter().map(|(date, _)| date).collect::<Vec<_>>(),
            [early, late]
        );
        assert_eq!(series.get(late).map(|price| price.close()), Some(14.0));
        assert_eq!(
            series.opens().iter().collect::<Vec<_>>(),
            [(early, &8.0), (late, &12.0)]
        );
        assert_eq!(
            series.highs().iter().collect::<Vec<_>>(),
            [(early, &11.0), (late, &15.0)]
        );
        assert_eq!(
            series.lows().iter().collect::<Vec<_>>(),
            [(early, &7.0), (late, &11.0)]
        );
        assert_eq!(
            series.closes().iter().collect::<Vec<_>>(),
            [(early, &9.0), (late, &14.0)]
        );
    }

    #[test]
    fn empty_and_invalid_price_series_are_safe() {
        let empty = PriceSeries::from_slices(&[], &[], &[], &[], &[]).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.get(Date::from_serial(45_000)), None);
        assert!(empty.closes().is_empty());
        let date = Date::from_serial(45_000);
        assert!(PriceSeries::from_slices(&[date], &[1.0], &[], &[0.0], &[1.0]).is_err());
        assert!(PriceSeries::from_slices(&[Date::null()], &[1.0], &[2.0], &[0.0], &[1.0]).is_err());
        assert!(PriceSeries::from_slices(&[date], &[1.0], &[2.0], &[0.0], &[Real::NAN]).is_err());
    }
}
