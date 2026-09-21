//! Bermudan exercise schedules and the concrete Hull-White swaption tree engine.
use crate::boundary::*;
use crate::settings_api::settings;
use crate::time_api::date;
use libitofin::exercise::{BermudanExercise, Exercise};
use libitofin::models::HullWhite;
use libitofin::pricingengines::swaption::TreeSwaptionEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};

#[unsafe(no_mangle)]
/// Copy and sort the exercise dates, rejecting an empty schedule.
/// # Safety
/// Pointers must be valid, aligned and non-overlapping for their stated sizes.
/// Context and handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_bermudan_exercise_new(
    ctx: *mut Context,
    dates: *const i32,
    count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dates = input_slice(dates, count)?
                .iter()
                .map(|&d| date(d))
                .collect::<BindingResult<Vec<_>>>()?;
            let exercise = shared(BermudanExercise::new(dates, false)?) as Shared<dyn Exercise>;
            output(out, c.insert(exercise)?)
        })
    }
}

#[unsafe(no_mangle)]
/// Copy the sorted dates; capacity zero returns the required length.
/// # Safety
/// Pointers must be valid, aligned and non-overlapping for their stated sizes.
/// Context and handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_bermudan_exercise_dates(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let exercise = c.get::<Shared<dyn Exercise>>(id)?;
            let dates = exercise.dates();
            output(required, dates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < dates.len() {
                return Err(BindingError::invalid("date buffer too small"));
            }
            check_ptr(out)?;
            for (i, d) in dates.iter().enumerate() {
                output(out.add(i), d.serial_number())?;
            }
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
/// Retain the model and settings; steps must be positive.
/// # Safety
/// Pointers must be valid, aligned and non-overlapping for their stated sizes.
/// Context and handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_tree_swaption_engine_new(
    ctx: *mut Context,
    model: u64,
    steps: usize,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let engine = TreeSwaptionEngine::new(
                c.get::<SharedMut<HullWhite>>(model)?,
                steps,
                settings(c, settings_id)?,
            )?;
            output(out, c.insert(shared_mut(engine))?)
        })
    }
}

#[unsafe(no_mangle)]
/// Select par (true) or indexed Ibor forecasting before constructing instruments.
/// Existing cached prices are not invalidated by this setting.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_settings_set_using_at_par_coupons(
    ctx: *mut Context,
    id: u64,
    value: bool,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            settings(c, id)?.set_using_at_par_coupons(value);
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
/// Read the par Ibor coupon forecasting flag.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_settings_using_at_par_coupons(
    ctx: *mut Context,
    id: u64,
    out: *mut bool,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(out, settings(c, id)?.using_at_par_coupons())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_api::itofin_settings_new;
    use std::ptr::{null, null_mut};

    #[test]
    fn tree_swaption_c_inputs_copy_dates_and_recover_after_errors() {
        let mut c = Context::new();
        let mut exercise = 0;
        let mut settings_id = 0;
        let dates = [45_010, 45_000, 45_000];
        unsafe {
            assert_eq!(
                itofin_bermudan_exercise_new(&mut c, null(), 1, &mut exercise, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_bermudan_exercise_new(&mut c, null(), 0, &mut exercise, null_mut()),
                CORE_ERROR
            );
            assert_eq!(
                itofin_bermudan_exercise_new(
                    &mut c,
                    [i32::MAX].as_ptr(),
                    1,
                    &mut exercise,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_bermudan_exercise_new(
                    &mut c,
                    dates.as_ptr(),
                    dates.len(),
                    &mut exercise,
                    null_mut()
                ),
                0
            );
            let mut size = 0;
            assert_eq!(
                itofin_bermudan_exercise_dates(
                    &mut c,
                    exercise,
                    null_mut(),
                    0,
                    &mut size,
                    null_mut()
                ),
                0
            );
            assert_eq!(size, 3);
            let mut copied = [0; 3];
            assert_eq!(
                itofin_bermudan_exercise_dates(
                    &mut c,
                    exercise,
                    copied.as_mut_ptr(),
                    2,
                    &mut size,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_bermudan_exercise_dates(
                    &mut c,
                    exercise,
                    copied.as_mut_ptr(),
                    3,
                    &mut size,
                    null_mut()
                ),
                0
            );
            assert_eq!(copied, [45_000, 45_000, 45_010]);
            assert_eq!(itofin_settings_new(&mut c, &mut settings_id, null_mut()), 0);
            assert_eq!(
                itofin_settings_set_using_at_par_coupons(&mut c, settings_id, false, null_mut()),
                0
            );
            let mut mode = true;
            assert_eq!(
                itofin_settings_using_at_par_coupons(&mut c, settings_id, &mut mode, null_mut()),
                0
            );
            assert!(!mode);
            let mut engine = 0;
            assert_eq!(
                itofin_tree_swaption_engine_new(
                    &mut c,
                    exercise,
                    50,
                    settings_id,
                    &mut engine,
                    null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_tree_swaption_engine_new(&mut c, 0, 50, settings_id, null_mut(), null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(itofin_handle_release(&mut c, exercise, null_mut()), 0);
            assert_eq!(
                itofin_bermudan_exercise_dates(
                    &mut c,
                    exercise,
                    null_mut(),
                    0,
                    &mut size,
                    null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_settings_set_using_at_par_coupons(&mut c, settings_id, true, null_mut()),
                0
            );
        }
    }
}
