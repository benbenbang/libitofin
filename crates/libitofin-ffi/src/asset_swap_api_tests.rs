use super::*;
use crate::bond_api::{ItofinBondConfig, ItofinFixedBondConfig, itofin_fixed_rate_bond_new};
use crate::curves_api::itofin_flat_forward_new;
use crate::indexes_api::{ItofinIborConfig, itofin_ibor_new};
use crate::time_api::{ItofinScheduleConfig, itofin_schedule_new};
use libitofin::currency::Currency;
use libitofin::settings::Settings;
use libitofin::time::calendars::NullCalendar;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use std::ptr::{null, null_mut};

fn fixture() -> (Context, ItofinAssetSwapConfig, u64, u64) {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::from_serial(46024));
    let settings = c.insert(settings).unwrap();
    let dc = c.insert(Actual360::new()).unwrap();
    let dc365 = c.insert(Actual365Fixed::new()).unwrap();
    let calendar = c
        .insert(NativeCalendar {
            inner: NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        })
        .unwrap();
    let currency = c.insert(Currency::eur()).unwrap();
    let mut forward = 0;
    let mut discount = 0;
    let mut index = 0;
    let mut fixed_schedule = 0;
    let mut floating = 0;
    let mut bond = 0;
    unsafe {
        assert_eq!(
            itofin_flat_forward_new(&mut c, 46024, 0.029, dc, &mut forward, null_mut()),
            0
        );
        assert_eq!(
            itofin_flat_forward_new(&mut c, 46024, 0.036, dc365, &mut discount, null_mut()),
            0
        );
        let cfg = ItofinIborConfig {
            tenor_length: 6,
            tenor_unit: 2,
            settlement_days: 0,
            currency,
            fixing_calendar: calendar,
            value_calendar: 0,
            maturity_calendar: 0,
            convention: 2,
            end_of_month: false,
            day_counter: dc,
            forwarding: forward,
            settings,
        };
        assert_eq!(
            itofin_ibor_new(
                &mut c,
                b"AssetBinding".as_ptr(),
                12,
                &cfg,
                false,
                &mut index,
                null_mut()
            ),
            0
        );
        for (frequency, out) in [(0, &mut fixed_schedule), (1, &mut floating)] {
            let cfg = ItofinScheduleConfig {
                start: 46027,
                end: 47123,
                frequency,
                calendar,
                convention: 2,
                rule: 0,
                termination_convention: -1,
            };
            assert_eq!(itofin_schedule_new(&mut c, cfg, out, null_mut()), 0);
        }
        let cfg = ItofinFixedBondConfig {
            bond: ItofinBondConfig {
                settlement_days: 0,
                face_amount: 100.,
                redemption: 100.,
                settings,
                issue_date: 0,
                payment_convention: 2,
                calendar,
            },
            schedule: fixed_schedule,
            day_counter: dc,
            first_period_day_counter: 0,
            ex_coupon_calendar: 0,
            ex_coupon_length: 0,
            ex_coupon_unit: 0,
            ex_coupon_convention: 2,
            has_ex_coupon: 0,
            ex_coupon_end_of_month: 0,
        };
        assert_eq!(
            itofin_fixed_rate_bond_new(&mut c, cfg, &0.043, 1, &mut bond, null_mut()),
            0
        );
    }
    (
        c,
        ItofinAssetSwapConfig {
            bond,
            index,
            floating_schedule: floating,
            floating_day_counter: 0,
            clean_price: 97.25,
            spread: 0.0017,
            pay_bond_coupon: 1,
        },
        discount,
        settings,
    )
}
#[test]
fn asset_swap_preserves_outputs_and_context_ownership() {
    let (mut c, mut cfg, discount, settings) = fixture();
    let mut id = 0;
    unsafe {
        assert_eq!(itofin_asset_swap_new(&mut c, &cfg, &mut id, null_mut()), 0);
        assert_eq!(
            itofin_asset_swap_set_engine(&mut c, id, discount, settings, null_mut()),
            0
        );
        let expected: Vec<f64> = include_str!("../tests/asset_swap_native.tsv")
            .lines()
            .nth(1)
            .unwrap()
            .split('\t')
            .skip(12)
            .map(|value| value.parse().unwrap())
            .collect();
        for (query, want) in expected.into_iter().enumerate() {
            let mut value = 99.;
            assert_eq!(
                itofin_asset_swap_value(&mut c, id, query as i32, &mut value, null_mut()),
                0
            );
            assert!((value - want).abs() <= 2e-10);
        }
        let mut output = 12345.;
        assert_eq!(
            itofin_asset_swap_value(&mut c, id, 99, &mut output, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(output, 12345.);
        let mut other = Context::new();
        assert_eq!(
            itofin_asset_swap_value(&mut other, id, 0, &mut output, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(output, 12345.);
        assert_eq!(
            itofin_asset_swap_new(&mut c, null(), &mut id, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_asset_swap_new(&mut c, &cfg, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_asset_swap_action(&mut c, id, 99, null_mut()),
            INVALID_ARGUMENT
        );
        cfg.pay_bond_coupon = 2;
        let mut sentinel = 12345;
        assert_eq!(
            itofin_asset_swap_new(&mut c, &cfg, &mut sentinel, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(sentinel, 12345);
        cfg.pay_bond_coupon = 1;
        cfg.spread = f64::NAN;
        assert_ne!(
            itofin_asset_swap_new(&mut c, &cfg, &mut sentinel, null_mut()),
            0
        );
        assert_eq!(sentinel, 12345);
    }
    let address = (&mut c as *mut Context) as usize;
    let status = std::thread::spawn(move || {
        let mut out = 12345.;
        let status = unsafe {
            itofin_asset_swap_value(address as *mut Context, id, 0, &mut out, null_mut())
        };
        assert_eq!(out, 12345.);
        status
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
}

#[test]
fn asset_swap_misaligned_error_is_rejected_before_outputs_handles_or_mutation() {
    let (mut c, cfg, discount, settings) = fixture();
    let mut backing = ItofinError {
        code: 12345,
        message: [7; 1024],
    };
    let error = unsafe {
        (&mut backing as *mut ItofinError)
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>()
    };
    let before = c.insert(0u8).unwrap();
    let mut sentinel = 12345;
    assert_eq!(
        unsafe { itofin_asset_swap_new(&mut c, &cfg, &mut sentinel, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(sentinel, 12345);
    let after = c.insert(0u8).unwrap();
    for id in before + 1..after {
        assert!(c.get::<NativeAssetSwap>(id).is_err());
    }
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_asset_swap_new(&mut c, &cfg, &mut id, null_mut()) },
        0
    );
    assert_eq!(
        unsafe { itofin_asset_swap_set_engine(&mut c, id, discount, settings, error) },
        INVALID_ARGUMENT
    );
    assert!(
        c.get::<NativeAssetSwap>(id)
            .unwrap()
            .discount
            .borrow()
            .is_none()
    );
    assert_eq!(
        unsafe { itofin_asset_swap_action(&mut c, id, 2, error) },
        INVALID_ARGUMENT
    );
    assert!(!c.get::<NativeAssetSwap>(id).unwrap().frozen.get());
    let mut value = 12345.;
    assert_eq!(
        unsafe { itofin_asset_swap_value(&mut c, id, 6, &mut value, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(value, 12345.);
    assert_eq!(backing.code, 12345);
    assert_eq!(backing.message, [7; 1024]);
}

#[test]
fn asset_swap_explicit_schedule_can_precede_setting_the_evaluation_date() {
    let (mut c, mut cfg, _, settings) = fixture();
    c.get::<Shared<Settings<Date>>>(settings)
        .unwrap()
        .reset_evaluation_date();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_asset_swap_new(&mut c, &cfg, &mut id, null_mut()) },
        0
    );
    cfg.floating_schedule = 0;
    let mut sentinel = 12345;
    assert_ne!(
        unsafe { itofin_asset_swap_new(&mut c, &cfg, &mut sentinel, null_mut()) },
        0
    );
    assert_eq!(sentinel, 12345);
}
