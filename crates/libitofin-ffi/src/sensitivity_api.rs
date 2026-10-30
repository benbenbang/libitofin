//! Bump-and-reprice sensitivities over session quotes and instruments.
//!
//! Supported instruments: every bond handle (fixed, zero, floating, amortizing
//! and callable), vanilla swaps and overnight indexed swaps. A bond runs the
//! same refresh and floating-coupon preflight as `itofin_bond_real` once,
//! before the first reprice; the bump loop then reprices through the core
//! `npv()`. The settings flags that refresh synchronizes cannot change inside
//! the loop, and the preflight guards depend on dates and fixings, not on quote
//! values.
//!
//! Deferred and rejected with an explicit error: options, swaptions,
//! cap/floors and CDS (#1322).
use crate::bond_api::NativeBond;
use crate::bond_backing::BondBacking;
use crate::boundary::*;
use crate::rates_api::{NativeOis, finite};
use libitofin::instrument::Instrument;
use libitofin::instruments::FixedVsFloatingSwap;
use libitofin::quotes::SimpleQuote;
use libitofin::risk::{self, SensitivityAnalysis};
use libitofin::shared::{Shared, SharedMut};
use libitofin::types::Real;

/// First and, for a centered scheme, second derivative of the aggregate NPV.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItofinSensitivity {
    pub first: f64,
    pub second: f64,
    /// Zero for a one-sided scheme, whose `second` is zero.
    pub has_second: u8,
}
impl From<(Real, Option<Real>)> for ItofinSensitivity {
    fn from((first, second): (Real, Option<Real>)) -> Self {
        Self {
            first,
            second: second.unwrap_or(0.0),
            has_second: u8::from(second.is_some()),
        }
    }
}

pub(crate) fn instrument(c: &Context, id: u64) -> BindingResult<SharedMut<dyn Instrument>> {
    if let Ok(native) = c.get::<NativeBond>(id) {
        native.refresh();
        let settlement = native.settlement(None)?;
        native.preflight(0, settlement, false)?;
        return Ok(match native.bond {
            BondBacking::Plain(bond) => bond,
            BondBacking::CallableFixed(bond) => bond,
            BondBacking::CallableZero(bond) => bond,
        });
    }
    if let Ok(swap) = c.get::<SharedMut<FixedVsFloatingSwap>>(id) {
        return Ok(swap);
    }
    if let Ok(ois) = c.get::<NativeOis>(id) {
        return Ok(ois.0);
    }
    Err(BindingError::invalid(
        "sensitivity analysis supports bonds, vanilla swaps and overnight indexed swaps; \
         options, swaptions, cap/floors and CDS are deferred (#1322)",
    ))
}
fn kind(value: i32) -> BindingResult<SensitivityAnalysis> {
    match value {
        0 => Ok(SensitivityAnalysis::OneSide),
        1 => Ok(SensitivityAnalysis::Centered),
        _ => Err(BindingError::invalid("invalid sensitivity analysis kind")),
    }
}
type Instruments = Vec<SharedMut<dyn Instrument>>;
/// # Safety
/// Each pointer must reference its stated count of initialized ids.
unsafe fn inputs(
    c: &Context,
    quotes: (*const u64, usize),
    instruments: (*const u64, usize),
) -> BindingResult<(Vec<Shared<SimpleQuote>>, Instruments)> {
    let quotes = unsafe { input_slice(quotes.0, quotes.1)? }
        .iter()
        .map(|&id| c.get(id))
        .collect::<BindingResult<_>>()?;
    let instruments = unsafe { input_slice(instruments.0, instruments.1)? }
        .iter()
        .map(|&id| instrument(c, id))
        .collect::<BindingResult<_>>()?;
    Ok((quotes, instruments))
}
#[unsafe(no_mangle)]
/// Parallel PV01: every quote bumped together. `kind`: 0 one-sided, 1 centered.
/// A null `reference_npv` computes the reference from the instruments; empty
/// `quantities` mean unit weights. Quotes are restored on success and error.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_sensitivity_parallel(
    ctx: *mut Context,
    quotes: *const u64,
    quote_count: usize,
    instruments: *const u64,
    instrument_count: usize,
    quantities: *const f64,
    quantity_count: usize,
    shift: f64,
    kind: i32,
    reference_npv: *const f64,
    out: *mut ItofinSensitivity,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let kind = self::kind(kind)?;
            let reference = if reference_npv.is_null() {
                None
            } else {
                check_ptr(reference_npv)?;
                Some(finite(*reference_npv)?)
            };
            let (quotes, instruments) =
                inputs(c, (quotes, quote_count), (instruments, instrument_count))?;
            let result = risk::parallel_analysis(
                &quotes,
                &instruments,
                input_slice(quantities, quantity_count)?,
                finite(shift)?,
                kind,
                reference,
            )?;
            output(out, result.into())
        })
    }
}
#[unsafe(no_mangle)]
/// Bucket PV01: each quote bumped alone, with one shared reference NPV. Writes
/// `quote_count` results to `out`, index-aligned with `quotes`. Same `kind`,
/// `quantities` and restore conventions as `itofin_sensitivity_parallel`.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_sensitivity_bucket(
    ctx: *mut Context,
    quotes: *const u64,
    quote_count: usize,
    instruments: *const u64,
    instrument_count: usize,
    quantities: *const f64,
    quantity_count: usize,
    shift: f64,
    kind: i32,
    out: *mut ItofinSensitivity,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let kind = self::kind(kind)?;
            let (quotes, instruments) =
                inputs(c, (quotes, quote_count), (instruments, instrument_count))?;
            let (first, second) = risk::bucket_analysis_vector(
                &quotes,
                &instruments,
                input_slice(quantities, quantity_count)?,
                finite(shift)?,
                kind,
            )?;
            let out = std::slice::from_raw_parts_mut(out, quote_count);
            for (slot, value) in out.iter_mut().zip(first.into_iter().zip(second)) {
                *slot = value.into();
            }
            Ok(())
        })
    }
}
