use super::*;
use crate::boundary::{INVALID_ARGUMENT, INVALID_HANDLE, itofin_handle_release};
use std::ptr::{null, null_mut};

#[test]
fn weighted_state_and_atomic_batch() {
    let mut ctx = Context::new();
    let mut id = 0;
    let mut value = 91.0;
    let mut count = 99;
    let values = [-4.0, -2.0, 2.0, 8.0];
    let weights = [1.0, 2.0, 1.0, 0.0];
    unsafe {
        assert_eq!(
            itofin_incremental_statistics_new(&mut ctx, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_add_batch(
                &mut ctx,
                id,
                values.as_ptr(),
                4,
                weights.as_ptr(),
                4,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 0, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 4);
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 1, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 2);
        for (measure, expected) in [
            (0, 4.0),
            (1, 3.0),
            (2, -4.0),
            (3, 8.0),
            (4, -1.5),
            (5, 19.0 / 3.0),
            (10, 16.0),
            (11, 4.0),
        ] {
            assert_eq!(
                itofin_incremental_statistics_query(&mut ctx, id, measure, &mut value, null_mut()),
                0
            );
            assert!((value - expected).abs() < 1e-12, "measure {measure}");
        }
        let bad = [1.0, Real::NAN];
        assert_eq!(
            itofin_incremental_statistics_add_batch(
                &mut ctx,
                id,
                bad.as_ptr(),
                2,
                null(),
                0,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 0, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 4);
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, id, Real::INFINITY, 1.0, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_incremental_statistics_query(&mut ctx, id, 99, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert!((value - 4.0).abs() < 1e-12);
        assert_eq!(
            itofin_incremental_statistics_reset(&mut ctx, id, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 0, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 0);
        assert_ne!(
            itofin_incremental_statistics_query(&mut ctx, id, 4, &mut value, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, id, 1e100, 1.0, null_mut()),
            INVALID_ARGUMENT
        );
        let extreme = [1.0, 1e100];
        assert_eq!(
            itofin_incremental_statistics_add_batch(
                &mut ctx,
                id,
                extreme.as_ptr(),
                extreme.len(),
                null(),
                0,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 0, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 0);
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, id, 2.0, 1.0, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_query(&mut ctx, id, 4, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 2.0);
        assert_eq!(itofin_handle_release(&mut ctx, id, null_mut()), 0);
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, id, 1.0, 1.0, null_mut()),
            INVALID_HANDLE
        );
        let other = ctx.insert(8_u64).unwrap();
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, other, 1.0, 1.0, null_mut()),
            INVALID_HANDLE
        );
    }
}

#[test]
fn large_offset_small_spread_and_zero_weight() {
    let mut ctx = Context::new();
    let mut id = 0;
    let mut value = 0.0;
    let mut count = 0;
    unsafe {
        assert_eq!(
            itofin_incremental_statistics_new(&mut ctx, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_add(&mut ctx, id, -20.0, 0.0, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_query(&mut ctx, id, 2, &mut value, null_mut()),
            0
        );
        assert_eq!(value, -20.0);
        assert_ne!(
            itofin_incremental_statistics_query(&mut ctx, id, 4, &mut value, null_mut()),
            0
        );
        for offset in 0..4 {
            assert_eq!(
                itofin_incremental_statistics_add(
                    &mut ctx,
                    id,
                    1e12 + offset as Real,
                    1.0,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 0, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 5);
        assert_eq!(
            itofin_incremental_statistics_query(&mut ctx, id, 4, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 1e12 + 1.5);
        assert_eq!(
            itofin_incremental_statistics_query(&mut ctx, id, 5, &mut value, null_mut()),
            0
        );
        assert!((value - 1.5625).abs() < 1e-10);
        assert_eq!(
            itofin_incremental_statistics_count(&mut ctx, id, 1, &mut count, null_mut()),
            0
        );
        assert_eq!(count, 1);
        assert_ne!(
            itofin_incremental_statistics_query(&mut ctx, id, 10, &mut value, null_mut()),
            0
        );
    }
}

#[test]
fn moments_and_context_identity() {
    let mut first = Context::new();
    let mut second = Context::new();
    let mut id = 0;
    let mut out = 91.0;
    unsafe {
        assert_eq!(
            itofin_incremental_statistics_new(&mut first, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_incremental_statistics_query(&mut second, id, 4, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, 91.0);
        for value in [1.0, 2.0, 3.0, 4.0] {
            assert_eq!(
                itofin_incremental_statistics_add(&mut first, id, value, 1.0, null_mut()),
                0
            );
        }
        assert_eq!(
            itofin_incremental_statistics_query(&mut first, id, 8, &mut out, null_mut()),
            0
        );
        assert_eq!(out, 0.0);
        assert_eq!(
            itofin_incremental_statistics_query(&mut first, id, 9, &mut out, null_mut()),
            0
        );
        assert!((out + 1.2).abs() < 1e-12);
        assert_eq!(
            itofin_incremental_statistics_query(&mut first, id, 4, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
    }
}
