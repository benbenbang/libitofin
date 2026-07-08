//! Helper to build interpolated term structures.
//!
//! Port of `ql/termstructures/interpolatedcurve.hpp`: concrete interpolated
//! curves embed an [`InterpolatedCurve`] holding the node times and values,
//! the [`Interpolator`] factory, and the interpolation rebuilt from them.
//!
//! ## Divergences from QuantLib
//!
//! - C++ leaves the interpolation empty until `setupInterpolation()` and
//!   crashes if evaluated before; here
//!   [`interpolation`](InterpolatedCurve::interpolation) returns an `Err`
//!   until [`setup_interpolation`](InterpolatedCurve::setup_interpolation)
//!   builds it.
//! - `setupTimes` writes into the member vector of a half-built curve; the
//!   associated function
//!   [`times_from_dates`](InterpolatedCurve::times_from_dates) returns the
//!   times instead, so the holder is only ever constructed whole.
//! - The `maxDate_` slot uses `Option` where C++ uses the null date.

use crate::errors::QlResult;
use crate::math::comparison::close;
use crate::math::interpolations::Interpolator;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Time};
use crate::{fail, require};

/// Shared data of an interpolated term structure: the node times and values,
/// the factory, the interpolation built from them, and a slot for curves
/// whose maximum date lies past the last node (e.g. after bootstrapping an
/// instrument maturing beyond its pillar).
pub struct InterpolatedCurve<I: Interpolator> {
    times: Vec<Time>,
    data: Vec<Real>,
    interpolator: I,
    interpolation: Option<I::Output>,
    max_date: Option<Date>,
}

impl<I: Interpolator> InterpolatedCurve<I> {
    /// Stores the node times and values; the interpolation is built by
    /// [`setup_interpolation`](Self::setup_interpolation).
    pub fn new(times: Vec<Time>, data: Vec<Real>, interpolator: I) -> InterpolatedCurve<I> {
        InterpolatedCurve {
            times,
            data,
            interpolator,
            interpolation: None,
            max_date: None,
        }
    }

    /// Converts curve dates into node times (C++'s `setupTimes`): dates must
    /// be strictly increasing and must not collapse onto the same time under
    /// the curve's day count convention.
    pub fn times_from_dates(
        dates: &[Date],
        reference_date: Date,
        day_counter: &DayCounter,
    ) -> QlResult<Vec<Time>> {
        require!(!dates.is_empty(), "no dates given");
        let mut times = Vec::with_capacity(dates.len());
        times.push(day_counter.year_fraction(reference_date, dates[0]));
        for i in 1..dates.len() {
            require!(
                dates[i] > dates[i - 1],
                "dates not sorted: {} passed after {}",
                dates[i],
                dates[i - 1]
            );
            let t = day_counter.year_fraction(reference_date, dates[i]);
            require!(
                !close(t, times[i - 1]),
                "two passed dates ({} and {}) correspond to the same time under this curve's day count convention ({day_counter})",
                dates[i - 1],
                dates[i]
            );
            times.push(t);
        }
        Ok(times)
    }

    /// Rebuilds the interpolation from the current times and values (C++'s
    /// `setupInterpolation`).
    pub fn setup_interpolation(&mut self) -> QlResult<()> {
        self.interpolation = Some(self.interpolator.interpolate(&self.times, &self.data)?);
        Ok(())
    }

    /// The interpolation over the node times and values.
    pub fn interpolation(&self) -> QlResult<&I::Output> {
        match &self.interpolation {
            Some(interpolation) => Ok(interpolation),
            None => fail!("interpolation not set up: call setup_interpolation first"),
        }
    }

    /// The interpolator factory.
    pub fn interpolator(&self) -> &I {
        &self.interpolator
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        &self.times
    }

    /// The node values.
    pub fn data(&self) -> &[Real] {
        &self.data
    }

    /// The stored maximum date, when it extends past the last node.
    pub fn max_date(&self) -> Option<Date> {
        self.max_date
    }

    /// Stores a maximum date past the last node.
    pub fn set_max_date(&mut self, date: Date) {
        self.max_date = Some(date);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::Interpolation;
    use crate::math::interpolations::linear::Linear;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};

    fn sample() -> InterpolatedCurve<Linear> {
        InterpolatedCurve::new(vec![0.0, 1.0, 3.0], vec![0.02, 0.04, 0.04], Linear)
    }

    #[test]
    fn interpolation_requires_setup() {
        let mut curve = sample();
        let Err(err) = curve.interpolation() else {
            panic!("expected an error before setup")
        };
        assert!(err.message().contains("not set up"));

        curve.setup_interpolation().unwrap();
        let f = curve.interpolation().unwrap();
        assert_eq!(f.value(0.0).unwrap(), 0.02);
        assert_eq!(f.value(0.5).unwrap(), 0.03);
        assert_eq!(f.value(2.0).unwrap(), 0.04);
    }

    #[test]
    fn setup_surfaces_interpolation_errors() {
        let mut curve = InterpolatedCurve::new(vec![0.0], vec![0.02], Linear);
        assert!(curve.setup_interpolation().is_err());
        assert!(curve.interpolation().is_err());
    }

    #[test]
    fn accessors_expose_the_nodes() {
        let curve = sample();
        assert_eq!(curve.times(), &[0.0, 1.0, 3.0]);
        assert_eq!(curve.data(), &[0.02, 0.04, 0.04]);
        assert_eq!(curve.interpolator().required_points(), 2);
    }

    #[test]
    fn times_from_dates_uses_the_day_counter() {
        let reference = Date::new(15, Month::June, 2026);
        let dates = [reference, reference + 180, reference + 360];
        let times =
            InterpolatedCurve::<Linear>::times_from_dates(&dates, reference, &Actual360::new())
                .unwrap();
        assert_eq!(times, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn times_from_dates_rejects_unsorted_and_empty_dates() {
        let reference = Date::new(15, Month::June, 2026);
        let err = InterpolatedCurve::<Linear>::times_from_dates(
            &[reference + 180, reference + 90],
            reference,
            &Actual360::new(),
        )
        .unwrap_err();
        assert!(err.message().contains("dates not sorted"));

        assert!(
            InterpolatedCurve::<Linear>::times_from_dates(&[], reference, &Actual360::new())
                .is_err()
        );
    }

    #[test]
    fn times_from_dates_rejects_dates_collapsing_onto_the_same_time() {
        // Under 30/360 with a reference on the 30th, July 30th and 31st both
        // count 30 days, so two distinct dates map to the same time.
        let reference = Date::new(30, Month::June, 2026);
        let dates = [
            Date::new(30, Month::July, 2026),
            Date::new(31, Month::July, 2026),
        ];
        let err = InterpolatedCurve::<Linear>::times_from_dates(
            &dates,
            reference,
            &Thirty360::with_convention(Convention::BondBasis),
        )
        .unwrap_err();
        assert!(err.message().contains("correspond to the same time"));
    }

    #[test]
    fn max_date_slot_starts_empty() {
        let mut curve = sample();
        assert!(curve.max_date().is_none());
        let date = Date::new(15, Month::June, 2027);
        curve.set_max_date(date);
        assert_eq!(curve.max_date(), Some(date));
    }
}
