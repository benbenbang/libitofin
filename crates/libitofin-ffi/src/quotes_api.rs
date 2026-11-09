//! Dependent quotes: DerivedQuote and CompositeQuote over a fixed function set.
//!
//! The core types are generic over any closure, which cannot cross the C ABI,
//! so the binding fixes the functions: derived `a * x + b` and composite
//! add, subtract or multiply. Divide and power are not bound. Dependent quotes
//! are read-only: `itofin_quote_value` reads them and every read-only quote
//! input accepts them, while `itofin_quote_set` and the sensitivity bumps take
//! a SimpleQuote only.
use crate::boundary::*;
use crate::market_api::quote;
use libitofin::quotes::{CompositeQuote, DerivedQuote, Quote};
use libitofin::shared::{Shared, shared};
use libitofin::types::Real;

/// Composite operation codes for `itofin_composite_quote_new`.
pub const ITOFIN_COMPOSITE_ADD: i32 = 0;
pub const ITOFIN_COMPOSITE_SUBTRACT: i32 = 1;
pub const ITOFIN_COMPOSITE_MULTIPLY: i32 = 2;

#[unsafe(no_mangle)]
/// Build a quote worth `a * source + b`, recomputed when `source` changes.
/// `source` may be a SimpleQuote or another dependent quote.
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_derived_quote_new(
    ctx: *mut Context,
    source: u64,
    a: f64,
    b: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let derived = DerivedQuote::new(quote(c, source)?, move |x: Real| a * x + b);
            output(out, c.insert(shared(derived) as Shared<dyn Quote>)?)
        })
    }
}

#[unsafe(no_mangle)]
/// Build a quote combining `q1` and `q2` with `op`: 0 add, 1 subtract
/// (`q1 - q2`), 2 multiply; any other code is an error. Pass the same quote
/// twice for a square. Either source may be a dependent quote.
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_composite_quote_new(
    ctx: *mut Context,
    q1: u64,
    q2: u64,
    op: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let f: fn(Real, Real) -> Real = match op {
                ITOFIN_COMPOSITE_ADD => |x, y| x + y,
                ITOFIN_COMPOSITE_SUBTRACT => |x, y| x - y,
                ITOFIN_COMPOSITE_MULTIPLY => |x, y| x * y,
                _ => return Err(BindingError::invalid("unknown composite quote operation")),
            };
            let composite = CompositeQuote::new(quote(c, q1)?, quote(c, q2)?, f);
            output(out, c.insert(shared(composite) as Shared<dyn Quote>)?)
        })
    }
}
