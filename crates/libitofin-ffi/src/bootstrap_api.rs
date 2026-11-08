//! Global bootstrap construction with retained callbacks and quote variables.
use crate::bootstrap_callbacks::{Callbacks, ItofinBootstrapCallbacks, ItofinBootstrapState};
use crate::bootstrap_variables::Variables;
use crate::boundary::*;
use crate::iterative_bootstrap_api::{CurveAnchor, CurveJumps, anchored, moving_anchor};
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::{linear::Linear, loglinear::LogLinear};
use libitofin::quotes::Quote;
use libitofin::shared::shared;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::globalbootstrap::GlobalBootstrap;
use std::cell::Cell;

/// Construct a global discount curve, kind 0 log-linear or 1 linear.
/// Callback ownership transfers exactly when `adopted` becomes true, including
/// failed construction; the last native curve owner releases the callbacks.
/// Initialize `adopted` to false before calling, including when retrying.
/// # Safety
/// Follow the crate-level pointer/context contract. Callback functions must remain
/// valid until release; borrowed callback inputs/output cannot escape their call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_global_curve_new(
    ctx: *mut Context,
    reference: i32,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    additional: *const u64,
    additional_len: usize,
    variables: u64,
    callbacks: *const ItofinBootstrapCallbacks,
    adopted: *mut bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        let anchor =
            move |_: &Context| Ok((CurveAnchor::Fixed(date(reference)?), CurveJumps::default()));
        global_curve(
            ctx,
            anchor,
            helpers,
            len,
            dc,
            kind,
            additional,
            additional_len,
            variables,
            callbacks,
            adopted,
            out,
            error,
        )
    }
}

/// The itofin_global_curve_new twin whose reference date moves with the
/// evaluation date of settings, settlement_days business days on calendar;
/// settings must be the instance the helpers' index carries. Callback
/// ownership follows itofin_global_curve_new.
/// # Safety
/// As itofin_global_curve_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_global_curve_new_moving(
    ctx: *mut Context,
    settlement_days: u32,
    calendar: u64,
    settings: u64,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    additional: *const u64,
    additional_len: usize,
    variables: u64,
    callbacks: *const ItofinBootstrapCallbacks,
    adopted: *mut bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        let anchor = move |c: &Context| {
            Ok((
                moving_anchor(c, settlement_days, calendar, settings)?,
                CurveJumps::default(),
            ))
        };
        global_curve(
            ctx,
            anchor,
            helpers,
            len,
            dc,
            kind,
            additional,
            additional_len,
            variables,
            callbacks,
            adopted,
            out,
            error,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn global_curve(
    ctx: *mut Context,
    anchor: impl FnOnce(&Context) -> BindingResult<(CurveAnchor, CurveJumps)>,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    additional: *const u64,
    additional_len: usize,
    variables: u64,
    callbacks: *const ItofinBootstrapCallbacks,
    adopted: *mut bool,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(callbacks)?;
            check_ptr(adopted)?;
            let callbacks = *callbacks;
            if callbacks.release.is_none() {
                return Err(BindingError::invalid(
                    "callback release function is required",
                ));
            }
            output(adopted, true)?;
            let callbacks = shared(Callbacks(callbacks));
            let (anchor, jumps) = anchor(c)?;
            let dc = day_counter(c, dc)?;
            let helpers = input_slice(helpers, len)?
                .iter()
                .map(|id| crate::helpers_api::helper(c, *id))
                .collect::<BindingResult<Vec<_>>>()?;
            let additional = input_slice(additional, additional_len)?
                .iter()
                .map(|id| crate::helpers_api::helper(c, *id))
                .collect::<BindingResult<Vec<_>>>()?;
            let variables = if variables == 0 {
                None
            } else {
                Some(c.get::<Variables>(variables)?)
            };
            let quotes = variables
                .as_ref()
                .map_or_else(Vec::new, |v| v.quotes.clone());
            let residual_count = shared(Cell::new(None));
            let date_count = residual_count.clone();
            let date_callbacks = callbacks.clone();
            let dates = Box::new(move || {
                date_count.set(None);
                date_callbacks
                    .call(date_callbacks.0.dates, "additional_dates", None)?
                    .into_iter()
                    .map(|serial| {
                        libitofin::require!(
                            serial.is_finite()
                                && serial.fract() == 0.0
                                && serial >= i32::MIN as f64
                                && serial <= i32::MAX as f64,
                            "additional_dates must return valid date serials"
                        );
                        date(serial as i32).map_err(|e| {
                            libitofin::errors::QlError::new(e.message, file!(), line!())
                        })
                    })
                    .collect()
            });
            let penalty_helpers = additional.clone();
            let mut bootstrap = GlobalBootstrap::with_fallible_penalties(
                additional,
                Some(dates),
                None,
                None,
                Vec::new(),
                move |times, data| {
                    let quote_values = quotes
                        .iter()
                        .map(|q| q.value())
                        .collect::<libitofin::errors::QlResult<Vec<_>>>()?;
                    let helper_errors = if callbacks.0.penalties.is_some() {
                        penalty_helpers
                            .iter()
                            .map(|h| h.quote_error())
                            .collect::<libitofin::errors::QlResult<Vec<_>>>()?
                    } else {
                        Vec::new()
                    };
                    let state = ItofinBootstrapState {
                        times: times.as_ptr(),
                        data: data.as_ptr(),
                        node_count: times.len(),
                        quote_values: quote_values.as_ptr(),
                        quote_count: quote_values.len(),
                        helper_errors: helper_errors.as_ptr(),
                        helper_count: helper_errors.len(),
                    };
                    let values = callbacks.call(
                        callbacks.0.penalties,
                        "additional_penalties",
                        Some(&state),
                    )?;
                    libitofin::require!(
                        values.iter().all(|v| v.is_finite()),
                        "additional_penalties must return finite residuals"
                    );
                    libitofin::require!(
                        residual_count
                            .get()
                            .is_none_or(|count| count == values.len()),
                        "additional_penalties residual count changed during calculation"
                    );
                    residual_count.set(Some(values.len()));
                    Ok(values)
                },
            );
            if let Some(variables) = variables {
                bootstrap = bootstrap.with_additional_variables(Box::new(variables.build()?));
            }
            let curve = match kind {
                0 => anchored::<Discount, _, _>(anchor, helpers, dc, LogLinear, bootstrap, jumps)?,
                1 => anchored::<Discount, _, _>(anchor, helpers, dc, Linear, bootstrap, jumps)?,
                _ => {
                    return Err(BindingError::invalid(
                        "global callbacks require a log-linear or linear discount curve",
                    ));
                }
            };
            output(out, c.insert(Handle::new(curve))?)
        })
    }
}
