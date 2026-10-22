use super::*;
use std::ptr::null_mut;

#[test]
fn currency_presets_match_source_fields_and_existing_selectors() {
    let mut context = Context::new();
    for (kind, code) in ["EUR", "USD", "GBP", "JPY"].iter().enumerate() {
        let mut id = 0;
        assert_eq!(
            unsafe { itofin_currency_new(&mut context, kind as i32, &mut id, null_mut()) },
            0
        );
        assert_eq!(context.get::<Currency>(id).unwrap().code(), *code);
    }
    for row in include_str!("../tests/currency_presets.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let kind = fields[0].parse().unwrap();
        let mut id = 0;
        assert_eq!(
            unsafe { itofin_currency_new(&mut context, kind, &mut id, null_mut()) },
            0
        );
        let currency = context.get::<Currency>(id).unwrap();
        assert_eq!(currency.name(), fields[1]);
        assert_eq!(currency.code(), fields[2]);
        assert_eq!(currency.numeric_code(), fields[3].parse::<i32>().unwrap());
        assert_eq!(currency.symbol(), fields[4]);
        assert_eq!(currency.fraction_symbol(), fields[5]);
        assert_eq!(
            currency.fractions_per_unit(),
            fields[6].parse::<i32>().unwrap()
        );
        let mut out = [91; 4];
        assert_eq!(
            unsafe { itofin_currency_code(&mut context, id, out.as_mut_ptr(), 4, null_mut()) },
            0
        );
        assert_eq!(&out[..3], fields[2].as_bytes());
        assert_eq!(out[3], 0);
        let mut foreign = Context::new();
        out = [91; 4];
        assert_ne!(
            unsafe { itofin_currency_code(&mut foreign, id, out.as_mut_ptr(), 4, null_mut()) },
            0
        );
        assert_eq!(out, [91; 4]);
        assert_eq!(
            unsafe { itofin_handle_release(&mut context, id, null_mut()) },
            0
        );
        assert_ne!(
            unsafe { itofin_currency_code(&mut context, id, out.as_mut_ptr(), 4, null_mut()) },
            0
        );
        assert_eq!(out, [91; 4]);
    }
}

#[test]
fn currency_preset_bad_selector_and_null_output_preserve_sentinels() {
    let mut context = Context::new();
    for kind in [-1, 24, i32::MAX] {
        let mut out = 91;
        assert_ne!(
            unsafe { itofin_currency_new(&mut context, kind, &mut out, null_mut()) },
            0
        );
        assert_eq!(out, 91);
    }
    assert_ne!(
        unsafe { itofin_currency_new(&mut context, 4, null_mut(), null_mut()) },
        0
    );
}
