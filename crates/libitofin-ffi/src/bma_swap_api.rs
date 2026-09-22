//! Municipal swap and bootstrap handles.
use crate::boundary::*;
use crate::indexes_api::ibor_index;
use crate::market_api::quote;
use crate::rates_api::{curve, finite, period, settings, swap_type};
use crate::time_api::{calendar, convention, day_counter};
use libitofin::indexes::BMAIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::BMASwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::BMASwapRateHelper;
use libitofin::time::schedule::Schedule;

/// Complete municipal swap conventions; payer pays BMA and receives Ibor.
#[repr(C)]
pub struct ItofinBmaSwapConfig {
    pub swap_type: i32,
    pub nominal: f64,
    pub libor_schedule: u64,
    pub libor_fraction: f64,
    pub libor_spread: f64,
    pub libor_index: u64,
    pub libor_day_counter: u64,
    pub bma_schedule: u64,
    pub bma_index: u64,
    pub bma_day_counter: u64,
    pub settings: u64,
}
/// Construct a municipal swap retaining both legs and their market dependencies.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_swap_new(
    ctx: *mut Context,
    cfg: *const ItofinBmaSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let swap = BMASwap::new(
                swap_type(a.swap_type)?,
                finite(a.nominal)?,
                c.get::<Schedule>(a.libor_schedule)?,
                finite(a.libor_fraction)?,
                finite(a.libor_spread)?,
                ibor_index(c, a.libor_index)?,
                day_counter(c, a.libor_day_counter)?,
                c.get::<Schedule>(a.bma_schedule)?,
                c.get::<Shared<BMAIndex>>(a.bma_index)?,
                day_counter(c, a.bma_day_counter)?,
                settings(c, a.settings)?,
            )?;
            output(out, c.insert(shared_mut(swap))?)
        })
    }
}
/// Attach a retained discounting engine with settings-driven defaults.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    settings_id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let swap = c.get::<SharedMut<BMASwap>>(id)?;
            let engine = shared_mut(DiscountingSwapEngine::new(
                curve(c, discount)?,
                None,
                None,
                None,
                settings(c, settings_id)?,
            ));
            swap.borrow_mut().base_mut().set_pricing_engine(engine);
            Ok(())
        })
    }
}
/// Query zero: NPV; one: fair Ibor fraction; two: fair Ibor spread; three: cached flag;
/// four/five: Ibor/BMA leg NPV; six/seven: Ibor/BMA leg BPS.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_swap_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let swap = c.get::<SharedMut<BMASwap>>(id)?;
            let mut swap = swap.borrow_mut();
            output(
                out,
                match query {
                    0 => swap.npv()?,
                    1 => swap.fair_libor_fraction()?,
                    2 => swap.fair_libor_spread()?,
                    3 => {
                        if swap.base().is_calculated() {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    4 | 5 => swap.swap_mut().leg_npv((query - 4) as usize)?,
                    6 | 7 => swap.swap_mut().leg_bps((query - 6) as usize)?,
                    _ => return Err(BindingError::invalid("unknown BMA swap query")),
                },
            )
        })
    }
}
/// Conventions for the quoted municipal-to-Ibor fraction helper.
#[repr(C)]
pub struct ItofinBmaHelperConfig {
    pub quote: u64,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub bma_length: i32,
    pub bma_unit: i32,
    pub bma_convention: i32,
    pub bma_day_counter: u64,
    pub bma_index: u64,
    pub libor_index: u64,
}
/// Construct a rate helper usable by existing piecewise yield curves.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bma_helper_new(
    ctx: *mut Context,
    cfg: *const ItofinBmaHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let helper = BMASwapRateHelper::new(
                quote(c, a.quote)?,
                period(a.tenor_length, a.tenor_unit)?,
                a.settlement_days,
                calendar(c, a.calendar)?,
                period(a.bma_length, a.bma_unit)?,
                convention(a.bma_convention)?,
                day_counter(c, a.bma_day_counter)?,
                &c.get::<Shared<BMAIndex>>(a.bma_index)?,
                &ibor_index(c, a.libor_index)?,
            )?;
            output(out, c.insert(helper as Shared<dyn RateHelper>)?)
        })
    }
}
