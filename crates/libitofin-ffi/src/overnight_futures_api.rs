//! Engine-free overnight futures and their bootstrap helpers.
use crate::boundary::*;
use crate::market_api::quote;
use crate::time_api::{date, frequency};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::OvernightIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::OvernightIndexFuture;
use libitofin::quotes::Quote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::yields::{
    OvernightIndexFutureRateHelper, Pillar, SofrFutureRateHelper,
};
use libitofin::time::date::{Date, Month};

#[repr(C)]
pub struct ItofinOvernightFutureConfig {
    pub index: u64,
    pub value_date: i32,
    pub maturity_date: i32,
    pub convexity: u64,
    /// Zero simple, one compound.
    pub averaging: i32,
}

fn averaging(value: i32) -> BindingResult<RateAveraging> {
    match value {
        0 => Ok(RateAveraging::Simple),
        1 => Ok(RateAveraging::Compound),
        _ => Err(BindingError::invalid("invalid averaging method")),
    }
}
fn convexity(c: &Context, id: u64) -> BindingResult<Handle<dyn Quote>> {
    if id == 0 {
        Ok(Handle::empty())
    } else {
        quote(c, id)
    }
}
fn pillar(value: i32, custom: i32) -> BindingResult<Pillar> {
    match (value, custom) {
        (0, 0) => Ok(Pillar::MaturityDate),
        (1, 0) => Ok(Pillar::LastRelevantDate),
        (2, _) => Ok(Pillar::CustomDate(date(custom)?)),
        _ => Err(BindingError::invalid(
            "invalid pillar or unexpected custom date",
        )),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_overnight_future_new(
    ctx: *mut Context,
    a: ItofinOvernightFutureConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let future = OvernightIndexFuture::new(
                c.get::<Shared<OvernightIndex>>(a.index)?,
                date(a.value_date)?,
                date(a.maturity_date)?,
                convexity(c, a.convexity)?,
                averaging(a.averaging)?,
            )?;
            output(out, c.insert(shared_mut(future))?)
        })
    }
}

/// Query zero NPV, one convexity adjustment, two expired (zero or one).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_overnight_future_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let future = c.get::<SharedMut<OvernightIndexFuture>>(id)?;
            let mut future = future.borrow_mut();
            let value = match field {
                0 => future.npv()?,
                1 => future.convexity_adjustment()?,
                2 => f64::from(future.is_expired()?),
                _ => return Err(BindingError::invalid("invalid futures field")),
            };
            output(out, value)
        })
    }
}

/// Query zero value date, one maturity date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_overnight_future_date(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let future = c.get::<SharedMut<OvernightIndexFuture>>(id)?;
            let future = future.borrow();
            let value = match field {
                0 => future.value_date(),
                1 => future.maturity_date(),
                _ => return Err(BindingError::invalid("invalid futures date field")),
            };
            output(out, value.serial_number())
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_overnight_future_helper_new(
    ctx: *mut Context,
    a: ItofinOvernightFutureConfig,
    price: u64,
    pillar_choice: i32,
    custom_date: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let helper = OvernightIndexFutureRateHelper::new(
                quote(c, price)?,
                date(a.value_date)?,
                date(a.maturity_date)?,
                &c.get::<Shared<OvernightIndex>>(a.index)?,
                convexity(c, a.convexity)?,
                averaging(a.averaging)?,
                pillar(pillar_choice, custom_date)?,
            )?;
            output(out, c.insert(helper as Shared<dyn RateHelper>)?)
        })
    }
}

#[repr(C)]
pub struct ItofinSofrFutureHelperConfig {
    pub price: u64,
    pub month: u32,
    pub year: i32,
    pub frequency: i32,
    pub settings: u64,
    pub convexity: u64,
    pub pillar: i32,
    pub custom_date: i32,
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_sofr_future_helper_new(
    ctx: *mut Context,
    a: ItofinSofrFutureHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !(1..=12).contains(&a.month) {
                return Err(BindingError::invalid("month outside 1-12"));
            }
            let helper = SofrFutureRateHelper::new(
                quote(c, a.price)?,
                Month::from_ordinal(a.month as i32),
                a.year,
                frequency(a.frequency)?,
                convexity(c, a.convexity)?,
                pillar(a.pillar, a.custom_date)?,
                c.get::<Shared<Settings<Date>>>(a.settings)?,
            )?;
            output(out, c.insert(helper as Shared<dyn RateHelper>)?)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_sofr_new(
    ctx: *mut Context,
    forwarding: u64,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = libitofin::indexes::ibor::Sofr::new(
                crate::curves_api::optional_curve(c, forwarding)?,
                crate::settings_api::settings(c, settings)?,
            );
            output(out, c.insert(libitofin::shared::shared(index))?)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers follow the crate-level C caller contract.
pub unsafe extern "C" fn itofin_overnight_add_fixing(
    ctx: *mut Context,
    index: u64,
    fixing_date: i32,
    value: f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            use libitofin::indexes::Index;
            c.get::<Shared<OvernightIndex>>(index)?
                .add_fixing(date(fixing_date)?, crate::rates_api::finite(value)?)?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "overnight_futures_tests.rs"]
mod tests;
