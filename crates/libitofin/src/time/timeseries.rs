//! Date-keyed values kept in ascending date order.

use std::collections::BTreeMap;

use crate::errors::QlResult;
use crate::require;
use crate::time::date::Date;

/// A series with at most one value per non-null date.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSeries<T> {
    values: BTreeMap<Date, T>,
}

impl<T> Default for TimeSeries<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TimeSeries<T> {
    /// Builds an empty series.
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Builds a series from date-value pairs. Later values replace earlier ones.
    ///
    /// # Errors
    ///
    /// Returns an error if a date is null.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Date, T)>) -> QlResult<Self> {
        let mut series = Self::new();
        for (date, value) in pairs {
            series.insert(date, value)?;
        }
        Ok(series)
    }

    /// Inserts a value, returning the previous value at that date if present.
    ///
    /// # Errors
    ///
    /// Returns an error if `date` is null.
    pub fn insert(&mut self, date: Date, value: T) -> QlResult<Option<T>> {
        require!(
            date.serial_number() != 0,
            "time series date must not be null"
        );
        Ok(self.values.insert(date, value))
    }

    /// Returns the value at a date, or `None` if it is absent.
    pub fn get(&self, date: Date) -> Option<&T> {
        self.values.get(&date)
    }

    /// Returns the number of distinct dates.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the series has no observations.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the earliest date, if any.
    pub fn first_date(&self) -> Option<Date> {
        self.values.first_key_value().map(|(date, _)| *date)
    }

    /// Returns the latest date, if any.
    pub fn last_date(&self) -> Option<Date> {
        self.values.last_key_value().map(|(date, _)| *date)
    }

    /// Iterates over date-value pairs in ascending date order.
    pub fn iter(&self) -> impl Iterator<Item = (Date, &T)> {
        self.values.iter().map(|(date, value)| (*date, value))
    }

    /// Transforms values while retaining their dates and ordering.
    pub fn map_values<U>(&self, mut f: impl FnMut(&T) -> U) -> TimeSeries<U> {
        TimeSeries {
            values: self
                .values
                .iter()
                .map(|(date, value)| (*date, f(value)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_series_and_missing_lookup_are_safe() {
        let series = TimeSeries::<i32>::new();
        assert!(series.is_empty());
        assert_eq!(series.len(), 0);
        assert_eq!(series.first_date(), None);
        assert_eq!(series.last_date(), None);
        assert_eq!(series.get(Date::null()), None);
        assert_eq!(series.iter().count(), 0);
    }

    #[test]
    fn dates_are_sorted_and_last_write_wins() {
        let early = Date::from_serial(45_000);
        let late = Date::from_serial(45_002);
        let mut series = TimeSeries::from_pairs([(late, 3), (early, 1), (late, 4)]).unwrap();
        assert_eq!(series.iter().collect::<Vec<_>>(), [(early, &1), (late, &4)]);
        assert_eq!(series.first_date(), Some(early));
        assert_eq!(series.last_date(), Some(late));
        assert_eq!(series.get(Date::from_serial(45_001)), None);
        assert_eq!(series.insert(early, 2).unwrap(), Some(1));
        assert_eq!(series.get(early), Some(&2));
    }

    #[test]
    fn null_date_is_rejected_without_mutation() {
        let mut series = TimeSeries::new();
        assert!(series.insert(Date::null(), 1).is_err());
        assert!(TimeSeries::from_pairs([(Date::null(), 1)]).is_err());
        assert!(series.is_empty());
    }
}
