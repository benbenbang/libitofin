//! Inflation bootstrap helpers retain quotes and their native instrument graphs.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, yoy_index, zero_index};
use crate::time_api::{calendar, convention, date, day_counter, time_unit};
use libitofin::handle::Handle;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::Shared;
use libitofin::termstructures::inflation::inflationhelpers::{
    YearOnYearInflationSwapHelper, YoYInflationHelper, ZeroCouponInflationSwapHelper,
    ZeroInflationHelper,
};
use libitofin::termstructures::yields::Pillar;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, period::Period};
#[derive(Clone)]
pub(crate) struct ZeroHelper {
    pub inner: Shared<dyn ZeroInflationHelper>,
    concrete: Shared<ZeroCouponInflationSwapHelper>,
}
#[derive(Clone)]
pub(crate) struct YoyHelper {
    pub inner: Shared<dyn YoYInflationHelper>,
}
#[repr(C)]
pub struct ItofinInflationHelperConfig {
    pub quote: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub maturity: i32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub index: u64,
    pub interpolation: i32,
    pub discount: u64,
    pub settings: u64,
    pub pillar: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_helper_new(
    ctx: *mut Context,
    a: *const ItofinInflationHelperConfig,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(a)?;
            check_ptr(out)?;
            let a = &*a;
            let q: Handle<dyn Quote> = Handle::new(c.get::<Shared<SimpleQuote>>(a.quote)?);
            let lag = Period::new(a.lag_length, time_unit(a.lag_unit)?);
            let maturity = date(a.maturity)?;
            let cal = calendar(c, a.calendar)?;
            let conv = convention(a.convention)?;
            let dc = day_counter(c, a.day_counter)?;
            let interpolation = cpi_interpolation(a.interpolation)?;
            let settings = c.get::<Shared<Settings<Date>>>(a.settings)?;
            let pillar = match a.pillar {
                0 => Pillar::MaturityDate,
                1 => Pillar::LastRelevantDate,
                _ => return Err(BindingError::invalid("unknown pillar")),
            };
            let id = match kind {
                0 => {
                    let p = ZeroCouponInflationSwapHelper::new(
                        q,
                        lag,
                        maturity,
                        cal,
                        conv,
                        dc,
                        &zero_index(c, a.index)?,
                        interpolation,
                        pillar,
                        settings,
                    )?;
                    c.insert(ZeroHelper {
                        inner: p.clone(),
                        concrete: p,
                    })?
                }
                1 => {
                    let p = YearOnYearInflationSwapHelper::new(
                        q,
                        lag,
                        maturity,
                        cal,
                        conv,
                        dc,
                        &yoy_index(c, a.index)?,
                        interpolation,
                        c.get::<Handle<dyn YieldTermStructure>>(a.discount)?,
                        pillar,
                        settings,
                    )?;
                    c.insert(YoyHelper { inner: p })?
                }
                _ => return Err(BindingError::invalid("unknown inflation helper kind")),
            };
            output(out, id)
        })
    }
}
/// query 0 pillar, 1 latest, 2 inflation fixing date (zero only).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_inflation_helper_date(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    query: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let d = match kind {
                0 => {
                    let h = c.get::<ZeroHelper>(id)?;
                    match query {
                        0 => h.inner.pillar_date(),
                        1 => h.inner.latest_date(),
                        2 => {
                            let swap = h.concrete.swap();
                            let swap = swap.as_ref().map_err(|e| BindingError::from(e.clone()))?;
                            swap.inflation_cash_flow().fixing_date()
                        }
                        _ => return Err(BindingError::invalid("unknown helper date")),
                    }
                }
                1 => {
                    let h = c.get::<YoyHelper>(id)?;
                    match query {
                        0 => h.inner.pillar_date(),
                        1 => h.inner.latest_date(),
                        _ => return Err(BindingError::invalid("unknown helper date")),
                    }
                }
                _ => return Err(BindingError::invalid("unknown inflation helper kind")),
            };
            output(out, d.serial_number())
        })
    }
}
