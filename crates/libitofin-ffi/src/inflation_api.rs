//! Inflation indices retain relinkable curve handles and native fixing histories.
use crate::boundary::*;
use crate::time_api::{date, frequency, time_unit};
use libitofin::currency::Currency;
use libitofin::handle::RelinkableHandle;
use libitofin::indexes::index::Index;
use libitofin::indexes::inflation::{EuHicp, UkHicp, UkRpi};
use libitofin::indexes::inflationindex::{
    CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex,
};
use libitofin::indexes::region::Region;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::inflation::inflationtermstructure::{
    YoYInflationTermStructure, ZeroInflationTermStructure,
};
use libitofin::time::{date::Date, period::Period};
use std::ffi::c_char;

#[derive(Clone)]
pub(crate) struct ZeroIndex {
    pub inner: Shared<ZeroInflationIndex>,
    pub curve: RelinkableHandle<dyn ZeroInflationTermStructure>,
}
#[derive(Clone)]
pub(crate) struct YoyIndex {
    pub inner: Shared<YoYInflationIndex>,
    pub curve: RelinkableHandle<dyn YoYInflationTermStructure>,
    pub underlying: Option<ZeroIndex>,
}
pub(crate) fn zero_index(c: &Context, id: u64) -> BindingResult<Shared<ZeroInflationIndex>> {
    Ok(c.get::<ZeroIndex>(id)?.inner)
}
pub(crate) fn yoy_index(c: &Context, id: u64) -> BindingResult<Shared<YoYInflationIndex>> {
    Ok(c.get::<YoyIndex>(id)?.inner)
}
pub(crate) use crate::inflation_curves_api::{yoy_curve, zero_curve};
pub(crate) fn cpi_interpolation(v: i32) -> BindingResult<CpiInterpolationType> {
    match v {
        0 => Ok(CpiInterpolationType::Flat),
        1 => Ok(CpiInterpolationType::Linear),
        _ => Err(BindingError::invalid("unknown CPI interpolation")),
    }
}
pub(crate) fn bool_value(v: i32) -> BindingResult<bool> {
    match v {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("boolean must be 0 or 1")),
    }
}
unsafe fn text(p: *const c_char, n: usize) -> BindingResult<String> {
    Ok(
        std::str::from_utf8(unsafe { input_slice(p.cast::<u8>(), n)? })
            .map_err(|_| BindingError::invalid("invalid UTF-8"))?
            .to_owned(),
    )
}
/// kind 0 UK RPI, 1 UK HICP, 2 EU HICP.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_zero_index_new(
    ctx: *mut Context,
    kind: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let s = c.get::<Shared<Settings<Date>>>(settings)?;
            let curve = RelinkableHandle::empty();
            let inner = match kind {
                0 => UkRpi::new(s),
                1 => UkHicp::new(s),
                2 => EuHicp::new(s),
                _ => return Err(BindingError::invalid("unknown inflation index")),
            }
            .with_term_structure(curve.handle());
            output(
                out,
                c.insert(ZeroIndex {
                    inner: shared(inner),
                    curve,
                })?,
            )
        })
    }
}
#[repr(C)]
pub struct ItofinYoyIndexConfig {
    pub family: *const c_char,
    pub family_length: usize,
    pub region_name: *const c_char,
    pub region_name_length: usize,
    pub region_code: *const c_char,
    pub region_code_length: usize,
    pub revised: i32,
    pub frequency: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub currency_name: *const c_char,
    pub currency_name_length: usize,
    pub currency_code: *const c_char,
    pub currency_code_length: usize,
    pub currency_numeric: i32,
    pub currency_symbol: *const c_char,
    pub currency_symbol_length: usize,
    pub currency_fraction_symbol: *const c_char,
    pub currency_fraction_symbol_length: usize,
    pub currency_fractions: i32,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_index_new(
    ctx: *mut Context,
    a: *const ItofinYoyIndexConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let curve = RelinkableHandle::empty();
            let inner = YoYInflationIndex::new(
                text(a.family, a.family_length)?,
                Region::new(
                    text(a.region_name, a.region_name_length)?,
                    text(a.region_code, a.region_code_length)?,
                ),
                bool_value(a.revised)?,
                frequency(a.frequency)?,
                Period::new(a.lag_length, time_unit(a.lag_unit)?),
                Currency::new(
                    text(a.currency_name, a.currency_name_length)?,
                    text(a.currency_code, a.currency_code_length)?,
                    a.currency_numeric,
                    text(a.currency_symbol, a.currency_symbol_length)?,
                    text(
                        a.currency_fraction_symbol,
                        a.currency_fraction_symbol_length,
                    )?,
                    a.currency_fractions,
                ),
                c.get::<Shared<Settings<Date>>>(a.settings)?,
            )
            .with_term_structure(curve.handle());
            output(
                out,
                c.insert(YoyIndex {
                    inner: shared(inner),
                    curve,
                    underlying: None,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_index_from_underlying(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let underlying = c.get::<ZeroIndex>(id)?;
            let curve = RelinkableHandle::empty();
            let inner = shared(
                YoYInflationIndex::from_underlying(underlying.inner.clone())
                    .with_term_structure(curve.handle()),
            );
            output(
                out,
                c.insert(YoyIndex {
                    inner,
                    curve,
                    underlying: Some(underlying),
                })?,
            )
        })
    }
}
/// Returns zero for a quoted index, otherwise a new external reference to its underlying.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_index_underlying(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let underlying = c.get::<YoyIndex>(id)?.underlying;
            output(
                out,
                match underlying {
                    Some(v) => c.insert(v)?,
                    None => 0,
                },
            )
        })
    }
}
/// kind 0 zero, 1 YoY. Names preserve embedded NUL; count includes an extra trailing NUL.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_index_name(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    out: *mut c_char,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let name = match kind {
                0 => zero_index(c, id)?.name(),
                1 => yoy_index(c, id)?.name(),
                _ => return Err(BindingError::invalid("unknown index kind")),
            };
            output(count, name.len() + 1)?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < name.len() + 1 {
                return Err(BindingError::invalid("name buffer too small"));
            }
            for (i, b) in name.bytes().chain(std::iter::once(0)).enumerate() {
                output(out.add(i), b as c_char)?;
            }
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_index_fixing(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    serial: i32,
    forecast: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let d = date(serial)?;
            let f = bool_value(forecast)?;
            output(
                out,
                match kind {
                    0 => zero_index(c, id)?.fixing(d, f)?,
                    1 => yoy_index(c, id)?.fixing(d, f)?,
                    _ => return Err(BindingError::invalid("unknown index kind")),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_index_add_fixing(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    serial: i32,
    value: f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !value.is_finite() {
                return Err(BindingError::invalid("nonfinite fixing"));
            }
            let d = date(serial)?;
            match kind {
                0 => zero_index(c, id)?.add_fixing(d, value)?,
                1 => yoy_index(c, id)?.add_fixing(d, value)?,
                _ => return Err(BindingError::invalid("unknown index kind")),
            };
            Ok(())
        })
    }
}
/// query 0 needs_forecast, 1 last_fixing_date serial, 2 ratio (YoY only).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_index_info(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    query: i32,
    serial: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = match kind {
                0 => {
                    let i = zero_index(c, id)?;
                    match query {
                        0 => i.needs_forecast(date(serial)?)? as i32,
                        1 => i.last_fixing_date()?.serial_number(),
                        _ => return Err(BindingError::invalid("unknown zero index query")),
                    }
                }
                1 => {
                    let i = yoy_index(c, id)?;
                    match query {
                        0 => i.needs_forecast(date(serial)?)? as i32,
                        1 => i.last_fixing_date()?.serial_number(),
                        2 => i.ratio() as i32,
                        _ => return Err(BindingError::invalid("unknown YoY index query")),
                    }
                }
                _ => return Err(BindingError::invalid("unknown index kind")),
            };
            output(out, v)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. The context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_index_link(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    curve: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            match kind {
                0 => c
                    .get::<ZeroIndex>(id)?
                    .curve
                    .link_to(zero_curve(c, curve)?.current_link()?),
                1 => c
                    .get::<YoyIndex>(id)?
                    .curve
                    .link_to(yoy_curve(c, curve)?.current_link()?),
                _ => return Err(BindingError::invalid("unknown index kind")),
            };
            Ok(())
        })
    }
}
