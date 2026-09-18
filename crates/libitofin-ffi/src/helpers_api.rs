//! Bootstrap helper adapters retaining the original observable market quotes.
use crate::boundary::*;
use crate::curves_api::optional_curve;
use crate::indexes_api::period;
use crate::market_api::quote;
use crate::settings_api::settings;
use crate::time_api::{calendar, convention, date, day_counter, frequency};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::OvernightIndex;
use libitofin::instruments::{BondPriceType, FuturesType};
use libitofin::quotes::Quote;
use libitofin::shared::Shared;
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{
    DepositRateHelper, FixedRateBondHelper, FraRateHelper, FuturesRateHelper, OISRateHelper,
    Pillar, SwapRateHelper,
};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, calendars::nullcalendar::NullCalendar,
    schedule::Schedule,
};

pub(crate) fn helper(c: &Context, id: u64) -> BindingResult<Shared<dyn RateHelper>> {
    match c.get::<Shared<dyn RateHelper>>(id) {
        Ok(v) => Ok(v),
        Err(_) => Ok(c.get::<Shared<FuturesRateHelper>>(id)? as Shared<dyn RateHelper>),
    }
}
fn optional_quote(c: &Context, id: u64) -> BindingResult<Handle<dyn Quote>> {
    if id == 0 {
        Ok(Handle::empty())
    } else {
        quote(c, id)
    }
}
fn pillar(value: i32) -> BindingResult<Pillar> {
    match value {
        0 => Ok(Pillar::MaturityDate),
        1 => Ok(Pillar::LastRelevantDate),
        _ => Err(BindingError::invalid("unknown pillar")),
    }
}
fn futures_type(value: i32) -> BindingResult<FuturesType> {
    match value {
        0 => Ok(FuturesType::Imm),
        1 => Ok(FuturesType::Asx),
        2 => Ok(FuturesType::Custom),
        _ => Err(BindingError::invalid("unknown futures type")),
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_deposit_helper_new(
    ctx: *mut Context,
    quote_id: u64,
    rate: f64,
    from_rate: bool,
    index: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let i = crate::indexes_api::ibor_index(c, index)?;
            let v = if from_rate {
                DepositRateHelper::from_rate(rate, &i)
            } else {
                DepositRateHelper::new(quote(c, quote_id)?, &i)
            };
            output(out, c.insert(v as Shared<dyn RateHelper>)?)
        })
    }
}
#[repr(C)]
pub struct ItofinSwapHelperConfig {
    pub quote: u64,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub calendar: u64,
    pub frequency: i32,
    pub convention: i32,
    pub day_counter: u64,
    pub index: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swap_helper_new(
    ctx: *mut Context,
    cfg: *const ItofinSwapHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let i = crate::indexes_api::ibor_index(c, a.index)?;
            let v = SwapRateHelper::new(
                quote(c, a.quote)?,
                period(a.tenor_length, a.tenor_unit)?,
                calendar(c, a.calendar)?,
                frequency(a.frequency)?,
                convention(a.convention)?,
                day_counter(c, a.day_counter)?,
                &i,
            );
            output(out, c.insert(v as Shared<dyn RateHelper>)?)
        })
    }
}
#[repr(C)]
pub struct ItofinFraHelperConfig {
    pub quote: u64,
    pub rate: f64,
    pub start_length: i32,
    pub start_unit: i32,
    pub months: u32,
    pub start_date: i32,
    pub end_date: i32,
    pub index: u64,
    pub indexed: bool,
    pub pillar: i32,
}
/// Mode 0 quote+period, 1 fixed rate+period, 2 months, 3 explicit dates.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_fra_helper_new(
    ctx: *mut Context,
    mode: i32,
    cfg: *const ItofinFraHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let i = crate::indexes_api::ibor_index(c, a.index)?;
            let p = pillar(a.pillar)?;
            let v = match mode {
                0 => FraRateHelper::new(
                    quote(c, a.quote)?,
                    period(a.start_length, a.start_unit)?,
                    &i,
                    a.indexed,
                    p,
                ),
                1 => FraRateHelper::from_rate(
                    a.rate,
                    period(a.start_length, a.start_unit)?,
                    &i,
                    a.indexed,
                    p,
                ),
                2 => FraRateHelper::from_months(quote(c, a.quote)?, a.months, &i, a.indexed, p),
                3 => FraRateHelper::from_dates(
                    quote(c, a.quote)?,
                    date(a.start_date)?,
                    date(a.end_date)?,
                    &i,
                    a.indexed,
                    p,
                ),
                _ => return Err(BindingError::invalid("unknown FRA constructor")),
            };
            output(out, c.insert(v as Shared<dyn RateHelper>)?)
        })
    }
}
#[repr(C)]
pub struct ItofinFuturesHelperConfig {
    pub price: u64,
    pub start_date: i32,
    pub end_date: i32,
    pub has_end_date: bool,
    pub months: u32,
    pub calendar: u64,
    pub convention: i32,
    pub end_of_month: bool,
    pub day_counter: u64,
    pub convexity: u64,
    pub futures_type: i32,
    pub index: u64,
}
/// Mode 0 tenor months, 1 explicit/optional end date, 2 index conventions.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_futures_helper_new(
    ctx: *mut Context,
    mode: i32,
    cfg: *const ItofinFuturesHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { itofin_futures_helper_new_with_observation(ctx, mode, cfg, true, out, error) }
}
/// Futures helper with explicit convexity observation. Disable for bootstrap variables.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_futures_helper_new_with_observation(
    ctx: *mut Context,
    mode: i32,
    cfg: *const ItofinFuturesHelperConfig,
    observe_convexity: bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let q = quote(c, a.price)?;
            let d = date(a.start_date)?;
            let adj = if a.convexity != 0 && !observe_convexity {
                Handle::new_unregistered(
                    c.get::<Shared<libitofin::quotes::SimpleQuote>>(a.convexity)?
                        as Shared<dyn libitofin::quotes::Quote>,
                )
            } else {
                optional_quote(c, a.convexity)?
            };
            let ft = futures_type(a.futures_type)?;
            let v = match mode {
                0 => FuturesRateHelper::new(
                    q,
                    d,
                    a.months,
                    calendar(c, a.calendar)?,
                    convention(a.convention)?,
                    a.end_of_month,
                    day_counter(c, a.day_counter)?,
                    adj,
                    ft,
                )?,
                1 => FuturesRateHelper::from_end_date(
                    q,
                    d,
                    if a.has_end_date {
                        Some(date(a.end_date)?)
                    } else {
                        None
                    },
                    day_counter(c, a.day_counter)?,
                    adj,
                    ft,
                )?,
                2 => {
                    let i = crate::indexes_api::ibor_index(c, a.index)?;
                    FuturesRateHelper::from_index(q, d, &i, adj, ft)?
                }
                _ => return Err(BindingError::invalid("unknown futures constructor")),
            };
            output(out, c.insert(v)?)
        })
    }
}
#[repr(C)]
pub struct ItofinOisHelperConfig {
    pub settlement_days: u32,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub quote: u64,
    pub index: u64,
    pub payment_lag: i32,
    pub convention: i32,
    pub frequency: i32,
    pub forward_length: i32,
    pub forward_unit: i32,
    pub settings: u64,
    pub discounting: u64,
    pub spread: u64,
    pub pillar: i32,
    pub averaging: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ois_helper_new(
    ctx: *mut Context,
    cfg: *const ItofinOisHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let i = c.get::<Shared<OvernightIndex>>(a.index)?;
            let averaging = match a.averaging {
                0 => RateAveraging::Simple,
                1 => RateAveraging::Compound,
                _ => return Err(BindingError::invalid("unknown averaging method")),
            };
            let v = OISRateHelper::new(
                a.settlement_days,
                period(a.tenor_length, a.tenor_unit)?,
                quote(c, a.quote)?,
                &i,
                if a.discounting == 0 {
                    None
                } else {
                    Some(optional_curve(c, a.discounting)?)
                },
                a.payment_lag,
                convention(a.convention)?,
                frequency(a.frequency)?,
                period(a.forward_length, a.forward_unit)?,
                optional_quote(c, a.spread)?,
                pillar(a.pillar)?,
                averaging,
                settings(c, a.settings)?,
            );
            output(out, c.insert(v as Shared<dyn RateHelper>)?)
        })
    }
}
#[repr(C)]
pub struct ItofinBondHelperConfig {
    pub price: u64,
    pub settlement_days: u32,
    pub face_amount: f64,
    pub schedule: u64,
    pub day_counter: u64,
    pub convention: i32,
    pub redemption: f64,
    pub price_type: i32,
    pub settings: u64,
    pub issue_date: i32,
    pub has_issue_date: bool,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_bond_helper_new(
    ctx: *mut Context,
    cfg: *const ItofinBondHelperConfig,
    coupons: *const f64,
    len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let price_type = match a.price_type {
                0 => BondPriceType::Clean,
                1 => BondPriceType::Dirty,
                _ => return Err(BindingError::invalid("unknown bond price type")),
            };
            let v = FixedRateBondHelper::new(
                quote(c, a.price)?,
                a.settlement_days,
                a.face_amount,
                c.get::<Schedule>(a.schedule)?,
                input_slice(coupons, len)?.to_vec(),
                day_counter(c, a.day_counter)?,
                convention(a.convention)?,
                a.redemption,
                if a.has_issue_date {
                    Some(date(a.issue_date)?)
                } else {
                    None
                },
                None,
                None,
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
                price_type,
                settings(c, a.settings)?,
            )?;
            output(out, c.insert(v as Shared<dyn RateHelper>)?)
        })
    }
}
/// Query 0 implied quote, 1 quote error, 2 market quote, 3 futures convexity adjustment.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_helper_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = match query {
                0 => helper(c, id)?.implied_quote()?,
                1 => helper(c, id)?.quote_error()?,
                2 => helper(c, id)?.base().quote_value()?,
                3 => c
                    .get::<Shared<FuturesRateHelper>>(id)?
                    .convexity_adjustment()?,
                _ => return Err(BindingError::invalid("unknown helper query")),
            };
            output(out, v)
        })
    }
}
/// Query 0 maturity, 1 pillar, 2 earliest, 3 latest, 4 latest relevant date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_helper_date(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = helper(c, id)?;
            let d = match query {
                0 => v.maturity_date(),
                1 => v.pillar_date(),
                2 => v.earliest_date(),
                3 => v.latest_date(),
                4 => v.latest_relevant_date(),
                _ => return Err(BindingError::invalid("unknown helper date")),
            };
            output(out, d.serial_number())
        })
    }
}
