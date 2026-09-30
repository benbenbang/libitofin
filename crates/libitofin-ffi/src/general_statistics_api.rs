//! Context-owned empirical statistics accumulator.

use crate::boundary::{
    BindingError, Context, ItofinError, check_ptr, input_slice, output, with_context,
};
use libitofin::math::statistics::{
    EmpiricalStatistics, GeneralStatistics, MeanStdDev, RiskStatistics, Statistics,
};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::types::Real;

fn validate(value: Real, weight: Real) -> crate::boundary::BindingResult<()> {
    if !value.is_finite() || !weight.is_finite() || weight < 0.0 {
        return Err(BindingError::invalid(
            "observations must be finite and weights finite and nonnegative",
        ));
    }
    Ok(())
}

fn statistic(c: &Context, id: u64) -> crate::boundary::BindingResult<SharedMut<GeneralStatistics>> {
    c.get::<SharedMut<GeneralStatistics>>(id)
}

fn positive_tail(state: &GeneralStatistics, target: Real) -> crate::boundary::BindingResult<()> {
    if !state
        .data()
        .iter()
        .any(|&(value, weight)| value < target && weight > 0.0)
    {
        return Err(BindingError::invalid(
            "no positive-weight data below the target",
        ));
    }
    Ok(())
}

/// Create an empty accumulator. Release the handle with `itofin_handle_release`.
/// # Safety
/// Follow the crate-level C caller contract; `out` holds one handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_general_statistics_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(shared_mut(GeneralStatistics::new()))?)
        })
    }
}

/// Add one finite weighted observation. Zero weight still increases count.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_general_statistics_add(
    ctx: *mut Context,
    id: u64,
    value: Real,
    weight: Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            validate(value, weight)?;
            let state = statistic(c, id)?;
            let mut state = state.borrow_mut();
            if !(state.weight_sum() + weight).is_finite() {
                return Err(BindingError::invalid("total weight would be nonfinite"));
            }
            state.add_weighted(value, weight)?;
            Ok(())
        })
    }
}

/// Atomically append a batch. Null weights with zero length selects unit weights.
/// An invalid row or weight sum leaves the accumulator unchanged.
/// # Safety
/// `values` holds `len` doubles; when present, `weights` holds `weights_len`
/// doubles. Follow the crate-level C pointer contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_general_statistics_add_batch(
    ctx: *mut Context,
    id: u64,
    values: *const Real,
    len: usize,
    weights: *const Real,
    weights_len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let values = input_slice(values, len)?;
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != len {
                    return Err(BindingError::invalid(
                        "observation and weight lengths differ",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let state = statistic(c, id)?;
            let mut state = state.borrow_mut();
            let mut next = state.clone();
            next.reserve(len);
            let mut total = next.weight_sum();
            for (index, &value) in values.iter().enumerate() {
                let weight = weights.map_or(1.0, |items| items[index]);
                validate(value, weight)?;
                total += weight;
                if !total.is_finite() {
                    return Err(BindingError::invalid("total weight would be nonfinite"));
                }
                next.add_weighted(value, weight)?;
            }
            *state = next;
            Ok(())
        })
    }
}

/// Reset to an empty accumulator.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_general_statistics_reset(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            statistic(c, id)?.borrow_mut().reset();
            Ok(())
        })
    }
}

/// Copy the observation count and total weight. Either output may be null.
/// # Safety
/// Non-null outputs hold one value and follow the crate-level pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_general_statistics_summary(
    ctx: *mut Context,
    id: u64,
    out_samples: *mut usize,
    out_weight_sum: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if out_samples.is_null() && out_weight_sum.is_null() {
                return Err(BindingError::invalid(
                    "at least one summary output is required",
                ));
            }
            if !out_samples.is_null() {
                check_ptr(out_samples)?;
            }
            if !out_weight_sum.is_null() {
                check_ptr(out_weight_sum)?;
            }
            let state = statistic(c, id)?;
            let state = state.borrow();
            if !out_samples.is_null() {
                out_samples.write(state.samples());
            }
            if !out_weight_sum.is_null() {
                out_weight_sum.write(state.weight_sum());
            }
            Ok(())
        })
    }
}

/// Query a statistic. Selectors 0-19 are min, max, mean, variance, standard
/// deviation, error estimate, skewness, kurtosis, percentile, top percentile,
/// semi variance, semi deviation, downside variance, downside deviation,
/// regret, potential upside, VaR, ES, shortfall, and average shortfall.
/// `argument` supplies a probability, confidence, or target where applicable.
/// Nonfinite results and invalid selectors leave `out` unchanged.
/// # Safety
/// `out` holds one double and follows the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_general_statistics_query(
    ctx: *mut Context,
    id: u64,
    selector: i32,
    argument: Real,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let state = statistic(c, id)?;
            let mut state = state.borrow_mut();
            let weight_sum = state.weight_sum();
            if !matches!(selector, 0 | 1)
                && (!weight_sum.is_finite() || (state.samples() > 0 && weight_sum <= 0.0))
            {
                return Err(BindingError::invalid(
                    "total weight must be finite and positive",
                ));
            }
            if matches!(selector, 8 | 9)
                && !(argument.is_finite() && argument > 0.0 && argument <= 1.0)
            {
                return Err(BindingError::invalid("percentile must be in (0, 1]"));
            }
            if matches!(selector, 14 | 18 | 19) && !argument.is_finite() {
                return Err(BindingError::invalid("target must be finite"));
            }
            if matches!(selector, 10..=14 | 19) {
                let target = if matches!(selector, 10 | 11) {
                    state.mean()?
                } else if matches!(selector, 12 | 13) {
                    0.0
                } else {
                    argument
                };
                positive_tail(&state, target)?;
            }
            let result = match selector {
                0 => state.min()?,
                1 => state.max()?,
                2 => state.mean()?,
                3 => state.variance()?,
                4 => state.standard_deviation()?,
                5 => state.error_estimate()?,
                6 => state.skewness()?,
                7 => state.kurtosis()?,
                8 => state.percentile(argument)?,
                9 => state.top_percentile(argument)?,
                10 => state.semi_variance()?,
                11 => state.semi_deviation()?,
                12 => state.downside_variance()?,
                13 => state.downside_deviation()?,
                14 => state.regret(argument)?,
                15 => state.potential_upside(argument)?,
                16 => state.value_at_risk(argument)?,
                17 => {
                    let threshold = -state.value_at_risk(argument)?;
                    positive_tail(&state, threshold)?;
                    state.expected_shortfall(argument)?
                }
                18 => state.shortfall(argument)?,
                19 => state.average_shortfall(argument)?,
                _ => return Err(BindingError::invalid("unknown general statistic selector")),
            };
            if !result.is_finite() {
                return Err(BindingError::invalid("statistic result is nonfinite"));
            }
            out.write(result);
            Ok(())
        })
    }
}
