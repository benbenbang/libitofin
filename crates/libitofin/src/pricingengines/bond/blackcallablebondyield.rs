use std::cell::RefCell;

use crate::cashflow::Leg;
use crate::cashflows::{CashFlows, Duration};
use crate::errors::{QlError, QlResult};
use crate::interestrate::{Compounding, InterestRate};
use crate::math::solver1d::{DerivativeSolver, func1d};
use crate::math::solvers1d::newtonsafe::NewtonSafe;
use crate::require;
use crate::settings::Settings;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::Real;

pub(super) fn solve_forward_yield(
    leg: &Leg,
    forward_npv: Real,
    face: Real,
    day_counter: DayCounter,
    frequency: Frequency,
    settings: &Settings<Date>,
    exercise: Date,
) -> QlResult<Real> {
    let target = forward_npv / face;
    require!(
        target.is_finite() && target > 0.0,
        "invalid normalized forward bond NPV"
    );
    let failure = RefCell::new(None);
    let present_value = |yield_rate| -> QlResult<(InterestRate, Real)> {
        let rate = InterestRate::new(
            yield_rate,
            day_counter.clone(),
            Compounding::Compounded,
            frequency,
        )?;
        let npv = CashFlows::npv_at_yield(
            leg,
            &rate,
            settings,
            Some(false),
            Some(exercise),
            Some(exercise),
        )? / face;
        require!(npv.is_finite(), "nonfinite normalized forward yield NPV");
        Ok((rate, npv))
    };
    if present_value(0.0)?.1 == target {
        return Ok(0.0);
    }
    let park = |error: QlError| {
        failure.borrow_mut().get_or_insert(error);
        Real::NAN
    };
    let objective = func1d(
        |yield_rate| match present_value(yield_rate) {
            Ok((_, npv)) => npv - target,
            Err(error) => park(error),
        },
        |yield_rate| {
            let derivative = || -> QlResult<Real> {
                let (rate, npv) = present_value(yield_rate)?;
                let duration = CashFlows::duration(
                    leg,
                    &rate,
                    Duration::Modified,
                    settings,
                    Some(false),
                    Some(exercise),
                    Some(exercise),
                )?;
                let derivative = -duration * npv;
                require!(
                    derivative.is_finite(),
                    "nonfinite normalized forward yield derivative"
                );
                Ok(derivative)
            };
            match derivative() {
                Ok(value) => value,
                Err(error) => park(error),
            }
        },
    );
    let root = NewtonSafe::new()
        .with_max_evaluations(100)
        .solve(objective, 1.0e-10, 0.05, 0.005);
    match failure.into_inner() {
        Some(error) => Err(error),
        None => root,
    }
}
