//! Interpolated simply compounded zero-rates structure.
//!
//! Port of `ql/termstructures/yield/interpolatedsimplezerocurve.hpp`:
//! [`InterpolatedSimpleZeroCurve`] is a [`YieldTermStructure`] built from
//! (date, zero-rate) nodes interpolated in rate space, where the rates are
//! read with `Simple` compounding, so the discount factor is `1/(1 + z*t)`
//! and not `exp(-z*t)`. Past the last node the last instantaneous forward
//! continues flat. This is the curve type the `SimpleZeroYield` bootstrap
//! traits name (`bootstraptraits.hpp:317`).
//!
//! ## Divergences from QuantLib
//!
//! - The conversion lives directly in `discount_impl`, the way
//!   [`InterpolatedDiscountCurve`](super::InterpolatedDiscountCurve) writes
//!   its own, rather than going through the
//!   [`ZeroYieldStructure`](super::ZeroYieldStructure) adapter
//!   [`InterpolatedZeroCurve`](super::InterpolatedZeroCurve) uses: that
//!   adapter's base converts a zero rate continuously, which is exactly the
//!   compounding this curve does not use.
//! - Jump quotes (`jumps`/`jumpDates`) are not ported, per the
//!   [`YieldTermStructure`] precedent.
//! - The protected detached / reference-date / settlement-days constructors
//!   (`interpolatedsimplezerocurve.hpp:65-74`) exist only for bootstrapped
//!   subclasses and are not ported, matching the sibling curves.
//! - C++ declares no typedef for the linearly interpolated case;
//!   [`SimpleZeroCurve`] is a port convenience mirroring `ZeroCurve`.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{DiscountFactor, Rate, Real, Time};

/// Yield term structure based on interpolation of simply compounded zero
/// rates.
///
/// The first passed date is the reference date.
pub struct InterpolatedSimpleZeroCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

/// Term structure based on linear interpolation of simply compounded zero
/// rates.
pub type SimpleZeroCurve = InterpolatedSimpleZeroCurve<Linear>;

impl<I: Interpolator + Default> InterpolatedSimpleZeroCurve<I> {
    /// Builds the curve with a default-constructed interpolator factory
    /// (C++'s defaulted `Interpolator()` argument).
    pub fn new(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
    ) -> QlResult<InterpolatedSimpleZeroCurve<I>> {
        Self::with_interpolator(dates, yields, day_counter, calendar, I::default())
    }
}

impl<I: Interpolator> InterpolatedSimpleZeroCurve<I> {
    /// Builds the curve from (date, zero-rate) nodes (C++'s data constructors
    /// plus `initialize`, `:181-189`): enough nodes for the interpolator,
    /// matching lengths, and dates strictly increasing without collapsing onto
    /// the same time. The rates themselves are unconstrained - a simply
    /// compounded zero rate may be negative.
    pub fn with_interpolator(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<InterpolatedSimpleZeroCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points(),
            "not enough input dates given"
        );
        require!(yields.len() == dates.len(), "dates/data count mismatch");
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, dates[0], &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, yields, interpolator);
        curve.setup_interpolation()?;
        Ok(InterpolatedSimpleZeroCurve {
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

    /// The node values (the simply compounded zero rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node zero rates.
    pub fn zero_rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The (date, zero-rate) nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }
}

impl<I: Interpolator> AsObservable for InterpolatedSimpleZeroCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedSimpleZeroCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        *self.dates.last().expect("the constructor requires nodes")
    }
}

impl<I: Interpolator> YieldTermStructure for InterpolatedSimpleZeroCurve<I> {
    /// The port of `discountImpl` (`:114-128`): the rate is interpolated in
    /// range and continues the last instantaneous forward flat past the last
    /// node, and the discount factor is the simply compounded `1/(1 + R*t)`.
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        let interpolation = self.curve.interpolation()?;
        let t_max = *self
            .curve
            .times()
            .last()
            .expect("the constructor requires nodes");
        let rate = if t <= t_max {
            interpolation.value(t)?
        } else {
            let z_max = *self
                .curve
                .data()
                .last()
                .expect("the constructor requires nodes");
            let inst_fwd_max = z_max + t_max * interpolation.derivative(t_max)?;
            (z_max * t_max + inst_fwd_max * (t - t_max)) / t
        };
        Ok(1.0 / (1.0 + rate * t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::QlError;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn expect_err<T>(result: QlResult<T>) -> QlError {
        match result {
            Ok(_) => panic!("expected an error"),
            Err(err) => err,
        }
    }

    fn sample_dates() -> Vec<Date> {
        let reference = reference();
        vec![reference, reference + 180, reference + 360, reference + 720]
    }

    fn sample_rates() -> Vec<Rate> {
        vec![0.04, 0.045, 0.05, 0.055]
    }

    fn sample_curve() -> SimpleZeroCurve {
        SimpleZeroCurve::new(sample_dates(), sample_rates(), Actual360::new(), None).unwrap()
    }

    /// The node conversion is `1/(1 + z*t)` (`:127`). The pin discriminates it
    /// from the continuous `exp(-z*t)` the sibling zero curve applies: at the
    /// 2Y node the two differ by about 5e-3, far outside the tolerance.
    #[test]
    fn nodes_discount_with_simple_compounding() {
        let curve = sample_curve();
        assert_eq!(curve.reference_date().unwrap(), reference());
        assert_eq!(curve.max_date(), reference() + 720);
        assert_eq!(curve.times(), &[0.0, 0.5, 1.0, 2.0]);
        for ((date, rate), t) in sample_dates()
            .into_iter()
            .zip(sample_rates())
            .zip([0.0, 0.5, 1.0, 2.0])
        {
            let discount = curve.discount_date(date, false).unwrap();
            let expected = 1.0 / (1.0 + rate * t);
            assert!(
                (discount - expected).abs() < 1.0e-15,
                "t {t}: discount {discount} vs 1/(1 + z*t) {expected}"
            );
        }
    }

    #[test]
    fn rates_interpolate_between_nodes() {
        let curve = sample_curve();
        let discount = curve.discount(0.75, false).unwrap();
        assert!((discount - 1.0 / (1.0 + 0.0475 * 0.75)).abs() < 1.0e-15);
    }

    /// Past the last node the last instantaneous forward continues flat
    /// (`:118-125`), and only then is the simple conversion applied.
    #[test]
    fn extrapolation_continues_the_last_forward_flat() {
        let curve = sample_curve();
        assert!(curve.discount(3.0, false).is_err());

        let inst_fwd_max = 0.055 + 2.0 * (0.055 - 0.05) / 1.0;
        let rate = (0.055 * 2.0 + inst_fwd_max * 1.0) / 3.0;
        let expected = 1.0 / (1.0 + rate * 3.0);
        let discount = curve.discount(3.0, true).unwrap();
        assert!((discount - expected).abs() < 1.0e-14);
    }

    /// The C++ `initialize` (`:181-189`) checks the node count and the date
    /// ordering only; a simply compounded zero rate may be negative, so no
    /// sign check is added.
    #[test]
    fn constructor_rejects_invalid_input() {
        let day_counter = Actual360::new();

        let err = expect_err(SimpleZeroCurve::new(
            vec![reference()],
            vec![0.04],
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("not enough input dates"));

        let err = expect_err(SimpleZeroCurve::new(
            sample_dates(),
            vec![0.04, 0.045],
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("dates/data count mismatch"));

        let mut dates = sample_dates();
        dates.swap(1, 2);
        let err = expect_err(SimpleZeroCurve::new(
            dates,
            sample_rates(),
            day_counter.clone(),
            None,
        ));
        assert!(err.message().contains("dates not sorted"));

        let negative = SimpleZeroCurve::new(
            sample_dates(),
            vec![-0.01, -0.005, 0.0, 0.01],
            day_counter,
            None,
        )
        .unwrap();
        assert!((negative.discount(1.0, false).unwrap() - 1.0 / (1.0 + 0.0)).abs() < 1.0e-15);
        assert_eq!(negative.zero_rates()[0], -0.01);
        assert_eq!(negative.nodes()[3], (reference() + 720, 0.01));
        assert_eq!(negative.dates(), &sample_dates()[..]);
        assert_eq!(negative.data()[1], -0.005);
    }
}
