//! Cap/floor term-volatility stripping and the optionlet adapter.
use crate::boundary::*;
use crate::settings_api::settings;
use crate::smile_api::volatility_type;
use crate::time_api::time_unit;
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    CapFloorTermVolSurface, OptionletStripper1, OptionletVolatilityStructure,
    StrippedOptionletAdapter, StrippedOptionletBase,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::period::Period;
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_stripper_new(
    ctx: *mut Context,
    surface: u64,
    index: u64,
    kind: i32,
    accuracy: f64,
    max_iter: u32,
    displacement: f64,
    discount: u64,
    frequency_length: i32,
    frequency_unit: i32,
    has_frequency: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !accuracy.is_finite()
                || accuracy <= 0.0
                || !displacement.is_finite()
                || max_iter == 0
            {
                return Err(BindingError::invalid("invalid stripping solver parameters"));
            }
            let frequency = match has_frequency {
                0 => None,
                1 => Some(Period::new(frequency_length, time_unit(frequency_unit)?)),
                _ => return Err(BindingError::invalid("frequency presence must be 0 or 1")),
            };
            let discount = if discount == 0 {
                Handle::<dyn YieldTermStructure>::empty()
            } else {
                c.get(discount)?
            };
            let stripper = OptionletStripper1::new(
                c.get::<Shared<CapFloorTermVolSurface>>(surface)?,
                crate::indexes_api::ibor_index(c, index)?,
                discount,
                accuracy,
                max_iter,
                volatility_type(kind)?,
                displacement,
                frequency,
            )?;
            output(out, c.insert(shared(stripper))?)
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_stripper_switch_strike(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.get::<Shared<OptionletStripper1>>(id)?.switch_strike()?,
            )
        })
    }
}
/// Pass null output/capacity zero to size the caller-owned rates buffer.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_stripper_rates(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    capacity: usize,
    written: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(written)?;
            let rates = c
                .get::<Shared<OptionletStripper1>>(id)?
                .atm_optionlet_rates()?;
            output(written, rates.len())?;
            if out.is_null() && capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < rates.len() {
                return Err(BindingError::invalid("rates buffer too small"));
            }
            for (i, v) in rates.iter().enumerate() {
                out.add(i).write(*v);
            }
            Ok(())
        })
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_stripped_optionlet_adapter_new(
    ctx: *mut Context,
    stripper: u64,
    setting: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let base =
                c.get::<Shared<OptionletStripper1>>(stripper)? as Shared<dyn StrippedOptionletBase>;
            let v = shared(StrippedOptionletAdapter::new(base, settings(c, setting)?)?)
                as Shared<dyn OptionletVolatilityStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
