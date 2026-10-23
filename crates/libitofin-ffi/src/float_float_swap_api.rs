//! Plain common-notional two-Ibor swaps with a dedicated typed owner.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::floating_bond_api::advance;
use crate::indexes_api::NativeIbor;
use crate::rates_api::{curve, settings};
use crate::time_api::{convention, day_counter, schedule_calendar};
use libitofin::handle::Handle;
use libitofin::indexes::{Index, InterestRateIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::FloatFloatSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc, date::Date, schedule::Schedule,
    timeunit::TimeUnit,
};
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
struct NativeFloatFloat {
    swap: SharedMut<FloatFloatSwap>,
    settings: Shared<Settings<Date>>,
    indexes: [NativeIbor; 2],
    _schedules: [Schedule; 2],
    _calendars: [NativeCalendar; 2],
    discount: Shared<RefCell<Option<Handle<dyn YieldTermStructure>>>>,
    frozen: Shared<Cell<bool>>,
    flags: Shared<Cell<Flags>>,
}
impl NativeFloatFloat {
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
        for (leg, index) in s.swap().legs().iter().zip(&self.indexes) {
            for flow in leg {
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
                    index,
                    coupon.accrual_start_date(),
                    coupon.accrual_end_date(),
                    &self.settings,
                )?;
            }
        }
        Ok(())
    }
}
pub(crate) fn check_coupon(
    index: &NativeIbor,
    start: Date,
    end: Date,
    settings: &Settings<Date>,
) -> BindingResult<()> {
    let days = i32::try_from(index.inner.fixing_days())
        .map_err(|_| BindingError::invalid("index fixing days overflow"))?;
    let fixing = advance(
        &index.fixing_calendar,
        start,
        -days,
        TimeUnit::Days,
        Bdc::Preceding,
        false,
    )?;
    index.fixing_calendar.checked(fixing)?;
    let Some(today) = settings.evaluation_date() else {
        return Ok(());
    };
    if fixing < today
        || (fixing == today
            && (settings.enforces_todays_historic_fixings()
                || index.inner.has_historical_fixing(fixing)))
    {
        return Ok(());
    }
    let value = advance(
        &index.fixing_calendar,
        fixing,
        days,
        TimeUnit::Days,
        Bdc::Following,
        false,
    )?;
    let tenor = index.inner.tenor();
    let maturity = |value| {
        advance(
            &index.maturity_calendar,
            value,
            tenor.length(),
            tenor.units(),
            index.inner.business_day_convention(),
            index.inner.end_of_month(),
        )
    };
    maturity(value)?;
    let natural = advance(
        &index.value_calendar,
        fixing,
        days,
        TimeUnit::Days,
        Bdc::Following,
        false,
    )?;
    maturity(crate::time_api::adjust(
        &index.maturity_calendar,
        natural,
        Bdc::Following,
    )?)?;
    if settings.using_at_par_coupons() {
        let next = advance(
            &index.fixing_calendar,
            end,
            -days,
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        advance(
            &index.fixing_calendar,
            next,
            days,
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        crate::time_api::date(
            value
                .serial_number()
                .checked_add(1)
                .ok_or_else(|| BindingError::invalid("coupon value date overflow"))?,
        )?;
    }
    Ok(())
}
pub(crate) fn check_schedule(
    schedule: &Schedule,
    calendar: &NativeCalendar,
    index: &NativeIbor,
    payment: Option<Bdc>,
) -> BindingResult<()> {
    let days = i32::try_from(index.inner.fixing_days())
        .map_err(|_| BindingError::invalid("index fixing days overflow"))?;
    for pair in schedule.dates().windows(2) {
        advance(
            calendar,
            pair[1],
            0,
            TimeUnit::Days,
            payment.unwrap_or_else(|| schedule.business_day_convention()),
            false,
        )?;
        advance(
            &index.fixing_calendar,
            pair[0],
            -days,
            TimeUnit::Days,
            Bdc::Preceding,
            false,
        )?;
    }
    if schedule.len() >= 2 && schedule.has_tenor() && schedule.has_is_regular() {
        let tenor = schedule.tenor();
        for (position, anchor, amount) in [
            (1, schedule.date(1), -tenor.length()),
            (
                schedule.len() - 1,
                schedule.date(schedule.len() - 2),
                tenor.length(),
            ),
        ] {
            if !schedule.is_regular_at(position) {
                advance(
                    calendar,
                    anchor,
                    amount,
                    tenor.units(),
                    schedule.business_day_convention(),
                    false,
                )?;
            }
        }
    }
    Ok(())
}
/// Plain same-currency paid and received Ibor legs. Flag zero preserves native defaults.
#[repr(C)]
pub struct ItofinFloatFloatSwapConfig {
    pub nominal: f64,
    pub pay_schedule: u64,
    pub pay_index: u64,
    pub pay_spread: f64,
    pub pay_day_counter: u64,
    pub receive_schedule: u64,
    pub receive_index: u64,
    pub receive_spread: f64,
    pub receive_day_counter: u64,
    pub settings: u64,
    pub has_payment_convention: u8,
    pub payment_convention: i32,
}
/// Construct and retain the original native instrument, Settings and both indexes.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_float_float_swap_new(
    ctx: *mut Context,
    cfg: *const ItofinFloatFloatSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let payment = match a.has_payment_convention {
                0 => None,
                1 => Some(convention(a.payment_convention)?),
                _ => return Err(BindingError::invalid("invalid payment convention flag")),
            };
            let shared_settings = settings(c, a.settings)?;
            let indexes = [
                c.get::<NativeIbor>(a.pay_index)?,
                c.get::<NativeIbor>(a.receive_index)?,
            ];
            let schedules = [
                c.get::<Schedule>(a.pay_schedule)?,
                c.get::<Schedule>(a.receive_schedule)?,
            ];
            let calendars = [
                schedule_calendar(c, a.pay_schedule)?,
                schedule_calendar(c, a.receive_schedule)?,
            ];
            for ((schedule, index), calendar) in schedules.iter().zip(&indexes).zip(&calendars) {
                if !std::ptr::eq(Index::settings(&*index.inner), &*shared_settings) {
                    return Err(BindingError::invalid("indexes must share swap settings"));
                }
                check_schedule(schedule, calendar, index, payment)?;
            }
            let swap = FloatFloatSwap::new(
                a.nominal,
                schedules[0].clone(),
                indexes[0].inner.clone(),
                a.pay_spread,
                day_counter(c, a.pay_day_counter)?,
                schedules[1].clone(),
                indexes[1].inner.clone(),
                a.receive_spread,
                day_counter(c, a.receive_day_counter)?,
                payment,
                shared_settings.clone(),
            )?;
            let owner = NativeFloatFloat {
                swap: shared_mut(swap),
                flags: libitofin::shared::shared(Cell::new(flags(&shared_settings))),
                settings: shared_settings,
                indexes,
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
pub unsafe extern "C" fn itofin_float_float_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    settings_id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let owner = c.get::<NativeFloatFloat>(id)?;
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
/// Query 0 NPV, 1/2 paid/received NPV, 3/4 paid/received BPS, 5/6 absolute fair
/// paid/received spread, 7 cached state, 8 expiry. Failures leave output untouched.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_float_float_swap_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let owner = c.get::<NativeFloatFloat>(id)?;
            if !(0..=8).contains(&query) {
                return Err(BindingError::invalid("invalid float-float swap query"));
            }
            if query <= 6 {
                owner.prepare(false)?;
            }
            let mut s = owner.swap.borrow_mut();
            let value = match query {
                0 => s.npv()?,
                1 => s.pay_leg_npv()?,
                2 => s.receive_leg_npv()?,
                3 => s.pay_leg_bps()?,
                4 => s.receive_leg_bps()?,
                5 => s.fair_pay_spread()?,
                6 => s.fair_receive_spread()?,
                7 => u8::from(s.base().is_calculated()) as f64,
                8 => u8::from(s.is_expired()?) as f64,
                _ => unreachable!(),
            };
            if query <= 6 {
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
pub unsafe extern "C" fn itofin_float_float_swap_action(
    ctx: *mut Context,
    id: u64,
    action: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let owner = c.get::<NativeFloatFloat>(id)?;
            if !(0..=3).contains(&action) {
                return Err(BindingError::invalid("invalid float-float swap action"));
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
                _ => unreachable!(),
            }
            if action <= 1 {
                owner.flags.set(flags(&owner.settings));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "float_float_swap_api_tests.rs"]
mod tests;
