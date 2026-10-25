use super::*;
use libitofin::cashflow::CashFlow;
use libitofin::cashflows::SimpleCashFlow;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Month;
use std::ptr::null_mut;

fn query(settings: u64) -> ItofinPaymentQuery {
    ItofinPaymentQuery {
        settings,
        settlement: Date::new(15, Month::June, 2026).serial_number(),
        include_settlement: 0,
        direction: 1,
        with_amount: 1,
    }
}
fn sentinel() -> ItofinPaymentResult {
    ItofinPaymentResult {
        present: 7,
        date: -123,
        amount: -456.0,
    }
}
fn unchanged(value: &ItofinPaymentResult) {
    assert_eq!((value.present, value.date, value.amount), (7, -123, -456.0));
}
fn setup(amounts: &[f64]) -> (Context, u64, u64) {
    let mut c = Context::new();
    let settings = shared(Settings::<Date>::new());
    let settings_id = c.insert(settings.clone()).unwrap();
    let date = Date::new(15, Month::July, 2026);
    let leg: Leg = amounts
        .iter()
        .map(|&v| shared(SimpleCashFlow::new(v, date).unwrap()) as Shared<dyn CashFlow>)
        .collect();
    let id = c.insert(leg).unwrap();
    (c, id, settings_id)
}

#[test]
fn amount_aggregates_same_date_and_distinguishes_zero_from_absence() {
    let (mut c, id, settings) = setup(&[0.0, 125.0, 875.0]);
    let mut out = sentinel();
    unsafe {
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), &mut out, null_mut()),
            0
        );
        assert_eq!((out.present, out.amount), (1, 1000.0));
        let mut previous = query(settings);
        previous.direction = 0;
        assert_eq!(
            itofin_leg_payment(&mut c, id, previous, &mut out, null_mut()),
            0
        );
        assert_eq!((out.present, out.date, out.amount), (0, 0, 0.0));
    }
    let (mut c, id, settings) = setup(&[0.0]);
    unsafe {
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), &mut out, null_mut()),
            0
        );
    }
    assert_eq!((out.present, out.amount), (1, 0.0));
}

#[test]
fn failed_queries_leave_output_untouched_without_poisoning_context() {
    let (mut c, id, settings) = setup(&[1.0]);
    let mut out = sentinel();
    unsafe {
        for (which, expected) in [
            (0, INVALID_ARGUMENT),
            (1, INVALID_ARGUMENT),
            (2, INVALID_ARGUMENT),
            (3, INVALID_ARGUMENT),
            (4, INVALID_HANDLE),
            (5, INVALID_HANDLE),
        ] {
            let mut q = query(settings);
            let mut handle = id;
            match which {
                0 => q.direction = 2,
                1 => q.include_settlement = 2,
                2 => q.with_amount = 2,
                3 => q.settlement = -1,
                4 => q.settings = id,
                _ => handle = settings,
            }
            assert_eq!(
                itofin_leg_payment(&mut c, handle, q, &mut out, null_mut()),
                expected
            );
            unchanged(&out);
        }
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let bad_error = std::ptr::with_exposed_provenance_mut::<ItofinError>(1);
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), &mut out, bad_error),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        let bad_out = std::ptr::with_exposed_provenance_mut::<ItofinPaymentResult>(1);
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), bad_out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), &mut out, null_mut()),
            INVALID_HANDLE
        );
        unchanged(&out);
    }
}

#[test]
fn aggregate_overflow_is_an_error_but_date_only_does_not_read_amounts() {
    let (mut c, id, settings) = setup(&[f64::MAX, f64::MAX]);
    let mut out = sentinel();
    unsafe {
        assert_eq!(
            itofin_leg_payment(&mut c, id, query(settings), &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        let mut q = query(settings);
        q.with_amount = 0;
        assert_eq!(itofin_leg_payment(&mut c, id, q, &mut out, null_mut()), 0);
        assert_eq!((out.present, out.amount), (1, 0.0));
    }
}

#[test]
fn cpi_identity_foreign_context_and_empty_leg_are_checked() {
    let (mut c, id, settings_id) = setup(&[1.0]);
    let original = crate::rates_api::settings(&c, settings_id).unwrap();
    let date = Date::new(15, Month::June, 2026);
    original.set_evaluation_date(date);
    let other = shared(Settings::<Date>::new());
    other.set_evaluation_date(date);
    let other_id = c.insert(other).unwrap();
    let indexed = c
        .insert_with_metadata(c.get::<Leg>(id).unwrap(), CpiLegSettings(original))
        .unwrap();
    let mut out = sentinel();
    unsafe {
        let mut q = query(other_id);
        q.with_amount = 0;
        assert_eq!(
            itofin_leg_payment(&mut c, indexed, q, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        let (mut foreign, _, foreign_settings) = setup(&[1.0]);
        assert_eq!(
            itofin_leg_payment(
                &mut foreign,
                indexed,
                query(foreign_settings),
                &mut out,
                null_mut()
            ),
            INVALID_HANDLE
        );
        unchanged(&out);
        let empty = c.insert(Leg::new()).unwrap();
        let mut span = -123;
        assert_ne!(
            itofin_leg_span_date(&mut c, empty, 0, &mut span, null_mut()),
            0
        );
        assert_eq!(span, -123);
        assert_eq!(
            itofin_leg_payment(&mut c, empty, query(settings_id), &mut out, null_mut()),
            0
        );
        assert_eq!((out.present, out.date, out.amount), (0, 0, 0.0));
    }
}

#[test]
fn bond_inspection_rejects_invalid_outputs_and_keeps_retained_settings() {
    use crate::calendar_api::NativeCalendar;
    use libitofin::instruments::ZeroCouponBond;
    use libitofin::time::businessdayconvention::BusinessDayConvention;
    use libitofin::time::calendars::NullCalendar;
    let (mut c, _, settings_id) = setup(&[1.0]);
    let settings = crate::rates_api::settings(&c, settings_id).unwrap();
    let maturity = Date::new(15, Month::July, 2026);
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let bond = ZeroCouponBond::new(
        0,
        calendar.inner.clone(),
        100.0,
        maturity,
        BusinessDayConvention::Unadjusted,
        100.0,
        None,
        settings,
    )
    .unwrap()
    .into_bond();
    let id = c
        .insert(NativeBond::new(bond, calendar, None).unwrap())
        .unwrap();
    let mut span = -123;
    let mut out = sentinel();
    unsafe {
        assert_eq!(
            itofin_bond_inspection(&mut c, id, 0, 0, &mut span, null_mut()),
            0
        );
        assert_eq!(span, maturity.serial_number());
        span = -123;
        assert_eq!(
            itofin_bond_inspection(&mut c, id, 2, 0, &mut span, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(span, -123);
        assert_eq!(
            itofin_bond_inspection(&mut c, id, 0, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bond_inspection(
                &mut c,
                id,
                0,
                0,
                &mut span,
                std::ptr::with_exposed_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(span, -123);
        assert_eq!(
            itofin_bond_inspection(
                &mut c,
                id,
                0,
                0,
                std::ptr::with_exposed_provenance_mut::<i32>(1),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(span, -123);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                0,
                0,
                maturity.serial_number(),
                &mut out,
                std::ptr::with_exposed_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                0,
                0,
                maturity.serial_number(),
                std::ptr::with_exposed_provenance_mut::<ItofinPaymentResult>(1),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                2,
                0,
                maturity.serial_number(),
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                0,
                2,
                maturity.serial_number(),
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        unchanged(&out);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                0,
                0,
                maturity.serial_number(),
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(itofin_handle_release(&mut c, settings_id, null_mut()), 0);
        assert_eq!(
            itofin_bond_payment(
                &mut c,
                id,
                0,
                1,
                maturity.serial_number(),
                &mut out,
                null_mut()
            ),
            0
        );
        assert_eq!((out.present, out.amount), (1, 100.0));
    }
}
