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
//! - Jump quotes (`jumps`/`jumpDates`) are taken by
//!   [`with_jumps`](InterpolatedSimpleZeroCurve::with_jumps); the C++
//!   defaulted arguments otherwise collapse to
//!   [`new`](InterpolatedSimpleZeroCurve::new) and
//!   [`with_interpolator`](InterpolatedSimpleZeroCurve::with_interpolator).
//! - The protected detached / reference-date / settlement-days constructors
//!   (`interpolatedsimplezerocurve.hpp:65-74`) exist only for bootstrapped
//!   subclasses and are not ported, matching the sibling curves.
//! - C++ declares no typedef for the linearly interpolated case;
//!   [`SimpleZeroCurve`] is a port convenience mirroring `ZeroCurve`.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::Quote;
use crate::require;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yieldtermstructure::{YieldJumps, YieldTermStructure};
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
    jumps: Option<YieldJumps>,
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
            jumps: None,
        })
    }

    /// Builds the curve as [`with_interpolator`](Self::with_interpolator) with
    /// multiplicative discount jumps (C++'s `jumps`/`jumpDates` constructor
    /// arguments, `:44`).
    ///
    /// Empty `jump_dates` generates consecutive year-end dates from the
    /// reference year. Jump values are read and validated only after their
    /// dates have been crossed.
    ///
    /// # Errors
    /// Returns the [`with_interpolator`](Self::with_interpolator) errors, or a
    /// [`YieldJumps`] construction error.
    pub fn with_jumps(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
        jumps: Vec<Handle<dyn Quote>>,
        jump_dates: Vec<Date>,
    ) -> QlResult<InterpolatedSimpleZeroCurve<I>> {
        let mut curve =
            Self::with_interpolator(dates, yields, day_counter, calendar, interpolator)?;
        curve.jumps = Some(YieldJumps::new(&curve.base, jumps, jump_dates)?);
        Ok(curve)
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
    fn jumps(&self) -> Option<&YieldJumps> {
        self.jumps.as_ref()
    }

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
    use crate::interestrate::Compounding;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

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

    fn quote(value: Real) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    fn jumped_curve(
        jumps: Vec<Handle<dyn Quote>>,
        jump_dates: Vec<Date>,
    ) -> QlResult<SimpleZeroCurve> {
        SimpleZeroCurve::with_jumps(
            sample_dates(),
            sample_rates(),
            Actual360::new(),
            None,
            Linear,
            jumps,
            jump_dates,
        )
    }

    /// The analytic oracle `base(t) * prod(jumps with 0 < t_i < t)`
    /// (`yieldtermstructure.cpp:83-95`), with `base(t)` written out as the
    /// simply compounded `1/(1 + R*t)` on the linearly interpolated rate.
    #[test]
    fn discount_is_the_base_discount_times_every_jump_before_t() {
        let base = sample_curve();
        let first = reference() + 90;
        let second = reference() + 450;
        let curve = jumped_curve(vec![quote(0.99), quote(0.98)], vec![first, second]).unwrap();
        assert!(base.jumps().is_none());
        assert_eq!(curve.jump_dates(), [first, second]);
        assert_eq!(curve.jump_times().unwrap(), vec![0.25, 1.25]);
        for (t, rate, factor) in [
            (0.1, 0.041, 1.0),
            (0.25, 0.0425, 1.0),
            (0.75, 0.0475, 0.99),
            (1.25, 0.05125, 0.99),
            (1.5, 0.0525, 0.99 * 0.98),
            (2.0, 0.055, 0.99 * 0.98),
        ] {
            let expected = factor / (1.0 + rate * t);
            let discount = curve.discount(t, false).unwrap();
            assert!(
                (discount - expected).abs() < 1.0e-15,
                "t {t}: discount {discount} vs analytic {expected}"
            );
        }
        for (date, before, jump) in [(first, 1.0, 0.99), (second, 0.99, 0.98)] {
            let at = curve.discount_date(date, false).unwrap();
            let expected = before * base.discount_date(date, false).unwrap();
            assert!((at - expected).abs() < 1.0e-15);
            let after = curve.discount_date(date + 1, false).unwrap();
            let expected = before * jump * base.discount_date(date + 1, false).unwrap();
            assert!((after - expected).abs() < 1.0e-15);
        }
        let expected = 0.99 * 0.98 * base.discount(3.0, true).unwrap();
        assert!((curve.discount(3.0, true).unwrap() - expected).abs() < 1.0e-15);
    }

    /// The zero rate is implied from the jumped discount, so both conventions
    /// include the jumps: the simple rate is not the node-interpolated 0.0525.
    #[test]
    fn zero_rate_includes_the_jumps() {
        let curve = jumped_curve(
            vec![quote(0.99), quote(0.98)],
            vec![reference() + 90, reference() + 450],
        )
        .unwrap();
        let t = 1.5;
        let compound = (1.0 + 0.0525 * t) / (0.99 * 0.98);
        for (comp, expected) in [
            (Compounding::Simple, (compound - 1.0) / t),
            (Compounding::Continuous, f64::ln(compound) / t),
        ] {
            let rate = curve
                .zero_rate(t, comp, Frequency::Annual, false)
                .unwrap()
                .rate();
            assert!(
                (rate - expected).abs() < 1.0e-14,
                "zero rate {rate} vs analytic {expected}"
            );
        }
    }

    #[test]
    fn out_of_order_jump_dates_are_all_applied() {
        let base = sample_curve();
        let curve = jumped_curve(
            vec![quote(0.97), quote(0.99)],
            vec![reference() + 450, reference() + 90],
        )
        .unwrap();
        let expected = 0.99 * base.discount(1.0, false).unwrap();
        assert!((curve.discount(1.0, false).unwrap() - expected).abs() < 1.0e-15);
        let expected = 0.99 * 0.97 * base.discount(1.5, false).unwrap();
        assert!((curve.discount(1.5, false).unwrap() - expected).abs() < 1.0e-15);
    }

    #[test]
    fn empty_jump_dates_generate_year_ends() {
        let curve = jumped_curve(vec![quote(0.99), quote(1.01)], vec![]).unwrap();
        let year_end = Date::new(31, Month::December, 2026);
        assert_eq!(
            curve.jump_dates(),
            [year_end, Date::new(31, Month::December, 2027)]
        );
        let after = curve.discount_date(year_end + 1, false).unwrap();
        let expected = 0.99 * sample_curve().discount_date(year_end + 1, false).unwrap();
        assert!((after - expected).abs() < 1.0e-15);
    }

    #[test]
    fn invalid_jumps_are_rejected() {
        let date = reference() + 90;
        assert!(jumped_curve(vec![quote(0.99)], vec![date, date + 1]).is_err());
        let curve = jumped_curve(vec![quote(0.0)], vec![date]).unwrap();
        assert!(curve.discount(0.2, false).is_ok());
        assert!(curve.discount(0.3, false).is_err());
        let curve = jumped_curve(vec![Handle::empty()], vec![date]).unwrap();
        assert!(curve.discount(0.3, false).is_err());
    }

    #[test]
    fn jump_quote_change_notifies_and_reprices() {
        let jump = shared(SimpleQuote::new(0.99));
        let curve = jumped_curve(
            vec![Handle::new(jump.clone() as Shared<dyn Quote>)],
            vec![reference() + 90],
        )
        .unwrap();
        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));
        jump.set_value(0.95);
        assert!(Flag::is_up(&flag));
        let expected = 0.95 * sample_curve().discount(1.0, false).unwrap();
        assert!((curve.discount(1.0, false).unwrap() - expected).abs() < 1.0e-15);
    }
}
