//! Black volatility surfaces; all handles retain the common trait representation.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::{interpolations::linear::Linear, matrix::Matrix};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure,
    BlackVolTimeExtrapolation,
};
use libitofin::time::calendar::Calendar;

pub(crate) type BlackVol = Handle<dyn BlackVolTermStructure>;
fn calendar(c: &Context, id: u64) -> BindingResult<Option<Calendar>> {
    if id == 0 {
        Ok(None)
    } else {
        crate::time_api::calendar(c, id).map(Some)
    }
}
fn flag(value: i32) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("expected boolean 0 or 1")),
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_constant_vol_new(
    ctx: *mut Context,
    reference_date: i32,
    volatility: f64,
    dc: u64,
    cal: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !volatility.is_finite() || volatility < 0.0 {
                return Err(BindingError::invalid(
                    "volatility must be finite and nonnegative",
                ));
            }
            let v = shared(BlackConstantVol::new(
                date(reference_date)?,
                calendar(c, cal)?,
                volatility,
                day_counter(c, dc)?,
            )) as Shared<dyn BlackVolTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_variance_curve_new(
    ctx: *mut Context,
    reference_date: i32,
    dates: *const i32,
    vols: *const f64,
    count: usize,
    dc: u64,
    monotone: i32,
    extrapolation: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dates = input_slice(dates, count)?
                .iter()
                .copied()
                .map(date)
                .collect::<BindingResult<Vec<_>>>()?;
            let vols = input_slice(vols, count)?;
            if vols.iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err(BindingError::invalid(
                    "volatilities must be finite and nonnegative",
                ));
            }
            let extrapolation = match extrapolation {
                0 => BlackVolTimeExtrapolation::FlatVolatility,
                1 => BlackVolTimeExtrapolation::UseInterpolator,
                2 => BlackVolTimeExtrapolation::LinearVariance,
                _ => return Err(BindingError::invalid("unknown time extrapolation")),
            };
            let v = shared(BlackVarianceCurve::with_interpolator(
                date(reference_date)?,
                &dates,
                vols,
                day_counter(c, dc)?,
                flag(monotone)?,
                extrapolation,
                Linear,
            )?) as Shared<dyn BlackVolTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_variance_surface_new(
    ctx: *mut Context,
    reference_date: i32,
    dates: *const i32,
    date_count: usize,
    strikes: *const f64,
    strike_count: usize,
    vols: *const f64,
    vol_count: usize,
    dc: u64,
    cal: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if date_count == 0
                || strike_count == 0
                || date_count.checked_mul(strike_count) != Some(vol_count)
            {
                return Err(BindingError::invalid(
                    "volatility matrix must have strike_count rows and date_count columns",
                ));
            }
            let dates = input_slice(dates, date_count)?
                .iter()
                .copied()
                .map(date)
                .collect::<BindingResult<Vec<_>>>()?;
            let strikes = input_slice(strikes, strike_count)?.to_vec();
            let vols = input_slice(vols, vol_count)?;
            if strikes.iter().any(|x| !x.is_finite())
                || vols.iter().any(|v| !v.is_finite() || *v < 0.0)
            {
                return Err(BindingError::invalid(
                    "strikes must be finite and volatilities finite and nonnegative",
                ));
            }
            let mut matrix = Matrix::with_size(strike_count, date_count);
            for i in 0..strike_count {
                for j in 0..date_count {
                    matrix[(i, j)] = vols[i * date_count + j];
                }
            }
            let v = shared(BlackVarianceSurface::new(
                date(reference_date)?,
                calendar(c, cal)?,
                &dates,
                strikes,
                &matrix,
                day_counter(c, dc)?,
            )?) as Shared<dyn BlackVolTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
/// Query kind: 0 vol, 1 variance, 2 forward vol, 3 forward variance,
/// 4 minimum strike, 5 maximum strike. Times are year fractions.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_vol_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    t1: f64,
    t2: f64,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = c.get::<BlackVol>(id)?.current_link()?;
            let extrapolate = flag(extrapolate)?;
            if (0..=3).contains(&kind)
                && (!t1.is_finite() || !t2.is_finite() || !strike.is_finite())
            {
                return Err(BindingError::invalid("query inputs must be finite"));
            }
            let result = match kind {
                0 => v.black_vol(t1, strike, extrapolate)?,
                1 => v.black_variance(t1, strike, extrapolate)?,
                2 => v.black_forward_vol(t1, t2, strike, extrapolate)?,
                3 => v.black_forward_variance(t1, t2, strike, extrapolate)?,
                4 => v.min_strike(),
                5 => v.max_strike(),
                _ => return Err(BindingError::invalid("unknown Black volatility query")),
            };
            output(out, result)
        })
    }
}
/// Date query kind: 0 vol, 1 variance.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_vol_date_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    serial: i32,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !strike.is_finite() {
                return Err(BindingError::invalid("strike must be finite"));
            }
            let v = c.get::<BlackVol>(id)?.current_link()?;
            let d = date(serial)?;
            let extrapolate = flag(extrapolate)?;
            output(
                out,
                match kind {
                    0 => v.black_vol_date(d, strike, extrapolate)?,
                    1 => v.black_variance_date(d, strike, extrapolate)?,
                    _ => return Err(BindingError::invalid("unknown date volatility query")),
                },
            )
        })
    }
}
/// Metadata action: 0 max date, 1 allows extrapolation, 2 enable, 3 disable.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_vol_control(
    ctx: *mut Context,
    id: u64,
    action: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = c.get::<BlackVol>(id)?.current_link()?;
            output(
                out,
                match action {
                    0 => v.max_date().serial_number(),
                    1 => i32::from(v.allows_extrapolation()),
                    2 => {
                        v.enable_extrapolation();
                        1
                    }
                    3 => {
                        v.disable_extrapolation();
                        0
                    }
                    _ => return Err(BindingError::invalid("unknown volatility action")),
                },
            )
        })
    }
}
