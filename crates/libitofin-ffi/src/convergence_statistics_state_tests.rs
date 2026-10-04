use super::*;
use crate::boundary::{INVALID_ARGUMENT, INVALID_HANDLE, itofin_handle_release};

fn blank_error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn state_batches_summary_snapshot_and_reset_are_atomic() {
    let mut context = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_convergence_statistics_new(&mut context, &mut id, &mut error) },
        0
    );
    let mut mean = 91.0;
    assert_ne!(
        unsafe { itofin_convergence_statistics_mean(&mut context, id, &mut mean, &mut error) },
        0
    );
    assert_eq!(mean, 91.0);
    assert_ne!(
        unsafe { itofin_convergence_statistics_add(&mut context, id, 2.0, 0.0, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_add(&mut context, id, 2.0, 1.0, &mut error) },
        0
    );
    let invalid = [4.0, Real::NAN];
    assert_ne!(
        unsafe {
            itofin_convergence_statistics_add_batch(
                &mut context,
                id,
                invalid.as_ptr(),
                2,
                std::ptr::null(),
                0,
                &mut error,
            )
        },
        0
    );
    let mut samples = 91;
    let mut weight = 91.0;
    let mut entries = 91;
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_summary(
                &mut context,
                id,
                &mut samples,
                &mut weight,
                &mut entries,
                &mut error,
            )
        },
        0
    );
    assert_eq!((samples, weight, entries), (1, 1.0, 1));
    let values = [4.0, 8.0, 10.0];
    let weights = [2.0, 1.0, 0.0];
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_add_batch(
                &mut context,
                id,
                values.as_ptr(),
                3,
                weights.as_ptr(),
                3,
                &mut error,
            )
        },
        0
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_mean(&mut context, id, &mut mean, &mut error) },
        0
    );
    assert_eq!(mean, 4.5);
    let mut counts = [91; 2];
    let mut means = [91.0; 2];
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_table(
                &mut context,
                id,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        0
    );
    assert_eq!(counts, [1, 3]);
    assert_eq!(means, [2.0, 4.5]);
    counts[0] = 91;
    means[0] = 91.0;
    assert_ne!(
        unsafe {
            itofin_convergence_statistics_table(
                &mut context,
                id,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                1,
                &mut error,
            )
        },
        0
    );
    assert_eq!(counts, [91, 3]);
    assert_eq!(means, [91.0, 4.5]);
    assert_eq!(
        unsafe { itofin_convergence_statistics_reset(&mut context, id, &mut error) },
        0
    );
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_summary(
                &mut context,
                id,
                &mut samples,
                &mut weight,
                &mut entries,
                &mut error,
            )
        },
        0
    );
    assert_eq!((samples, weight, entries), (0, 0.0, 0));
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_table(
                &mut context,
                id,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                &mut error,
            )
        },
        0
    );
    assert_eq!(
        unsafe { itofin_handle_release(&mut context, id, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_reset(&mut context, id, &mut error) },
        INVALID_HANDLE
    );
}

#[test]
fn state_invalid_pointers_handles_and_sizes_do_not_mutate() {
    let mut context = Context::new();
    let mut foreign = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    let misaligned_error = (std::mem::align_of::<ItofinError>() + 1) as *mut ItofinError;
    assert_eq!(
        unsafe { itofin_convergence_statistics_new(&mut context, &mut id, misaligned_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 0);
    assert_eq!(
        unsafe { itofin_convergence_statistics_new(&mut context, &mut id, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_add(&mut context, id, 2.0, 1.0, misaligned_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_add(&mut foreign, id, 2.0, 1.0, &mut error) },
        INVALID_HANDLE
    );
    let wrong = context.insert(17_i32).unwrap();
    assert_eq!(
        unsafe { itofin_convergence_statistics_add(&mut context, wrong, 2.0, 1.0, &mut error) },
        INVALID_HANDLE
    );
    assert_ne!(
        unsafe {
            itofin_convergence_statistics_add_batch(
                &mut context,
                id,
                std::ptr::dangling(),
                usize::MAX,
                std::ptr::null(),
                0,
                &mut error,
            )
        },
        0
    );
    assert_ne!(
        unsafe {
            itofin_convergence_statistics_add_batch(
                &mut context,
                id,
                std::ptr::dangling(),
                100001,
                std::ptr::null(),
                0,
                &mut error,
            )
        },
        0
    );
    let mut samples = 91;
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_summary(
                &mut context,
                id,
                &mut samples,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut error,
            )
        },
        0
    );
    assert_eq!(samples, 0);
    assert_eq!(
        unsafe { itofin_convergence_statistics_add(&mut context, id, 2.0, 1.0, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_convergence_statistics_reset(&mut context, id, misaligned_error) },
        INVALID_ARGUMENT
    );
    let mut weight = 91.0;
    samples = 91;
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_summary(
                &mut context,
                id,
                &mut samples,
                (std::mem::align_of::<Real>() + 1) as *mut Real,
                std::ptr::null_mut(),
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(samples, 91);
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_summary(
                &mut context,
                id,
                &mut samples,
                &mut weight,
                std::ptr::null_mut(),
                &mut error,
            )
        },
        0
    );
    assert_eq!((samples, weight), (1, 1.0));
}
