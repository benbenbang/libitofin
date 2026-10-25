//! Immutable scalar payoff adapters over the existing five safe core payoffs.
use crate::boundary::*;
use crate::options_api::option_type;
use libitofin::instruments::{
    AssetOrNothingPayoff, CashOrNothingPayoff, GapPayoff, PercentageStrikePayoff,
    PlainVanillaPayoff, StrikedTypePayoff,
};
use std::rc::Rc;

/// Stored scalar terms. Kinds: vanilla=0, cash=1, asset=2, gap=3, percentage=4.
/// Option kind is call=0 or put=1. Extra is cash payoff for cash, payment strike
/// for gap, and zero otherwise. Percentage strike is a dimensionless ratio.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinScalarPayoffInfo {
    pub kind: i32,
    pub option_kind: i32,
    pub strike: f64,
    pub extra: f64,
}

#[derive(Clone)]
struct ScalarPayoff {
    inner: Rc<dyn StrikedTypePayoff>,
    info: ItofinScalarPayoffInfo,
}

/// Copy scalar terms into an immutable payoff. All inputs must be finite;
/// signed prices, strikes and amounts are supported. Extra must be zero for
/// vanilla, asset and percentage kinds. This handle is not an engine input.
/// # Safety
/// Follow the crate-level caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_scalar_payoff_new(
    ctx: *mut Context,
    kind: i32,
    option_kind: i32,
    strike: f64,
    extra: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !strike.is_finite() || !extra.is_finite() {
                return Err(BindingError::invalid("payoff parameters must be finite"));
            }
            let option = option_type(option_kind)?;
            let inner: Rc<dyn StrikedTypePayoff> = match kind {
                0 | 2 | 4 if extra != 0.0 => {
                    return Err(BindingError::invalid("unused payoff extra must be zero"));
                }
                0 => Rc::new(PlainVanillaPayoff::new(option, strike)),
                1 => Rc::new(CashOrNothingPayoff::new(option, strike, extra)),
                2 => Rc::new(AssetOrNothingPayoff::new(option, strike)),
                3 => Rc::new(GapPayoff::new(option, strike, extra)),
                4 => Rc::new(PercentageStrikePayoff::new(option, strike)),
                _ => return Err(BindingError::invalid("invalid scalar payoff kind")),
            };
            let info = ItofinScalarPayoffInfo {
                kind,
                option_kind,
                strike,
                extra,
            };
            output(out, c.insert(ScalarPayoff { inner, info })?)
        })
    }
}

/// Evaluate at a finite signed price, rejecting nonfinite computed results.
/// Cash/asset triggers are strict; gap triggers are inclusive and may pay negative.
/// Percentage payoff multiplies its floored ratio factor by the signed price.
/// # Safety
/// Follow the crate-level caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_scalar_payoff_value(
    ctx: *mut Context,
    payoff: u64,
    price: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !price.is_finite() {
                return Err(BindingError::invalid("payoff price must be finite"));
            }
            let value = c.get::<ScalarPayoff>(payoff)?.inner.value(price);
            if !value.is_finite() {
                return Err(BindingError::invalid("payoff value must be finite"));
            }
            output(out, value)
        })
    }
}

/// Copy stored terms without evaluating the payoff.
/// # Safety
/// Follow the crate-level caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_scalar_payoff_info(
    ctx: *mut Context,
    payoff: u64,
    out: *mut ItofinScalarPayoffInfo,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            output(out, c.get::<ScalarPayoff>(payoff)?.info)
        })
    }
}
