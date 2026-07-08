//! Interpolated discount-factor structure.
//!
//! Port of `ql/termstructures/yield/discountcurve.hpp`:
//! [`InterpolatedDiscountCurve`] is a [`YieldTermStructure`] built from
//! (date, discount-factor) nodes interpolated in discount space on the
//! [`InterpolatedCurve`] holder, with flat-forward extrapolation past the
//! last node. [`DiscountCurve`] is the C++ typedef with log-linear
//! interpolation, which guarantees piecewise-constant forward rates.
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (`jumps`/`jumpDates`) are not ported, per the
//!   [`YieldTermStructure`] precedent.
//! - The protected detached/moving constructors exist only for bootstrapped
//!   curves and follow with the bootstrap; the public data constructors are
//!   ported, with the empty calendar an `Option` per the base convention.
//! - C++ defaults the interpolator argument; here [`new`](InterpolatedDiscountCurve::new)
//!   default-constructs the factory and
//!   [`with_interpolator`](InterpolatedDiscountCurve::with_interpolator)
//!   takes one explicitly.

use crate::errors::QlResult;
use crate::math::interpolations::loglinear::LogLinear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{DiscountFactor, Real, Time};
use crate::{fail, require};

/// Yield term structure based on interpolation of discount factors.
///
/// The first passed date is the reference date and must carry a discount
/// factor of 1.0.
pub struct InterpolatedDiscountCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

/// Term structure based on log-linear interpolation of discount factors
/// (C++'s `DiscountCurve` typedef). Log-linear interpolation guarantees
/// piecewise-constant forward rates.
pub type DiscountCurve = InterpolatedDiscountCurve<LogLinear>;

impl<I: Interpolator + Default> InterpolatedDiscountCurve<I> {
    /// Builds the curve with a default-constructed interpolator factory
    /// (C++'s defaulted `Interpolator()` argument).
    pub fn new(
        dates: Vec<Date>,
        discounts: Vec<DiscountFactor>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
    ) -> QlResult<InterpolatedDiscountCurve<I>> {
        Self::with_interpolator(dates, discounts, day_counter, calendar, I::default())
    }
}

impl<I: Interpolator> InterpolatedDiscountCurve<I> {
    /// Builds the curve from (date, discount) nodes (C++'s data constructors
    /// plus `initialize`): enough nodes for the interpolator, matching
    /// lengths, first discount 1.0, positive discounts, and dates strictly
    /// increasing without collapsing onto the same time.
    pub fn with_interpolator(
        dates: Vec<Date>,
        discounts: Vec<DiscountFactor>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<InterpolatedDiscountCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points(),
            "not enough input dates given"
        );
        require!(discounts.len() == dates.len(), "dates/data count mismatch");
        require!(
            discounts[0] == 1.0,
            "the first discount must be == 1.0 to flag the corresponding date as reference date"
        );
        for &discount in discounts.iter().skip(1) {
            if discount.is_nan() || discount <= 0.0 {
                fail!("negative discount");
            }
        }
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, dates[0], &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, discounts, interpolator);
        curve.setup_interpolation()?;
        Ok(InterpolatedDiscountCurve {
            base: TermStructureBase::with_reference_date(dates[0], calendar, Some(day_counter)),
            dates,
            curve,
        })
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values (the discount factors).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node discount factors.
    pub fn discounts(&self) -> &[DiscountFactor] {
        self.curve.data()
    }

    /// The (date, discount) nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }
}

impl<I: Interpolator> AsObservable for InterpolatedDiscountCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedDiscountCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        match self.curve.max_date() {
            Some(date) => date,
            None => *self.dates.last().expect("the constructor requires nodes"),
        }
    }
}

impl<I: Interpolator> YieldTermStructure for InterpolatedDiscountCurve<I> {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        let interpolation = self.curve.interpolation()?;
        let t_max = *self
            .curve
            .times()
            .last()
            .expect("the constructor requires nodes");
        if t <= t_max {
            return interpolation.value(t);
        }
        let d_max = *self
            .curve
            .data()
            .last()
            .expect("the constructor requires nodes");
        let inst_fwd_max = -interpolation.derivative(t_max)? / d_max;
        Ok(d_max * (-inst_fwd_max * (t - t_max)).exp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn expect_err<T>(result: QlResult<T>) -> crate::errors::QlError {
        match result {
            Ok(_) => panic!("expected an error"),
            Err(err) => err,
        }
    }

    fn sample_dates() -> Vec<Date> {
        let reference = reference();
        vec![reference, reference + 180, reference + 360, reference + 720]
    }

    fn sample_discounts() -> Vec<DiscountFactor> {
        vec![1.0, 0.97, 0.94, 0.88]
    }

    fn sample_curve() -> DiscountCurve {
        DiscountCurve::new(sample_dates(), sample_discounts(), Actual360::new(), None).unwrap()
    }

    #[test]
    fn nodes_are_reproduced() {
        let curve = sample_curve();
        assert_eq!(curve.reference_date().unwrap(), reference());
        assert_eq!(curve.max_date(), reference() + 720);
        assert_eq!(curve.times(), &[0.0, 0.5, 1.0, 2.0]);
        for (date, discount) in sample_dates().into_iter().zip(sample_discounts()) {
            let df = curve.discount_date(date, false).unwrap();
            assert!((df - discount).abs() < 1.0e-15);
        }
    }

    #[test]
    fn log_linear_interpolates_geometrically_between_nodes() {
        let curve = sample_curve();
        let df = curve.discount(0.75, false).unwrap();
        assert!((df - (0.97_f64 * 0.94).sqrt()).abs() < 1.0e-15);
    }

    #[test]
    fn forwards_are_piecewise_constant_under_log_linear() {
        let curve = sample_curve();
        let expected = (0.94_f64 / 0.88).ln();
        for (t1, t2) in [(1.0, 1.25), (1.25, 1.75), (1.9, 2.0)] {
            let forward = curve
                .forward_rate(t1, t2, Compounding::Continuous, Frequency::Annual, false)
                .unwrap();
            assert!((forward.rate() - expected).abs() < 1.0e-12);
        }
    }

    #[test]
    fn extrapolation_continues_the_last_forward_flat() {
        let curve = sample_curve();
        assert!(curve.discount(3.0, false).is_err());

        let last_forward = (0.94_f64 / 0.88).ln();
        let expected = 0.88 * (-last_forward * 1.0).exp();
        let df = curve.discount(3.0, true).unwrap();
        assert!((df - expected).abs() < 1.0e-14);

        curve.enable_extrapolation();
        let df = curve.discount(3.0, false).unwrap();
        assert!((df - expected).abs() < 1.0e-14);
        let forward = curve
            .forward_rate(3.0, 3.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - last_forward).abs() < 1.0e-9);
    }

    #[test]
    fn zero_rates_round_trip_the_discounts() {
        let curve = sample_curve();
        let zero = curve
            .zero_rate(1.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - -(0.94_f64.ln())).abs() < 1.0e-14);
    }

    #[test]
    fn generic_interpolator_drives_the_interpolation_space() {
        let curve = InterpolatedDiscountCurve::<Linear>::new(
            sample_dates(),
            sample_discounts(),
            Actual360::new(),
            None,
        )
        .unwrap();
        let df = curve.discount(0.75, false).unwrap();
        assert!((df - 0.955).abs() < 1.0e-15);
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = sample_curve();
        assert_eq!(curve.dates(), &sample_dates()[..]);
        assert_eq!(curve.data(), &sample_discounts()[..]);
        assert_eq!(curve.discounts(), &sample_discounts()[..]);
        let nodes = curve.nodes();
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0], (reference(), 1.0));
        assert_eq!(nodes[3], (reference() + 720, 0.88));
    }

    #[test]
    fn constructor_rejects_invalid_input() {
        let day_counter = Actual360::new();

        let err = expect_err(DiscountCurve::new(
            vec![reference()],
            vec![1.0],
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("not enough input dates"));

        let err = expect_err(DiscountCurve::new(
            sample_dates(),
            vec![1.0, 0.97],
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("dates/data count mismatch"));

        let err = expect_err(DiscountCurve::new(
            sample_dates(),
            vec![0.99, 0.97, 0.94, 0.88],
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("first discount must be == 1.0"));

        for bad in [0.0, -0.5, DiscountFactor::NAN] {
            let err = expect_err(DiscountCurve::new(
                sample_dates(),
                vec![1.0, 0.97, bad, 0.88],
                day_counter.clone(),
                None,
            ));
            assert!(err.message().contains("negative discount"));
        }

        let mut dates = sample_dates();
        dates.swap(1, 2);
        let err = expect_err(DiscountCurve::new(
            dates,
            sample_discounts(),
            day_counter,
            None,
        ));
        assert!(err.message().contains("dates not sorted"));
    }
}
