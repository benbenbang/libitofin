//! Sensitivities of dependent quotes to a SimpleQuote bump.
//!
//! The bumped quotes are SimpleQuotes; the parameters may be any session
//! quote, simple or dependent. A parameter slot with no reference value is
//! skipped: its first and second derivatives are absent, not an error. Only a
//! parameter whose quote is invalid has no reference value when the values are
//! read from the parameters, and the C ABI cannot build an invalid quote, so in
//! practice that slot arises from supplied reference values only.
//!
//! There is no QuantLib test-suite oracle: the parameter forms were removed
//! upstream with `sensitivityanalysis.hpp`. The oracle is the analytic core
//! test `tests/parameter_sensitivity.rs`.
use crate::boundary::*;
use crate::market_api;
use crate::rates_api::finite;
use crate::sensitivity_api::kind;
use libitofin::quotes::SimpleQuote;
use libitofin::risk;
use libitofin::shared::Shared;
use libitofin::types::Real;

/// One parameter's first and, for a centered scheme, second derivative.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItofinParameterSensitivity {
    pub first: f64,
    pub second: f64,
    /// Zero for a skipped parameter, whose `first` is zero.
    pub has_first: u8,
    /// Zero for a skipped parameter or a one-sided scheme, whose `second` is zero.
    pub has_second: u8,
}
impl From<(Option<Real>, Option<Real>)> for ItofinParameterSensitivity {
    fn from((first, second): (Option<Real>, Option<Real>)) -> Self {
        Self {
            first: first.unwrap_or(0.0),
            second: second.unwrap_or(0.0),
            has_first: u8::from(first.is_some()),
            has_second: u8::from(second.is_some()),
        }
    }
}

#[unsafe(no_mangle)]
/// Sensitivities of `parameters` to a bump of the SimpleQuote `quote`.
/// `kind`: 0 one-sided, 1 centered. A null `reference_values` reads the
/// reference values from the parameters before the bump; otherwise
/// `reference_values` and `reference_has_value` each hold `reference_count`
/// entries, which must equal `parameter_count`, and a zero `has_value` entry
/// skips its parameter. Writes `parameter_count` results to `out`,
/// index-aligned with `parameters`. A zero shift is an error. The quote is
/// restored on success and error.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_parameter_sensitivity(
    ctx: *mut Context,
    quote: u64,
    parameters: *const u64,
    parameter_count: usize,
    reference_values: *const f64,
    reference_has_value: *const u8,
    reference_count: usize,
    shift: f64,
    kind: i32,
    out: *mut ItofinParameterSensitivity,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let kind = self::kind(kind)?;
            let references = if reference_values.is_null() {
                None
            } else {
                let values = input_slice(reference_values, reference_count)?;
                let has_value = input_slice(reference_has_value, reference_count)?;
                Some(
                    values
                        .iter()
                        .zip(has_value)
                        .map(|(&v, &has)| (has != 0).then(|| finite(v)).transpose())
                        .collect::<BindingResult<Vec<_>>>()?,
                )
            };
            let bumped = c.get::<Shared<SimpleQuote>>(quote)?;
            let parameters = input_slice(parameters, parameter_count)?
                .iter()
                .map(|&id| market_api::quote(c, id))
                .collect::<BindingResult<Vec<_>>>()?;
            let (first, second) = risk::parameter_sensitivity(
                &bumped,
                &parameters,
                references.as_deref(),
                finite(shift)?,
                kind,
            )?;
            let out = std::slice::from_raw_parts_mut(out, parameter_count);
            for (slot, value) in out.iter_mut().zip(first.into_iter().zip(second)) {
                *slot = value.into();
            }
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
/// `itofin_parameter_sensitivity` for each SimpleQuote of `quotes`, bumped one
/// at a time, with the reference values read once from the parameters. Writes
/// `quote_count * parameter_count` results to `out`, row-major: one row of
/// `parameter_count` results per quote. Same `kind`, shift and restore
/// conventions as `itofin_parameter_sensitivity`.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_parameter_sensitivity_vector(
    ctx: *mut Context,
    quotes: *const u64,
    quote_count: usize,
    parameters: *const u64,
    parameter_count: usize,
    shift: f64,
    kind: i32,
    out: *mut ItofinParameterSensitivity,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let kind = self::kind(kind)?;
            let total = quote_count
                .checked_mul(parameter_count)
                .ok_or_else(|| BindingError::invalid("slice length overflow"))?;
            let bumped = input_slice(quotes, quote_count)?
                .iter()
                .map(|&id| c.get::<Shared<SimpleQuote>>(id))
                .collect::<BindingResult<Vec<_>>>()?;
            let parameters = input_slice(parameters, parameter_count)?
                .iter()
                .map(|&id| market_api::quote(c, id))
                .collect::<BindingResult<Vec<_>>>()?;
            let (first, second) =
                risk::parameter_sensitivity_vector(&bumped, &parameters, finite(shift)?, kind)?;
            let out = std::slice::from_raw_parts_mut(out, total);
            for (slot, value) in out.iter_mut().zip(
                first
                    .into_iter()
                    .flatten()
                    .zip(second.into_iter().flatten()),
            ) {
                *slot = value.into();
            }
            Ok(())
        })
    }
}
