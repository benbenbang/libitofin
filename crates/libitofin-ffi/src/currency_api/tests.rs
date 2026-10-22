use super::*;
use crate::indexes_api::itofin_currency_code;
use std::ptr::{null, null_mut};

fn config(code: &str) -> ItofinCurrencyConfig {
    ItofinCurrencyConfig {
        name: "臺灣\0 dollar".as_ptr(),
        name_length: "臺灣\0 dollar".len(),
        code: code.as_ptr(),
        code_length: code.len(),
        numeric_code: 901,
        symbol: "NT$\0元".as_ptr(),
        symbol_length: "NT$\0元".len(),
        fraction_symbol: null(),
        fraction_symbol_length: 0,
        fractions_per_unit: 100,
    }
}

fn create(c: &mut Context) -> u64 {
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_currency_custom_new(c, &config("TWD"), &mut id, null_mut()) },
        0
    );
    id
}

#[test]
fn six_fields_round_trip_counted_utf8_and_name_equality() {
    let mut c = Context::new();
    let id = create(&mut c);
    for (field, expected) in ["臺灣\0 dollar", "TWD", "NT$\0元", ""].iter().enumerate() {
        let mut required = 91;
        assert_eq!(
            unsafe {
                itofin_currency_text(
                    &mut c,
                    id,
                    field as i32,
                    null_mut(),
                    0,
                    &mut required,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(required, expected.len());
        let mut buf = vec![99; required + 1];
        assert_eq!(
            unsafe {
                itofin_currency_text(
                    &mut c,
                    id,
                    field as i32,
                    buf.as_mut_ptr(),
                    required + 1,
                    &mut required,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(&buf[..required], expected.as_bytes());
        assert_eq!(buf[required], 99);
    }
    for (field, expected) in [901, 100].iter().enumerate() {
        let mut actual = 91;
        assert_eq!(
            unsafe { itofin_currency_integer(&mut c, id, field as i32, &mut actual, null_mut()) },
            0
        );
        assert_eq!(actual, *expected);
    }
    let mut buf = [99; 5];
    assert_eq!(
        unsafe { itofin_currency_code(&mut c, id, buf.as_mut_ptr(), 4, null_mut()) },
        0
    );
    assert_eq!(buf, [b'T', b'W', b'D', 0, 99]);
    let value = c.get::<Currency>(id).unwrap();
    assert_eq!(
        value,
        Currency::new("臺灣\0 dollar", "USD", 840, "$", "", 1)
    );
    assert_ne!(
        value,
        Currency::new("Different name", "TWD", 901, "NT$", "", 100)
    );
}

#[test]
fn constructor_rejects_invalid_codes_utf8_and_pointers_without_output_mutation() {
    let mut c = Context::new();
    let mut id = 91;
    for code in ["", "TW", "TWDD", "台幣", "TW\0", "T1D"] {
        assert_eq!(
            unsafe { itofin_currency_custom_new(&mut c, &config(code), &mut id, null_mut()) },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 91);
    }
    let invalid_utf8 = [255u8];
    let mut cfg = config("TWD");
    cfg.name = invalid_utf8.as_ptr();
    cfg.name_length = 1;
    assert_eq!(
        unsafe { itofin_currency_custom_new(&mut c, &cfg, &mut id, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 91);
    assert_eq!(
        unsafe { itofin_currency_custom_new(&mut c, null(), &mut id, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_currency_custom_new(&mut c, &config("TWD"), null_mut(), null_mut()) },
        INVALID_ARGUMENT
    );
    let mut cfg = config("TWD");
    cfg.symbol = null();
    cfg.symbol_length = 1;
    assert_eq!(
        unsafe { itofin_currency_custom_new(&mut c, &cfg, &mut id, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 91);
}

#[test]
fn failed_accessors_are_atomic_and_handles_are_typed_and_context_scoped() {
    let mut c = Context::new();
    let mut other = Context::new();
    let id = create(&mut c);
    let wrong = c.insert(123u32).unwrap();
    let mut bytes = [99; 32];
    let mut required = 91;
    for capacity in [1, 2, 3] {
        assert_eq!(
            unsafe {
                itofin_currency_text(
                    &mut c,
                    id,
                    0,
                    bytes.as_mut_ptr(),
                    capacity,
                    &mut required,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(required, 91);
        assert_eq!(bytes, [99; 32]);
        assert_eq!(
            unsafe { itofin_currency_code(&mut c, id, bytes.as_mut_ptr(), capacity, null_mut()) },
            INVALID_ARGUMENT
        );
        assert_eq!(bytes, [99; 32]);
    }
    for bad in [wrong, 0, u64::MAX] {
        assert_eq!(
            unsafe {
                itofin_currency_text(
                    &mut c,
                    bad,
                    0,
                    bytes.as_mut_ptr(),
                    32,
                    &mut required,
                    null_mut(),
                )
            },
            INVALID_HANDLE
        );
    }
    assert_eq!(
        unsafe {
            itofin_currency_text(
                &mut other,
                id,
                0,
                bytes.as_mut_ptr(),
                32,
                &mut required,
                null_mut(),
            )
        },
        INVALID_HANDLE
    );
    assert_eq!(
        unsafe {
            itofin_currency_text(
                &mut c,
                id,
                4,
                bytes.as_mut_ptr(),
                32,
                &mut required,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_currency_text(&mut c, id, 0, null_mut(), 32, &mut required, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_currency_text(
                &mut c,
                id,
                0,
                bytes.as_mut_ptr(),
                32,
                null_mut(),
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    let mut integer = 91;
    assert_eq!(
        unsafe { itofin_currency_integer(&mut c, id, 2, &mut integer, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(integer, 91);
    assert_eq!(
        unsafe { itofin_currency_integer(&mut c, wrong, 0, &mut integer, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(
        unsafe { itofin_currency_integer(&mut other, id, 0, &mut integer, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
        0
    );
    assert_eq!(
        unsafe { itofin_currency_integer(&mut c, id, 0, &mut integer, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(required, 91);
    assert_eq!(bytes, [99; 32]);
    assert_eq!(integer, 91);
}

#[test]
fn legacy_code_accessor_cannot_overrun_on_permissive_core_values() {
    let mut c = Context::new();
    let id = c
        .insert(Currency::new("Legacy", "TOOLONG", 0, "", "", 0))
        .unwrap();
    let mut buf = [99; 4];
    assert_eq!(
        unsafe { itofin_currency_code(&mut c, id, buf.as_mut_ptr(), buf.len(), null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(buf, [99; 4]);
}
