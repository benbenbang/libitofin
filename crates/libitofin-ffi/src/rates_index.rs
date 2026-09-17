//! Swap indexes share the same index and curve graph as their underlying swaps.
use crate::boundary::*;
use crate::rates_api::{curve, period, settings};
use crate::time_api::{bool_flag, calendar, convention, date, day_counter};
use libitofin::currency::Currency;
use libitofin::indexes::{Index, InterestRateIndex, SwapIndex};
use libitofin::shared::{Shared, shared};
use libitofin::types::Real;

#[repr(C)]
pub struct ItofinSwapIndexConfig {
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub settlement_days: u32,
    pub currency: u64,
    pub calendar: u64,
    pub fixed_length: i32,
    pub fixed_unit: i32,
    pub fixed_convention: i32,
    pub day_counter: u64,
    pub index: u64,
    pub discount: u64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swap_index_new(
    ctx: *mut Context,
    name: *const u8,
    name_len: usize,
    a: ItofinSwapIndexConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let name = std::str::from_utf8(input_slice(name, name_len)?)
                .map_err(|_| BindingError::invalid("index family must be UTF-8"))?
                .to_owned();
            if name.is_empty() {
                return Err(BindingError::invalid("index family required"));
            }
            let index = if a.discount == 0 {
                SwapIndex::new(
                    name,
                    period(a.tenor_length, a.tenor_unit)?,
                    a.settlement_days,
                    c.get::<Currency>(a.currency)?,
                    calendar(c, a.calendar)?,
                    period(a.fixed_length, a.fixed_unit)?,
                    convention(a.fixed_convention)?,
                    day_counter(c, a.day_counter)?,
                    crate::indexes_api::ibor_index(c, a.index)?,
                    settings(c, a.settings)?,
                )
            } else {
                SwapIndex::with_exogenous_discount(
                    name,
                    period(a.tenor_length, a.tenor_unit)?,
                    a.settlement_days,
                    c.get::<Currency>(a.currency)?,
                    calendar(c, a.calendar)?,
                    period(a.fixed_length, a.fixed_unit)?,
                    convention(a.fixed_convention)?,
                    day_counter(c, a.day_counter)?,
                    crate::indexes_api::ibor_index(c, a.index)?,
                    curve(c, a.discount)?,
                    settings(c, a.settings)?,
                )
            };
            output(out, c.insert(shared(index))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swap_index_fixing(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    forecast_today: u8,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.get::<Shared<SwapIndex>>(id)?
                    .fixing(date(serial)?, bool_flag(forecast_today)?)?,
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
pub unsafe extern "C" fn itofin_swap_index_currency(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let currency = c.get::<Shared<SwapIndex>>(id)?.currency().clone();
            output(out, c.insert(currency)?)
        })
    }
}
#[repr(C)]
pub struct ItofinSwapIndexDetails {
    pub fixed_length: i32,
    pub fixed_unit: i32,
    pub exogenous_discount: u8,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swap_index_details(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinSwapIndexDetails,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let index = c.get::<Shared<SwapIndex>>(id)?;
            let tenor = index.fixed_leg_tenor();
            use libitofin::time::timeunit::TimeUnit;
            let unit = match tenor.units() {
                TimeUnit::Days => 0,
                TimeUnit::Weeks => 1,
                TimeUnit::Months => 2,
                TimeUnit::Years => 3,
                _ => return Err(BindingError::invalid("unsupported index tenor unit")),
            };
            output(
                out,
                ItofinSwapIndexDetails {
                    fixed_length: tenor.length(),
                    fixed_unit: unit,
                    exogenous_discount: u8::from(index.exogenous_discount()),
                },
            )
        })
    }
}
