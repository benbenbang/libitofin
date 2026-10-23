//! Snapshot-spot FX forwards, with explicit dates and retained live curves.
use crate::boundary::*;
use crate::rates_api::curve;
use crate::time_api::date;
use libitofin::currency::Currency;
use libitofin::exchangerate::ExchangeRate;
use libitofin::fxforward::FxForward;
use libitofin::shared::{SharedMut, shared_mut};

/// Quote-per-base spot/strike, positive base notional, fixed unadjusted dates.
#[repr(C)]
pub struct ItofinFxForwardConfig {
    /// Typed Currency handle copied as the base/source specification.
    pub base_currency: u64,
    /// Typed Currency handle copied as the quote/target NPV denomination.
    pub quote_currency: u64,
    /// Immutable finite positive quote-currency units per one base unit.
    pub spot: f64,
    /// Typed yield-curve Handle retained for live base-currency discounting.
    pub base_curve: u64,
    /// Typed yield-curve Handle retained for live quote-currency discounting.
    pub quote_curve: u64,
    /// Finite positive notional denominated in the base currency.
    pub base_notional: f64,
    /// Finite positive contractual quote-per-base delivery rate.
    pub strike: f64,
    /// Fixed unadjusted spot-value-date serial, valid in 1901-2199.
    pub spot_settlement_date: i32,
    /// Fixed unadjusted delivery-date serial, on or after spot settlement.
    pub delivery_date: i32,
    /// 0 receives base and pays quote; 1 reverses both legs.
    pub position: i32,
}

/// Retain both live curves and copy both currencies and the spot snapshot.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fx_forward_new(
    ctx: *mut Context,
    a: ItofinFxForwardConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let long = match a.position {
                0 => true,
                1 => false,
                _ => return Err(BindingError::invalid("invalid FX position")),
            };
            let spot = ExchangeRate::checked_new(
                c.get::<Currency>(a.base_currency)?,
                c.get::<Currency>(a.quote_currency)?,
                a.spot,
            )?;
            let base = curve(c, a.base_curve)?;
            let quote = curve(c, a.quote_curve)?;
            let inner = FxForward::new(
                spot,
                base,
                quote,
                a.base_notional,
                a.strike,
                date(a.spot_settlement_date)?,
                date(a.delivery_date)?,
                long,
            )?;
            output(out, c.insert(shared_mut(inner))?)
        })
    }
}

/// Fields: 0 spot, 1 strike, 2 base notional, 3 fair forward, 4 quote-currency NPV.
/// Only NPV uses evaluation_date; NPV is at the quote curve reference date.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fx_forward_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    evaluation_date: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let object = c.get::<SharedMut<FxForward>>(id)?;
            let object = object.borrow();
            let value = match field {
                0 => object.spot().rate(),
                1 => object.strike(),
                2 => object.base_notional(),
                3 => object.fair_forward_rate()?,
                4 => object.npv(date(evaluation_date)?)?.value(),
                _ => return Err(BindingError::invalid("invalid FX value field")),
            };
            output(out, value)
        })
    }
}

/// Fields: 0 spot settlement, 1 delivery, 2 position (0 long/1 short), 3 expiry.
/// Only expiry uses evaluation_date. Delivery on evaluation remains active.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fx_forward_integer(
    ctx: *mut Context,
    id: u64,
    field: i32,
    evaluation_date: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let object = c.get::<SharedMut<FxForward>>(id)?;
            let object = object.borrow();
            let value = match field {
                0 => object.spot_settlement_date().serial_number(),
                1 => object.delivery_date().serial_number(),
                2 => i32::from(!object.is_long()),
                3 => i32::from(object.is_expired(date(evaluation_date)?)?),
                _ => return Err(BindingError::invalid("invalid FX integer field")),
            };
            output(out, value)
        })
    }
}

/// Copy an endpoint currency into a new owned handle. Side 0 base, 1 quote.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fx_forward_currency(
    ctx: *mut Context,
    id: u64,
    side: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let object = c.get::<SharedMut<FxForward>>(id)?;
            let object = object.borrow();
            let value = match side {
                0 => object.spot().source(),
                1 => object.spot().target(),
                _ => return Err(BindingError::invalid("invalid FX currency side")),
            }
            .clone();
            output(out, c.insert(value)?)
        })
    }
}
