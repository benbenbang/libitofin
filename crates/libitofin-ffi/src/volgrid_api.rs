//! Native matrix and cap/floor volatility grid constructors.
use crate::boundary::*;
use crate::market_api::quote;
use crate::settings_api::settings;
use crate::smile_api::volatility_type;
use crate::time_api::{calendar, convention, date, day_counter, time_unit};
use libitofin::handle::Handle;
use libitofin::math::matrix::Matrix;
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    CapFloorTermVolSurface, CapFloorTermVolatilityStructure, SwaptionVolatilityMatrix,
    SwaptionVolatilityStructure,
};
use libitofin::time::period::Period;
#[repr(C)]
pub struct ItofinVolGridConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub settings: u64,
    pub option_lengths: *const i32,
    pub option_units: *const i32,
    pub rows: usize,
    pub swap_lengths: *const i32,
    pub swap_units: *const i32,
    pub strikes: *const f64,
    pub columns: usize,
    pub values: *const f64,
    pub quotes: *const u64,
    pub count: usize,
    pub shifts: *const f64,
    pub shift_count: usize,
    pub volatility_type: i32,
    pub flat_extrapolation: i32,
}
pub(crate) unsafe fn periods(
    lengths: *const i32,
    units: *const i32,
    n: usize,
) -> BindingResult<Vec<Period>> {
    unsafe { input_slice(lengths, n)? }
        .iter()
        .zip(unsafe { input_slice(units, n)? })
        .map(|(&l, &u)| Ok(Period::new(l, time_unit(u)?)))
        .collect()
}
unsafe fn matrix(ptr: *const f64, rows: usize, cols: usize) -> BindingResult<Matrix> {
    let n = rows
        .checked_mul(cols)
        .ok_or_else(|| BindingError::invalid("matrix size overflow"))?;
    let data = unsafe { input_slice(ptr, n)? };
    if data.iter().any(|v| !v.is_finite()) {
        return Err(BindingError::invalid("matrix values must be finite"));
    }
    let mut m = Matrix::with_size(rows, cols);
    for i in 0..rows {
        for j in 0..cols {
            m[(i, j)] = data[i * cols + j];
        }
    }
    Ok(m)
}
unsafe fn quotes(
    c: &Context,
    x: &ItofinVolGridConfig,
) -> BindingResult<Vec<Vec<Handle<dyn Quote>>>> {
    unsafe { input_slice(x.quotes, x.count)? }
        .chunks(x.columns)
        .map(|r| r.iter().map(|&id| quote(c, id)).collect())
        .collect()
}
fn validate(x: &ItofinVolGridConfig) -> BindingResult<()> {
    if x.rows == 0 || x.columns == 0 || x.rows.checked_mul(x.columns) != Some(x.count) {
        return Err(BindingError::invalid(
            "grid must be nonempty and match rows times columns",
        ));
    }
    if x.shift_count != 0 && x.shift_count != x.count {
        return Err(BindingError::invalid("shift grid dimensions do not match"));
    }
    Ok(())
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_matrix_new(
    ctx: *mut Context,
    cfg: *const ItofinVolGridConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let x = &*cfg;
            validate(x)?;
            let options = periods(x.option_lengths, x.option_units, x.rows)?;
            let swaps = periods(x.swap_lengths, x.swap_units, x.columns)?;
            let cal = calendar(c, x.calendar)?;
            let bdc = convention(x.convention)?;
            let dc = day_counter(c, x.day_counter)?;
            let kind = volatility_type(x.volatility_type)?;
            let shifts = if x.shift_count == 0 {
                Matrix::new()
            } else {
                matrix(x.shifts, x.rows, x.columns)?
            };
            let flat = match x.flat_extrapolation {
                0 => false,
                1 => true,
                _ => return Err(BindingError::invalid("flat extrapolation must be 0 or 1")),
            };
            let v = if x.settings == 0 {
                let values = matrix(x.values, x.rows, x.columns)?;
                let build = if flat {
                    SwaptionVolatilityMatrix::new_flat
                } else {
                    SwaptionVolatilityMatrix::new
                };
                build(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    options,
                    swaps,
                    &values,
                    dc,
                    kind,
                    &shifts,
                )?
            } else {
                let shifts = if x.shift_count == 0 {
                    vec![]
                } else {
                    input_slice(x.shifts, x.count)?
                        .chunks(x.columns)
                        .map(|r| r.to_vec())
                        .collect()
                };
                let build = if flat {
                    SwaptionVolatilityMatrix::moving_flat
                } else {
                    SwaptionVolatilityMatrix::moving
                };
                build(
                    cal,
                    bdc,
                    options,
                    swaps,
                    quotes(c, x)?,
                    dc,
                    kind,
                    shifts,
                    settings(c, x.settings)?,
                )?
            };
            output(
                out,
                c.insert(Handle::new(
                    shared(v) as Shared<dyn SwaptionVolatilityStructure>
                ))?,
            )
        })
    }
}
enum SwaptionMatrixForm {
    FixedQuotes,
    MovingMatrix,
    OptionDates(*const i32, usize),
}

unsafe fn additional_swaption_matrix(
    c: &Context,
    x: &ItofinVolGridConfig,
    form: SwaptionMatrixForm,
) -> BindingResult<SwaptionVolatilityMatrix> {
    validate(x)?;
    let swaps = unsafe { periods(x.swap_lengths, x.swap_units, x.columns)? };
    let cal = calendar(c, x.calendar)?;
    let bdc = convention(x.convention)?;
    let dc = day_counter(c, x.day_counter)?;
    let kind = volatility_type(x.volatility_type)?;
    let shifts = if x.shift_count == 0 {
        Matrix::new()
    } else {
        unsafe { matrix(x.shifts, x.rows, x.columns)? }
    };
    let flat = match x.flat_extrapolation {
        0 => false,
        1 => true,
        _ => return Err(BindingError::invalid("flat extrapolation must be 0 or 1")),
    };
    Ok(match form {
        SwaptionMatrixForm::FixedQuotes => {
            if x.settings != 0 {
                return Err(BindingError::invalid(
                    "fixed quote matrix does not take settings",
                ));
            }
            SwaptionVolatilityMatrix::fixed_quotes(
                date(x.reference_date)?,
                cal,
                bdc,
                unsafe { periods(x.option_lengths, x.option_units, x.rows)? },
                swaps,
                unsafe { quotes(c, x)? },
                dc,
                kind,
                (0..shifts.rows()).map(|i| shifts[i].to_vec()).collect(),
                flat,
            )?
        }
        SwaptionMatrixForm::MovingMatrix => SwaptionVolatilityMatrix::moving_matrix(
            cal,
            bdc,
            unsafe { periods(x.option_lengths, x.option_units, x.rows)? },
            swaps,
            &unsafe { matrix(x.values, x.rows, x.columns)? },
            dc,
            kind,
            &shifts,
            settings(c, x.settings)?,
            flat,
        )?,
        SwaptionMatrixForm::OptionDates(ptr, count) => {
            if count != x.rows || x.settings != 0 {
                return Err(BindingError::invalid(
                    "option dates must match rows and use a fixed reference date",
                ));
            }
            let dates = unsafe { input_slice(ptr, count)? }
                .iter()
                .map(|&serial| date(serial))
                .collect::<BindingResult<Vec<_>>>()?;
            SwaptionVolatilityMatrix::with_option_dates(
                date(x.reference_date)?,
                cal,
                bdc,
                dates,
                swaps,
                &unsafe { matrix(x.values, x.rows, x.columns)? },
                dc,
                kind,
                &shifts,
                flat,
            )?
        }
    })
}

/// Fixed reference date with retained quote handles. Settings must be zero.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_matrix_fixed_quotes(
    ctx: *mut Context,
    cfg: *const ItofinVolGridConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let surface = additional_swaption_matrix(c, &*cfg, SwaptionMatrixForm::FixedQuotes)?;
            output(
                out,
                c.insert(Handle::new(
                    shared(surface) as Shared<dyn SwaptionVolatilityStructure>
                ))?,
            )
        })
    }
}

/// Moving reference date with copied numeric data. Settings must be a valid handle.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_matrix_moving_matrix(
    ctx: *mut Context,
    cfg: *const ItofinVolGridConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let surface = additional_swaption_matrix(c, &*cfg, SwaptionMatrixForm::MovingMatrix)?;
            output(
                out,
                c.insert(Handle::new(
                    shared(surface) as Shared<dyn SwaptionVolatilityStructure>
                ))?,
            )
        })
    }
}

/// Fixed reference and exercise dates with copied numeric data.
/// Settings must be zero; option tenor buffers are unused and may be null.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_vol_matrix_dates(
    ctx: *mut Context,
    cfg: *const ItofinVolGridConfig,
    option_dates: *const i32,
    date_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let surface = additional_swaption_matrix(
                c,
                &*cfg,
                SwaptionMatrixForm::OptionDates(option_dates, date_count),
            )?;
            output(
                out,
                c.insert(Handle::new(
                    shared(surface) as Shared<dyn SwaptionVolatilityStructure>
                ))?,
            )
        })
    }
}

/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_vol_surface_new(
    ctx: *mut Context,
    cfg: *const ItofinVolGridConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let x = &*cfg;
            validate(x)?;
            let options = periods(x.option_lengths, x.option_units, x.rows)?;
            let strikes = input_slice(x.strikes, x.columns)?.to_vec();
            if strikes.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("strikes must be finite"));
            }
            let cal = calendar(c, x.calendar)?;
            let bdc = convention(x.convention)?;
            let dc = day_counter(c, x.day_counter)?;
            let v = match (x.settings != 0, !x.quotes.is_null()) {
                (false, false) => CapFloorTermVolSurface::with_reference_date_from_matrix(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    options,
                    strikes,
                    &matrix(x.values, x.rows, x.columns)?,
                    dc,
                )?,
                (false, true) => CapFloorTermVolSurface::with_reference_date(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    options,
                    strikes,
                    quotes(c, x)?,
                    dc,
                )?,
                (true, false) => CapFloorTermVolSurface::moving_from_matrix(
                    x.settlement_days,
                    cal,
                    bdc,
                    options,
                    strikes,
                    &matrix(x.values, x.rows, x.columns)?,
                    dc,
                    settings(c, x.settings)?,
                )?,
                (true, true) => CapFloorTermVolSurface::moving(
                    x.settlement_days,
                    cal,
                    bdc,
                    options,
                    strikes,
                    quotes(c, x)?,
                    dc,
                    settings(c, x.settings)?,
                )?,
            };
            output(out, c.insert(shared(v))?)
        })
    }
}
/// Query 0 tenor, 1 date, 2 time.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_vol_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    length: i32,
    unit: i32,
    serial: i32,
    time: f64,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !time.is_finite() || !strike.is_finite() {
                return Err(BindingError::invalid("query inputs must be finite"));
            }
            let ext = match extrapolate {
                0 => false,
                1 => true,
                _ => return Err(BindingError::invalid("extrapolate must be 0 or 1")),
            };
            let v = c.get::<Shared<CapFloorTermVolSurface>>(id)?;
            output(
                out,
                match kind {
                    0 => v.volatility_tenor(Period::new(length, time_unit(unit)?), strike, ext)?,
                    1 => v.volatility_date(date(serial)?, strike, ext)?,
                    2 => v.volatility_time(time, strike, ext)?,
                    _ => return Err(BindingError::invalid("unknown cap/floor query")),
                },
            )
        })
    }
}

#[cfg(test)]
#[path = "volmatrix_completion_tests.rs"]
mod completion_tests;
