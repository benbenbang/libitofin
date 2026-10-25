use super::*;
use crate::calendar_api::NativeCalendar;
use crate::inflation_api::ZeroIndex;
use libitofin::handle::RelinkableHandle;
use libitofin::indexes::{Index, inflation::UkRpi};
use libitofin::shared::shared;
use libitofin::time::{
    calendars::NullCalendar, date::Month::*, daycounters::actual365fixed::Actual365Fixed,
};
use std::ptr::{null_mut, without_provenance_mut};

fn fixture() -> (Context, ItofinCpiCouponConfig) {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, June, 2025));
    let index = shared(UkRpi::new(settings));
    index.add_fixing(Date::new(1, April, 2025), 100.8).unwrap();
    index.add_fixing(Date::new(1, May, 2025), 101.0).unwrap();
    let index = c
        .insert(ZeroIndex {
            inner: index,
            curve: RelinkableHandle::empty(),
        })
        .unwrap();
    let dc = c.insert(Actual365Fixed::new()).unwrap();
    (
        c,
        ItofinCpiCouponConfig {
            index,
            base_cpi: 100.0,
            nominal: 100000.0,
            fixed_rate: 0.02,
            payment: Date::new(15, July, 2025).serial_number(),
            start: Date::new(15, January, 2025).serial_number(),
            end: Date::new(15, July, 2025).serial_number(),
            reference_start: 0,
            reference_end: 0,
            ex_coupon: 0,
            lag_length: 3,
            lag_unit: 2,
            interpolation: 0,
            day_counter: dc,
        },
    )
}
#[test]
fn cpi_coupon_boundary_output_preservation_and_pricer_lifetime() {
    let (mut c, a) = fixture();
    let mut coupon = 0;
    let mut pricer = 0;
    let mut value = -71.0;
    unsafe {
        assert_eq!(itofin_cpi_coupon_new(&mut c, a, &mut coupon, null_mut()), 0);
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 0, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, -71.0);
        assert_eq!(
            itofin_cpi_coupon_pricer_new(&mut c, 0, &mut pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_set_pricer(&mut c, coupon, pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 1, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 999.7150684931507).abs() < 1e-10);
        let saved = value;
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 99, 0, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(value, saved);
        assert_eq!(
            itofin_cpi_coupon_value(
                &mut c,
                coupon,
                1,
                0,
                &mut value,
                without_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(value, saved);
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 1, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_cpi_pricer_value(&mut c, pricer, coupon, 1, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, saved);
        assert_eq!(itofin_handle_release(&mut c, pricer, null_mut()), 0);
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 0, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 0.02016).abs() < 1e-14);
    }
}
#[test]
fn cpi_leg_retains_principal_and_rejects_wrong_npv_settings() {
    let (mut c, coupon) = fixture();
    let index = zero_index(&c, coupon.index).unwrap();
    let settings = index.inflation_base().settings().clone();
    let settings_id = c.insert(settings.clone()).unwrap();
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let calendar_id = c.insert(calendar.clone()).unwrap();
    let schedule = Schedule::from_dates(vec![
        Date::new(15, January, 2025),
        Date::new(15, July, 2025),
    ]);
    let schedule = c.insert_with_metadata(schedule, calendar).unwrap();
    let notionals = [100000.0];
    let rates = [0.02];
    let a = ItofinCpiLegConfig {
        schedule,
        index: coupon.index,
        base_cpi: 100.0,
        lag_length: 3,
        lag_unit: 2,
        interpolation: 0,
        day_counter: coupon.day_counter,
        payment_calendar: calendar_id,
        payment_convention: 4,
        notionals: notionals.as_ptr(),
        notionals_len: 1,
        fixed_rates: rates.as_ptr(),
        fixed_rates_len: 1,
        growth_only: 0,
        ex_coupon_length: 0,
        ex_coupon_unit: 0,
        ex_coupon_calendar: 0,
        ex_coupon_convention: 4,
        ex_coupon_end_of_month: 0,
    };
    let mut builder = 0;
    let mut leg = 0;
    unsafe {
        assert_eq!(itofin_cpi_leg_new(&mut c, a, &mut builder, null_mut()), 0);
        assert_eq!(
            itofin_cpi_leg_build(&mut c, builder, &mut leg, null_mut()),
            0
        );
        let flows = c.get::<Leg>(leg).unwrap();
        assert_eq!(flows.len(), 2);
        assert!((flows[1].amount().unwrap() - 100800.0).abs() < 1e-9);
        let mut discount = 0;
        assert_eq!(
            crate::curves_api::itofin_flat_forward_new(
                &mut c,
                Date::new(15, June, 2025).serial_number(),
                0.04,
                coupon.day_counter,
                &mut discount,
                null_mut()
            ),
            0
        );
        let other = shared(Settings::new());
        other.set_evaluation_date(Date::new(15, June, 2025));
        let other = c.insert(other).unwrap();
        let mut output = -99.0;
        assert_eq!(
            crate::cashflows_api::itofin_leg_npv(
                &mut c,
                leg,
                discount,
                other,
                -1,
                0,
                0,
                &mut output,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(output, -99.0);
        assert_eq!(
            crate::cashflows_api::itofin_leg_npv(
                &mut c,
                leg,
                discount,
                settings_id,
                -1,
                0,
                0,
                &mut output,
                null_mut()
            ),
            0
        );
        assert!(output.is_finite());
        let before = output;
        assert_eq!(
            crate::cashflows_api::itofin_leg_npv(
                &mut c,
                leg,
                discount,
                settings_id,
                -1,
                0,
                0,
                &mut output,
                without_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(output, before);
        let spread = crate::bond_analytics_api::ItofinCashFlowsZSpreadConfig {
            discount,
            settings: other,
            include_settlement: -1,
            settlement: 0,
            npv_date: 0,
            compounding: 2,
            frequency: 0,
        };
        let saved = output;
        assert_eq!(
            crate::bond_analytics_api::itofin_cashflows_npv_at_z_spread(
                &mut c,
                leg,
                0.0,
                spread,
                &mut output,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(output, saved);
        let solve = crate::bond_analytics_api::ItofinAnalyticsSolverConfig {
            flags: 0,
            accuracy: 0.0,
            max_evaluations: 0,
            guess: 0.0,
        };
        assert_eq!(
            crate::bond_analytics_api::itofin_cashflows_z_spread(
                &mut c,
                leg,
                saved,
                spread,
                solve,
                &mut output,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(output, saved);
        let metadata = c.metadata::<CpiLegSettings>(leg).unwrap();
        assert!(std::ptr::eq(&*metadata.0, &*settings));
        assert!(std::ptr::eq(
            &*metadata.0,
            &*c.get::<Shared<Settings<Date>>>(settings_id).unwrap()
        ));
        assert_eq!(itofin_handle_release(&mut c, coupon.index, null_mut()), 0);
        assert_eq!(itofin_handle_release(&mut c, builder, null_mut()), 0);
        assert!((flows[1].amount().unwrap() - 100800.0).abs() < 1e-9);
    }
}

#[test]
fn base_date_coupon_is_lazy_and_ignores_explicit_config_base() {
    let (mut c, mut a) = fixture();
    a.base_cpi = f64::NAN;
    let index = zero_index(&c, a.index).unwrap();
    let base = Date::new(15, December, 2024).serial_number();
    let mut coupon = 0;
    let mut value = -71.0;
    unsafe {
        assert_eq!(
            itofin_cpi_coupon_from_base_date(&mut c, a, base, &mut coupon, null_mut()),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 6, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, -71.0);
        index
            .add_fixing(Date::new(1, December, 2024), 100.0)
            .unwrap();
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 6, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 100.0);
        let retained = c.get::<Shared<CpiCoupon>>(coupon).unwrap();
        assert_eq!(retained.base_cpi(), f64::MAX);
        assert_eq!(retained.base_date(), Some(Date::new(15, December, 2024)));
        let mut pricer = 0;
        assert_eq!(
            itofin_cpi_coupon_pricer_new(&mut c, 0, &mut pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_set_pricer(&mut c, coupon, pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 1, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 999.7150684931507).abs() < 1e-10);
    }
}
#[test]
fn base_date_coupon_rejects_null_dates_and_pointers_before_output_write() {
    let (mut c, a) = fixture();
    let mut out = 191;
    unsafe {
        assert_eq!(
            itofin_cpi_coupon_from_base_date(&mut c, a, 0, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 191);
        let (_, a) = fixture();
        assert_eq!(
            itofin_cpi_coupon_from_base_date(
                &mut c,
                a,
                Date::new(1, January, 2024).serial_number(),
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 191);
        let (_, a) = fixture();
        assert_eq!(
            itofin_cpi_coupon_from_base_date(
                &mut c,
                a,
                Date::new(1, January, 2024).serial_number(),
                &mut out,
                without_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 191);
    }
}

fn base_date_leg_config(c: &mut Context, coupon: &ItofinCpiCouponConfig) -> ItofinCpiLegConfig {
    static NOTIONALS: [f64; 1] = [10000.0];
    static RATES: [f64; 1] = [0.02];
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let calendar_id = c.insert(calendar.clone()).unwrap();
    let schedule = c
        .insert_with_metadata(
            Schedule::from_dates(vec![
                Date::new(15, January, 2025),
                Date::new(15, July, 2025),
            ]),
            calendar,
        )
        .unwrap();
    ItofinCpiLegConfig {
        schedule,
        index: coupon.index,
        base_cpi: f64::NAN,
        lag_length: 3,
        lag_unit: 2,
        interpolation: 0,
        day_counter: coupon.day_counter,
        payment_calendar: calendar_id,
        payment_convention: 2,
        notionals: NOTIONALS.as_ptr(),
        notionals_len: 1,
        fixed_rates: RATES.as_ptr(),
        fixed_rates_len: 1,
        growth_only: 0,
        ex_coupon_length: 0,
        ex_coupon_unit: 0,
        ex_coupon_calendar: 0,
        ex_coupon_convention: 2,
        ex_coupon_end_of_month: 0,
    }
}
#[test]
fn both_base_date_factories_reject_null_and_misaligned_outputs_or_errors() {
    for mode in 0..3 {
        let (mut c, coupon) = fixture();
        let leg = base_date_leg_config(&mut c, &coupon);
        let mut sentinel = 719;
        let out = match mode {
            0 => null_mut(),
            1 => without_provenance_mut::<u64>(1),
            _ => &mut sentinel,
        };
        let error = if mode == 2 {
            without_provenance_mut::<ItofinError>(1)
        } else {
            null_mut()
        };
        unsafe {
            assert_eq!(
                itofin_cpi_coupon_from_base_date(
                    &mut c,
                    coupon,
                    Date::new(15, December, 2024).serial_number(),
                    out,
                    error
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(sentinel, 719);
            assert_eq!(
                itofin_cpi_leg_from_base_date(&mut c, leg, 0, out, error),
                INVALID_ARGUMENT
            );
            assert_eq!(sentinel, 719);
            let mut pricer = 0;
            assert_eq!(
                itofin_cpi_coupon_pricer_new(&mut c, 0, &mut pricer, null_mut()),
                0
            );
        }
    }
}
#[test]
fn effective_base_query_rejects_wrong_foreign_and_released_handles_without_writes() {
    let (mut c, a) = fixture();
    let wrong = a.day_counter;
    let mut other = Context::new();
    let mut coupon = 0;
    let mut value = -719.0;
    unsafe {
        assert_eq!(
            itofin_cpi_coupon_from_base_date(
                &mut c,
                a,
                Date::new(1, April, 2025).serial_number(),
                &mut coupon,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 6, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 100.8);
        value = -719.0;
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, wrong, 6, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(value, -719.0);
        assert_eq!(
            itofin_cpi_coupon_value(&mut other, coupon, 6, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(value, -719.0);
        assert_eq!(
            itofin_cpi_coupon_value(
                &mut c,
                coupon,
                6,
                0,
                &mut value,
                without_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(value, -719.0);
        assert_eq!(itofin_handle_release(&mut c, coupon, null_mut()), 0);
        assert_eq!(
            itofin_cpi_coupon_value(&mut c, coupon, 6, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(value, -719.0);
    }
}
