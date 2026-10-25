//! Payment navigation over retained bond and leg cash flows.
use crate::bond_api::{NativeBond, optional_date};
use crate::boundary::*;
use crate::cpi_cashflows_api::CpiLegSettings;
use crate::floating_bond_api::NativeFloating;
use crate::inflation_api::bool_value;
use crate::rates_api::{finite, settings};
use libitofin::cashflow::Leg;
use libitofin::cashflows::CashFlows;
use libitofin::pricingengines::bond::BondFunctions;
use libitofin::settings::Settings;
use libitofin::time::date::Date;

/// Navigation policy for an erased leg. Date zero uses the evaluation date.
#[repr(C)]
pub struct ItofinPaymentQuery {
    pub settings: u64,
    pub settlement: i32,
    /// -1 uses Settings, 0 excludes settlement-date flows, 1 includes them.
    pub include_settlement: i32,
    /// 0 previous payment, 1 next payment.
    pub direction: i32,
    /// 0 reads only the date, 1 also resolves and aggregates payment amounts.
    pub with_amount: u8,
}

/// No payment has present=0, date=0 and amount=0; a date-only query has amount=0.
#[repr(C)]
pub struct ItofinPaymentResult {
    pub present: u8,
    pub date: i32,
    pub amount: f64,
}

fn direction(value: i32) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("invalid payment direction")),
    }
}
fn inclusion(value: i32) -> BindingResult<Option<bool>> {
    match value {
        -1 => Ok(None),
        0 => Ok(Some(false)),
        1 => Ok(Some(true)),
        _ => Err(BindingError::invalid("invalid settlement flag")),
    }
}
fn payment(
    leg: &Leg,
    settings: &Settings<Date>,
    include: Option<bool>,
    settlement: Option<Date>,
    next: bool,
    with_amount: bool,
    floating: Option<&NativeFloating>,
) -> BindingResult<ItofinPaymentResult> {
    let index = if next {
        CashFlows::next_cash_flow(leg, settings, include, settlement)?
    } else {
        CashFlows::previous_cash_flow(leg, settings, include, settlement)?
    };
    let Some(index) = index else {
        return Ok(ItofinPaymentResult {
            present: 0,
            date: 0,
            amount: 0.0,
        });
    };
    let date = leg[index].date();
    let amount = if with_amount {
        if let Some(floating) = floating {
            let mut first = index;
            let mut end = index + 1;
            if next {
                while end < leg.len() && leg[end].date() == date {
                    end += 1;
                }
            } else {
                while first > 0 && leg[first - 1].date() == date {
                    first -= 1;
                }
            }
            for position in first..end {
                floating.cashflow(position).guard()?;
            }
        }
        let value = if next {
            CashFlows::next_cash_flow_amount(leg, settings, include, settlement)?
        } else {
            CashFlows::previous_cash_flow_amount(leg, settings, include, settlement)?
        };
        finite(value.ok_or_else(|| BindingError::invalid("payment disappeared"))?)?
    } else {
        0.0
    };
    Ok(ItofinPaymentResult {
        present: 1,
        date: date.serial_number(),
        amount,
    })
}

/// Read the leg's accrual span: field 0 start, 1 maturity. Empty legs fail.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers, non-overlap and thread ownership must satisfy the C caller contract.
pub unsafe extern "C" fn itofin_leg_span_date(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let leg = c.get::<Leg>(id)?;
            let value = match field {
                0 => CashFlows::start_date(&leg)?,
                1 => CashFlows::maturity_date(&leg)?,
                _ => return Err(BindingError::invalid("invalid leg span field")),
            };
            output(out, value.serial_number())
        })
    }
}

/// Inspect a payment without valuing other dates or consulting a pricing engine.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers, non-overlap and thread ownership must satisfy the C caller contract.
pub unsafe extern "C" fn itofin_leg_payment(
    ctx: *mut Context,
    id: u64,
    query: ItofinPaymentQuery,
    out: *mut ItofinPaymentResult,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let next = direction(query.direction)?;
            let with_amount = bool_value(i32::from(query.with_amount))?;
            let include = inclusion(query.include_settlement)?;
            let settlement = optional_date(query.settlement)?;
            let settings = settings(c, query.settings)?;
            let leg = c.get::<Leg>(id)?;
            if let Some(cpi) = c.optional_metadata::<CpiLegSettings>(id)
                && !std::ptr::eq(&*settings, &*cpi.0)
            {
                return Err(BindingError::invalid(
                    "CPI leg and inspection settings must match",
                ));
            }
            let floating = c.optional_metadata::<NativeFloating>(id);
            output(
                out,
                payment(
                    &leg,
                    &settings,
                    include,
                    settlement,
                    next,
                    with_amount,
                    floating.as_ref(),
                )?,
            )
        })
    }
}

/// Read field 0 accrual start date, or field 1 tradability (0/1).
/// Settlement zero uses the bond's calendar-aware settlement date for field 1.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers, non-overlap and thread ownership must satisfy the C caller contract.
pub unsafe extern "C" fn itofin_bond_inspection(
    ctx: *mut Context,
    id: u64,
    field: i32,
    settlement: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let native = c.get::<NativeBond>(id)?;
            let settlement = optional_date(settlement)?;
            let value = match field {
                0 => CashFlows::start_date(native.bond.borrow().cashflows())?.serial_number(),
                1 => {
                    let settlement = settlement.map_or_else(|| native.settlement(None), Ok)?;
                    i32::from(BondFunctions::is_tradable(
                        &native.bond.borrow(),
                        Some(settlement),
                    )?)
                }
                _ => return Err(BindingError::invalid("invalid bond inspection field")),
            };
            output(out, value)
        })
    }
}

/// Read the previous (0) or next (1) payment with settlement-date exclusion.
/// Settings includeTodaysCashFlows can override exclusion when settlement is today.
/// with_amount is 0 for date-only or 1 for date and aggregated actual amount.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers, non-overlap and thread ownership must satisfy the C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_bond_payment(
    ctx: *mut Context,
    id: u64,
    direction_value: i32,
    with_amount: u8,
    settlement: i32,
    out: *mut ItofinPaymentResult,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let next = direction(direction_value)?;
            let with_amount = bool_value(i32::from(with_amount))?;
            let native = c.get::<NativeBond>(id)?;
            let settlement =
                optional_date(settlement)?.map_or_else(|| native.settlement(None), Ok)?;
            let bond = native.bond.borrow();
            output(
                out,
                payment(
                    bond.cashflows(),
                    bond.settings(),
                    Some(false),
                    Some(settlement),
                    next,
                    with_amount,
                    native.floating.as_ref(),
                )?,
            )
        })
    }
}

#[cfg(test)]
#[path = "payment_inspection_tests.rs"]
mod tests;
