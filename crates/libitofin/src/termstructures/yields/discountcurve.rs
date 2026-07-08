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
