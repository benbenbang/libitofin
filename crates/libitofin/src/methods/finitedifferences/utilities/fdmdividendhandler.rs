//! The cash-dividend jump of a finite-difference rollback.
//!
//! Port of `ql/methods/finitedifferences/utilities/fdmdividendhandler.hpp:37`
//! and its `.cpp`.

use crate::cashflows::Dividend;
use crate::errors::QlResult;
use crate::math::array::Array;
use crate::methods::finitedifferences::StepCondition;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::require;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Size, Time};

/// Shifts the rolled-back grid across a cash dividend along one equity
/// direction.
///
/// At a dividend time the value of holding the equity just before the payment
/// is the value just after it at `S - D`, so every grid point reads the grid at
/// `max(x[0], x[k] - D)` in physical (not log) units, linearly interpolated
/// and extrapolated (`cpp:68-106`). The clamp at the lowest grid point keeps
/// the read inside the mesher rather than below a zero spot.
pub struct FdmDividendHandler {
    x: Vec<Real>,
    dividend_times: Vec<Time>,
    dividend_dates: Vec<Date>,
    dividends: Vec<Real>,
    mesher: Shared<dyn FdmMesher>,
    equity_direction: Size,
}

impl FdmDividendHandler {
    /// The handler for `schedule` over `mesher`, whose `equity_direction`
    /// holds `ln(S)` (`cpp:30-54`).
    ///
    /// The dividend times are `day_counter.year_fraction(reference_date,
    /// date)`, which must be the clock the solver steps on: `apply_to` matches
    /// them by exact equality.
    ///
    /// # Errors
    ///
    /// Propagates the failure of a dividend whose amount cannot be computed.
    /// Fails on an equity axis of fewer than two points, which C++ reports
    /// from `apply_to` the first time a dividend is paid
    /// (`ql/math/interpolation.hpp:83`).
    pub fn new(
        schedule: &[Shared<dyn Dividend>],
        mesher: Shared<dyn FdmMesher>,
        reference_date: Date,
        day_counter: &DayCounter,
        equity_direction: Size,
    ) -> QlResult<FdmDividendHandler> {
        let mut dividends = Vec::with_capacity(schedule.len());
        let mut dividend_dates = Vec::with_capacity(schedule.len());
        let mut dividend_times = Vec::with_capacity(schedule.len());
        for dividend in schedule {
            dividends.push(dividend.amount()?);
            dividend_dates.push(dividend.date());
            dividend_times.push(day_counter.year_fraction(reference_date, dividend.date()));
        }

        let layout = mesher.layout();
        let locations = mesher.locations(equity_direction);
        let spacing = layout.spacing()[equity_direction];
        let x: Vec<Real> = (0..layout.dim()[equity_direction])
            .map(|i| locations[i * spacing].exp())
            .collect();
        require!(
            x.len() >= 2,
            "not enough points to interpolate: at least 2 required, {} provided",
            x.len()
        );

        Ok(FdmDividendHandler {
            x,
            dividend_times,
            dividend_dates,
            dividends,
            mesher,
            equity_direction,
        })
    }

    /// The dividend dates on the solver's clock (`cpp:56-58`).
    pub fn dividend_times(&self) -> &[Time] {
        &self.dividend_times
    }

    /// The dividend payment dates (`cpp:60-62`).
    pub fn dividend_dates(&self) -> &[Date] {
        &self.dividend_dates
    }

    /// The dividend amounts (`cpp:64-66`).
    pub fn dividends(&self) -> &[Real] {
        &self.dividends
    }

    /// The rolled `values` read at `max(x[0], x[k] - dividend)`.
    ///
    /// This is the arithmetic of C++'s `LinearInterpolation`
    /// (`linearinterpolation.hpp:80,89`, `interpolation.hpp:104-115`)
    /// rather than the shared type, which rejects the non-finite values C++
    /// lets through. The read never falls below `x[0]`, and the last segment
    /// extrapolates above the top of the grid.
    fn shifted(&self, values: &[Real], dividend: Real) -> Vec<Real> {
        let x = &self.x;
        x.iter()
            .map(|&x_k| {
                let read = x[0].max(x_k - dividend);
                let i = x[..x.len() - 1]
                    .partition_point(|&node| node <= read)
                    .saturating_sub(1);
                values[i] + (read - x[i]) * ((values[i + 1] - values[i]) / (x[i + 1] - x[i]))
            })
            .collect()
    }
}

impl StepCondition for FdmDividendHandler {
    /// `cpp:68-106`: at a dividend time, the grid read off at the
    /// dividend-shifted spot; at any other time, nothing.
    ///
    /// The membership test is exact equality on `Time`, as in C++'s
    /// `std::find` (`cpp:71`): the composite hands these very times to the
    /// solver as stopping times, so the step lands on the same bit pattern.
    ///
    /// The multi-dimensional branch (`cpp:82-104`) is ported as written: it
    /// shifts the equity slices that run along each other direction from the
    /// origin, which covers every slice of a two-dimensional layout.
    ///
    /// A non-finite rolled value is not an error: as in C++, it propagates to
    /// the reads on the two segments that touch it.
    fn apply_to(&self, a: &mut Array, t: Time) {
        let Some(position) = self.dividend_times.iter().position(|&time| time == t) else {
            return;
        };
        let dividend = self.dividends[position];

        let layout = self.mesher.layout();
        if layout.dim().len() == 1 {
            let shifted = self.shifted(a, dividend);
            a.copy_from_slice(&shifted);
            return;
        }

        let a_copy = a.clone();
        let x_spacing = layout.spacing()[self.equity_direction];
        for i in 0..layout.dim().len() {
            if i == self.equity_direction {
                continue;
            }
            let y_spacing = layout.spacing()[i];
            for j in 0..layout.dim()[i] {
                let slice: Vec<Real> = (0..self.x.len())
                    .map(|k| a_copy[j * y_spacing + k * x_spacing])
                    .collect();
                for (k, value) in self.shifted(&slice, dividend).into_iter().enumerate() {
                    a[j * y_spacing + k * x_spacing] = value;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cashflows::dividend_vector;
    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::FdmLinearOpLayout;
    use crate::shared::shared;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    const TOLERANCE: Real = 1e-12;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// Five spots from 50 to 150, equidistant in `ln(S)`: `x[1]` is about
    /// 65.8, so a dividend of 20 clamps both `x[0]` and `x[1]` to `x[0]`.
    const EQUITY_BOUNDARIES: (Real, Real) = (3.912_023_005_428_146, 5.010_635_294_096_256);

    fn built(
        dim: Vec<Size>,
        boundaries: &[(Real, Real)],
        direction: Size,
        amounts: &[Real; 2],
    ) -> QlResult<FdmDividendHandler> {
        let layout = shared(FdmLinearOpLayout::new(dim));
        let mesher = shared(UniformGridMesher::new(layout, boundaries).unwrap());
        let schedule = dividend_vector(&[reference() + 90, reference() + 180], amounts).unwrap();
        FdmDividendHandler::new(
            &schedule,
            mesher as Shared<dyn FdmMesher>,
            reference(),
            &Actual360::new(),
            direction,
        )
    }

    fn handler(dim: Vec<Size>, boundaries: &[(Real, Real)], direction: Size) -> FdmDividendHandler {
        built(dim, boundaries, direction, &[20.0, 5.0])
            .ok()
            .unwrap()
    }

    fn spots() -> Vec<Real> {
        let (lower, upper) = EQUITY_BOUNDARIES;
        let step = (upper - lower) / 4.0;
        (0..5).map(|i| (lower + i as Real * step).exp()).collect()
    }

    fn assert_close(actual: Real, expected: Real) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{actual} vs {expected}"
        );
    }

    #[test]
    fn the_schedule_is_placed_on_the_solver_clock() {
        let handler = handler(vec![5], &[EQUITY_BOUNDARIES], 0);

        assert_eq!(handler.dividend_times(), &[0.25, 0.5]);
        assert_eq!(
            handler.dividend_dates(),
            &[reference() + 90, reference() + 180]
        );
        assert_eq!(handler.dividends(), &[20.0, 5.0]);
    }

    /// A payoff linear in the spot, so the shifted grid is exactly
    /// `max(x[0], x[k] - D)`: the interior moves down by `D` and the two
    /// lowest points clamp to `x[0]`.
    #[test]
    fn a_dividend_time_shifts_a_one_dimensional_grid_by_the_dividend() {
        let handler = handler(vec![5], &[EQUITY_BOUNDARIES], 0);
        let x = spots();

        let mut first = Array::from(x.clone());
        handler.apply_to(&mut first, 0.25);
        let mut second = Array::from(x.clone());
        handler.apply_to(&mut second, 0.5);

        for k in 0..x.len() {
            assert_close(first[k], x[0].max(x[k] - 20.0));
            assert_close(second[k], x[0].max(x[k] - 5.0));
        }
        assert_close(first[0], x[0]);
        assert_close(first[1], x[0]);
        assert_close(first[4], x[4] - 20.0);
    }

    #[test]
    fn any_other_time_leaves_the_grid_untouched() {
        let handler = handler(vec![5], &[EQUITY_BOUNDARIES], 0);
        let x = Array::from(spots());

        for t in [0.0, 0.25 + 1e-5, 0.3, 1.0] {
            let mut values = x.clone();
            handler.apply_to(&mut values, t);
            assert_eq!(values, x);
        }
    }

    /// Each equity slice moves along the equity direction only: the offset
    /// carried by the other direction survives untouched.
    #[test]
    fn a_two_dimensional_grid_shifts_each_slice_along_the_equity_direction() {
        let x = spots();
        for direction in [0, 1] {
            let other = 1 - direction;
            let mut dim = vec![3, 3];
            dim[direction] = 5;
            let mut boundaries = vec![(0.0, 1.0), (0.0, 1.0)];
            boundaries[direction] = EQUITY_BOUNDARIES;
            let handler = handler(dim.clone(), &boundaries, direction);
            let layout = FdmLinearOpLayout::new(dim);

            let mut values = Array::with_size(layout.size());
            for iter in layout.iter() {
                let c = iter.coordinates();
                values[iter.index()] = x[c[direction]] + 100.0 * c[other] as Real;
            }
            handler.apply_to(&mut values, 0.25);

            for iter in layout.iter() {
                let c = iter.coordinates();
                let expected = x[0].max(x[c[direction]] - 20.0) + 100.0 * c[other] as Real;
                assert_close(values[iter.index()], expected);
            }
        }
    }

    /// A negative dividend reads above the top of the grid, which the last
    /// segment extrapolates (`cpp:79`).
    #[test]
    fn a_negative_dividend_extrapolates_past_the_top_of_the_grid() {
        let handler = built(vec![5], &[EQUITY_BOUNDARIES], 0, &[-20.0, 5.0])
            .ok()
            .unwrap();
        let x = spots();

        let mut values = Array::from(x.clone());
        handler.apply_to(&mut values, 0.25);

        for k in 0..x.len() {
            assert_close(values[k], x[k] + 20.0);
        }
    }

    /// C++ `update()` has no finite check, so a NaN at node 2 reaches exactly
    /// the reads on the two segments that touch it: `x[2] - 20` falls on
    /// segment 1, `x[3] - 20` on segment 2, `x[4] - 20` on segment 3.
    #[test]
    fn a_nan_rolled_value_reaches_only_the_reads_on_its_segments() {
        let handler = handler(vec![5], &[EQUITY_BOUNDARIES], 0);
        let x = spots();

        let mut rolled = x.clone();
        rolled[2] = Real::NAN;
        let mut values = Array::from(rolled);
        handler.apply_to(&mut values, 0.25);

        assert_close(values[0], x[0]);
        assert_close(values[1], x[0]);
        assert!(values[2].is_nan());
        assert!(values[3].is_nan());
        assert_close(values[4], x[4] - 20.0);
    }

    /// An infinite node follows the same arithmetic: segment 2 reads
    /// `y + positive * inf`, and segment 3 starts on the node, so `inf - inf`.
    #[test]
    fn an_infinite_rolled_value_follows_the_segment_arithmetic() {
        let handler = handler(vec![5], &[EQUITY_BOUNDARIES], 0);
        let x = spots();

        let mut rolled = x.clone();
        rolled[3] = Real::INFINITY;
        let mut values = Array::from(rolled);
        handler.apply_to(&mut values, 0.25);

        assert_close(values[0], x[0]);
        assert_close(values[1], x[0]);
        assert_close(values[2], x[2] - 20.0);
        assert_eq!(values[3], Real::INFINITY);
        assert!(values[4].is_nan());
    }

    #[test]
    fn a_nan_stays_in_its_slice_of_a_two_dimensional_grid() {
        let handler = handler(vec![3, 5], &[(0.0, 1.0), EQUITY_BOUNDARIES], 1);
        let layout = FdmLinearOpLayout::new(vec![3, 5]);
        let x = spots();

        let mut values = Array::with_size(layout.size());
        for iter in layout.iter() {
            let c = iter.coordinates();
            values[iter.index()] = if c == [1, 2] { Real::NAN } else { x[c[1]] };
        }
        handler.apply_to(&mut values, 0.25);

        for iter in layout.iter() {
            let c = iter.coordinates();
            if c[0] == 1 && (c[1] == 2 || c[1] == 3) {
                assert!(values[iter.index()].is_nan());
            } else {
                assert_close(values[iter.index()], x[0].max(x[c[1]] - 20.0));
            }
        }
    }

    #[test]
    fn a_one_point_equity_axis_is_rejected_at_construction() {
        for (dim, boundaries, direction) in [
            (vec![1], vec![EQUITY_BOUNDARIES], 0),
            (vec![1, 3], vec![EQUITY_BOUNDARIES, (0.0, 1.0)], 0),
        ] {
            let error = built(dim, &boundaries, direction, &[20.0, 5.0])
                .err()
                .expect("a one-point equity axis cannot be interpolated");
            assert!(error.to_string().contains("not enough points"), "{error}");
        }
    }
}
