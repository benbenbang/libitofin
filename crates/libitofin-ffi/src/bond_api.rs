//! Shared bond instruments and retained discounting engines.
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::rates_api::{curve, finite, settings};
use crate::time_api::{adjust, advance, convention, date, day_counter, time_unit};
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, FixedRateBond, ZeroCouponBond};
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, daycounter::DayCounter,
    frequency::Frequency, period::Period, schedule::Schedule, timeunit::TimeUnit,
};
use std::cell::Cell;

#[derive(Clone)]
pub(crate) struct NativeBond {
    pub bond: BondBacking,
    pub calendar: NativeCalendar,
    pub(crate) fixed: Option<(Frequency, DayCounter, Option<DayCounter>)>,
    pub(crate) flags: Shared<Cell<(bool, Option<bool>)>>,
    pub(crate) floating: Option<crate::floating_bond_api::NativeFloating>,
}
impl NativeBond {
    pub(crate) fn new(
        bond: Bond,
        calendar: NativeCalendar,
        fixed: Option<(Frequency, DayCounter, Option<DayCounter>)>,
    ) -> BindingResult<Self> {
        for flow in bond.cashflows() {
            finite(flow.amount()?)?;
        }
        Ok(Self::new_unpriced(bond, calendar, fixed))
    }
    pub(crate) fn new_unpriced(
        bond: Bond,
        calendar: NativeCalendar,
        fixed: Option<(Frequency, DayCounter, Option<DayCounter>)>,
    ) -> Self {
        let flags = (
            bond.settings().include_reference_date_events(),
            bond.settings().include_todays_cash_flows(),
        );
        Self {
            bond: BondBacking::Plain(shared_mut(bond)),
            calendar,
            fixed,
            flags: shared(Cell::new(flags)),
            floating: None,
        }
    }
    pub(crate) fn settlement(&self, supplied: Option<Date>) -> BindingResult<Date> {
        let bond = self.bond.borrow();
        let reference = supplied
            .or_else(|| bond.settings().evaluation_date())
            .ok_or_else(|| BindingError::invalid("evaluation date required"))?;
        let days = i32::try_from(bond.settlement_days())
            .map_err(|_| BindingError::invalid("settlement days overflow"))?;
        let settlement = advance(
            &self.calendar,
            reference,
            days,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )?;
        Ok(bond
            .issue_date()
            .map_or(settlement, |issue| issue.max(settlement)))
    }
    pub(crate) fn preflight(&self, field: i32, settlement: Date, force: bool) -> BindingResult<()> {
        if let Some(floating) = &self.floating {
            let bond = self.bond.borrow();
            if field <= 3 {
                floating.guard_bond(&bond, settlement, force)?;
            }
            if field == 2 || field == 4 {
                floating.guard_accrued(&bond, settlement)?;
            }
        }
        Ok(())
    }
    pub(crate) fn refresh(&self) {
        if let Some(floating) = &self.floating
            && let BondBacking::Plain(bond) = &self.bond
        {
            floating.refresh(bond);
        }
        if let BondBacking::CallableZero(_) = &self.bond {
            self.bond.with_instrument(|bond| {
                bond.base();
            });
            return;
        }
        let bond = self.bond.borrow();
        let flags = (
            bond.settings().include_reference_date_events(),
            bond.settings().include_todays_cash_flows(),
        );
        if self.flags.replace(flags) != flags {
            bond.base().observer().borrow_mut().update();
        }
    }
}
#[derive(Clone)]
pub(crate) struct NativeBondEngine {
    engine: SharedMut<DiscountingBondEngine>,
    settings: Shared<Settings<Date>>,
    discount: libitofin::handle::Handle<
        dyn libitofin::termstructures::yieldtermstructure::YieldTermStructure,
    >,
    include: Option<bool>,
}
#[repr(C)]
pub struct ItofinBondConfig {
    pub settlement_days: u32,
    pub face_amount: f64,
    pub redemption: f64,
    pub settings: u64,
    /// Zero means no issue date.
    pub issue_date: i32,
    pub payment_convention: i32,
    /// Required for zero-coupon bonds; zero selects the schedule calendar for fixed bonds.
    pub calendar: u64,
}
#[repr(C)]
pub struct ItofinFixedBondConfig {
    pub bond: ItofinBondConfig,
    pub schedule: u64,
    pub day_counter: u64,
    /// Zero means no first-period override.
    pub first_period_day_counter: u64,
    pub ex_coupon_calendar: u64,
    pub ex_coupon_length: i32,
    pub ex_coupon_unit: i32,
    pub ex_coupon_convention: i32,
    pub has_ex_coupon: u8,
    pub ex_coupon_end_of_month: u8,
}
pub(crate) fn money(a: &ItofinBondConfig) -> BindingResult<()> {
    if finite(a.face_amount)? <= 0.0
        || finite(a.redemption)? <= 0.0
        || i32::try_from(a.settlement_days).is_err()
    {
        return Err(BindingError::invalid(
            "positive face/redemption and bounded settlement days required",
        ));
    }
    let payment = a.face_amount * (a.redemption / 100.0);
    if finite(payment)? <= 0.0 || !(a.face_amount * a.redemption / 100.0).is_finite() {
        return Err(BindingError::invalid("redemption payment out of range"));
    }
    Ok(())
}
pub(crate) fn optional_date(serial: i32) -> BindingResult<Option<Date>> {
    if serial == 0 {
        Ok(None)
    } else {
        Ok(Some(date(serial)?))
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_zero_coupon_bond_new(
    ctx: *mut Context,
    a: ItofinBondConfig,
    maturity: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            money(&a)?;
            let calendar = c.get::<NativeCalendar>(a.calendar)?;
            let maturity = date(maturity)?;
            let rule = convention(a.payment_convention)?;
            adjust(&calendar, maturity, rule)?;
            let bond = ZeroCouponBond::new(
                a.settlement_days,
                calendar.inner.clone(),
                a.face_amount,
                maturity,
                rule,
                a.redemption,
                optional_date(a.issue_date)?,
                settings(c, a.settings)?,
            )?
            .into_bond();
            output(out, c.insert(NativeBond::new(bond, calendar, None)?)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_fixed_rate_bond_new(
    ctx: *mut Context,
    a: ItofinFixedBondConfig,
    coupons: *const f64,
    count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            money(&a.bond)?;
            if a.has_ex_coupon > 1 || a.ex_coupon_end_of_month > 1 {
                return Err(BindingError::invalid("invalid ex-coupon flag"));
            }
            let rates = input_slice(coupons, count)?
                .iter()
                .map(|&r| finite(r))
                .collect::<BindingResult<Vec<_>>>()?;
            let schedule = c.get::<Schedule>(a.schedule)?;
            let calendar = if a.bond.calendar == 0 {
                crate::time_api::schedule_calendar(c, a.schedule)?
            } else {
                c.get::<NativeCalendar>(a.bond.calendar)?
            };
            let schedule_calendar = crate::time_api::schedule_calendar(c, a.schedule)?;
            let rule = convention(a.bond.payment_convention)?;
            let ex_rule = convention(a.ex_coupon_convention)?;
            let ex_calendar = if a.has_ex_coupon == 1 {
                c.get::<NativeCalendar>(a.ex_coupon_calendar)?
            } else {
                calendar.clone()
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
                        &schedule_calendar,
                        schedule.date(1),
                        -p.length(),
                        p.units(),
                        schedule.business_day_convention(),
                        schedule.has_end_of_month() && schedule.end_of_month(),
                    )?;
                }
                if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                    advance(
                        &schedule_calendar,
                        schedule.date(schedule.len() - 2),
                        p.length(),
                        p.units(),
                        schedule.business_day_convention(),
                        schedule.has_end_of_month() && schedule.end_of_month(),
                    )?;
                }
            }
            let first = if a.first_period_day_counter == 0 {
                None
            } else {
                Some(day_counter(c, a.first_period_day_counter)?)
            };
            let bond = FixedRateBond::new(
                a.bond.settlement_days,
                a.bond.face_amount,
                schedule,
                rates,
                day_counter(c, a.day_counter)?,
                rule,
                a.bond.redemption,
                optional_date(a.bond.issue_date)?,
                Some(calendar.inner.clone()),
                ex_period,
                ex_calendar.inner,
                ex_rule,
                a.ex_coupon_end_of_month == 1,
                first,
                settings(c, a.bond.settings)?,
            )?;
            let fixed = Some((
                bond.frequency(),
                bond.day_counter().clone(),
                bond.first_period_day_counter().cloned(),
            ));
            output(
                out,
                c.insert(NativeBond::new(bond.into_bond(), calendar, fixed)?)?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// include_flows is -1 for settings default, 0 false or 1 true.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_discounting_bond_engine_new(
    ctx: *mut Context,
    discount: u64,
    settings_id: u64,
    include_flows: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let include = match include_flows {
                -1 => None,
                0 => Some(false),
                1 => Some(true),
                _ => return Err(BindingError::invalid("invalid include-flows flag")),
            };
            let settings = settings(c, settings_id)?;
            let discount = curve(c, discount)?;
            let engine = shared_mut(DiscountingBondEngine::new(
                discount.clone(),
                include,
                settings.clone(),
            ));
            output(
                out,
                c.insert(NativeBondEngine {
                    engine,
                    settings,
                    discount,
                    include,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_set_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let bond = c.get::<NativeBond>(id)?;
            let engine = c.get::<NativeBondEngine>(engine)?;
            let plain = bond.bond.plain()?;
            if !Shared::ptr_eq(&bond.bond.borrow().settings_handle(), &engine.settings) {
                return Err(BindingError::invalid("bond and engine settings differ"));
            }
            if let Some(floating) = &bond.floating {
                floating.set_engine_guard(engine.discount, engine.include);
            }
            plain
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(engine.engine);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// Fields: 0 NPV, 1 settlement value, 2 clean price, 3 dirty price, 4 accrual, 5 notional.
/// Date zero selects current settlement for accrual/notional.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_real(
    ctx: *mut Context,
    id: u64,
    field: i32,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let native = c.get::<NativeBond>(id)?;
            let settlement = if field >= 4 {
                optional_date(serial)?.map_or_else(|| native.settlement(None), Ok)?
            } else {
                native.settlement(None)?
            };
            native.refresh();
            if field != 3 || native.bond.borrow().notional(Some(settlement))? != 0.0 {
                native.preflight(field, settlement, false)?;
            }
            let value = native.bond.real(field, settlement)?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Fields: 0 settlement days, 1 settlement date (reference date or zero), 2 maturity,
/// 3 issue date (zero means absent), 4 expired, 5 calculated, 6 fixed frequency.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_integer(
    ctx: *mut Context,
    id: u64,
    field: i32,
    serial: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let native = c.get::<NativeBond>(id)?;
            native.refresh();
            let value = match field {
                1 => native.settlement(optional_date(serial)?)?.serial_number(),
                0 => i32::try_from(native.bond.borrow().settlement_days())
                    .map_err(|_| BindingError::invalid("settlement days overflow"))?,
                2 => native.bond.borrow().maturity_date()?.serial_number(),
                3 => native
                    .bond
                    .borrow()
                    .issue_date()
                    .map_or(0, |d| d.serial_number()),
                4 => i32::from(native.bond.with_instrument(|bond| bond.is_expired())?),
                5 => i32::from(
                    native
                        .bond
                        .with_instrument(|bond| bond.base().is_calculated()),
                ),
                6 => match native
                    .fixed
                    .as_ref()
                    .ok_or_else(|| BindingError::invalid("not a fixed-rate bond"))?
                    .0
                {
                    Frequency::Annual => 0,
                    Frequency::Semiannual => 1,
                    Frequency::Quarterly => 2,
                    Frequency::Monthly => 3,
                    _ => return Err(BindingError::invalid("unsupported bond frequency")),
                },
                _ => return Err(BindingError::invalid("invalid bond integer field")),
            };
            output(out, value)
        })
    }
}
#[unsafe(no_mangle)]
/// Fields: 0 calendar, 1 cashflows, 2 redemptions, 3 fixed day counter,
/// 4 fixed first-period day counter (zero means absent).
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_handle(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let native = c.get::<NativeBond>(id)?;
            let result = match field {
                0 => c.insert(native.calendar)?,
                1 => {
                    let leg = native.bond.borrow().cashflows().clone();
                    if let Some(floating) = native.floating {
                        c.insert_with_metadata(leg, floating)?
                    } else {
                        c.insert(leg)?
                    }
                }
                2 => c.insert(native.bond.borrow().redemptions().clone())?,
                3 | 4 => {
                    let fixed = native
                        .fixed
                        .ok_or_else(|| BindingError::invalid("not a fixed-rate bond"))?;
                    if field == 3 {
                        c.insert(fixed.1)?
                    } else {
                        fixed.2.map_or(Ok(0), |dc| c.insert(dc))?
                    }
                }
                _ => return Err(BindingError::invalid("invalid bond handle field")),
            };
            output(out, result)
        })
    }
}
#[unsafe(no_mangle)]
/// Actions: 0 recalculate, 1 freeze, 2 unfreeze.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_action(
    ctx: *mut Context,
    id: u64,
    action: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let native = c.get::<NativeBond>(id)?;
            native.refresh();
            if action == 0 {
                let settlement = native.settlement(None)?;
                native.preflight(0, settlement, true)?;
            }
            if let Some(floating) = &native.floating {
                if action == 1 {
                    floating.freeze(true);
                } else if action == 2 {
                    floating.freeze(false);
                }
            }
            native.bond.with_instrument_mut(|bond| {
                match action {
                    0 => bond.recalculate()?,
                    1 => bond.base_mut().freeze(),
                    2 => bond.base_mut().unfreeze(),
                    _ => return Err(BindingError::invalid("invalid bond action")),
                };
                Ok(())
            })
        })
    }
}
