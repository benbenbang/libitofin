//! Standalone currency specifications with counted UTF-8 strings.
use crate::boundary::*;
use libitofin::currency::Currency;

/// Owned-on-construction UTF-8 fields. Code must contain three ASCII letters.
#[repr(C)]
pub struct ItofinCurrencyConfig {
    pub name: *const u8,
    pub name_length: usize,
    pub code: *const u8,
    pub code_length: usize,
    pub numeric_code: i32,
    pub symbol: *const u8,
    pub symbol_length: usize,
    pub fraction_symbol: *const u8,
    pub fraction_symbol_length: usize,
    pub fractions_per_unit: i32,
}

unsafe fn text(ptr: *const u8, len: usize) -> BindingResult<String> {
    std::str::from_utf8(unsafe { input_slice(ptr, len)? })
        .map(str::to_owned)
        .map_err(|_| BindingError::invalid("currency field must be UTF-8"))
}

/// Construct a reusable currency, copying all input bytes. No ISO registry lookup.
/// Numeric fields retain the core's signed integer semantics.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_currency_custom_new(
    ctx: *mut Context,
    config: *const ItofinCurrencyConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let code = text(a.code, a.code_length)?;
            if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_alphabetic()) {
                return Err(BindingError::invalid(
                    "currency code needs three ASCII letters",
                ));
            }
            let value = Currency::new(
                text(a.name, a.name_length)?,
                code,
                a.numeric_code,
                text(a.symbol, a.symbol_length)?,
                text(a.fraction_symbol, a.fraction_symbol_length)?,
                a.fractions_per_unit,
            );
            output(out, c.insert(value)?)
        })
    }
}

/// UTF-8 bytes without a trailing NUL, retaining embedded NULs.
/// Field: 0 name, 1 code, 2 symbol, 3 fraction symbol. Capacity zero queries length.
/// On error neither the buffer nor required length is changed.
/// # Safety
/// Follow the crate-level C caller contract, including non-overlapping outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_currency_text(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut u8,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            let value = c.get::<Currency>(id)?;
            let text = match field {
                0 => value.name(),
                1 => value.code(),
                2 => value.symbol(),
                3 => value.fraction_symbol(),
                _ => return Err(BindingError::invalid("unknown currency text field")),
            };
            if capacity != 0 {
                if capacity < text.len() {
                    return Err(BindingError::invalid("currency text buffer too small"));
                }
                check_ptr(out)?;
                std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
            }
            output(required, text.len())
        })
    }
}

/// Field: 0 numeric code, 1 fractions per unit.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_currency_integer(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = c.get::<Currency>(id)?;
            output(
                out,
                match field {
                    0 => value.numeric_code(),
                    1 => value.fractions_per_unit(),
                    _ => return Err(BindingError::invalid("unknown currency integer field")),
                },
            )
        })
    }
}

#[cfg(test)]
mod tests;
