//! Pure bond and cash-flow yield and zero-spread analytics.
use crate::bond_api::{NativeBond, optional_date};
use crate::boundary::*;
use crate::rates_api::{curve, finite, settings};
use crate::time_api::{day_counter, frequency};
use libitofin::cashflow::Leg;
use libitofin::cashflows::CashFlows;
use libitofin::instruments::BondPrice;
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::bond::BondFunctions;
use libitofin::time::date::Date;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinBondYieldAnalyticsConfig {
    pub day_counter: u64,
    /// Simple=0, Compounded=1, Continuous=2, SimpleThenCompounded=3, CompoundedThenSimple=4.
    pub compounding: i32,
    /// Annual=0, Semiannual=1, Quarterly=2, Monthly=3.
    pub frequency: i32,
    /// Zero uses the bond's checked default settlement. Nonzero is actual settlement.
    pub settlement: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinBondZSpreadAnalyticsConfig {
    pub discount: u64,
    pub compounding: i32,
    pub frequency: i32,
    pub settlement: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinAnalyticsSolverConfig {
    /// Presence bits: accuracy=1, max_evaluations=2, guess=4. Omitted values use core defaults.
    pub flags: u32,
    pub accuracy: f64,
    pub max_evaluations: u64,
    pub guess: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCashFlowsZSpreadConfig {
    pub discount: u64,
    pub settings: u64,
    /// -1 unset, 0 false, 1 true.
    pub include_settlement: i32,
    /// Zero means omitted; nonzero dates are actual dates without adjustment.
    pub settlement: i32,
    pub npv_date: i32,
    pub compounding: i32,
    pub frequency: i32,
}
pub(crate) fn compounding(value: i32) -> BindingResult<Compounding> {
    match value {
        0 => Ok(Compounding::Simple),
        1 => Ok(Compounding::Compounded),
        2 => Ok(Compounding::Continuous),
        3 => Ok(Compounding::SimpleThenCompounded),
        4 => Ok(Compounding::CompoundedThenSimple),
        _ => Err(BindingError::invalid("invalid compounding convention")),
    }
}
fn price(value: f64, kind: i32) -> BindingResult<BondPrice> {
    let value = finite(value)?;
    match kind {
        0 => Ok(BondPrice::Clean(value)),
        1 => Ok(BondPrice::Dirty(value)),
        _ => Err(BindingError::invalid("invalid bond price type")),
    }
}
struct Solver {
    accuracy: Option<f64>,
    evaluations: Option<usize>,
    guess: Option<f64>,
}
fn solver(a: ItofinAnalyticsSolverConfig) -> BindingResult<Solver> {
    if a.flags & !7 != 0 {
        return Err(BindingError::invalid("invalid solver presence flags"));
    }
    let accuracy = if a.flags & 1 != 0 {
        if finite(a.accuracy)? <= 0.0 {
            return Err(BindingError::invalid("accuracy must be positive"));
        }
        Some(a.accuracy)
    } else {
        None
    };
    let evaluations = if a.flags & 2 != 0 {
        let count = usize::try_from(a.max_evaluations)
            .map_err(|_| BindingError::invalid("evaluation limit overflow"))?;
        if count == 0 {
            return Err(BindingError::invalid("evaluation limit must be positive"));
        }
        Some(count)
    } else {
        None
    };
    let guess = if a.flags & 4 != 0 {
        Some(finite(a.guess)?)
    } else {
        None
    };
    Ok(Solver {
        accuracy,
        evaluations,
        guess,
    })
}
fn settlement(bond: &NativeBond, serial: i32, clean: bool) -> BindingResult<Date> {
    let actual = match optional_date(serial)? {
        Some(date) => date,
        None => bond.settlement(None)?,
    };
    if let Some(floating) = &bond.floating {
        let b = bond.bond.borrow();
        floating.guard_leg(b.cashflows(), b.settings(), Some(false), Some(actual))?;
        if clean {
            floating.guard_accrued(&b, actual)?;
        }
    }
    Ok(actual)
}
fn include(value: i32) -> BindingResult<Option<bool>> {
    match value {
        -1 => Ok(None),
        0 => Ok(Some(false)),
        1 => Ok(Some(true)),
        _ => Err(BindingError::invalid("invalid settlement flag")),
    }
}
#[unsafe(no_mangle)]
/// Flat-yield risk for ordinary fixed, zero and floating bonds, independent of engines.
/// Fields: 0 Simple duration, 1 Macaulay duration, 2 Modified duration,
/// 3 convexity, 4 basis-point value, 5 yield-value basis point.
/// BPV is signed currency value and YVBP is yield change per currency cent, not per 100.
/// Macaulay requires compounded yield. Settlement-date flows are excluded.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_risk(
    ctx: *mut Context,
    id: u64,
    field: i32,
    rate: f64,
    a: ItofinBondYieldAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !(0..=5).contains(&field) {
                return Err(BindingError::invalid("invalid bond risk field"));
            }
            let yield_rate = libitofin::interestrate::InterestRate::new(
                finite(rate)?,
                day_counter(c, a.day_counter)?,
                compounding(a.compounding)?,
                frequency(a.frequency)?,
            )?;
            let bond = c.get::<NativeBond>(id)?;
            bond.bond.plain()?;
            let actual = settlement(&bond, a.settlement, false)?;
            let native = bond.bond.borrow();
            let value = match field {
                0..=2 => {
                    use libitofin::cashflows::Duration;
                    let kind = match field {
                        0 => Duration::Simple,
                        1 => Duration::Macaulay,
                        _ => Duration::Modified,
                    };
                    BondFunctions::duration(&native, &yield_rate, kind, Some(actual))?
                }
                3 => BondFunctions::convexity(&native, &yield_rate, Some(actual))?,
                4 => BondFunctions::basis_point_value(&native, &yield_rate, Some(actual))?,
                _ => BondFunctions::yield_value_basis_point(&native, &yield_rate, Some(actual))?,
            };
            output(out, finite(value)?)
        })
    }
}
fn coupon_sensitivity_settlement(bond: &NativeBond, serial: i32) -> BindingResult<Date> {
    match optional_date(serial)? {
        Some(date) => Ok(date),
        None => bond.settlement(None),
    }
}
#[unsafe(no_mangle)]
/// Coupon-rate BPS per 100 of notional, not signed yield BPV. Coupon amounts are not read.
/// Settlement-date and ex-coupon flows are excluded. No pricing engine is required.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_bps_from_curve(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(id)?;
            bond.bond.plain()?;
            let actual = coupon_sensitivity_settlement(&bond, serial)?;
            let discount = curve(c, discount)?.current_link()?;
            let value = BondFunctions::bps(&bond.bond.borrow(), &*discount, Some(actual))?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Coupon-rate BPS per 100 under flat-yield discounting, not signed yield BPV.
/// Coupon amounts are not read. Settlement-date and ex-coupon flows are excluded.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_bps_from_yield(
    ctx: *mut Context,
    id: u64,
    rate: f64,
    a: ItofinBondYieldAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let rate = libitofin::interestrate::InterestRate::new(
                finite(rate)?,
                day_counter(c, a.day_counter)?,
                compounding(a.compounding)?,
                frequency(a.frequency)?,
            )?;
            let bond = c.get::<NativeBond>(id)?;
            bond.bond.plain()?;
            let actual = coupon_sensitivity_settlement(&bond, a.settlement)?;
            let value = BondFunctions::bps_at_yield(&bond.bond.borrow(), &rate, Some(actual))?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// The coupon rate repricing the bond's own curve NPV, not a par-price target.
/// Reads live coupon amounts without a pricing engine. Ordinary bonds only.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_atm_rate_from_curve(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(id)?;
            bond.bond.plain()?;
            let actual = settlement(&bond, serial, false)?;
            let discount = curve(c, discount)?.current_link()?;
            let value = BondFunctions::atm_rate(&bond.bond.borrow(), &*discount, Some(actual))?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Coupon rate repricing an explicit quote per 100 of current settlement notional.
/// Price kind is Clean=0 or Dirty=1. Clean includes settlement accrued interest.
/// Reads live coupon amounts, excludes settlement-date and ex-coupon flows.
/// Ordinary bonds only; the existing own-NPV ATM API is unchanged.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_atm_rate_from_price(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    amount: f64,
    price_kind: i32,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let price = price(amount, price_kind)?;
            let bond = c.get::<NativeBond>(id)?;
            bond.bond.plain()?;
            let actual = settlement(&bond, serial, matches!(price, BondPrice::Clean(_)))?;
            let discount = curve(c, discount)?.current_link()?;
            let value = BondFunctions::atm_rate_from_price(
                &bond.bond.borrow(),
                &*discount,
                price,
                Some(actual),
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_clean_price_from_curve(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(id)?;
            let discount = curve(c, discount)?.current_link()?;
            let actual = settlement(&bond, serial, true)?;
            let value = BondFunctions::clean_price(&bond.bond.borrow(), &*discount, Some(actual))?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_dirty_price_from_curve(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let bond = c.get::<NativeBond>(id)?;
            let discount = curve(c, discount)?.current_link()?;
            let actual = settlement(&bond, serial, false)?;
            let value = BondFunctions::dirty_price(&bond.bond.borrow(), &*discount, Some(actual))?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_clean_price_from_yield(
    ctx: *mut Context,
    id: u64,
    rate: f64,
    a: ItofinBondYieldAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let rate = finite(rate)?;
            let dc = day_counter(c, a.day_counter)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, true)?;
            let value = BondFunctions::clean_price_from_yield(
                &bond.bond.borrow(),
                rate,
                dc,
                comp,
                freq,
                Some(actual),
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_dirty_price_from_yield(
    ctx: *mut Context,
    id: u64,
    rate: f64,
    a: ItofinBondYieldAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let rate = finite(rate)?;
            let dc = day_counter(c, a.day_counter)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, false)?;
            let value = BondFunctions::dirty_price_from_yield(
                &bond.bond.borrow(),
                rate,
                dc,
                comp,
                freq,
                Some(actual),
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_bond_yield_rate(
    ctx: *mut Context,
    id: u64,
    target: f64,
    price_type: i32,
    a: ItofinBondYieldAnalyticsConfig,
    solve: ItofinAnalyticsSolverConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let target = price(target, price_type)?;
            let dc = day_counter(c, a.day_counter)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let solve = solver(solve)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, price_type == 0)?;
            let value = BondFunctions::yield_rate(
                &bond.bond.borrow(),
                target,
                dc,
                comp,
                freq,
                Some(actual),
                solve.accuracy,
                solve.evaluations,
                solve.guess,
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_clean_price_at_z_spread(
    ctx: *mut Context,
    id: u64,
    spread: f64,
    a: ItofinBondZSpreadAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let spread = finite(spread)?;
            let discount = curve(c, a.discount)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, true)?;
            let value = BondFunctions::clean_price_at_z_spread(
                &bond.bond.borrow(),
                discount,
                spread,
                comp,
                freq,
                Some(actual),
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_bond_dirty_price_at_z_spread(
    ctx: *mut Context,
    id: u64,
    spread: f64,
    a: ItofinBondZSpreadAnalyticsConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let spread = finite(spread)?;
            let discount = curve(c, a.discount)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, false)?;
            let value = BondFunctions::dirty_price_at_z_spread(
                &bond.bond.borrow(),
                discount,
                spread,
                comp,
                freq,
                Some(actual),
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_bond_z_spread(
    ctx: *mut Context,
    id: u64,
    target: f64,
    price_type: i32,
    a: ItofinBondZSpreadAnalyticsConfig,
    solve: ItofinAnalyticsSolverConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let target = price(target, price_type)?;
            let discount = curve(c, a.discount)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let solve = solver(solve)?;
            let bond = c.get::<NativeBond>(id)?;
            let actual = settlement(&bond, a.settlement, price_type == 0)?;
            let value = BondFunctions::z_spread(
                &bond.bond.borrow(),
                target,
                discount,
                comp,
                freq,
                Some(actual),
                solve.accuracy,
                solve.evaluations,
                solve.guess,
            )?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_cashflows_npv_at_z_spread(
    ctx: *mut Context,
    id: u64,
    value: f64,
    a: ItofinCashFlowsZSpreadConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = finite(value)?;
            let discount = curve(c, a.discount)?;
            let settings = settings(c, a.settings)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let include = include(a.include_settlement)?;
            let settlement = optional_date(a.settlement)?;
            let npv_date = optional_date(a.npv_date)?;

            let leg = c.get::<Leg>(id)?;
            if let Some(floating) =
                c.optional_metadata::<crate::floating_bond_api::NativeFloating>(id)
            {
                floating.guard_leg(&leg, &settings, include, settlement)?;
            }
            let result = CashFlows::npv_at_z_spread(
                &leg, discount, value, comp, freq, &settings, include, settlement, npv_date,
            )?;
            output(out, finite(result)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_cashflows_z_spread(
    ctx: *mut Context,
    id: u64,
    value: f64,
    a: ItofinCashFlowsZSpreadConfig,
    solve: ItofinAnalyticsSolverConfig,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = finite(value)?;
            let discount = curve(c, a.discount)?;
            let settings = settings(c, a.settings)?;
            let comp = compounding(a.compounding)?;
            let freq = frequency(a.frequency)?;
            let include = include(a.include_settlement)?;
            let settlement = optional_date(a.settlement)?;
            let npv_date = optional_date(a.npv_date)?;
            let solve = solver(solve)?;
            let leg = c.get::<Leg>(id)?;
            if let Some(floating) =
                c.optional_metadata::<crate::floating_bond_api::NativeFloating>(id)
            {
                floating.guard_leg(&leg, &settings, include, settlement)?;
            }
            let result = CashFlows::z_spread(
                &leg,
                value,
                discount,
                comp,
                freq,
                &settings,
                include,
                settlement,
                npv_date,
                solve.accuracy,
                solve.evaluations,
                solve.guess,
            )?;
            output(out, finite(result)?)
        })
    }
}

#[cfg(test)]
#[path = "bond_analytics_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bond_price_atm_api_tests.rs"]
mod price_atm_tests;
