//! Explicit evaluation settings retained by dependent instruments.
use crate::boundary::*;
use crate::time_api::date;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Date;

pub(crate) fn settings(c: &Context, id: u64) -> BindingResult<Shared<Settings<Date>>> {
    c.get(id)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_settings_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(shared(Settings::<Date>::new()))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_settings_set_evaluation_date(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            settings(c, id)?.set_evaluation_date(date(serial)?);
            Ok(())
        })
    }
}
/// -1 means unset, 0 means exclude, 1 means include.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_settings_set_include_todays_cash_flows(
    ctx: *mut Context,
    id: u64,
    value: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let value = match value {
                -1 => None,
                0 => Some(false),
                1 => Some(true),
                _ => return Err(BindingError::invalid("cash-flow flag must be -1, 0 or 1")),
            };
            settings(c, id)?.set_include_todays_cash_flows(value);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_settings_include_todays_cash_flows(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                settings(c, id)?
                    .include_todays_cash_flows()
                    .map(i32::from)
                    .unwrap_or(-1),
            )
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_settings_are_independent_and_retained() {
        let mut c = Context::new();
        let a = c.insert(shared(Settings::<Date>::new())).unwrap();
        let b = c.insert(shared(Settings::<Date>::new())).unwrap();
        let retained = settings(&c, a).unwrap();
        unsafe {
            assert_eq!(
                itofin_settings_set_evaluation_date(&mut c, a, 45_000, std::ptr::null_mut()),
                0
            );
            assert_eq!(
                itofin_settings_set_include_todays_cash_flows(&mut c, a, 2, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_settings_set_include_todays_cash_flows(&mut c, a, 1, std::ptr::null_mut()),
                0
            );
            assert_eq!(itofin_handle_release(&mut c, a, std::ptr::null_mut()), 0);
        }
        assert_eq!(retained.evaluation_date(), Some(date(45_000).unwrap()));
        assert_eq!(retained.include_todays_cash_flows(), Some(true));
        assert_eq!(settings(&c, b).unwrap().evaluation_date(), None);
        assert!(settings(&c, a).is_err());
    }
}
