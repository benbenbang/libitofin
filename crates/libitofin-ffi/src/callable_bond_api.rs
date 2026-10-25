//! Retained typed callable bonds and copied hard call/put schedules.
use crate::bond_api::{ItofinBondConfig, ItofinFixedBondConfig, NativeBond, optional_date};
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::floating_bond_api::advance;
use crate::rates_api::{finite, settings};
use crate::time_api::{convention, date, day_counter, time_unit};
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::shared::{shared, shared_mut};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, frequency::Frequency, period::Period,
};
use std::cell::Cell;

#[repr(C)]
#[derive(Clone, Copy)]
/// An immutable exercise quote per 100. Clean=0/dirty=1; call=0/put=1.
pub struct ItofinCallability {
    pub price: f64,
    pub price_type: i32,
    pub call_type: i32,
    pub date: i32,
}
fn adjust(
    calendar: &crate::calendar_api::NativeCalendar,
    d: Date,
    rule: BusinessDayConvention,
) -> BindingResult<Date> {
    date(d.serial_number())?;
    if rule == BusinessDayConvention::Unadjusted {
        Ok(d)
    } else {
        crate::time_api::adjust(calendar, d, rule)
    }
}
fn callability(a: ItofinCallability) -> BindingResult<Callability> {
    let amount = finite(a.price)?;
    let price = match a.price_type {
        0 => BondPrice::Clean(amount),
        1 => BondPrice::Dirty(amount),
        _ => return Err(BindingError::invalid("invalid callability price type")),
    };
    let kind = match a.call_type {
        0 => CallabilityType::Call,
        1 => CallabilityType::Put,
        _ => return Err(BindingError::invalid("invalid callability type")),
    };
    Ok(Callability::new(price, kind, date(a.date)?)?)
}
fn record(a: &Callability) -> ItofinCallability {
    let (price, price_type) = match a.price() {
        BondPrice::Clean(price) => (price, 0),
        BondPrice::Dirty(price) => (price, 1),
    };
    ItofinCallability {
        price,
        price_type,
        call_type: match a.call_type() {
            CallabilityType::Call => 0,
            CallabilityType::Put => 1,
        },
        date: a.date().serial_number(),
    }
}
fn money(a: &ItofinBondConfig) -> BindingResult<()> {
    if finite(a.face_amount)? <= 0.0
        || finite(a.redemption)? < 0.0
        || i64::from(a.settlement_days) > i64::from(Date::max_date() - Date::min_date())
        || !(a.face_amount * a.redemption / 100.0).is_finite()
    {
        return Err(BindingError::invalid(
            "invalid callable bond amount or settlement days",
        ));
    }
    Ok(())
}
fn native(
    bond: BondBacking,
    calendar: crate::calendar_api::NativeCalendar,
    fixed: Option<(
        Frequency,
        libitofin::time::daycounter::DayCounter,
        Option<libitofin::time::daycounter::DayCounter>,
    )>,
) -> BindingResult<NativeBond> {
    let flags = {
        let projected = bond.borrow();
        for flow in projected.cashflows() {
            finite(flow.amount()?)?;
        }
        (
            projected.settings().include_reference_date_events(),
            projected.settings().include_todays_cash_flows(),
        )
    };
    Ok(NativeBond {
        bond,
        calendar,
        fixed,
        flags: shared(Cell::new(flags)),
        floating: None,
        indexed: false,
    })
}
#[unsafe(no_mangle)]
/// Validate and copy a callability value. Every bond constructor also validates all records.
/// # Safety
/// Pointers must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callability_new(
    a: ItofinCallability,
    out: *mut ItofinCallability,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            output(out, record(&callability(a)?))
        })
    }
}
#[unsafe(no_mangle)]
/// The common calendar and first-period day-counter fields must be zero.
/// Zero ex-coupon calendar selects NullCalendar when an ex-coupon period is present.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_callable_fixed_rate_bond_new(
    ctx: *mut Context,
    a: ItofinFixedBondConfig,
    coupons: *const f64,
    count: usize,
    calls: *const ItofinCallability,
    call_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            money(&a.bond)?;
            if a.bond.calendar != 0 || a.first_period_day_counter != 0 {
                return Err(BindingError::invalid(
                    "callable fixed bond does not support calendar or first-period overrides",
                ));
            }
            if a.has_ex_coupon > 1 || a.ex_coupon_end_of_month > 1 {
                return Err(BindingError::invalid("invalid ex-coupon flag"));
            }
            let rates = input_slice(coupons, count)?
                .iter()
                .map(|&r| finite(r))
                .collect::<BindingResult<Vec<_>>>()?;
            let calls = input_slice(calls, call_count)?
                .iter()
                .map(|&a| callability(a))
                .collect::<BindingResult<Vec<_>>>()?;
            let schedule = c.get::<libitofin::time::schedule::Schedule>(a.schedule)?;
            let calendar = crate::time_api::schedule_calendar(c, a.schedule)?;
            let rule = convention(a.bond.payment_convention)?;
            let ex_rule = convention(a.ex_coupon_convention)?;
            let ex_calendar = if a.has_ex_coupon == 1 && a.ex_coupon_calendar != 0 {
                c.get::<crate::calendar_api::NativeCalendar>(a.ex_coupon_calendar)?
            } else {
                crate::calendar_api::NativeCalendar {
                    inner: libitofin::time::calendars::NullCalendar::new(),
                    horizon: None,
                    first_year: 1901,
                }
            };
            let ex_period = if a.has_ex_coupon == 1 {
                if a.ex_coupon_length < 0 {
                    return Err(BindingError::invalid(
                        "ex-coupon period must be nonnegative",
                    ));
                }
                Some(Period::new(
                    a.ex_coupon_length,
                    time_unit(a.ex_coupon_unit)?,
                ))
            } else {
                None
            };
            for &d in schedule.dates().iter().skip(1) {
                let payment = adjust(&calendar, d, rule)?;
                if let Some(p) = ex_period {
                    advance(
                        &ex_calendar,
                        payment,
                        -p.length(),
                        p.units(),
                        ex_rule,
                        a.ex_coupon_end_of_month == 1,
                    )?;
                }
            }
            if schedule.len() >= 2 && schedule.has_tenor() && schedule.has_is_regular() {
                let p = schedule.tenor();
                if !schedule.is_regular_at(1) {
                    advance(
                        &calendar,
                        schedule.date(1),
                        -p.length(),
                        p.units(),
                        schedule.business_day_convention(),
                        schedule.has_end_of_month() && schedule.end_of_month(),
                    )?;
                }
                if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                    advance(
                        &calendar,
                        schedule.date(schedule.len() - 2),
                        p.length(),
                        p.units(),
                        schedule.business_day_convention(),
                        schedule.has_end_of_month() && schedule.end_of_month(),
                    )?;
                }
            }
            let dc = day_counter(c, a.day_counter)?;
            let frequency = if schedule.has_tenor() {
                schedule.tenor().frequency()
            } else {
                Frequency::NoFrequency
            };
            let bond = CallableFixedRateBond::with_ex_coupon(
                a.bond.settlement_days,
                a.bond.face_amount,
                schedule,
                rates,
                dc.clone(),
                rule,
                a.bond.redemption,
                optional_date(a.bond.issue_date)?,
                calls,
                ex_period,
                ex_calendar.inner,
                ex_rule,
                a.ex_coupon_end_of_month == 1,
                settings(c, a.bond.settings)?,
            )?;
            output(
                out,
                c.insert(native(
                    BondBacking::CallableFixed(shared_mut(bond)),
                    calendar,
                    Some((frequency, dc, None)),
                )?)?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_callable_zero_coupon_bond_new(
    ctx: *mut Context,
    a: ItofinBondConfig,
    maturity: i32,
    counter: u64,
    calls: *const ItofinCallability,
    call_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            money(&a)?;
            let calendar = c.get::<crate::calendar_api::NativeCalendar>(a.calendar)?;
            let maturity = date(maturity)?;
            let rule = convention(a.payment_convention)?;
            adjust(&calendar, maturity, rule)?;
            let calls = input_slice(calls, call_count)?
                .iter()
                .map(|&a| callability(a))
                .collect::<BindingResult<Vec<_>>>()?;
            let bond = CallableZeroCouponBond::new(
                a.settlement_days,
                a.face_amount,
                calendar.inner.clone(),
                maturity,
                day_counter(c, counter)?,
                rule,
                a.redemption,
                optional_date(a.issue_date)?,
                calls,
                settings(c, a.settings)?,
            )?;
            output(
                out,
                c.insert(native(
                    BondBacking::CallableZero(shared_mut(bond)),
                    calendar,
                    None,
                )?)?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// Copy schedule in original order. Capacity zero queries required length.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_bond_callability(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinCallability,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            let bond = c.get::<NativeBond>(id)?;
            let calls = match &bond.bond {
                BondBacking::Plain(_) => return Err(BindingError::invalid("not a callable bond")),
                BondBacking::CallableFixed(bond) => bond.borrow().callability().clone(),
                BondBacking::CallableZero(bond) => bond.borrow().callability().clone(),
            };
            if capacity != 0 {
                if capacity < calls.len() {
                    return Err(BindingError::invalid("callability buffer too small"));
                }
                check_ptr(out)?;
                for (i, call) in calls.iter().enumerate() {
                    output(out.add(i), record(call))?;
                }
            }
            output(required, calls.len())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_callable_zero_bond_day_counter(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(id)?;
            let BondBacking::CallableZero(bond) = bond.bond else {
                return Err(BindingError::invalid("not a callable zero-coupon bond"));
            };
            let dc = bond.borrow().day_counter().clone();
            output(out, c.insert(dc)?)
        })
    }
}

#[cfg(test)]
#[path = "callable_bond_api_tests.rs"]
mod tests;
