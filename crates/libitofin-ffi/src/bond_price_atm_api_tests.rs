use super::*;
use crate::calendar_api::NativeCalendar;
use libitofin::cashflows::FixedRateCoupon;
use libitofin::handle::Handle;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::NullCalendar;
use libitofin::time::date::Month;
use libitofin::time::daycounters::actual360::Actual360;
use std::ptr::null_mut;

fn date(year: i32) -> Date {
    Date::new(1, Month::January, year)
}

fn fixture() -> (Context, u64, u64, u64) {
    let mut context = Context::new();
    let settings = shared(Settings::new());
    let leg: Leg = [375.0, 250.0, 125.0]
        .into_iter()
        .enumerate()
        .map(|(i, n)| {
            let y = 2025 + i as i32;
            shared(FixedRateCoupon::from_rate(
                date(y + 1),
                n,
                0.04,
                Actual360::new(),
                date(y),
                date(y + 1),
                None,
                None,
                None,
            )) as _
        })
        .collect();
    let bond = libitofin::instruments::Bond::from_coupons(
        0,
        NullCalendar::new(),
        None,
        leg,
        settings.clone(),
    )
    .unwrap();
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: Some(2100),
        first_year: 1901,
    };
    let bond = context
        .insert(NativeBond::new(bond, calendar, None).unwrap())
        .unwrap();
    let curve = FlatForward::with_rate(
        date(2025),
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        libitofin::time::frequency::Frequency::Annual,
    );
    let curve: Handle<dyn YieldTermStructure> =
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>);
    let curve = context.insert(curve).unwrap();
    let settings = context.insert(settings).unwrap();
    (context, bond, curve, settings)
}

#[test]
fn price_atm_native_current_notional_and_boundary_errors() {
    let (mut context, bond, discount, settings) = fixture();
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let mut out = -123.0;
    let serial = date(2026).serial_number();
    let df = |y| (-0.03 * (date(y) - date(2026)) as f64 / 360.0).exp();
    let annuity = 250.0 * 365.0 / 360.0 * df(2027) + 125.0 * 365.0 / 360.0 * df(2028);
    let expected = (101.25 / 100.0 * 250.0 - 125.0 * (df(2027) + df(2028))) / annuity;
    for kind in [0, 1] {
        assert_eq!(
            unsafe {
                itofin_bond_atm_rate_from_price(
                    &mut context,
                    bond,
                    discount,
                    101.25,
                    kind,
                    serial,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert!(out.is_finite() && (out - expected).abs() < 3e-13);
    }
    for (b, c, price, kind, settlement) in [
        (bond, discount, f64::NAN, 0, serial),
        (bond, discount, f64::INFINITY, 1, serial),
        (bond, discount, f64::NEG_INFINITY, 1, serial),
        (bond, discount, f64::MAX, 1, serial),
        (bond, discount, 100.0, -1, serial),
        (bond, discount, 100.0, 2, serial),
        (bond, discount, 100.0, 0, 0),
        (bond, discount, 100.0, 1, -1),
        (bond, discount, 100.0, 1, date(2028).serial_number()),
        (discount, discount, 100.0, 0, serial),
        (bond, settings, 100.0, 0, serial),
        (u64::MAX, discount, 100.0, 0, serial),
    ] {
        out = -123.0;
        assert_ne!(
            unsafe {
                itofin_bond_atm_rate_from_price(
                    &mut context,
                    b,
                    c,
                    price,
                    kind,
                    settlement,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(out, -123.0);
    }
    let (_foreign, foreign_bond, foreign_curve, _) = fixture();
    for (b, curve) in [(foreign_bond, discount), (bond, foreign_curve)] {
        out = -123.0;
        assert_eq!(
            unsafe {
                itofin_bond_atm_rate_from_price(
                    &mut context,
                    b,
                    curve,
                    100.0,
                    1,
                    serial,
                    &mut out,
                    &mut error,
                )
            },
            INVALID_HANDLE
        );
        assert_eq!(out, -123.0);
    }
    let misaligned_error = (&raw mut error)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinError>();
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                discount,
                100.0,
                1,
                serial,
                &mut out,
                misaligned_error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, -123.0);
    let misaligned_out = (&raw mut out).cast::<u8>().wrapping_add(1).cast::<f64>();
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                discount,
                100.0,
                1,
                serial,
                misaligned_out,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, -123.0);
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                discount,
                100.0,
                1,
                serial,
                null_mut(),
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                null_mut(),
                bond,
                discount,
                100.0,
                1,
                serial,
                &mut out,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    let address = (&raw mut context) as usize;
    let wrong_thread = std::thread::spawn(move || {
        let mut sentinel = -123.0;
        let code = unsafe {
            itofin_bond_atm_rate_from_price(
                address as *mut Context,
                bond,
                discount,
                100.0,
                1,
                serial,
                &mut sentinel,
                null_mut(),
            )
        };
        (code, sentinel)
    })
    .join()
    .unwrap();
    assert_eq!(wrong_thread, (WRONG_THREAD, -123.0));
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut context, settings, &mut error) },
        0
    );
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                discount,
                101.25,
                1,
                serial,
                &mut out,
                &mut error,
            )
        },
        0
    );
    assert!((out - expected).abs() < 3e-13);
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut context, discount, &mut error) },
        0
    );
    out = -123.0;
    assert_eq!(
        unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                discount,
                101.25,
                1,
                serial,
                &mut out,
                &mut error,
            )
        },
        INVALID_HANDLE
    );
    assert_eq!(out, -123.0);
}

fn oracle_date(text: &str) -> Date {
    let parts: Vec<i32> = text.split('-').map(|n| n.parse().unwrap()).collect();
    Date::new(parts[2], Month::from_ordinal(parts[1]), parts[0])
}

fn oracle_cases() -> std::collections::BTreeMap<String, serde_json::Value> {
    let records = include_str!("../../../sdk/go/testdata/bond_price_atm/cases.jsonl");
    let mut cases = std::collections::BTreeMap::new();
    for line in records.lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        if row["record_type"] == "case" {
            let case = row["value"].clone();
            assert!(
                cases
                    .insert(case["id"].as_str().unwrap().to_owned(), case)
                    .is_none()
            );
        }
    }
    assert_eq!(cases.len(), 49);
    cases
}

fn oracle_fixture(case: &serde_json::Value) -> (Context, u64, u64) {
    use libitofin::instruments::{Bond, ZeroCouponBond};
    use libitofin::time::businessdayconvention::BusinessDayConvention;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    let dates: Vec<Date> = case["dates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| oracle_date(v.as_str().unwrap()))
        .collect();
    let balances: Vec<f64> = case["balances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let settings = shared(Settings::new());
    let days = u32::try_from(case["settlement_days"].as_u64().unwrap()).unwrap();
    let redemption = case["redemption"].as_f64().unwrap();
    let bond = if case["kind"] == "zero" {
        ZeroCouponBond::new(
            days,
            NullCalendar::new(),
            balances[0],
            *dates.last().unwrap(),
            BusinessDayConvention::Unadjusted,
            redemption,
            Some(dates[0]),
            settings,
        )
        .unwrap()
        .into_bond()
    } else {
        let rates = case["coupon_rates"].as_array().unwrap();
        let ex_days = i32::try_from(case["ex_days"].as_i64().unwrap()).unwrap();
        let leg: Leg = balances
            .iter()
            .enumerate()
            .map(|(i, &nominal)| {
                let rate = rates[i.min(rates.len() - 1)].as_f64().unwrap();
                shared(FixedRateCoupon::from_rate(
                    dates[i + 1],
                    nominal,
                    rate,
                    Actual360::new(),
                    dates[i],
                    dates[i + 1],
                    None,
                    None,
                    (ex_days > 0).then(|| dates[i + 1] - ex_days),
                )) as _
            })
            .collect();
        let mut bond = Bond::new(days, NullCalendar::new(), Some(dates[0]), leg, settings).unwrap();
        bond.add_redemptions_to_cashflows(&[redemption]).unwrap();
        bond
    };
    let mut context = Context::new();
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let bond = context
        .insert(NativeBond::new(bond, calendar, None).unwrap())
        .unwrap();
    let curve = FlatForward::with_rate(
        oracle_date(case["reference"].as_str().unwrap()),
        case["discount_rate"].as_f64().unwrap(),
        Actual365Fixed::new(),
        Compounding::Continuous,
        libitofin::time::frequency::Frequency::Annual,
    );
    let curve: Handle<dyn YieldTermStructure> =
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>);
    let curve = context.insert(curve).unwrap();
    (context, bond, curve)
}

#[test]
fn price_atm_all_independent_fixed_amortizing_and_zero_oracle_rows() {
    let cases = oracle_cases();
    let mut checked = 0;
    for line in
        include_str!("../../../sdk/go/testdata/bond_price_atm/bond_atm_from_price_reference.csv")
            .lines()
            .skip(1)
    {
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 15);
        let case = &cases[fields[0]];
        let (mut context, bond, curve) = oracle_fixture(case);
        let settlement = oracle_date(fields[6]);
        let native = context.get::<NativeBond>(bond).unwrap();
        assert_eq!(
            native.bond.borrow().notional(Some(settlement)).unwrap(),
            fields[11].parse::<f64>().unwrap()
        );
        let accrued = native
            .bond
            .borrow()
            .accrued_amount(Some(settlement))
            .unwrap();
        assert!(
            accrued.is_finite() && (accrued - fields[12].parse::<f64>().unwrap()).abs() <= 2e-12
        );
        let mut out = -123.0;
        let price = fields[10].parse::<f64>().unwrap();
        let kind = i32::from(fields[9] == "dirty");
        let code = unsafe {
            itofin_bond_atm_rate_from_price(
                &mut context,
                bond,
                curve,
                price,
                kind,
                settlement.serial_number(),
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(code, 0, "{} {} {price}", fields[0], fields[9]);
        for expected in [fields[13], fields[14]] {
            let expected = expected.parse::<f64>().unwrap();
            assert!(
                out.is_finite() && expected.is_finite() && (out - expected).abs() <= 2e-13,
                "{} {out} {expected}",
                fields[0]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 242);
}

#[test]
fn price_atm_independent_maturity_and_null_bps_error_rows_preserve_output() {
    let cases = oracle_cases();
    let mut checked = 0;
    for line in
        include_str!("../../../sdk/go/testdata/bond_price_atm/bond_atm_from_price_errors.csv")
            .lines()
            .skip(1)
    {
        let fields: Vec<&str> = line.split(',').collect();
        let case = &cases[fields[0]];
        let (mut context, bond, curve) = oracle_fixture(case);
        let settlement = oracle_date(case["settlement"].as_str().unwrap()).serial_number();
        let mut value = -123.0;
        let price = fields[3].parse::<f64>().unwrap();
        let kind = i32::from(fields[2] == "dirty");
        assert_ne!(
            unsafe {
                itofin_bond_atm_rate_from_price(
                    &mut context,
                    bond,
                    curve,
                    price,
                    kind,
                    settlement,
                    &mut value,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(value, -123.0);
        assert_eq!(
            unsafe {
                crate::bond_api::itofin_bond_real(
                    &mut context,
                    bond,
                    5,
                    settlement,
                    &mut value,
                    null_mut(),
                )
            },
            0
        );
        assert!(value.is_finite());
        checked += 1;
    }
    assert_eq!(checked, 52);
}
