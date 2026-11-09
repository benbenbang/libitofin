//! Interpolated zero-rates structure.
//!
//! Port of `ql/termstructures/yield/zerocurve.hpp`:
//! [`InterpolatedZeroCurve`] builds a [`YieldTermStructure`] from
//! (date, zero-rate) nodes interpolated in zero-rate space, wiring the
//! [`ZeroYieldStructure`] adapter over an embedded
//! [`InterpolatedCurve`]; [`ZeroCurve`] is the C++ `typedef` for the
//! linearly interpolated case.
//!
//! ## Divergences from QuantLib
//!
//! - The protected detached/moving constructors (used only by bootstrapped
//!   subclasses) follow with the bootstrap work; the public date-vector
//!   constructors are ported.
//! - C++ reads `dates.at(0)` before checking sizes, throwing `out_of_range`
//!   on an empty vector; here every input problem is a `QlError` per D4.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::{Compounding, InterestRate};
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::Quote;
use crate::require;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yields::ZeroYieldStructure;
use crate::termstructures::yieldtermstructure::{YieldJumps, YieldTermStructure};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Rate, Real, Time};

/// Yield term structure based on interpolation of zero rates.
///
/// The first date is the reference date; rates are stored as annual
/// continuous compounding, converting on construction when quoted otherwise.
pub struct InterpolatedZeroCurve<I: Interpolator> {
    base: TermStructureBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
    jumps: Option<YieldJumps>,
}

/// Term structure based on linear interpolation of zero yields (C++'s
/// `ZeroCurve` typedef).
pub type ZeroCurve = InterpolatedZeroCurve<Linear>;

impl<I: Interpolator> InterpolatedZeroCurve<I> {
    /// Curve through continuously compounded zero rates at the given dates;
    /// the first date is the reference date.
    pub fn new(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        Self::with_compounding(
            dates,
            yields,
            day_counter,
            None,
            interpolator,
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// Builds the curve as [`new`](Self::new) with a calendar and
    /// multiplicative discount jumps (C++'s `jumps`/`jumpDates` constructor
    /// arguments).
    ///
    /// Empty `jump_dates` generates consecutive year-end dates from the
    /// reference year. Jump values are read and validated only after their
    /// dates have been crossed.
    ///
    /// # Errors
    /// Returns the [`new`](Self::new) errors, or a [`YieldJumps`]
    /// construction error.
    pub fn with_jumps(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
        jumps: Vec<Handle<dyn Quote>>,
        jump_dates: Vec<Date>,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        Self::with_jumps_and_compounding(
            dates,
            yields,
            day_counter,
            calendar,
            interpolator,
            jumps,
            jump_dates,
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// Builds the curve as [`with_compounding`](Self::with_compounding) with
    /// multiplicative discount jumps as in [`with_jumps`](Self::with_jumps)
    /// (C++'s full constructor taking both `jumps`/`jumpDates` and a
    /// compounding convention).
    ///
    /// # Errors
    /// Returns the [`with_compounding`](Self::with_compounding) errors, or a
    /// [`YieldJumps`] construction error.
    #[allow(clippy::too_many_arguments)]
    pub fn with_jumps_and_compounding(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
        jumps: Vec<Handle<dyn Quote>>,
        jump_dates: Vec<Date>,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        let mut curve = Self::with_compounding(
            dates,
            yields,
            day_counter,
            calendar,
            interpolator,
            compounding,
            frequency,
        )?;
        curve.jumps = Some(YieldJumps::new(&curve.base, jumps, jump_dates)?);
        Ok(curve)
    }

    /// Curve through zero rates quoted with an explicit compounding
    /// convention, converted to annual continuous compounding on
    /// construction (C++'s `initialize`).
    pub fn with_compounding(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points(),
            "not enough input dates given"
        );
        require!(yields.len() == dates.len(), "dates/data count mismatch");
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, dates[0], &day_counter)?;

        let mut data = yields;
        if compounding != Compounding::Continuous {
            let dt = 1.0 / 365.0;
            let r = InterestRate::new(data[0], day_counter.clone(), compounding, frequency)?;
            data[0] = r
                .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, dt)?
                .rate();
            for i in 1..dates.len() {
                let r = InterestRate::new(data[i], day_counter.clone(), compounding, frequency)?;
                data[i] = r
                    .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, times[i])?
                    .rate();
            }
        }

        let mut curve = InterpolatedCurve::new(times, data, interpolator);
        curve.setup_interpolation()?;
        let base = TermStructureBase::with_reference_date(dates[0], calendar, Some(day_counter));
        Ok(InterpolatedZeroCurve {
            base,
            curve,
            dates,
            jumps: None,
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

    /// The node values (continuously compounded zero rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node values (continuously compounded zero rates).
    pub fn zero_rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The curve nodes as (date, zero-rate) pairs.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }

    fn last_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("construction rejected empty dates")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedZeroCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedZeroCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| self.last_date())
    }
}

impl<I: Interpolator> ZeroYieldStructure for InterpolatedZeroCurve<I> {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        let interpolation = self.curve.interpolation()?;
        let t_max = *self
            .curve
            .times()
            .last()
            .expect("construction rejected empty dates");
        if t <= t_max {
            return interpolation.value(t);
        }

        let z_max = *self
            .curve
            .data()
            .last()
            .expect("construction rejected empty dates");
        let inst_fwd_max = z_max + t_max * interpolation.derivative(t_max)?;
        Ok((z_max * t_max + inst_fwd_max * (t - t_max)) / t)
    }
}

impl<I: Interpolator> YieldTermStructure for InterpolatedZeroCurve<I> {
    fn jumps(&self) -> Option<&YieldJumps> {
        self.jumps.as_ref()
    }

    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.discount_from_zero_yield(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn sample_dates() -> Vec<Date> {
        vec![
            reference(),
            reference() + 180,
            reference() + 360,
            reference() + 720,
        ]
    }

    fn sample_zeros() -> Vec<Rate> {
        vec![0.02, 0.03, 0.04, 0.045]
    }

    fn sample() -> ZeroCurve {
        ZeroCurve::new(sample_dates(), sample_zeros(), Actual360::new(), Linear).unwrap()
    }

    #[test]
    fn zero_rates_round_trip_through_the_discount() {
        let curve = sample();
        for (t, z) in [(0.5, 0.03), (1.0, 0.04), (2.0, 0.045)] {
            let zero = curve
                .zero_rate(t, Compounding::Continuous, Frequency::Annual, false)
                .unwrap();
            assert!((zero.rate() - z).abs() < 1.0e-14);
            let df = curve.discount(t, false).unwrap();
            assert!((df - (-z * t).exp()).abs() < 1.0e-15);
        }
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);

        let short_end = curve
            .zero_rate(0.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((short_end.rate() - 0.02).abs() < 1.0e-5);
    }

    #[test]
    fn zero_rates_interpolate_linearly_between_nodes() {
        let curve = sample();
        let zero = curve
            .zero_rate(0.75, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.035).abs() < 1.0e-14);
        let df = curve.discount(1.5, false).unwrap();
        assert!((df - (-0.0425_f64 * 1.5).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn extrapolation_extends_the_last_instantaneous_forward_flat() {
        let curve = sample();
        assert!(curve.discount(3.0, false).is_err());

        curve.enable_extrapolation();
        let z_max = 0.045;
        let t_max = 2.0;
        let inst_fwd_max = z_max + t_max * 0.005;
        let expected = (z_max * t_max + inst_fwd_max * (3.0 - t_max)) / 3.0;
        let zero = curve
            .zero_rate(3.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - expected).abs() < 1.0e-14);

        let forward = curve
            .forward_rate(2.5, 2.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - inst_fwd_max).abs() < 1.0e-12);
    }

    #[test]
    fn compounded_quotes_are_converted_to_continuous_on_construction() {
        let curve = ZeroCurve::with_compounding(
            sample_dates(),
            vec![0.05; 4],
            Actual360::new(),
            None,
            Linear,
            Compounding::Compounded,
            Frequency::Annual,
        )
        .unwrap();

        let continuous = 1.05_f64.ln();
        for &z in curve.zero_rates() {
            assert!((z - continuous).abs() < 1.0e-13);
        }
        let df = curve.discount(2.0, false).unwrap();
        assert!((df - 1.05_f64.powf(-2.0)).abs() < 1.0e-13);
        let zero = curve
            .zero_rate(1.0, Compounding::Compounded, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-14);
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = sample();
        assert_eq!(curve.times(), &[0.0, 0.5, 1.0, 2.0]);
        assert_eq!(curve.dates(), &sample_dates()[..]);
        assert_eq!(curve.data(), &sample_zeros()[..]);
        assert_eq!(curve.zero_rates(), curve.data());
        assert_eq!(
            curve.nodes(),
            sample_dates()
                .into_iter()
                .zip(sample_zeros())
                .collect::<Vec<_>>()
        );
        assert_eq!(curve.max_date(), reference() + 720);
        assert_eq!(curve.reference_date().unwrap(), reference());
    }

    fn build_err(dates: Vec<Date>, zeros: Vec<Rate>) -> String {
        match ZeroCurve::new(dates, zeros, Actual360::new(), Linear) {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err.message().to_string(),
        }
    }

    #[test]
    fn constructor_rejects_bad_inputs() {
        let err = build_err(vec![reference()], vec![0.02]);
        assert!(err.contains("not enough input dates"));

        let err = build_err(sample_dates(), vec![0.02]);
        assert!(err.contains("dates/data count mismatch"));

        let mut dates = sample_dates();
        dates.swap(1, 2);
        let err = build_err(dates, sample_zeros());
        assert!(err.contains("dates not sorted"));
    }

    #[test]
    fn cubic_factory_backs_a_standalone_zero_curve() {
        // Smoke test for the Cubic interpolator factory on a standalone curve:
        // it constructs, reproduces its node zero rates (the interpolant passes
        // through the nodes), and evaluates finite between them.
        use crate::math::interpolations::cubic::Cubic;

        let curve =
            InterpolatedZeroCurve::new(sample_dates(), sample_zeros(), Actual360::new(), Cubic)
                .unwrap();
        for (t, z) in [(0.5, 0.03), (1.0, 0.04), (2.0, 0.045)] {
            let zero = curve
                .zero_rate(t, Compounding::Continuous, Frequency::Annual, false)
                .unwrap();
            assert!((zero.rate() - z).abs() < 1.0e-12);
        }
        let mid = curve
            .zero_rate(0.75, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!(mid.rate().is_finite());
    }

    fn quote(value: Real) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    fn jumped_curve(jumps: Vec<Handle<dyn Quote>>, jump_dates: Vec<Date>) -> QlResult<ZeroCurve> {
        ZeroCurve::with_jumps(
            sample_dates(),
            sample_zeros(),
            Actual360::new(),
            None,
            Linear,
            jumps,
            jump_dates,
        )
    }

    #[test]
    fn discount_is_the_base_discount_times_every_jump_before_t() {
        let base = sample();
        let first = reference() + 90;
        let second = reference() + 450;
        let curve = jumped_curve(vec![quote(0.99), quote(0.98)], vec![first, second]).unwrap();
        assert!(base.jumps().is_none());
        assert_eq!(curve.jump_dates(), [first, second]);
        assert_eq!(curve.jump_times().unwrap(), vec![0.25, 1.25]);
        for (t, factor) in [
            (0.1, 1.0),
            (0.25, 1.0),
            (0.75, 0.99),
            (1.25, 0.99),
            (1.5, 0.99 * 0.98),
            (2.0, 0.99 * 0.98),
        ] {
            let expected = factor * base.discount(t, false).unwrap();
            assert!((curve.discount(t, false).unwrap() - expected).abs() < 1.0e-15);
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
        let zero = |c: &ZeroCurve| {
            c.zero_rate(1.5, Compounding::Continuous, Frequency::Annual, false)
                .unwrap()
                .rate()
        };
        let expected = zero(&base) - (0.99_f64 * 0.98).ln() / 1.5;
        assert!((zero(&curve) - expected).abs() < 1.0e-14);
    }

    #[test]
    fn out_of_order_jump_dates_are_all_applied() {
        let base = sample();
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
        let expected = 0.99 * sample().discount_date(year_end + 1, false).unwrap();
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
        let expected = 0.95 * sample().discount(1.0, false).unwrap();
        assert!((curve.discount(1.0, false).unwrap() - expected).abs() < 1.0e-15);
    }

    fn simple_oracle_dates() -> (Date, Vec<Date>, Vec<Date>) {
        let r = reference();
        let nodes = [0, 1, 2, 5]
            .map(|n| r + Period::new(n, TimeUnit::Years))
            .to_vec();
        let jumps = [6, 18]
            .map(|n| r + Period::new(n, TimeUnit::Months))
            .to_vec();
        (r, nodes, jumps)
    }

    fn simple_oracle_curve(compounding: Compounding, frequency: Frequency) -> ZeroCurve {
        let (_, nodes, jump_dates) = simple_oracle_dates();
        ZeroCurve::with_jumps_and_compounding(
            nodes,
            vec![0.02, 0.025, 0.03, 0.035],
            Actual365Fixed::new(),
            None,
            Linear,
            vec![quote(0.99), quote(0.98)],
            jump_dates,
            compounding,
            frequency,
        )
        .unwrap()
    }

    #[test]
    fn simple_compounded_quotes_combine_with_jumps() {
        let (r, nodes, jump_dates) = simple_oracle_dates();
        let dc = Actual365Fixed::new();
        let curve = simple_oracle_curve(Compounding::Simple, Frequency::Annual);
        let rates: [Rate; 4] = [0.02, 0.025, 0.03, 0.035];
        let times: Vec<Time> = nodes.iter().map(|&d| dc.year_fraction(r, d)).collect();
        let zeros: Vec<Rate> = (0..4)
            .map(|i| {
                if i == 0 {
                    365.0 * (1.0 + rates[0] / 365.0).ln()
                } else {
                    (1.0 + rates[i] * times[i]).ln() / times[i]
                }
            })
            .collect();
        let jump_times: Vec<Time> = jump_dates.iter().map(|&d| dc.year_fraction(r, d)).collect();
        let jump_values = [0.99, 0.98];
        let expected = |t: Time| {
            let i = (1..3).find(|&i| t <= times[i]).unwrap_or(3) - 1;
            let z =
                zeros[i] + (t - times[i]) * (zeros[i + 1] - zeros[i]) / (times[i + 1] - times[i]);
            let factor: Real = jump_times
                .iter()
                .zip(jump_values)
                .filter(|&(&tk, _)| 0.0 < tk && tk < t)
                .map(|(_, j)| j)
                .product();
            (-z * t).exp() * factor
        };
        let rel = |got: Real, exp: Real| (got - exp).abs() / exp.abs();

        let unjumped = ZeroCurve::with_compounding(
            nodes,
            rates.to_vec(),
            dc,
            None,
            Linear,
            Compounding::Simple,
            Frequency::Annual,
        )
        .unwrap();
        assert_eq!(curve.jump_times().unwrap(), jump_times);
        for t in [0.25, 1.0, 1.25, 3.0, jump_times[0], jump_times[1]] {
            let got = curve.discount(t, false).unwrap();
            assert!(rel(got, expected(t)) < 1.0e-14, "t = {t}");
            let product: Real = jump_times
                .iter()
                .zip(jump_values)
                .filter(|&(&tk, _)| tk < t)
                .map(|(_, j)| j)
                .product();
            let composed = unjumped.discount(t, false).unwrap() * product;
            assert!(rel(got, composed) < 1.0e-14, "t = {t}");
        }
    }

    #[test]
    fn semiannual_compounded_quotes_combine_with_jumps() {
        let (r, nodes, jump_dates) = simple_oracle_dates();
        let dc = Actual365Fixed::new();
        let curve = simple_oracle_curve(Compounding::Compounded, Frequency::Semiannual);
        let annual = simple_oracle_curve(Compounding::Compounded, Frequency::Annual);
        let rates: [Rate; 4] = [0.02, 0.025, 0.03, 0.035];
        let times: Vec<Time> = nodes.iter().map(|&d| dc.year_fraction(r, d)).collect();
        let zeros: Vec<Rate> = rates.iter().map(|&q| 2.0 * (1.0 + q / 2.0).ln()).collect();
        let jump_times: Vec<Time> = jump_dates.iter().map(|&d| dc.year_fraction(r, d)).collect();
        let expected = |t: Time| {
            let i = (1..3).find(|&i| t <= times[i]).unwrap_or(3) - 1;
            let z =
                zeros[i] + (t - times[i]) * (zeros[i + 1] - zeros[i]) / (times[i + 1] - times[i]);
            let factor: Real = jump_times
                .iter()
                .zip([0.99, 0.98])
                .filter(|&(&tk, _)| 0.0 < tk && tk < t)
                .map(|(_, j)| j)
                .product();
            (-z * t).exp() * factor
        };
        for t in [0.25, 1.0, 1.25, 3.0, jump_times[0], jump_times[1]] {
            let got = curve.discount(t, false).unwrap();
            let exp = expected(t);
            assert!((got - exp).abs() / exp < 1.0e-14, "t = {t}");
            let other = annual.discount(t, false).unwrap();
            assert!((got - other).abs() / exp > 1.0e-6, "t = {t}");
        }
    }

    #[test]
    fn continuous_annual_matches_with_jumps_bitwise() {
        let (_, nodes, jump_dates) = simple_oracle_dates();
        let combined = simple_oracle_curve(Compounding::Continuous, Frequency::Annual);
        let jumped = ZeroCurve::with_jumps(
            nodes,
            vec![0.02, 0.025, 0.03, 0.035],
            Actual365Fixed::new(),
            None,
            Linear,
            vec![quote(0.99), quote(0.98)],
            jump_dates,
        )
        .unwrap();
        assert_eq!(combined.zero_rates(), jumped.zero_rates());
        for t in [0.25, 0.5, 1.0, 1.25, 1.5, 3.0] {
            assert_eq!(
                combined.discount(t, false).unwrap().to_bits(),
                jumped.discount(t, false).unwrap().to_bits()
            );
        }
    }
}
