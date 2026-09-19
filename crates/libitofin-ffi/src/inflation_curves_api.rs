//! Zero and year-on-year curves preserve native bootstrap and seasonality behavior.
use crate::boundary::*;
use crate::inflation_api::bool_value;
use crate::inflation_helpers_api::{YoyHelper, ZeroHelper};
use crate::time_api::{date, day_counter, frequency};
use libitofin::handle::Handle;
use libitofin::math::interpolations::linear::Linear;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::inflation::{
    inflationtermstructure::{YoYInflationTermStructure, ZeroInflationTermStructure},
    interpolatedyoyinflationcurve::InterpolatedYoYInflationCurve,
    interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve,
    piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve,
    piecewisezeroinflationcurve::PiecewiseZeroInflationCurve,
};
use libitofin::time::frequency::Frequency;
pub(crate) fn frequency_code(f: Frequency) -> BindingResult<i32> {
    match f {
        Frequency::Annual => Ok(0),
        Frequency::Semiannual => Ok(1),
        Frequency::Quarterly => Ok(2),
        Frequency::Monthly => Ok(3),
        _ => Err(BindingError::invalid("frequency unavailable in Python API")),
    }
}
#[repr(C)]
pub struct ItofinInflationCurveConfig {
    pub reference: i32,
    pub base_date: i32,
    pub base_rate: f64,
    pub frequency: i32,
    pub day_counter: u64,
}
#[repr(C)]
pub struct ItofinInflationNode {
    pub date: i32,
    pub time: f64,
    pub rate: f64,
}

#[derive(Clone)]
pub(crate) struct ZeroCurve {
    pub handle: Handle<dyn ZeroInflationTermStructure>,
    interpolated: Option<Shared<InterpolatedZeroInflationCurve<Linear>>>,
    piecewise: Option<Shared<PiecewiseZeroInflationCurve<Linear>>>,
}
pub(crate) fn zero_curve(
    c: &Context,
    id: u64,
) -> BindingResult<Handle<dyn ZeroInflationTermStructure>> {
    Ok(c.get::<ZeroCurve>(id)?.handle)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_curve_new(
    ctx: *mut Context,
    a: *const ItofinInflationCurveConfig,
    dates: *const i32,
    rates: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let dates = input_slice(dates, n)?
                .iter()
                .map(|v| date(*v))
                .collect::<BindingResult<Vec<_>>>()?;
            let rates = input_slice(rates, n)?.to_vec();
            if rates.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite inflation rate"));
            }
            let p = shared(InterpolatedZeroInflationCurve::new(
                date(a.reference)?,
                dates,
                rates,
                frequency(a.frequency)?,
                day_counter(c, a.day_counter)?,
                Linear,
                None,
            )?);
            output(
                out,
                c.insert(ZeroCurve {
                    handle: Handle::new(p.clone()),
                    interpolated: Some(p),
                    piecewise: None,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_zero_inflation_new(
    ctx: *mut Context,
    a: *const ItofinInflationCurveConfig,
    helpers: *const u64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let helpers = input_slice(helpers, n)?
                .iter()
                .map(|id| c.get::<ZeroHelper>(*id).map(|h| h.inner))
                .collect::<BindingResult<Vec<_>>>()?;
            let p = PiecewiseZeroInflationCurve::<Linear>::new(
                date(a.reference)?,
                date(a.base_date)?,
                frequency(a.frequency)?,
                day_counter(c, a.day_counter)?,
                helpers,
                None,
            )?;
            output(
                out,
                c.insert(ZeroCurve {
                    handle: Handle::new(p.clone()),
                    interpolated: None,
                    piecewise: Some(p),
                })?,
            )
        })
    }
}
/// Build a zero inflation curve using an unlinked index clone's last fixing date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_zero_inflation_last_fixing_new(
    ctx: *mut Context,
    a: *const ItofinInflationCurveConfig,
    index: u64,
    helpers: *const u64,
    n: usize,
    seasonality: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let helpers = input_slice(helpers, n)?
                .iter()
                .map(|id| c.get::<ZeroHelper>(*id).map(|helper| helper.inner))
                .collect::<BindingResult<Vec<_>>>()?;
            let seasonality = if seasonality == 0 {
                None
            } else {
                Some(crate::inflation_seasonality_api::seasonality(
                    c,
                    seasonality,
                )?)
            };
            let p = PiecewiseZeroInflationCurve::<Linear>::with_last_fixing_date(
                date(a.reference)?,
                &crate::inflation_api::zero_index(c, index)?,
                frequency(a.frequency)?,
                day_counter(c, a.day_counter)?,
                helpers,
                seasonality,
            )?;
            output(
                out,
                c.insert(ZeroCurve {
                    handle: Handle::new(p.clone()),
                    interpolated: None,
                    piecewise: Some(p),
                })?,
            )
        })
    }
}
/// query 0 time rate, 1 date rate, 2 base date serial, 3 frequency, 4 has seasonality, 5 calculate.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_curve_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    t: f64,
    serial: i32,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let obj = c.get::<ZeroCurve>(id)?;
            let p = obj.handle.current_link()?;
            let ex = bool_value(extrapolate)?;
            let v = match query {
                0 => {
                    if !t.is_finite() {
                        return Err(BindingError::invalid("nonfinite time"));
                    }
                    p.zero_rate(t, ex)?
                }
                1 => p.zero_rate_date(date(serial)?, ex)?,
                2 => p.try_base_date()?.serial_number() as f64,
                3 => frequency_code(p.frequency())? as f64,
                4 => p.has_seasonality() as i32 as f64,
                5 => {
                    obj.piecewise
                        .ok_or_else(|| BindingError::invalid("not a piecewise curve"))?
                        .calculate()?;
                    0.0
                }
                _ => return Err(BindingError::invalid("unknown inflation curve query")),
            };
            output(out, v)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_set_seasonality(
    ctx: *mut Context,
    id: u64,
    seasonality: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let season = if seasonality == 0 {
                None
            } else {
                Some(crate::inflation_seasonality_api::seasonality(
                    c,
                    seasonality,
                )?)
            };
            zero_curve(c, id)?.current_link()?.set_seasonality(season)?;
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_inflation_nodes(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinInflationNode,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let obj = c.get::<ZeroCurve>(id)?;
            let (nodes, times) = if let Some(p) = obj.interpolated {
                (p.nodes(), p.times().to_vec())
            } else {
                let p = obj
                    .piecewise
                    .ok_or_else(|| BindingError::invalid("curve has no node data"))?;
                (p.nodes()?, p.times()?)
            };
            output(count, nodes.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < nodes.len() {
                return Err(BindingError::invalid("node buffer too small"));
            }
            for (i, ((d, r), t)) in nodes.into_iter().zip(times).enumerate() {
                output(
                    out.add(i),
                    ItofinInflationNode {
                        date: d.serial_number(),
                        time: t,
                        rate: r,
                    },
                )?;
            }
            Ok(())
        })
    }
}

#[derive(Clone)]
pub(crate) struct YoYCurve {
    pub handle: Handle<dyn YoYInflationTermStructure>,
    interpolated: Option<Shared<InterpolatedYoYInflationCurve<Linear>>>,
    piecewise: Option<Shared<PiecewiseYoYInflationCurve<Linear>>>,
}
pub(crate) fn yoy_curve(
    c: &Context,
    id: u64,
) -> BindingResult<Handle<dyn YoYInflationTermStructure>> {
    Ok(c.get::<YoYCurve>(id)?.handle)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_curve_new(
    ctx: *mut Context,
    a: *const ItofinInflationCurveConfig,
    dates: *const i32,
    rates: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let dates = input_slice(dates, n)?
                .iter()
                .map(|v| date(*v))
                .collect::<BindingResult<Vec<_>>>()?;
            let rates = input_slice(rates, n)?.to_vec();
            if rates.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite inflation rate"));
            }
            let p = shared(InterpolatedYoYInflationCurve::new(
                date(a.reference)?,
                dates,
                rates,
                frequency(a.frequency)?,
                day_counter(c, a.day_counter)?,
                Linear,
                None,
            )?);
            output(
                out,
                c.insert(YoYCurve {
                    handle: Handle::new(p.clone()),
                    interpolated: Some(p),
                    piecewise: None,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_yoy_inflation_new(
    ctx: *mut Context,
    a: *const ItofinInflationCurveConfig,
    helpers: *const u64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let helpers = input_slice(helpers, n)?
                .iter()
                .map(|id| c.get::<YoyHelper>(*id).map(|h| h.inner))
                .collect::<BindingResult<Vec<_>>>()?;
            let p = PiecewiseYoYInflationCurve::<Linear>::new(
                date(a.reference)?,
                date(a.base_date)?,
                a.base_rate,
                frequency(a.frequency)?,
                day_counter(c, a.day_counter)?,
                helpers,
                None,
            )?;
            output(
                out,
                c.insert(YoYCurve {
                    handle: Handle::new(p.clone()),
                    interpolated: None,
                    piecewise: Some(p),
                })?,
            )
        })
    }
}
/// query 0 time rate, 1 date rate, 2 base date serial, 3 frequency, 4 has seasonality, 5 calculate, 6 base rate.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_curve_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    t: f64,
    serial: i32,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let obj = c.get::<YoYCurve>(id)?;
            let p = obj.handle.current_link()?;
            let ex = bool_value(extrapolate)?;
            let v = match query {
                0 => {
                    if !t.is_finite() {
                        return Err(BindingError::invalid("nonfinite time"));
                    }
                    p.yoy_rate(t, ex)?
                }
                1 => p.yoy_rate_date(date(serial)?, ex)?,
                2 => p.base_date().serial_number() as f64,
                3 => frequency_code(p.frequency())? as f64,
                4 => p.has_seasonality() as i32 as f64,
                5 => {
                    obj.piecewise
                        .ok_or_else(|| BindingError::invalid("not a piecewise curve"))?
                        .calculate()?;
                    0.0
                }
                6 => p.base_rate()?,
                _ => return Err(BindingError::invalid("unknown inflation curve query")),
            };
            output(out, v)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_set_seasonality(
    ctx: *mut Context,
    id: u64,
    seasonality: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let season = if seasonality == 0 {
                None
            } else {
                Some(crate::inflation_seasonality_api::seasonality(
                    c,
                    seasonality,
                )?)
            };
            yoy_curve(c, id)?.current_link()?.set_seasonality(season)?;
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_inflation_nodes(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinInflationNode,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let obj = c.get::<YoYCurve>(id)?;
            let (nodes, times) = if let Some(p) = obj.interpolated {
                (p.nodes(), p.times().to_vec())
            } else {
                let p = obj
                    .piecewise
                    .ok_or_else(|| BindingError::invalid("curve has no node data"))?;
                (p.nodes()?, p.times()?)
            };
            output(count, nodes.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < nodes.len() {
                return Err(BindingError::invalid("node buffer too small"));
            }
            for (i, ((d, r), t)) in nodes.into_iter().zip(times).enumerate() {
                output(
                    out.add(i),
                    ItofinInflationNode {
                        date: d.serial_number(),
                        time: t,
                        rate: r,
                    },
                )?;
            }
            Ok(())
        })
    }
}
