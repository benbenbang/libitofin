//! Retained plain-bond forwards with live native ownership and preparation.
use crate::bond_api::NativeBond;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::rates_api::{curve, finite};
use crate::time_api::{adjust, advance, convention, date, day_counter};
use libitofin::instrument::Instrument;
use libitofin::instruments::BondForward;
use libitofin::position::Position;
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::time::{businessdayconvention::BusinessDayConvention, timeunit::TimeUnit};

pub(crate) struct NativeBondForward {
    inner: BondForward,
    bond: NativeBond,
    calendar: NativeCalendar,
    settlement_days: i32,
    frozen: bool,
}
impl NativeBondForward {
    fn check_settlement(&self) -> BindingResult<()> {
        let evaluation = self
            .bond
            .bond
            .borrow()
            .settings()
            .evaluation_date()
            .ok_or_else(|| BindingError::invalid("evaluation date required"))?;
        advance(
            &self.calendar,
            evaluation,
            self.settlement_days,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )?;
        Ok(())
    }
    fn prepare(&self, force: bool) -> BindingResult<()> {
        self.check_settlement()?;
        self.bond.refresh();
        if !force && (self.inner.base().is_calculated() || self.frozen) {
            return Ok(());
        }
        if self.inner.is_expired()? {
            return Ok(());
        }
        let settlement = self.bond.settlement(None)?;
        self.bond.preflight(3, settlement, force)
    }
}
#[repr(C)]
pub struct ItofinBondForwardConfig {
    pub bond: u64,
    pub value_date: i32,
    pub delivery_date: i32,
    /// 0 Long, 1 Short. Strike is a dirty delivery quote, not a cash notional.
    pub position: i32,
    pub strike: f64,
    pub settlement_days: u32,
    pub day_counter: u64,
    pub calendar: u64,
    pub convention: i32,
    pub discount: u64,
    /// Zero explicitly defaults income discounting to the financing curve.
    pub income_discount: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_forward_new(
    ctx: *mut Context,
    a: ItofinBondForwardConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(a.bond)?;
            if bond.floating.is_some() {
                return Err(BindingError::invalid("fixed or zero plain bond required"));
            }
            let plain = bond.bond.plain()?.clone();
            let position = match a.position {
                0 => Position::Long,
                1 => Position::Short,
                _ => return Err(BindingError::invalid("invalid position")),
            };
            let days = i32::try_from(a.settlement_days)
                .map_err(|_| BindingError::invalid("settlement days overflow"))?;
            let calendar = c.get::<NativeCalendar>(a.calendar)?;
            let rule = convention(a.convention)?;
            let value = date(a.value_date)?;
            let delivery = date(a.delivery_date)?;
            adjust(&calendar, delivery, rule)?;
            let dc = day_counter(c, a.day_counter)?;
            let discount = curve(c, a.discount)?;
            let inner = if a.income_discount == 0 {
                BondForward::new(
                    plain,
                    value,
                    delivery,
                    position,
                    finite(a.strike)?,
                    a.settlement_days,
                    dc,
                    calendar.inner.clone(),
                    rule,
                    discount,
                )?
            } else {
                BondForward::with_income_curve(
                    plain,
                    value,
                    delivery,
                    position,
                    finite(a.strike)?,
                    a.settlement_days,
                    dc,
                    calendar.inner.clone(),
                    rule,
                    discount,
                    curve(c, a.income_discount)?,
                )?
            };
            output(
                out,
                c.insert(shared_mut(NativeBondForward {
                    inner,
                    bond,
                    calendar,
                    settlement_days: days,
                    frozen: false,
                }))?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// Fields: 0 NPV, 1 fair dirty forward quote, 2 clean forward quote, 3 strike.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_forward_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !(0..=3).contains(&field) {
                return Err(BindingError::invalid("invalid forward value field"));
            }
            let object = c.get::<SharedMut<NativeBondForward>>(id)?;
            let mut object = object.borrow_mut();
            if field == 3 {
                return output(out, object.inner.strike());
            }
            object.prepare(false)?;
            let value = match field {
                0 => object.inner.npv()?,
                1 => object.inner.fair_forward_price()?,
                _ => object.inner.clean_forward_price()?,
            };
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Fields: 0 checked settlement, 1 adjusted delivery, 2 value date, 3 expiry flag, 4 position.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_forward_integer(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let object = c.get::<SharedMut<NativeBondForward>>(id)?;
            let object = object.borrow();
            let value = match field {
                0 => {
                    object.check_settlement()?;
                    object.inner.settlement_date()?.serial_number()
                }
                1 => object.inner.delivery_date().serial_number(),
                2 => object.inner.value_date().serial_number(),
                3 => {
                    object.check_settlement()?;
                    i32::from(object.inner.is_expired()?)
                }
                4 => match object.inner.position() {
                    Position::Long => 0,
                    Position::Short => 1,
                },
                _ => return Err(BindingError::invalid("invalid forward integer field")),
            };
            output(out, value)
        })
    }
}
#[unsafe(no_mangle)]
/// Actions: 0 force recalculation, 1 freeze, 2 unfreeze.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_forward_control(
    ctx: *mut Context,
    id: u64,
    action: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let object = c.get::<SharedMut<NativeBondForward>>(id)?;
            let mut object = object.borrow_mut();
            match action {
                0 => {
                    object.prepare(true)?;
                    object.inner.recalculate()?;
                }
                1 => {
                    object.inner.base_mut().freeze();
                    object.frozen = true;
                }
                2 => {
                    object.inner.base_mut().unfreeze();
                    object.frozen = false;
                }
                _ => return Err(BindingError::invalid("invalid forward action")),
            }
            Ok(())
        })
    }
}
