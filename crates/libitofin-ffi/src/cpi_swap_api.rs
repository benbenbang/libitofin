//! CPI-scaled fixed coupons exchanged against plain Ibor coupons.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;

use crate::float_float_swap_api::{check_coupon, check_schedule};
use crate::indexes_api::NativeIbor;
use crate::inflation_api::{bool_value, cpi_interpolation, zero_index};
use crate::rates_api::{curve, settings};
use crate::rates_api::{period, swap_type};
use crate::time_api::{convention, day_counter, schedule_calendar};
use libitofin::handle::Handle;
use libitofin::indexes::Index;
use libitofin::instrument::Instrument;
use libitofin::instruments::CpiSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, schedule::Schedule};
use std::cell::{Cell, RefCell};

type Flags = (bool, Option<bool>, bool, bool);
fn flags(s: &Settings<Date>) -> Flags {
    (
        s.include_reference_date_events(),
        s.include_todays_cash_flows(),
        s.enforces_todays_historic_fixings(),
        s.using_at_par_coupons(),
    )
}
#[derive(Clone)]
struct NativeCpiSwap {
    swap: SharedMut<CpiSwap>,
    settings: Shared<Settings<Date>>,
    index: NativeIbor,
    _schedules: [Schedule; 2],
    _calendars: [NativeCalendar; 2],
    discount: Shared<RefCell<Option<Handle<dyn YieldTermStructure>>>>,
    frozen: Shared<Cell<bool>>,
    flags: Shared<Cell<Flags>>,
}
impl NativeCpiSwap {
    fn prepare(&self, force: bool) -> BindingResult<()> {
        let s = self.swap.borrow();
        let f = flags(&self.settings);
        if !force && (self.frozen.get() || (s.base().is_calculated() && self.flags.get() == f)) {
            return Ok(());
        }
        if s.is_expired()? {
            return Ok(());
        }
        let Some(discount) = self.discount.borrow().clone() else {
            return Ok(());
        };
        let settlement = discount.current_link()?.reference_date()?;
        for flow in s.float_leg() {
            if flow.has_occurred(
                &self.settings,
                Some(settlement),
                Some(self.settings.include_reference_date_events()),
            )? {
                continue;
            }
            let Some(coupon) = flow.as_coupon() else {
                continue;
            };
            check_coupon(
                &self.index,
                coupon.accrual_start_date(),
                coupon.accrual_end_date(),
                &self.settings,
            )?;
        }
        Ok(())
    }
}
/// Constant nominal CPI-versus-Ibor swap. Payer pays Ibor and receives CPI.
#[repr(C)]
pub struct ItofinCpiSwapConfig {
    pub swap_type: i32,
    pub nominal: f64,
    pub inflation_nominal: f64,
    pub subtract_inflation_nominal: i32,
    pub spread: f64,
    pub float_schedule: u64,
    pub float_index: u64,
    pub float_day_counter: u64,
    pub float_convention: i32,
    pub fixed_rate: f64,
    pub base_cpi: f64,
    pub fixed_schedule: u64,
    pub fixed_index: u64,
    pub fixed_day_counter: u64,
    pub fixed_convention: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub settings: u64,
}
/// Retain live indexes and construct both coupon legs plus terminal exchanges.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_cpi_swap_new(
    ctx: *mut Context,
    cfg: *const ItofinCpiSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let shared_settings = settings(c, a.settings)?;
            let index = c.get::<NativeIbor>(a.float_index)?;
            let fixed_index = zero_index(c, a.fixed_index)?;
            for same in [
                std::ptr::eq(Index::settings(&*index.inner), &*shared_settings),
                std::ptr::eq(Index::settings(&*fixed_index), &*shared_settings),
            ] {
                if !same {
                    return Err(BindingError::invalid("indexes must share swap settings"));
                }
            }
            let schedules = [
                c.get::<Schedule>(a.fixed_schedule)?,
                c.get::<Schedule>(a.float_schedule)?,
            ];
            let calendars = [
                schedule_calendar(c, a.fixed_schedule)?,
                schedule_calendar(c, a.float_schedule)?,
            ];
            let fixed_roll = convention(a.fixed_convention)?;
            let float_roll = convention(a.float_convention)?;
            check_schedule(&schedules[1], &calendars[1], &index, Some(float_roll))?;
            crate::cpi_cashflows_api::check_leg_dates(
                &schedules[0],
                &calendars[0],
                &calendars[0],
                fixed_roll,
            )?;
            let swap = CpiSwap::new(
                swap_type(a.swap_type)?,
                a.nominal,
                a.inflation_nominal,
                bool_value(a.subtract_inflation_nominal)?,
                a.spread,
                day_counter(c, a.float_day_counter)?,
                schedules[1].clone(),
                float_roll,
                index.inner.clone(),
                a.fixed_rate,
                a.base_cpi,
                day_counter(c, a.fixed_day_counter)?,
                schedules[0].clone(),
                fixed_roll,
                period(a.lag_length, a.lag_unit)?,
                fixed_index,
                cpi_interpolation(a.interpolation)?,
                shared_settings.clone(),
            )?;
            let owner = NativeCpiSwap {
                swap: shared_mut(swap),
                flags: libitofin::shared::shared(Cell::new(flags(&shared_settings))),
                settings: shared_settings,
                index,
                _schedules: schedules,
                _calendars: calendars,
                discount: libitofin::shared::shared(RefCell::new(None)),
                frozen: libitofin::shared::shared(Cell::new(false)),
            };
            output(out, c.insert(owner)?)
        })
    }
}
/// Attach a live discount curve with native date/flow defaults and identical Settings.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_cpi_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    settings_id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            let owner = c.get::<NativeCpiSwap>(id)?;
            let s = settings(c, settings_id)?;
            if !Shared::ptr_eq(&owner.settings, &s) {
                return Err(BindingError::invalid("engine must share swap settings"));
            }
            let discount = curve(c, discount)?;
            let engine = shared_mut(DiscountingSwapEngine::new(
                discount.clone(),
                None,
                None,
                None,
                s,
            ));
            owner
                .swap
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(engine);
            *owner.discount.borrow_mut() = Some(discount);
            Ok(())
        })
    }
}
/// Query 0 NPV, 1 CPI NPV, 2 Ibor NPV, 3 Ibor BPS, 4 fair Ibor spread,
/// 5 cache state, 6 expiry. Failures leave output untouched.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_cpi_swap_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let owner = c.get::<NativeCpiSwap>(id)?;
            if !(0..=6).contains(&query) {
                return Err(BindingError::invalid("invalid CPI swap query"));
            }
            if query <= 4 {
                owner.prepare(false)?;
            }
            let mut s = owner.swap.borrow_mut();
            let value = match query {
                0 => s.npv()?,
                1 => s.fixed_leg_npv()?,
                2 => s.float_leg_npv()?,
                3 => s.float_leg_bps()?,
                4 => s.fair_spread()?,
                5 => f64::from(u8::from(s.base().is_calculated())),
                6 => f64::from(u8::from(s.is_expired()?)),
                _ => return Err(BindingError::invalid("invalid CPI swap query")),
            };
            if query <= 4 {
                owner.flags.set(flags(&owner.settings));
            }
            output(out, value)
        })
    }
}
/// Action 0 calculate, 1 recalculate, 2 freeze, 3 unfreeze.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_cpi_swap_action(
    ctx: *mut Context,
    id: u64,
    action: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            let owner = c.get::<NativeCpiSwap>(id)?;
            if !(0..=3).contains(&action) {
                return Err(BindingError::invalid("invalid CPI swap action"));
            }
            if action <= 1 {
                owner.prepare(action == 1)?;
            }
            let mut s = owner.swap.borrow_mut();
            match action {
                0 => s.calculate()?,
                1 => s.recalculate()?,
                2 => {
                    s.base_mut().freeze();
                    owner.frozen.set(true);
                }
                3 => {
                    s.base_mut().unfreeze();
                    owner.frozen.set(false);
                }
                _ => return Err(BindingError::invalid("invalid CPI swap action")),
            }
            if action <= 1 {
                owner.flags.set(flags(&owner.settings));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "cpi_swap_api_tests.rs"]
mod tests;
