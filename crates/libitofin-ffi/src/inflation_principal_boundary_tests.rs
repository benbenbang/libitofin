use super::tests::{explicit, fixture, live};
use super::*;
use std::fmt::Debug;
use std::ptr::{null_mut, without_provenance_mut};

fn check_outputs<T: Copy + Debug + PartialEq>(
    c: &mut Context,
    sentinel: T,
    mut call: impl FnMut(&mut Context, *mut T, *mut ItofinError) -> i32,
) {
    assert_eq!(call(c, null_mut(), null_mut()), INVALID_ARGUMENT);
    assert_eq!(
        call(c, without_provenance_mut::<T>(1), null_mut()),
        INVALID_ARGUMENT
    );
    let mut value = sentinel;
    assert_eq!(
        call(c, &mut value, without_provenance_mut::<ItofinError>(1)),
        INVALID_ARGUMENT
    );
    assert_eq!(value, sentinel);
    assert_eq!(call(c, &mut value, null_mut()), 0);
}

#[test]
fn inflation_principal_all_exports_validate_outputs_before_work() {
    let (mut c, index, _) = fixture();
    unsafe {
        check_outputs(&mut c, 715_u64, |c, out, error| {
            itofin_cpi_cashflow_new(c, explicit(index), out, error)
        });
        check_outputs(&mut c, 716_u64, |c, out, error| {
            itofin_zero_inflation_cashflow_new(c, live(index), out, error)
        });
        let mut id = 0;
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut id, null_mut()),
            0
        );
        check_outputs(&mut c, -17.0_f64, |c, out, error| {
            itofin_inflation_cashflow_value(c, id, 0, out, error)
        });
        check_outputs(&mut c, -73_i32, |c, out, error| {
            itofin_inflation_cashflow_date(c, id, out, error)
        });
        for as_leg in [0, 1] {
            check_outputs(&mut c, 717_u64, |c, out, error| {
                itofin_inflation_cashflow_convert(c, id, as_leg, out, error)
            });
        }
        let mut value = 0.0;
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 100.0).abs() < 1e-10);
    }
}

#[test]
fn inflation_principal_exports_reject_foreign_released_and_wrong_type_handles() {
    let (mut c, index, settings) = fixture();
    let (_foreign, foreign_index, _) = fixture();
    let mut released = 0;
    unsafe {
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut released, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, released, null_mut()), 0);
        for wrong in [0, index, settings, foreign_index, released] {
            let mut scalar = 29.0;
            let mut date = -81;
            let mut converted = 825;
            assert_eq!(
                itofin_inflation_cashflow_value(&mut c, wrong, 0, &mut scalar, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(scalar, 29.0);
            assert_eq!(
                itofin_inflation_cashflow_date(&mut c, wrong, &mut date, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(date, -81);
            assert_eq!(
                itofin_inflation_cashflow_convert(&mut c, wrong, 0, &mut converted, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(converted, 825);
        }
        for wrong in [0, settings, foreign_index, released] {
            let mut out = 919;
            assert_eq!(
                itofin_cpi_cashflow_new(&mut c, explicit(wrong), &mut out, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(out, 919);
            assert_eq!(
                itofin_zero_inflation_cashflow_new(&mut c, live(wrong), &mut out, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(out, 919);
        }
        let mut out = 0;
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut out, null_mut()),
            0
        );
    }
}
