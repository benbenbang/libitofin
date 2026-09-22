use super::*;
use crate::bma_swap_api::{itofin_bma_helper_new, itofin_bma_swap_new, itofin_bma_swap_value};
use libitofin::settings::Settings;
use libitofin::time::date::Month;
use std::ptr::{null, null_mut};

#[test]
fn bma_invalid_calls_preserve_outputs_and_history_recovers() {
    let mut ctx = Context::default();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(23, Month::October, 2025));
    let settings_id = ctx.insert(settings).unwrap();
    let fixing = Date::new(22, Month::October, 2025).serial_number();
    let mut id = 123;
    unsafe {
        assert_ne!(itofin_bma_index_new(&mut ctx, 0, 0, &mut id, null_mut()), 0);
        assert_eq!(id, 123);
        assert_eq!(
            itofin_bma_index_new(&mut ctx, 0, settings_id, &mut id, null_mut()),
            0
        );
        let mut value = 456.0;
        let mut found = 9;
        assert_ne!(
            itofin_bma_fixing(&mut ctx, id, fixing, 2, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 456.0);
        assert_ne!(
            itofin_bma_fixing(&mut ctx, id, fixing, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 456.0);
        assert_ne!(
            itofin_bma_add_fixing(&mut ctx, id, fixing, f64::NAN, null_mut()),
            0
        );
        assert_ne!(
            itofin_bma_past_fixing(&mut ctx, id, fixing, &mut value, null_mut(), null_mut()),
            0
        );
        assert_eq!(value, 456.0);
        assert_eq!(
            itofin_bma_add_fixing(&mut ctx, id, fixing, 0.03, null_mut()),
            0
        );
        assert_eq!(
            itofin_bma_past_fixing(&mut ctx, id, fixing, &mut value, &mut found, null_mut()),
            0
        );
        assert_eq!((value, found), (0.03, 1));
        let mut serial = 789;
        assert_ne!(
            itofin_bma_date(&mut ctx, id, 99, fixing, &mut serial, null_mut()),
            0
        );
        assert_eq!(serial, 789);
        assert_eq!(
            itofin_bma_date(&mut ctx, id, 0, fixing, &mut serial, null_mut()),
            0
        );
        assert_eq!(serial, fixing + 1);
        let mut length = 42;
        let mut buffer = [123; 1];
        assert_ne!(
            itofin_bma_dates(
                &mut ctx,
                id,
                0,
                fixing,
                fixing + 30,
                buffer.as_mut_ptr(),
                1,
                &mut length,
                null_mut()
            ),
            0
        );
        assert_eq!((buffer, length), ([123], 42));
        assert_eq!(
            itofin_bma_dates(
                &mut ctx,
                id,
                0,
                fixing,
                fixing + 30,
                null_mut(),
                0,
                &mut length,
                null_mut()
            ),
            0
        );
        assert!(length > 1);
        assert_ne!(
            itofin_bma_coupon_value(&mut ctx, id, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 0.03);
        assert_ne!(
            itofin_bma_coupon_new(&mut ctx, null(), &mut id, null_mut()),
            0
        );
        assert_ne!(
            itofin_bma_swap_new(&mut ctx, null(), &mut id, null_mut()),
            0
        );
        assert_ne!(
            itofin_bma_helper_new(&mut ctx, null(), &mut id, null_mut()),
            0
        );
        assert_ne!(
            itofin_bma_swap_value(&mut ctx, id, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 0.03);
        assert_eq!(itofin_bma_clear_fixings(&mut ctx, id, null_mut()), 0);
        assert_eq!(
            itofin_bma_past_fixing(&mut ctx, id, fixing, &mut value, &mut found, null_mut()),
            0
        );
        assert_eq!((value, found), (0.0, 0));
        assert_eq!(
            itofin_bma_add_fixing(&mut ctx, id, fixing, 0.031, null_mut()),
            0
        );
        assert_eq!(
            itofin_bma_fixing(&mut ctx, id, fixing, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 0.031);
    }
}
