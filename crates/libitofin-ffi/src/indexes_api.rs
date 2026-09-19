//! Currency and interest-rate indexes, retaining core fixings and calendar behavior.
use crate::boundary::*;
use crate::curves_api::optional_curve;
use crate::settings_api::settings;
use crate::time_api::{calendar, convention, date, day_counter};
use libitofin::currency::Currency;
use libitofin::indexes::ibor::{CustomIborIndex, EurLibor, GbpLibor, JpyLibor};
use libitofin::indexes::{
    Eonia, Estr, Euribor, IborIndex, Index, InterestRateIndex, OvernightIndex, UsdLibor,
};
use libitofin::shared::{Shared, shared};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, period::Period, timeunit::TimeUnit,
};

#[derive(Clone)]
pub(crate) struct NativeIbor {
    inner: Shared<IborIndex>,
    fixing_calendar: crate::calendar_api::NativeCalendar,
}
impl NativeIbor {
    pub(crate) fn builtin(inner: Shared<IborIndex>) -> Self {
        let fixing_calendar = crate::calendar_api::NativeCalendar {
            inner: inner.fixing_calendar(),
            horizon: None,
            first_year: 1901,
        };
        Self {
            inner,
            fixing_calendar,
        }
    }
}
pub(crate) fn ibor_index(c: &Context, id: u64) -> BindingResult<Shared<IborIndex>> {
    Ok(c.get::<NativeIbor>(id)?.inner)
}

pub(crate) fn period(length: i32, unit: i32) -> BindingResult<Period> {
    Ok(Period::new(
        length,
        match unit {
            0 => TimeUnit::Days,
            1 => TimeUnit::Weeks,
            2 => TimeUnit::Months,
            3 => TimeUnit::Years,
            _ => return Err(BindingError::invalid("unknown period unit")),
        },
    ))
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_currency_new(
    ctx: *mut Context,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = match kind {
                0 => Currency::eur(),
                1 => Currency::usd(),
                2 => Currency::gbp(),
                3 => Currency::jpy(),
                _ => return Err(BindingError::invalid("unknown currency")),
            };
            output(out, c.insert(v)?)
        })
    }
}
/// Fixed-size output containing the three ASCII ISO currency letters and a NUL byte.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_currency_code(
    ctx: *mut Context,
    id: u64,
    out: *mut u8,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<Currency>(id)?;
            if capacity < 4 {
                return Err(BindingError::invalid("currency buffer needs four bytes"));
            };
            check_ptr(out)?;
            for (i, b) in v.code().bytes().chain([0]).enumerate() {
                output(out.add(i), b)?;
            }
            Ok(())
        })
    }
}
/// Family: 0 Euribor, 1 USD Libor, 2 JPY Libor, 3 GBP Libor, 4 EUR Libor.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_family_new(
    ctx: *mut Context,
    family: i32,
    length: i32,
    unit: i32,
    forwarding: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let t = period(length, unit)?;
            let f = optional_curve(c, forwarding)?;
            let s = settings(c, settings_id)?;
            let v = match family {
                0 => shared(Euribor::new(t, f, s)?),
                1 => shared(UsdLibor::new(t, f, s)?),
                2 => shared(JpyLibor::new(t, f, s)?),
                3 => shared(GbpLibor::new(t, f, s)?),
                4 => EurLibor::new(t, f, s)?.upcast(),
                _ => return Err(BindingError::invalid("unknown Ibor family")),
            };
            output(out, c.insert(NativeIbor::builtin(v))?)
        })
    }
}
#[repr(C)]
pub struct ItofinIborConfig {
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub settlement_days: u32,
    pub currency: u64,
    pub fixing_calendar: u64,
    pub value_calendar: u64,
    pub maturity_calendar: u64,
    pub convention: i32,
    pub end_of_month: bool,
    pub day_counter: u64,
    pub forwarding: u64,
    pub settings: u64,
}
/// custom=true uses distinct fixing/value/maturity calendars.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_new(
    ctx: *mut Context,
    name: *const u8,
    name_len: usize,
    cfg: *const ItofinIborConfig,
    custom: bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let name = std::str::from_utf8(input_slice(name, name_len)?)
                .map_err(|_| BindingError::invalid("index name must be UTF-8"))?
                .to_owned();
            let t = period(a.tenor_length, a.tenor_unit)?;
            let cur = c.get::<Currency>(a.currency)?;
            let fixing_calendar =
                c.get::<crate::calendar_api::NativeCalendar>(a.fixing_calendar)?;
            let cal = fixing_calendar.inner.clone();
            let bdc = convention(a.convention)?;
            let dc = day_counter(c, a.day_counter)?;
            let f = optional_curve(c, a.forwarding)?;
            let s = settings(c, a.settings)?;
            let v = if custom {
                CustomIborIndex::new(
                    name,
                    t,
                    a.settlement_days,
                    cur,
                    cal,
                    calendar(c, a.value_calendar)?,
                    calendar(c, a.maturity_calendar)?,
                    bdc,
                    a.end_of_month,
                    dc,
                    f,
                    s,
                )
                .upcast()
            } else {
                shared(IborIndex::new(
                    name,
                    t,
                    a.settlement_days,
                    cur,
                    cal,
                    bdc,
                    a.end_of_month,
                    dc,
                    f,
                    s,
                ))
            };
            output(
                out,
                c.insert(NativeIbor {
                    inner: v,
                    fixing_calendar,
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
pub unsafe extern "C" fn itofin_estr_new(
    ctx: *mut Context,
    forwarding: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = shared(Estr::new(
                optional_curve(c, forwarding)?,
                settings(c, settings_id)?,
            ));
            output(out, c.insert(v)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_eonia_new(
    ctx: *mut Context,
    forwarding: u64,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = shared(Eonia::new(
                optional_curve(c, forwarding)?,
                settings(c, settings_id)?,
            ));
            output(out, c.insert(v)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_index_fixing(
    ctx: *mut Context,
    id: u64,
    overnight: bool,
    serial: i32,
    forecast_today: bool,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let d = date(serial)?;
            let v = if overnight {
                c.get::<Shared<OvernightIndex>>(id)?
                    .fixing(d, forecast_today)?
            } else {
                crate::indexes_api::ibor_index(c, id)?.fixing(d, forecast_today)?
            };
            output(out, v)
        })
    }
}
/// Query 0 value date; 1 fixing date; 2 maturity date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_date(
    ctx: *mut Context,
    id: u64,
    query: i32,
    serial: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = crate::indexes_api::ibor_index(c, id)?;
            let d = date(serial)?;
            let d = match query {
                0 => v.value_date(d)?,
                1 => v.fixing_date(d),
                2 => v.maturity_date(d)?,
                _ => return Err(BindingError::invalid("unknown index date query")),
            };
            output(out, d.serial_number())
        })
    }
}
/// Query 0 day counter; 1 fixing calendar; 2 currency. Output is a newly owned handle.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_component(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = c.get::<NativeIbor>(id)?;
            let v = index.inner;
            let id = match query {
                0 => c.insert(v.day_counter().clone())?,
                1 => c.insert(index.fixing_calendar)?,
                2 => c.insert(v.currency().clone())?,
                _ => return Err(BindingError::invalid("unknown index component")),
            };
            output(out, id)
        })
    }
}
#[repr(C)]
pub struct ItofinIborInfo {
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub convention: i32,
    pub end_of_month: bool,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_info(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinIborInfo,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = crate::indexes_api::ibor_index(c, id)?;
            let tenor = v.tenor();
            let unit = match tenor.units() {
                TimeUnit::Days => 0,
                TimeUnit::Weeks => 1,
                TimeUnit::Months => 2,
                TimeUnit::Years => 3,
                _ => return Err(BindingError::invalid("unsupported index tenor unit")),
            };
            let conv = match v.business_day_convention() {
                BusinessDayConvention::ModifiedFollowing => 0,
                BusinessDayConvention::Following => 1,
                BusinessDayConvention::Unadjusted => 2,
                BusinessDayConvention::Preceding => 3,
                BusinessDayConvention::ModifiedPreceding => 4,
                BusinessDayConvention::HalfMonthModifiedFollowing => 5,
                BusinessDayConvention::Nearest => 6,
            };
            output(
                out,
                ItofinIborInfo {
                    tenor_length: tenor.length(),
                    tenor_unit: unit,
                    convention: conv,
                    end_of_month: v.end_of_month(),
                },
            )
        })
    }
}
/// UTF-8, caller-owned buffer; first query capacity=0 to obtain length excluding NUL.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_name(
    ctx: *mut Context,
    id: u64,
    out: *mut u8,
    capacity: usize,
    length: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(length)?;
            let name = crate::indexes_api::ibor_index(c, id)?.name();
            output(length, name.len())?;
            if capacity == 0 {
                return Ok(());
            };
            if capacity <= name.len() {
                return Err(BindingError::invalid("index name buffer too small"));
            };
            check_ptr(out)?;
            for (i, b) in name.bytes().chain([0]).enumerate() {
                output(out.add(i), b)?;
            }
            Ok(())
        })
    }
}

/// Components: 0 day counter, 1 fixing calendar, 2 currency. Returned handles
/// retain their values independently of the overnight index.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles must belong
/// to the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_overnight_component(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = c.get::<Shared<OvernightIndex>>(id)?;
            let value = match query {
                0 => c.insert(index.day_counter().clone())?,
                1 => c.insert(crate::calendar_api::NativeCalendar {
                    inner: index.fixing_calendar(),
                    horizon: None,
                    first_year: 1901,
                })?,
                2 => c.insert(index.currency().clone())?,
                _ => return Err(BindingError::invalid("unknown overnight index component")),
            };
            output(out, value)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles must belong
/// to the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_overnight_fixing_days(
    ctx: *mut Context,
    id: u64,
    out: *mut u32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.get::<Shared<OvernightIndex>>(id)?.fixing_days())
        })
    }
}
